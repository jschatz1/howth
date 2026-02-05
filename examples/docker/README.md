# Docker Example

Run Howth in a Docker container.

## Quick Start

```bash
docker build -t my-app .
docker run --rm my-app
```

## Files

- `Dockerfile` - Multi-stage build using howth as base
- `hello.js` - Simple script that prints system info

## Using as a Base Image

```dockerfile
FROM ghcr.io/jschatz1/howth:latest
WORKDIR /app
COPY . .
CMD ["run", "index.js"]
```

## Volume Mounting

Run scripts from your host machine:

```bash
docker run -v $(pwd):/app ghcr.io/jschatz1/howth run script.js
```
