# howth under gVisor — container image
#
# This image is meant to be run under the gVisor (`runsc`) OCI runtime, which
# interposes a userspace kernel between the workload and the host. That is the
# only tier that contains *native-code* escapes — N-API addons or a V8 RCE that
# run machine code past howth's op-level gates and the OS sandbox. See README.md.
#
# Build:  docker build -f deploy/gvisor/Dockerfile -t howth:gvisor .
# Run:    docker run --runtime=runsc --rm howth:gvisor run --sandbox app.js
#
# The build context is the repository root (howth is a cargo workspace).

# ── Stage 1: build a release `howth` with the native runtime ────────────────
FROM rust:1-bookworm AS builder

# rusty_v8 / deno_core need clang; the rest are standard native-build deps.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        python3 \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
# Copy the whole workspace (path-dependency crates need to be present).
COPY . .

# native-runtime enables howth's own V8 runtime + the op-level permission gates.
RUN cargo build --release -p fastnode-cli --features native-runtime --bin howth \
    && strip target/release/howth || true

# ── Stage 2: minimal runtime image ──────────────────────────────────────────
# Base on node-slim so the `--node` compatibility path also works inside the
# sandbox. gVisor contains this process regardless of which path executes.
FROM node:22-bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/howth /usr/local/bin/howth

# Run as an unprivileged user. gVisor is the containment boundary, but least
# privilege inside it is still good hygiene.
RUN useradd --create-home --uid 10001 howth
USER howth
WORKDIR /work

ENTRYPOINT ["howth"]
CMD ["--help"]
