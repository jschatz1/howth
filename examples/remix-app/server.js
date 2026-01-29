/**
 * Remix Example (Express Adapter)
 *
 * Demonstrates Remix running on the Howth runtime via Express:
 * - Server-side rendering with React
 * - Remix loaders and actions
 * - File-based routing
 *
 * Build: cd examples/remix-app && pnpm install && pnpm build
 * Run:   howth run server.js
 * Then visit: http://localhost:3000
 */

const express = require("express");
const path = require("path");
const { createRequestHandler } = require("@remix-run/express");

const app = express();
const PORT = process.env.PORT || 3000;

// Request logging
app.use((req, res, next) => {
  const start = Date.now();
  res.on("finish", () => {
    const ms = Date.now() - start;
    console.log(`${req.method} ${req.url} ${res.statusCode} ${ms}ms`);
  });
  next();
});

// Serve static build assets
app.use(
  "/build",
  express.static(path.join(__dirname, "public", "build"), {
    immutable: true,
    maxAge: "1y",
  })
);

// Serve public static files
app.use(express.static(path.join(__dirname, "public"), { maxAge: "1h" }));

// Remix request handler
app.all(
  "*",
  createRequestHandler({
    build: require("./build"),
    mode: process.env.NODE_ENV || "production",
  })
);

app.listen(PORT, () => {
  console.log(`Remix server running at http://localhost:${PORT}`);
  console.log("Routes:");
  console.log("  GET  /       - Home page (todo list)");
  console.log("  GET  /about  - About page (runtime info)");
});
