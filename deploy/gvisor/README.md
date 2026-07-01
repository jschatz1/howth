# Running howth under gVisor (`runsc`)

This is howth's **outer containment tier** — running the whole process inside
[gVisor](https://gvisor.dev), a userspace application kernel that intercepts
syscalls before they reach the host.

## Why this tier exists

howth enforces permissions at three levels. Each contains something the one
above it cannot:

| Tier | Mechanism | Binds… | Escapes it does **not** contain |
|------|-----------|--------|---------------------------------|
| 1. Op gates | in-process checks in howth's V8 runtime | JS calling `fs`/`net`/`run`/`env` | native code (N-API), a V8 RCE |
| 2. OS sandbox | `sandbox-exec` (macOS) / `bwrap` (Linux) around the `node` subprocess | the node child's syscalls | a kernel-level exploit; missing on Linux without `bwrap` |
| 3. **gVisor (this)** | userspace kernel around the whole process | **every syscall**, including from native/N-API code | (host-kernel exploits in gVisor itself — rare) |

Tiers 1 and 2 assume the runtime itself is trustworthy. The moment an N-API
addon or a V8 bug runs **native machine code**, those checks are bypassed —
the code makes raw syscalls the op layer never sees. gVisor is the only tier
that still contains that: the workload's syscalls hit gVisor's Sentry (a
userspace kernel), not the host, so even fully-native code stays boxed.

Use this tier when you run **untrusted code and assume the runtime can be
broken** — multi-tenant execution, running arbitrary npm packages with native
addons, etc.

## Prerequisites

- **Linux** (gVisor does not run on macOS or Windows).
- Docker.
- gVisor's `runsc` installed and registered as a Docker runtime. Follow the
  [official install guide](https://gvisor.dev/docs/user_guide/install/), then
  register the runtime — typically in `/etc/docker/daemon.json`:

  ```json
  {
    "runtimes": {
      "runsc": {
        "path": "/usr/local/bin/runsc"
      }
    }
  }
  ```

  Restart Docker (`sudo systemctl restart docker`) and confirm:

  ```console
  $ docker run --rm --runtime=runsc alpine dmesg | head -1
  [   0.000000] Starting gVisor...
  ```

  That `Starting gVisor...` line (instead of a normal Linux boot log) is proof
  the container is running under the gVisor kernel, not the host kernel.

## Build the image

From the repository root (the build context must be the workspace root):

```console
$ docker build -f deploy/gvisor/Dockerfile -t howth:gvisor .
```

The image ships a release `howth` (built with the `native-runtime` feature, so
the op-level gates work) on a slim Node base (so the `--node` path also works).

## Run

Directly:

```console
$ docker run --runtime=runsc --rm -v "$PWD:$PWD" -w "$PWD" \
    howth:gvisor run --sandbox --allow-read="$PWD" app.js
```

Or via the helper, which derives the container's mounts/network/env from
howth's own permission flags (see mapping below):

```console
$ deploy/gvisor/howth-isolate run --sandbox --allow-read=./data --allow-net app.js
```

Set `DRY_RUN=1` to print the `docker` command without running it — useful for
inspecting exactly what will be exposed:

```console
$ DRY_RUN=1 deploy/gvisor/howth-isolate run --sandbox --allow-read=./data app.js
docker run --rm -i --runtime=runsc --network=none -w /work \
  -v /work:/work:ro -v /work/data:/work/data:ro howth:gvisor run --sandbox --allow-read=./data app.js
```

## Flag → container mapping (used by `howth-isolate`)

| howth flag | Container effect |
|------------|------------------|
| `--allow-read=P1,P2` | read-only bind mount per path (`-v P:P:ro`) |
| `--allow-write=P1,P2` | read-write bind mount per path (`-v P:P:rw`) |
| `--allow-net[=…]` | container networking on (`--network=bridge`) — else `--network=none` |
| `--allow-env=V1,V2` | forward those host env vars (`-e V1 -e V2`) |
| `--sandbox` | deny-by-default: no network, only the workdir + explicit grants are mounted |

The working directory is always mounted (read-only unless a write grant covers
it) so the entry file is loadable. **Two layers, one set of flags:** the same
flags are also passed through to `howth` inside the container, so the op-level
gates apply *inside* the gVisor boundary — defense in depth.

Notes and limits:
- gVisor adds real overhead on syscall- and I/O-heavy workloads; benchmark
  before using it for latency-sensitive services.
- `--allow-net` cannot be filtered by host at the container layer (Docker
  networking is coarse); howth's in-process `check_net` still applies the
  host/port allowlist inside.
- Bare `--allow-read` / `--allow-write` (grant-all, no list) can't be expressed
  as a container bind of "everything"; the helper mounts the workdir + explicit
  paths and leaves the rest to howth's in-process gates. The helper prints a
  note when it sees these.
