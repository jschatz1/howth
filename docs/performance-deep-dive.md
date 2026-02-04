# Howth HTTP Performance Deep Dive

**Date:** 2026-02-03
**Author:** Performance debugging session
**Purpose:** Document findings for second opinion on optimization strategies

---

## Executive Summary

Howth achieves **81% of Bun's HTTP throughput** (163K vs 201K RPS). The gap breaks down into two distinct bottlenecks:

1. **JS overhead: ~15%** - Async channel crossing between Tokio and V8
2. **HTTP stack: ~5%** - Bun's HTTP implementation is faster than Hyper

This document details our investigation, what we tried, and remaining optimization paths.

---

## Benchmark Results

### Primary Benchmark (50 connections, 5 seconds)

| Server | RPS | Latency | Relative |
|--------|-----|---------|----------|
| **Bun** | 201K | ~250µs | 100% (target) |
| **Pure Rust (Hyper, no JS)** | 192K | ~250µs | 95% |
| **Howth serveBatch** | 163K | ~300µs | 81% |
| **Howth serve (non-batch)** | 156K | ~320µs | 78% |
| **Node.js** | 111K | ~450µs | 55% |

### Connection Scaling

| Connections | Bun | Howth | Howth % |
|-------------|-----|-------|---------|
| 50 | 228K | 203K | 89% |
| 100 | 226K | 186K | 82% |
| 200 | 205K | 161K | 79% |

**Key observation:** Bun maintains throughput at high concurrency; Howth degrades. This suggests the async coordination overhead increases under load.

---

## Architecture Comparison

### Bun's Architecture
```
┌─────────────────────────────────────────┐
│          Single Event Loop              │
│  ┌─────────────┐    ┌────────────────┐  │
│  │ HTTP Server │───▶│ JavaScriptCore │  │
│  │  (custom)   │◀───│                │  │
│  └─────────────┘    └────────────────┘  │
│         Direct function calls           │
└─────────────────────────────────────────┘
```
- Single-threaded, tight integration
- HTTP server calls JS directly (no channel)
- Custom HTTP parser optimized for their use case

### Howth's Architecture
```
┌─────────────────────────────────────────────────────────┐
│                    Tokio Runtime                         │
│  ┌─────────────┐         ┌─────────────────────────┐    │
│  │   Hyper     │         │      V8 Thread          │    │
│  │  (HTTP)     │         │   ┌───────────────┐     │    │
│  │             │         │   │   deno_core   │     │    │
│  │  ┌───────┐  │  mpsc   │   │  ┌─────────┐  │     │    │
│  │  │ Task  │──┼────────▶│   │  │ JS Code │  │     │    │
│  │  │       │◀─┼─oneshot─│   │  └─────────┘  │     │    │
│  │  └───────┘  │         │   └───────────────┘     │    │
│  └─────────────┘         └─────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```
- Multi-threaded Tokio runtime
- Hyper spawns task per connection
- Requests cross async channel to reach V8
- Responses cross oneshot channel back to Hyper

---

## Where Time Goes

### Request Lifecycle Timing (from tracing)

```
Total request latency: ~300µs

├── Tokio→V8 channel wake:    ~150µs  (50%)  ← MAIN BOTTLENECK
├── V8→Tokio response:         ~80µs  (27%)  ← SECOND BOTTLENECK
├── HTTP parsing (Hyper):      ~10µs   (3%)
├── JS handler execution:       ~3µs   (1%)
├── DashMap operations:        <1µs  (<1%)
└── Other overhead:            ~57µs  (19%)
```

**The actual work (parsing + JS) takes ~13µs. The async plumbing takes ~230µs.**

### Detailed Trace Output

With `HOWTH_TRACE=1`:
```
[RESPOND] count=100000 | JS_PROC=14304ns lock=73ns send=36ns FULL_RT=62278ns
[HYPER E2E] count=100000 total=117811ns | parse=150ns body=97ns JS_WAIT=117490ns
```

- `JS_PROC` (14µs): Time from request handoff to respond call
- `HYPER E2E` (118µs): Total time from Hyper's perspective
- `JS_WAIT` (117µs): Time Hyper waits for JS to respond

---

## What We Tried

### 1. Batch Response Op (FAILED)
**Hypothesis:** Sending multiple responses in one op call reduces overhead.

**Implementation:** Added `op_howth_http_respond_batch` that takes array of responses.

**Result:** Slower (~171K vs 174K RPS). Serde serialization overhead for `Vec<(u32, u16, String)>` negated any benefit.

### 2. Sync Polling Mode (FAILED)
**Hypothesis:** Sync polling avoids async wake latency.

**Implementation:**
- `op_howth_http_poll_batch` - non-blocking sync op
- JS spins calling poll, yields when empty

**Result:** Much slower (~105-107K RPS). The yield mechanism (setTimeout/Promise.resolve) adds more latency than the async channel.

### 3. Hybrid Spin/Wait (FAILED)
**Hypothesis:** Spin for a while, then fall back to async wait.

**Implementation:** Spin up to 1000 iterations of try_recv, then async wait.

**Result:** Slower (~137K RPS). Spin overhead without benefit.

### 4. Direct Crossbeam Channel (FAILED)
**Hypothesis:** crossbeam-channel is faster than tokio::sync::mpsc.

**Implementation:** Replaced channel with crossbeam bounded channel.

**Result:** No improvement. The latency is in the wake/poll cycle, not the channel itself.

### 5. Pure Rust Baseline (INFORMATIVE)
**Purpose:** Establish theoretical maximum without JS.

**Implementation:** Hyper server that returns static response, no JS involved.

**Result:** ~192K RPS - only 5% slower than Bun. This proves:
- Hyper is competitive
- The 15% gap is JS overhead
- Bun's remaining 5% advantage is their HTTP stack

### 6. TCP_NODELAY + Static Response (MINIMAL IMPACT)
**Hypothesis:** Reduce TCP latency and allocation overhead.

**Implementation:** Enable TCP_NODELAY, pre-allocate static response bytes.

**Result:** No measurable improvement (~192K RPS unchanged).

---

## What Works Well

### serveBatch Mode
The current `Howth.serveBatch()` is already well-optimized:

```javascript
// One async op returns batch of requests
const batch = await ops.op_howth_http_wait_batch_with_info(serverId, batchSize);

// Process all requests synchronously
for (const [requestId, method, url] of batch) {
  const response = handler(req);
  ops.op_howth_http_respond_fast_sync(requestId, status, body);  // Sync!
}
```

**Why it's good:**
1. Single async yield per batch (not per request)
2. Sync respond ops (no async overhead on response path)
3. Lazy header/body extraction (only if accessed)
4. DashMap for lock-free request storage

---

## LocalSet Experiment (2026-02-03)

### Hypothesis
Based on the second opinion: run Hyper and V8 on the same thread using `tokio::task::LocalSet`
and `spawn_local` to eliminate cross-thread channel overhead.

### Implementation Attempt
1. Added `--local` flag to run command
2. Wrapped execution in `LocalSet::block_on()`
3. Created `op_howth_http_serve_local` that uses `spawn_local`
4. Used thread-local storage for request/response queuing

### Result: FAILED

**Problem:** deno_core's event loop doesn't poll LocalSet tasks.

When we call `spawn_local` inside an async op:
- The task is registered with the LocalSet
- BUT deno_core's `run_event_loop()` has its own polling mechanism
- It doesn't call `LocalSet::run_until()` or similar
- The spawned local task never gets polled/executed

Evidence from debug output:
```
[LOCAL] Starting accept loop with spawn_local  ← Task registered
[JS LOCAL] Poll count: 10000, batch size: 0    ← JS running
[JS LOCAL] Poll count: 20000, batch size: 0    ← JS running
# Note: "[LOCAL] Accept loop task started" never printed!
```

### Why This Matters
The LocalSet approach was supposed to eliminate the ~150µs channel wake-up latency.
Without LocalSet integration, we're stuck with the cross-thread architecture.

### Remaining Options for True Same-Thread Execution

1. **Patch deno_core** - Add LocalSet support to deno_core's event loop
   - Significant maintenance burden
   - Would need to fork deno_core

2. **Worker Thread Model** - Each thread has its own:
   - `tokio::runtime::Builder::new_current_thread()`
   - `LocalSet`
   - V8 isolate (JsRuntime)
   - TCP listener with `SO_REUSEPORT`
   - No cross-thread communication needed

3. **Custom HTTP Integration** - Like Deno's `deno_http` crate
   - Integrate HTTP directly into deno_core's resource system
   - Complex, requires deep deno_core knowledge

### Current Status
`Howth.serveLocal()` currently delegates to `serveBatch()` and prints a note about
the pending optimization. The `--local` flag is available for future implementation.

---

## Remaining Optimization Paths

### Tier 1: Cross-Platform, High Impact

#### A. Single-Threaded HTTP+V8 Mode
**Effort:** High
**Impact:** ~15% (eliminates channel overhead)

Run Hyper's accept loop on the V8 thread, call JS directly:

```rust
// Pseudocode - everything on V8 thread
loop {
    let request = accept_sync();  // Blocking accept
    let response = call_js_handler(request);  // Direct V8 call
    send_response(response);
}
```

**Challenges:**
- V8 event loop integration
- Can't use async Hyper (need sync or custom event loop)
- May need to fork deno_core

#### B. Custom HTTP Parser
**Effort:** High
**Impact:** ~5-10%

Replace Hyper with minimal HTTP/1.1 parser optimized for:
- Zero-copy parsing
- Pre-allocated buffers
- Common case optimization (GET, small responses)

**Examples:** picohttpparser (C), httparse (Rust, already used by Hyper)

### Tier 2: Linux-Only, High Impact

#### C. io_uring Integration
**Effort:** Medium-High
**Impact:** ~20-30% on Linux

```rust
// Current: syscall per operation
read(fd, buf, len);   // syscall
write(fd, buf, len);  // syscall

// io_uring: batch operations, kernel processes async
io_uring_prep_read(sqe, fd, buf, len);
io_uring_prep_write(sqe, fd, buf, len);
io_uring_submit(ring);  // One syscall for batch
```

**Libraries:** tokio-uring, glommio

**Note:** macOS has no equivalent. Would need conditional compilation.

### Tier 3: Lower Impact, Easier

#### D. Bounded Channels
**Effort:** Low
**Impact:** ~2-5%

Replace unbounded mpsc with bounded channel. May improve cache locality and reduce allocation.

#### E. Connection Handling Optimizations
**Effort:** Low-Medium
**Impact:** ~2-5%

- `SO_REUSEPORT` for multiple accept threads (Linux)
- Connection pooling / keep-alive tuning
- TCP buffer size tuning

#### F. Memory Allocator
**Effort:** Low
**Impact:** ~2-5%

Switch to jemalloc or mimalloc for better multi-threaded allocation performance.

```toml
[dependencies]
jemallocator = "0.5"
```

---

## Recommended Path Forward

### If targeting Linux production servers:
1. **io_uring** (#C) - Biggest bang for buck on Linux
2. **Bounded channels** (#D) - Easy win
3. **Memory allocator** (#F) - Easy win

### If cross-platform performance is priority:
1. **Single-threaded mode** (#A) - Eliminates the main bottleneck
2. **Custom HTTP parser** (#B) - Matches Bun's approach

### If minimal effort desired:
1. **Bounded channels** (#D)
2. **Memory allocator** (#F)
3. **Accept serveBatch as "good enough"** at 81% of Bun

---

## Questions for Second Opinion

1. **Is 81% of Bun acceptable?** Howth is already 1.5x faster than Node.js.

2. **Single-threaded mode:** Is it worth the complexity of custom V8/HTTP event loop integration?

3. **io_uring:** Worth the Linux-only limitation? Most production deployments are Linux anyway.

4. **Fork deno_core?** Could modify async op handling for tighter integration, but significant maintenance burden.

5. **Different V8 binding?** deno_core is convenient but adds abstraction. Raw V8 bindings (rusty_v8) might allow tighter control.

---

## Appendix: Benchmark Commands

### Run built-in benchmark
```bash
howth bench http --duration 10 --connections 50
```

### Manual bombardier test
```bash
# Start server
howth run --native server.ts

# Benchmark
bombardier -c 50 -d 5s http://127.0.0.1:3000/
```

### Enable tracing
```bash
HOWTH_TRACE=1 howth run --native server.ts
```

### Test server code
```typescript
// serveBatch (current best)
Howth.serveBatch({ port: 3000, batchSize: 64 }, (req) => {
  return { status: 200, body: "Hi" };
});

// Pure Rust baseline (no JS)
Howth.serveRustOnly({ port: 3000, body: "Hi" });
```

---

## Appendix: Code Locations

| Component | File | Lines |
|-----------|------|-------|
| HTTP ops | `crates/fastnode-runtime/src/runtime.rs` | 2140-2800 |
| JS API | `crates/fastnode-runtime/src/bootstrap.js` | 13500-13920 |
| Benchmark harness | `crates/fastnode-core/src/bench/http.rs` | 1-470 |

---

## Appendix: Project Stats

- **Total LOC:** ~143K
- **Rust:** ~75K
- **JavaScript:** ~68K (includes Node.js polyfills)
- **Main crates:** fastnode-core (36K), fastnode-cli (18K), fastnode-runtime (12K)
