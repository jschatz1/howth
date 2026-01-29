# Node.js API Compatibility

Status of Node.js built-in module support in howth.

**Legend:** Full = all common APIs work | Partial = key APIs work, some missing | Stub = exists for import compat, limited function | Missing = not present

## Core Modules

| Module | Status | Notes |
|---|---|---|
| `assert` | Full | `strict` mode, deep equality, rejects/doesNotReject |
| `async_hooks` | Partial | `AsyncLocalStorage`, `AsyncResource` |
| `buffer` | Full | All encodings, read/write methods, `Buffer.alloc/from/concat` |
| `child_process` | Full | `exec`, `spawn`, `execFile`, `fork` (file-based IPC) |
| `cluster` | Missing | |
| `console` | Full | log/warn/error/debug, time/timeEnd, table, group |
| `constants` | Full | Legacy constants (deprecated) |
| `crypto` | Partial | Hash, HMAC, random, `createCipheriv`/`createDecipheriv` (AES-CBC/CTR/GCM), `sign`/`verify` (RSA), `publicEncrypt`/`privateDecrypt` (RSA-OAEP), `generateKeyPairSync` (RSA). **Missing:** `scrypt`, `createDiffieHellman`, X509, `createECDH` |
| `dgram` | Missing | UDP sockets |
| `diagnostics_channel` | Stub | Channel class exists, no real functionality |
| `dns` | Stub | Returns placeholder 127.0.0.1 results |
| `domain` | Full | Deprecated module, implemented for compat |
| `events` | Full | EventEmitter with full API |
| `fs` | Full | Sync, callback, and `fs.promises`. `createReadStream`/`createWriteStream`, `watch` |
| `http` | Full | Native Hyper/Tokio server, client with Agent |
| `http2` | Missing | |
| `https` | Partial | `request`/`get` work, `createServer` is basic stub |
| `inspector` | Stub | Session class, no real debugging |
| `module` | Full | `createRequire`, `builtinModules`, CommonJS loader |
| `net` | Partial | Socket, Server, `connect`, `createServer`. Missing advanced options |
| `os` | Full | `cpus`, `homedir`, `networkInterfaces`, `constants`, etc. |
| `path` | Full | POSIX and Win32 variants |
| `perf_hooks` | Partial | `performance.now/mark/measure`, `PerformanceObserver` |
| `process` | Full | Global. env, argv, cwd, exit, stdin/stdout/stderr, nextTick, hrtime |
| `punycode` | Full | Deprecated module, implemented for compat |
| `querystring` | Full | parse, stringify, escape, unescape |
| `readline` | Missing | |
| `repl` | Missing | |
| `stream` | Full | Readable, Writable, Duplex, Transform, PassThrough, pipeline, finished, promises |
| `stream/web` | Full | ReadableStream, WritableStream, TransformStream |
| `string_decoder` | Full | Multi-byte character handling |
| `timers` | Full | setTimeout/setInterval/setImmediate + `timers/promises` |
| `tls` | Stub | TLSSocket exists but no real TLS negotiation |
| `trace_events` | Missing | |
| `tty` | Partial | `isatty`, ReadStream, WriteStream |
| `url` | Full | Legacy `url.parse` + WHATWG `URL`/`URLSearchParams`, `pathToFileURL`/`fileURLToPath` |
| `util` | Full | format, inspect, promisify, callbackify, types.*, TextEncoder/TextDecoder |
| `v8` | Partial | `getHeapStatistics`, `serialize`/`deserialize` |
| `vm` | Partial | Script, createContext, runInContext/NewContext/ThisContext, compileFunction |
| `wasi` | Missing | |
| `worker_threads` | Partial | Worker, MessageChannel/MessagePort, isMainThread. Missing `SharedArrayBuffer` transfer |
| `zlib` | Full | gzip/gunzip/deflate/inflate sync + async + streaming |

## Web APIs (Globals)

| API | Status | Notes |
|---|---|---|
| `fetch` | Full | Via reqwest backend |
| `Request` / `Response` / `Headers` | Full | |
| `URL` / `URLSearchParams` | Full | |
| `TextEncoder` / `TextDecoder` | Full | |
| `AbortController` / `AbortSignal` | Full | |
| `Blob` / `File` / `FormData` | Full | |
| `crypto.subtle` | Partial | `digest` works. Missing `encrypt`/`decrypt`/`sign`/`verify`/`generateKey`/`importKey`/`exportKey` |
| `structuredClone` | Partial | JSON-based, no transferable support |
| `Event` / `EventTarget` | Full | |
| `ReadableStream` / `WritableStream` / `TransformStream` | Full | |
| `performance.now()` | Full | |
| `atob` / `btoa` | Full | |
| `queueMicrotask` | Full | |
| `setTimeout` / `setInterval` | Full | |

## Known Gaps Causing Real-World Failures

| Gap | Affected Packages | Priority |
|---|---|---|
| ~~`crypto.createCipheriv` / `createDecipheriv`~~ | ~~cookie-signature, express-session, passport~~ | ~~High~~ **Done** |
| ~~`crypto.sign` / `verify`~~ | ~~jsonwebtoken, google-auth-library~~ | ~~High~~ **Done** |
| ~~`crypto.publicEncrypt` / `privateDecrypt`~~ | ~~google-auth-library, node-jose~~ | ~~High~~ **Done** |
| `dns.resolve` (real resolution) | got, axios (for SRV lookups) | Medium |
| `tls.connect` (real TLS) | database drivers, custom HTTPS | Medium |
| `net` advanced options | database drivers, socket.io | Medium |
| `http2` | gRPC, modern HTTP clients | Medium |
| `cluster` | PM2, production deployments | Low |
| `readline` | interactive CLI tools | Low |
| `dgram` | DNS servers, game servers | Low |

## Totals

- **Full:** 22 modules
- **Partial:** 9 modules
- **Stub:** 3 modules
- **Missing:** 6 modules
- **Overall:** 34 / 40 modules present (85%)
