'use strict';
const assert = require('assert');
const http = require('http');

// Test module exports
assert.strictEqual(typeof http.createServer, 'function');
assert.strictEqual(typeof http.request, 'function');
assert.strictEqual(typeof http.get, 'function');
assert.ok(http.Agent);
assert.ok(http.Server);
assert.ok(http.IncomingMessage);
assert.ok(http.ServerResponse);
assert.ok(http.ClientRequest);
assert.ok(http.globalAgent);

// Test STATUS_CODES
assert.strictEqual(http.STATUS_CODES[200], 'OK');
assert.strictEqual(http.STATUS_CODES[404], 'Not Found');
assert.strictEqual(http.STATUS_CODES[500], 'Internal Server Error');

// Test METHODS
assert.ok(Array.isArray(http.METHODS));
assert.ok(http.METHODS.includes('GET'));
assert.ok(http.METHODS.includes('POST'));
assert.ok(http.METHODS.includes('PUT'));
assert.ok(http.METHODS.includes('DELETE'));

// Test maxHeaderSize
assert.strictEqual(typeof http.maxHeaderSize, 'number');
assert.ok(http.maxHeaderSize > 0);

// Test Agent
const agent = new http.Agent({ keepAlive: true, maxSockets: 10 });
assert.strictEqual(agent.keepAlive, true);
assert.strictEqual(agent.maxSockets, 10);
assert.strictEqual(typeof agent.destroy, 'function');
agent.destroy();

// Test globalAgent
assert.ok(http.globalAgent instanceof http.Agent);
assert.strictEqual(http.globalAgent.keepAlive, true);

// Test Server creation
const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end('Hello World');
});
assert.ok(server instanceof http.Server);
assert.strictEqual(typeof server.listen, 'function');
assert.strictEqual(typeof server.close, 'function');
assert.strictEqual(typeof server.address, 'function');
assert.strictEqual(server.listening, false);

// Test ServerResponse
const mockReq = {};
const res = new http.ServerResponse(mockReq);
assert.strictEqual(res.statusCode, 200);
res.setHeader('X-Custom', 'value');
assert.strictEqual(res.getHeader('x-custom'), 'value');
assert.ok(res.hasHeader('X-Custom'));
res.removeHeader('X-Custom');
assert.ok(!res.hasHeader('X-Custom'));
res.writeHead(201, 'Created', { 'Content-Type': 'application/json' });
assert.strictEqual(res.statusCode, 201);
assert.strictEqual(res.statusMessage, 'Created');

// Test IncomingMessage
const incoming = new http.IncomingMessage(null);
assert.strictEqual(incoming.httpVersion, '1.1');
assert.strictEqual(incoming.complete, false);
assert.deepStrictEqual(incoming.headers, {});

// Test ClientRequest creation
const req = http.request({
  hostname: 'example.com',
  port: 80,
  path: '/test',
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
});
assert.ok(req instanceof http.ClientRequest);
assert.strictEqual(req.method, 'POST');
assert.strictEqual(req.path, '/test');
req.setHeader('X-Test', 'value');
assert.strictEqual(req.getHeader('x-test'), 'value');
req.removeHeader('X-Test');
assert.strictEqual(req.getHeader('x-test'), undefined);

// Test validateHeaderName
assert.throws(() => http.validateHeaderName(''), TypeError);
assert.throws(() => http.validateHeaderName(123), TypeError);

// Test validateHeaderValue
assert.throws(() => http.validateHeaderValue('name', undefined), TypeError);

// Abort the request (we're not actually making it)
req.abort();
assert.strictEqual(req.aborted, true);

console.log('All http tests passed!');
