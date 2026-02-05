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
