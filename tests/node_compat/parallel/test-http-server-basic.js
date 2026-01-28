'use strict';
const assert = require('assert');
const http = require('http');

// Test basic HTTP server functionality
const PORT = 3456;

const server = http.createServer((req, res) => {
  console.log(`Received request: ${req.method} ${req.url}`);
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end('Hello World\n');
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`Server listening on port ${PORT}`);
  const addr = server.address();
  assert.strictEqual(addr.port, PORT);
  assert.strictEqual(addr.address, '127.0.0.1');
  console.log(`Server address: ${JSON.stringify(addr)}`);

  // Make a request to the server
  const req = http.get(`http://127.0.0.1:${PORT}/test`, (res) => {
    console.log(`Got response: ${res.statusCode}`);
    assert.strictEqual(res.statusCode, 200);

    let body = '';
    res.on('data', (chunk) => {
      body += chunk;
    });
    res.on('end', () => {
      console.log(`Response body: ${body.trim()}`);
      assert.strictEqual(body.trim(), 'Hello World');

      // Close the server
      server.close(() => {
        console.log('Server closed');
        console.log('All http server tests passed!');
        // Exit cleanly - the accept loop may still be waiting
        process.exit(0);
      });
    });
  });

  req.on('error', (err) => {
    console.error('Request error:', err);
    server.close();
    process.exit(1);
  });
});

server.on('error', (err) => {
  console.error('Server error:', err);
  process.exit(1);
});
