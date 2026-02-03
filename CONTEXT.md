# Howth V8 Test Runner — Calendar.dev Compatibility Work

## Goal

Make `howth test --cwd /Users/jacobschatz/projects/calendar.dev/packages/api` pass as many of the 586 tests (46 test files) as possible. Only howth code is modified — calendar.dev is never touched.

## Test Command

```bash
howth test --cwd /Users/jacobschatz/projects/calendar.dev/packages/api
```

Database is in Docker: `docker exec calendardev-db-1 psql -U postgres`

## Current State

- **Best result so far**: 377 passed, 205 failed, 4 skipped in ~540s (before sync listen change)
- **Latest run** (with nock fix + native timer fix): 146 passed, 439 failed — but this was because one `_originalSetTimeout` reference was missed, causing cascading failures. That's now fixed but hasn't been tested yet.
- Worker timeout is set to 600s

## Key Files

- `crates/fastnode-runtime/src/bootstrap.js` — All JS runtime shims (Node.js API compat)
- `crates/fastnode-runtime/src/runtime.rs` — Rust ops (fetch, HTTP server, TCP, etc.)
- `crates/fastnode-daemon/src/v8_test_worker.rs` — V8 test worker (timeout config)

## Build & Deploy Cycle

```bash
cargo install --path crates/fastnode-cli
pkill -f 'howth daemon' || true; sleep 1
howth daemon &  # run in background
howth test --cwd /Users/jacobschatz/projects/calendar.dev/packages/api
```

## What Was Fixed (This Session)

### 1. Nock HTTP Interception (CRITICAL FIX)
**Problem**: `httpRequest()` and `httpGet()` used closure-captured `ClientRequest` class. When nock overrides `httpModule.ClientRequest`, our functions still used the original, completely bypassing nock. This caused real HTTP requests to Google metadata (169.254.169.254), classification APIs (127.0.0.1:5000), etc.

**Fix**: Changed `httpRequest()` to use `httpModule.ClientRequest` (dynamic lookup) and `httpGet()` to use `httpModule.request`. Same for HTTPS module. Also added `ClientRequest`, `IncomingMessage`, `OutgoingMessage` exports to `httpsModule`.

**Location**: bootstrap.js ~line 11994-12005 (httpRequest/httpGet), ~line 12132-12149 (httpsRequest/httpsGet), ~line 12162 (httpsModule exports)

### 2. Native Timer Preservation for Test Runner
**Problem**: sinon's `FakeTimers` replaces `setTimeout`/`clearTimeout`. The test runner's per-test 30s timeout used `setTimeout`, so when sinon faked timers, tests hung forever (the timeout never fired). The "scheduler general" suite caused the entire run to hang.

**Fix**: Saved native timers to `globalThis.__nativeSetTimeout` and `globalThis.__nativeClearTimeout` at bootstrap time. Updated ALL test runner timeouts (before hook, beforeEach, callback-style test, promise-style test) to use `globalThis.__nativeSetTimeout` instead of `setTimeout`.

**Location**: bootstrap.js ~line 9287-9292 (storage), ~line 13253, 13289, 13296, 13303, 13305 (usage in test runner)

### 3. HTTP Accept Timeout (Server Cleanup)
**Problem**: `op_howth_http_accept` blocked forever on `listener.accept().await`. When `server.close()` was called, the accept loop stayed stuck because it had a cloned Arc of the listener. Each leaked server's accept loop hung indefinitely.

**Fix**: Added `tokio::time::timeout(200ms)` to accept. Returns `Ok(None)` on timeout, letting the JS loop check `this.listening` and exit. Also changed "Server not found" from an error to `Ok(None)` so the loop exits cleanly when the server is removed from the map.

**Location**: runtime.rs `op_howth_http_accept` (~line 1779-1792)

### 4. Sync HTTP Listen
**Problem**: `op_howth_http_listen` was async — `server.address()` returned null before the op resolved, causing 72 supertest failures.

**Fix**: Changed to sync using `std::net::TcpListener::bind()` → `set_nonblocking(true)` → `tokio::net::TcpListener::from_std()`. Server address is available immediately after `listen()`.

**Location**: runtime.rs `op_howth_http_listen` (~line 1728-1767)

### 5. ClientRequest Methods for Nock
**Problem**: Nock calls `req.getHeaders()`, `req.hasHeader()`, etc. which didn't exist on our ClientRequest.

**Fix**: Added `getHeaders()`, `getHeaderNames()`, `hasHeader()`, `getRawHeaderNames()`, `flushHeaders()` to ClientRequest.

**Location**: bootstrap.js ClientRequest class (~line 11621-11645)

### 6. Reqwest Connect Timeout
**Problem**: `op_howth_fetch` used `reqwest::blocking::Client::new()` with no timeout. Requests to unreachable services (metadata, external APIs) hung forever.

**Fix**: Added `.connect_timeout(5s)` and `.timeout(30s)` to reqwest client builder.

**Location**: runtime.rs `op_howth_fetch` (~line 2254-2258)

## What Was Fixed (Previous Sessions)

- **readline module**: Full implementation with async iterator support (for `populateCallSigns()`)
- **Stream class Proxy**: ES5 `.call()` compat for superagent's `Stream.call(this)` pattern
- **OutgoingMessage class**: Needed by nock for `OverriddenClientRequest`
- **ClientRequest.setHeader guard**: `if (!this.headers) this.headers = {};` for nock
- **Fetch body string conversion**: Convert Buffer/Uint8Array to string before `op_howth_fetch`
- **Per-test 30s timeout**: `Promise.race` with timeout for each test
- **Debug logging**: `[howth] running test:` and `[howth] running before hook` in test runner

## Known Remaining Issues

### Error Categories (from 205-failure run)
1. **`org is not defined`** (~25 failures) — Test setup issue, `org` variable not available in test scope
2. **`Cannot find module token-counter`** (~13 failures) — Missing module shim
3. **`calDevAdminUser is not defined`** (~10 failures) — Test setup issue
4. **`Right-hand side of 'instanceof' is not an object`** (~11 failures) — Some class/prototype issue
5. **Tests timing out at 30s** (~14 failures) — Tests that genuinely take >30s or have async issues
6. **Supertest HTTP round-trip** — Servers bind and accept connections, but the full request→handler→response cycle may not complete properly for all test patterns

### Diagnostic Logging Still Present
There's `console.error` debug logging in:
- `_doFetch` (logs `[howth:http] ClientRequest._doFetch: METHOD URL`)
- `_acceptLoop` (logs when started and when request received)
- `listen()` (logs server listening)
- `__howth_run_tests()` (logs each test and hook)

Remove these before final commit.

## Architecture Notes

- All 46 test files run in a **single V8 runtime** (not separate processes)
- The V8 runtime uses deno_core with a `current_thread` tokio runtime
- `op_howth_fetch` spawns a `std::thread` with `reqwest::blocking::Client` (separate from tokio)
- HTTP servers use `std::net::TcpListener` (sync bind) → `tokio::net::TcpListener` (async accept)
- Server state stored in `lazy_static! HTTP_SERVERS` map, connections in `PENDING_CONNECTIONS`
- Test runner uses mocha-compatible API (`describe`/`it`/`before`/`after`/`beforeEach`/`afterEach`)

## CLAUDE.md Rule
Never add co-author or signature lines in git commits.
