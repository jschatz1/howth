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
| `http` | Server, client, routing |
| `fs` | Read/write files, directories |
| `path` | Path manipulation |
| `url` | URL parsing |
| `process` | argv, env, cwd, exit |
| `events` | EventEmitter (via http) |
| `stream` | Readable/Writable streams |
| `buffer` | Buffer operations |
| `querystring` | Query parsing |

## Adding New Examples

1. Create a new directory under `examples/`
2. Add your main script (e.g., `index.js` or `server.js`)
3. Add a test case to `run-all.js`
4. Document it in this README
