# Howth Examples

Real-world example applications that demonstrate howth's capabilities and serve as end-to-end tests.

## Running Examples

```bash
# Build howth first
cargo build --features native-runtime -p fastnode-cli

# Run individual examples
./target/debug/howth run --native examples/http-server/server.js
./target/debug/howth run --native examples/cli-tool/cli.js --help
./target/debug/howth run --native examples/todo-api/server.js

# Run all examples as tests
node examples/run-all.js
```

## Examples

### 1. HTTP Server (`http-server/`)
Basic HTTP server with routing, JSON responses, and query parameter handling.

```bash
howth run --native examples/http-server/server.js
# Then visit http://localhost:3000
```

**Features demonstrated:**
- `http.createServer()`
- URL routing
- JSON responses
- Query parameter parsing
- Health check endpoint

---

### 2. CLI Tool (`cli-tool/`)
Command-line tool with argument parsing, file operations, and colored output.

```bash
howth run --native examples/cli-tool/cli.js --help
howth run --native examples/cli-tool/cli.js count ./src
howth run --native examples/cli-tool/cli.js search "TODO" .
howth run --native examples/cli-tool/cli.js tree ./examples
```

**Features demonstrated:**
- `process.argv` parsing
- Recursive file system traversal
- ANSI color codes
- Exit codes

---

### 3. TODO REST API (`todo-api/`)
Full CRUD REST API with in-memory data store.

```bash
howth run --native examples/todo-api/server.js

# Test endpoints
curl http://localhost:3001/todos
curl -X POST -H "Content-Type: application/json" \
     -d '{"title":"Learn howth"}' http://localhost:3001/todos
curl -X PUT -H "Content-Type: application/json" \
     -d '{"completed":true}' http://localhost:3001/todos/1
curl -X DELETE http://localhost:3001/todos/1
```

**Features demonstrated:**
- RESTful API design
- JSON request/response handling
- Path parameter routing
- CORS headers
- CRUD operations

---

### 4. Static File Server (`static-server/`)
Serves static files with MIME type detection and directory listing.

```bash
howth run --native examples/static-server/server.js ./public
```

**Features demonstrated:**
- Static file serving
- MIME type detection
- Directory listing
- Path traversal prevention
- Cache headers

---

### 5. File Processor (`file-processor/`)
Codebase analysis and transformation tool.

```bash
howth run --native examples/file-processor/processor.js analyze ./src
howth run --native examples/file-processor/processor.js todos .
howth run --native examples/file-processor/processor.js minify ./config
howth run --native examples/file-processor/processor.js unused ./src
```

**Features demonstrated:**
- Recursive directory traversal
- File content analysis
- Pattern matching (TODO extraction)
- Code transformations

---

### 6. API Client (`api-client/`)
HTTP client consuming external JSON API.

```bash
howth run --native examples/api-client/client.js
```

**Features demonstrated:**
- HTTP client (`http.get`)
- Async/await patterns
- JSON API consumption
- Error handling

---

### 7. Environment Loader (`env-loader/`)
Configuration and environment variable management.

```bash
howth run --native examples/env-loader/index.js
```

**Features demonstrated:**
- `.env` file parsing
- JSON config loading
- Environment interpolation
- Config validation
- Secret masking

---

### 8. JSON Database (`json-db/`)
File-based JSON database with MongoDB-like query syntax.

```bash
howth run --native examples/json-db/db.js
```

**Features demonstrated:**
- JSON file persistence
- CRUD operations (insert, find, update, delete)
- Query operators ($gt, $lt, $in, $regex, etc.)
- Aggregation pipeline ($match, $group, $sort, $limit)
- Indexes for faster lookups

---

### 9. Test Runner (`test-runner/`)
Minimal test framework with describe/it syntax (like Mocha/Jest).

```bash
howth run --native examples/test-runner/runner.js
```

**Features demonstrated:**
- describe/it DSL
- Assertion library (equal, deepEqual, throws, etc.)
- Async test support
- beforeAll/afterAll/beforeEach/afterEach hooks
- Test filtering with .skip and .only
- Colored output with timing

---

### 10. Proxy Server (`proxy-server/`)
HTTP proxy server with path rewriting and request logging.

```bash
howth run --native examples/proxy-server/proxy.js

# Test endpoints
curl http://localhost:3080/api/users    # Proxies to jsonplaceholder
curl http://localhost:3080/httpbin/get  # Proxies to httpbin.org
```

**Features demonstrated:**
- HTTP request forwarding
- Path rewriting rules
- Header modification
- Request logging with colors
- Error handling (502 Bad Gateway)

---

### 11. Markdown Processor (`markdown/`)
Markdown to HTML converter with frontmatter and TOC generation.

```bash
howth run --native examples/markdown/md.js
howth run --native examples/markdown/md.js -- README.md
```

**Features demonstrated:**
- Markdown parsing (headers, lists, code blocks, tables)
- YAML frontmatter extraction
- Table of contents generation
- HTML output with styling
- File I/O

---

### 12. LRU Cache (`lru-cache/`)
Least Recently Used cache implementation with TTL support.

```bash
howth run --native examples/lru-cache/cache.js
```

**Features demonstrated:**
- O(1) get/set operations
- Doubly-linked list for LRU ordering
- Time-to-live (TTL) expiration
- Cache statistics (hit rate, evictions)
- Memoization helper function

---

### 13. Task Scheduler (`task-scheduler/`)
Cron-like task scheduler with retry logic and dependencies.

```bash
howth run --native examples/task-scheduler/scheduler.js
```

**Features demonstrated:**
- Interval-based task scheduling
- Cron expression parsing
- Task priorities
- Retry logic with configurable delay
- Task dependencies
- Status monitoring and history

---

### 14. Dev Server (`dev-server/`)
Vite-like development server with live reload.

```bash
howth run --native examples/dev-server/server.js
# Then visit http://localhost:3000
```

**Features demonstrated:**
- Static file serving with ES modules
- Live reload via Server-Sent Events (SSE)
- File watching
- SPA routing fallback
- API endpoints
- Auto-generated example files

---

### 15. SSR App (`ssr-app/`)
Server-Side Rendering framework example.

```bash
howth run --native examples/ssr-app/server.js
# Then visit http://localhost:3000
```

**Features demonstrated:**
- Component-based architecture
- Template rendering
- State hydration to client
- Route handling
- API routes
- HTML escaping

---

### 16. Static Site Generator (`static-site/`)
Build static HTML sites from markdown.

```bash
howth run --native examples/static-site/build.js
# Output in examples/static-site/dist/
```

**Features demonstrated:**
- Markdown to HTML processing
- Frontmatter extraction
- Template layouts
- Static asset copying
- Sitemap generation
- Recursive directory processing

---

## Test Runner

Run all examples as a test suite:

```bash
node examples/run-all.js
```

This validates that all examples work correctly with howth's native runtime.

## Node.js Modules Used

These examples exercise the following Node.js APIs:

| Module | Usage |
|--------|-------|
| `http` | Server, client, routing, proxy |
| `fs` | Read/write files, directories, JSON |
| `path` | Path manipulation |
| `url` | URL parsing |
| `process` | argv, env, cwd, exit |
| `events` | EventEmitter (via http) |
| `stream` | Readable/Writable streams |
| `buffer` | Buffer operations |
| `querystring` | Query parsing |
| `timers` | setTimeout, setInterval, clearTimeout |
| `console` | Logging with colors |

## Adding New Examples

1. Create a new directory under `examples/`
2. Add your main script (e.g., `index.js` or `server.js`)
3. Add a test case to `run-all.js`
4. Document it in this README
