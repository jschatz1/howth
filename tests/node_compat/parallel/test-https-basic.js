'use strict';
const assert = require('assert');
const https = require('https');
const http = require('http');

// Test module exports
assert.strictEqual(typeof https.createServer, 'function');
assert.strictEqual(typeof https.request, 'function');
assert.strictEqual(typeof https.get, 'function');
assert.ok(https.Agent);
assert.ok(https.Server);
assert.ok(https.globalAgent);

// Test that https.Agent is the same as http.Agent
assert.strictEqual(https.Agent, http.Agent);

// Test globalAgent is separate from http
assert.ok(https.globalAgent instanceof https.Agent);

// Test Server is http.Server (for now)
assert.strictEqual(https.Server, http.Server);

// Test https.request creates ClientRequest
const req = https.request({
  hostname: 'example.com',
  port: 443,
  path: '/secure',
  method: 'GET',
});
assert.ok(req instanceof http.ClientRequest);
assert.strictEqual(req.protocol, 'https:');
assert.strictEqual(req.port, 443);
req.abort();

// Test https.get
const getReq = https.get('https://example.com/test');
assert.ok(getReq instanceof http.ClientRequest);
assert.strictEqual(getReq.method, 'GET');
getReq.abort();

console.log('All https tests passed!');
