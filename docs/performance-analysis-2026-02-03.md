# Howth HTTP Performance Analysis

**Date:** 2026-02-03 (Updated)
**Benchmark:** `howth bench http` (built-in, HTTP keep-alive)
**Current Performance:** ~203K RPS (89% of Bun's ~228K RPS)

## Latest Results (howth bench http)

Built-in benchmark with HTTP keep-alive at 50 connections:

| Runtime | RPS | Latency | Relative |
|---------|-----|---------|----------|
| **Bun** | ~228K | ~218µs | 100% |
| **Howth (serveBatch)** | ~203K | ~245µs | 89% |
| **Node.js** | ~111K | ~449µs | 49% |

### Connection Scaling

| Connections | Bun | Howth | Howth % |
|-------------|-----|-------|---------|
| 50 | 228K | 203K | 89% |
| 100 | 226K | 186K | 82% |

Bun maintains throughput at higher concurrency while Howth drops ~8%.

## Timing Breakdown (Steady-State)

| Metric | Time | % of E2E | Description |
|--------|------|----------|-------------|
| **CHANNEL_LAT** | **~220µs** | **56%** | Time from Hyper's `tx.send()` to V8's `recv()` completing |
| **Response Latency** | **~160µs** | **40%** | Time from V8's `response_tx.send()` to Hyper's `response_rx.await` completing |
| JS_PROC | ~3.2µs | 0.8% | Actual JS handler execution |
| Hyper parse + body | ~280ns | 0.07% | HTTP parsing |
| DashMap ops | ~160ns | 0.04% | Lock-free map operations |
| **Total E2E** | **~390µs** | 100% | Full request-response cycle |

## Detailed Trace Output

```
[WAIT_INFO] count=700000 | dashmap=45ns mutex=648ns recv=6512ns CHANNEL_LAT=225008ns extract=40ns insert=63ns
[RESPOND] count=700000 | JS_PROC=3252ns lock=60ns send=41ns FULL_RT=228460ns
[JS TRACE] count=700000 | waitOp=0.010ms createReq=0.000ms handler=0.001ms respondOp=0.001ms
[HYPER E2E] count=700000 total=397159ns | parse=162ns body=124ns JS_WAIT=396788ns
```

## Request Flow Diagram

```
TCP Request Arrives
        │
        ▼
┌───────────────────┐
│   Hyper Parse     │  ~160ns (parse) + ~120ns (body)
│   (Rust/Tokio)    │
└───────────────────┘
        │
        ▼ tx.send() ─────────────────────────────────────┐
        │                                                │
        │  ◄────────── 220µs CHANNEL_LAT ──────────────► │
        │                                                │
        │         (Tokio wake → V8 poll → op dispatch)   │
        │                                                │
┌───────────────────┐                                    │
│  V8 recv()        │  recv=6.5µs (actual recv time)     │
│  (deno_core op)   │                                    │
└───────────────────┘                                    │
        │                                                │
        ▼                                                │
┌───────────────────┐                                    │
│  JS Handler       │  ~3.2µs (JS_PROC)                  │
│  (user code)      │                                    │
└───────────────────┘                                    │
        │                                                │
        ▼ response_tx.send() ────────────────────────────┤
        │                                                │
        │  ◄────────── ~160µs Response Latency ────────► │
        │                                                │
        │         (Tokio wake → Hyper resumes)           │
        │                                                │
┌───────────────────┐                                    │
│  Hyper Response   │  lock=60ns, send=40ns              │
│  (Rust/Tokio)     │                                    │
└───────────────────┘
        │
        ▼
TCP Response Sent
```

## Key Finding

**96% of latency is async channel/wake time, not actual processing.**

The JS handler executes in ~3.2µs, but the total round-trip is ~390µs due to:
1. Tokio → V8 event loop wake latency (~220µs)
2. V8 → Tokio response delivery latency (~160µs)

## Architecture Components

### Rust Side (runtime.rs)
- `op_howth_http_serve_fast()` - Spawns Hyper HTTP server
- `op_howth_http_wait_with_info()` - Async op that waits for requests
- `op_howth_http_respond_fast()` - Async op that sends responses
- Uses `tokio::sync::mpsc::unbounded_channel` for request delivery
- Uses `tokio::sync::oneshot::channel` for response delivery
- Uses `dashmap::DashMap` for lock-free pending request storage

### JavaScript Side (bootstrap.js)
- Fire-and-forget handler pattern (accept loop doesn't await handler)
- Lazy request data fetching (headers/body only loaded if accessed)
- Combined `wait_with_info` op reduces 3 calls to 1

## Tracing

Enable detailed tracing with:
```bash
HOWTH_TRACE=1 howth run --native server.ts
```

## Comparison to Bun

| Runtime | RPS | Latency | Gap |
|---------|-----|---------|-----|
| Bun | ~228K | ~218µs | baseline |
| Howth (serveBatch) | ~203K | ~245µs | 11% slower |
| Node.js | ~111K | ~449µs | 51% slower |

**Key findings:**
- Howth is **89% of Bun's speed** at 50 connections
- Howth is **1.83x faster than Node.js**
- At higher concurrency (100+ connections), Howth's relative performance drops to ~82% of Bun
- The gap is due to async event loop integration overhead between Tokio and V8

## Optimizations Implemented

1. ✅ **Batch processing** - `serveBatch()` processes multiple requests per event loop tick (~6% improvement)
2. ✅ **Synchronous respond ops** - `op_howth_http_respond_fast_sync` uses sync path
3. ✅ **Lazy extraction** - Headers/body only fetched when accessed
4. ✅ **DashMap** - Lock-free concurrent access for pending requests

## Remaining Optimization Candidates

1. **Direct V8 integration** - Bypass deno_core async op layer entirely
2. **Custom event loop** - Tighter Tokio/V8 integration with direct wake
3. **io_uring integration** - Kernel-level event batching (Linux only)
4. **Shared memory ring buffer** - Zero-copy request/response passing
