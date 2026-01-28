/**
 * Basic HTTP Server Example
 *
 * Demonstrates:
 * - Creating an HTTP server
 * - Routing based on URL
 * - JSON responses
 * - Query parameter parsing
 *
 * Run: howth run --native examples/http-server/server.js
 * Test: curl http://localhost:3000/api/hello?name=World
 */

const http = require('http');
const url = require('url');

const PORT = process.env.PORT || 3000;

// Simple router
const routes = {
  'GET /': (req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`
      <h1>Welcome to Howth HTTP Server</h1>
      <p>Try these endpoints:</p>
      <ul>
        <li><a href="/api/hello?name=World">/api/hello?name=World</a></li>
        <li><a href="/api/time">/api/time</a></li>
        <li><a href="/api/echo">/api/echo</a> (POST)</li>
        <li><a href="/health">/health</a></li>
      </ul>
    `);
  },

  'GET /api/hello': (req, res, query) => {
    const name = query.name || 'Anonymous';
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ message: `Hello, ${name}!` }));
  },

  'GET /api/time': (req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      timestamp: Date.now(),
      iso: new Date().toISOString(),
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone
    }));
  },

  'POST /api/echo': (req, res) => {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        received: body,
        length: body.length,
        headers: req.headers
      }));
    });
  },

  'GET /health': (req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      status: 'ok',
      uptime: process.uptime?.() || 0,
      memory: process.memoryUsage?.() || {},
      version: process.version || 'howth'
    }));
  }
};

const server = http.createServer((req, res) => {
  const parsedUrl = url.parse(req.url, true);
  const routeKey = `${req.method} ${parsedUrl.pathname}`;

  console.log(`${new Date().toISOString()} ${req.method} ${req.url}`);

  const handler = routes[routeKey];
  if (handler) {
    handler(req, res, parsedUrl.query);
  } else {
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'Not Found', path: parsedUrl.pathname }));
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`Server running at http://127.0.0.1:${PORT}/`);
  console.log('Press Ctrl+C to stop');
});
