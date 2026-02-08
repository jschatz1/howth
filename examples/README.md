# Howth Examples

Example projects demonstrating Howth features.

## Examples

### [docker](./docker)
Run Howth in a Docker container. Shows how to use the official Docker image as a base.

### [http-server](./http-server)
Create HTTP servers with Node.js compatible APIs. Includes a basic server and a JSON API example.

### [typescript](./typescript)
Run TypeScript directly without compilation. Demonstrates type annotations, interfaces, and ES modules.

### [react-app](./react-app)
Minimal React application with Howth dev server. Features HMR and React Fast Refresh.

### [cli-tool](./cli-tool)
Command-line tool showing Node.js API compatibility: fs, path, process.argv, and more.

### [worker-atomics](./worker-atomics)
Demonstrates SharedArrayBuffer and Atomics operations between worker threads for lock-free concurrent programming.

### [parallel-compute](./parallel-compute)
Splits CPU-intensive work across multiple worker threads with shared result aggregation using SharedArrayBuffer.

### [real-time-game](./real-time-game)
Simple game loop demonstrating physics simulation in a worker thread while the main thread handles rendering.

### [data-pipeline](./data-pipeline)
Stream processing with a worker pool. Data flows through multiple processing stages handled by dedicated workers.

### [sass-app](./sass-app)
Sass/SCSS preprocessing demo showing variables, nesting, mixins, functions, and loops with Howth's built-in grass compiler.

### [markdown-api](./markdown-api)
Built-in CommonMark/GFM Markdown parser API (similar to Bun.markdown). Demonstrates tables, strikethrough, task lists, heading IDs, and smart punctuation.

### [cookies](./cookies)
HTTP Cookie APIs (similar to Bun.Cookie and Bun.CookieMap). Demonstrates parsing, creating, serializing cookies and generating Set-Cookie headers.

## Running Examples

Each example has its own README with specific instructions. Generally:

```bash
cd examples/<name>
howth run <entry-file>
```

For the React app:

```bash
cd examples/react-app
howth install
howth dev src/main.tsx
```
