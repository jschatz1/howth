/**
 * SvelteKit Example
 *
 * Demonstrates a built SvelteKit app served via Express on the Howth runtime:
 * - SvelteKit with adapter-node
 * - Express wrapper with logging middleware
 * - Server-side rendering with Svelte components
 *
 * Build: cd examples/sveltekit-app && pnpm install && pnpm build
 * Run:   howth run server.js
 * Then visit: http://localhost:3000
 */

import express from 'express';
import { handler } from './build/handler.js';

const app = express();
const PORT = process.env.PORT || 3000;

// Request logging
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => {
    const ms = Date.now() - start;
    console.log(`${req.method} ${req.url} ${res.statusCode} ${ms}ms`);
  });
  next();
});

// Delegate all requests to the SvelteKit handler
app.use(handler);

app.listen(PORT, () => {
  console.log(`SvelteKit server running at http://localhost:${PORT}`);
  console.log('Routes:');
  console.log('  GET  /       - Home page');
  console.log('  GET  /about  - About page');
});
