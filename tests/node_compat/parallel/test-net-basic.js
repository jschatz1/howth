'use strict';
const assert = require('assert');
const net = require('net');

// Test module exports
assert.strictEqual(typeof net.createServer, 'function');
assert.strictEqual(typeof net.connect, 'function');
assert.strictEqual(typeof net.createConnection, 'function');
assert.ok(net.Socket);
assert.ok(net.Server);

// Test isIP functions
assert.strictEqual(net.isIP('127.0.0.1'), 4);
assert.strictEqual(net.isIP('192.168.1.1'), 4);
assert.strictEqual(net.isIP('::1'), 6);
assert.strictEqual(net.isIP('2001:db8::1'), 6);
assert.strictEqual(net.isIP('not-an-ip'), 0);
assert.strictEqual(net.isIP(''), 0);

// Test isIPv4
assert.strictEqual(net.isIPv4('127.0.0.1'), true);
assert.strictEqual(net.isIPv4('192.168.1.1'), true);
assert.strictEqual(net.isIPv4('256.1.1.1'), false);
assert.strictEqual(net.isIPv4('::1'), false);
assert.strictEqual(net.isIPv4('not-an-ip'), false);

// Test isIPv6
assert.strictEqual(net.isIPv6('::1'), true);
assert.strictEqual(net.isIPv6('2001:db8::1'), true);
assert.strictEqual(net.isIPv6('127.0.0.1'), false);
assert.strictEqual(net.isIPv6('not-an-ip'), false);

// Test Socket creation
const socket = new net.Socket();
assert.strictEqual(socket.connecting, false);
assert.strictEqual(socket.destroyed, false);
assert.strictEqual(socket.pending, true);
assert.strictEqual(socket.readyState, 'closed');
assert.strictEqual(typeof socket.connect, 'function');
assert.strictEqual(typeof socket.setTimeout, 'function');
assert.strictEqual(typeof socket.setNoDelay, 'function');
assert.strictEqual(typeof socket.setKeepAlive, 'function');
assert.strictEqual(typeof socket.address, 'function');
assert.strictEqual(typeof socket.ref, 'function');
assert.strictEqual(typeof socket.unref, 'function');
assert.strictEqual(typeof socket.destroy, 'function');

// Test Socket methods return this
assert.strictEqual(socket.setTimeout(1000), socket);
assert.strictEqual(socket.setNoDelay(true), socket);
assert.strictEqual(socket.setKeepAlive(true, 1000), socket);
assert.strictEqual(socket.ref(), socket);
assert.strictEqual(socket.unref(), socket);

// Test Server creation
const server = net.createServer((socket) => {
  socket.end('Hello');
});
assert.ok(server instanceof net.Server);
assert.strictEqual(server.listening, false);
assert.strictEqual(typeof server.listen, 'function');
assert.strictEqual(typeof server.close, 'function');
assert.strictEqual(typeof server.address, 'function');
assert.strictEqual(typeof server.getConnections, 'function');
assert.strictEqual(typeof server.ref, 'function');
assert.strictEqual(typeof server.unref, 'function');

// Test Server methods return this
assert.strictEqual(server.ref(), server);
assert.strictEqual(server.unref(), server);

// Test connect returns Socket
const connectSocket = net.connect({ port: 80, host: 'localhost' });
assert.ok(connectSocket instanceof net.Socket);

// Test createConnection is alias for connect
assert.strictEqual(net.createConnection, net.connect);

console.log('All net tests passed!');
