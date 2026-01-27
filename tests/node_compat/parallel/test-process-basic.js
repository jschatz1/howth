'use strict';
// Basic process tests that don't require node:test
const assert = require('assert');

// process.platform
assert.strictEqual(typeof process.platform, 'string');
assert.ok(['darwin', 'linux', 'win32', 'freebsd', 'openbsd', 'sunos', 'aix'].includes(process.platform) || process.platform === 'unknown');

// process.arch
assert.strictEqual(typeof process.arch, 'string');

// process.version
assert.strictEqual(typeof process.version, 'string');
assert.ok(process.version.startsWith('v'));

// process.versions
assert.strictEqual(typeof process.versions, 'object');
assert.strictEqual(typeof process.versions.node, 'string');

// process.pid
assert.strictEqual(typeof process.pid, 'number');

// process.cwd
assert.strictEqual(typeof process.cwd, 'function');
assert.strictEqual(typeof process.cwd(), 'string');

// process.env
assert.strictEqual(typeof process.env, 'object');
// Test that we can read PATH (or Path on Windows)
const pathEnv = process.env.PATH || process.env.Path;
assert.strictEqual(typeof pathEnv, 'string');

// Test env set/get
process.env.TEST_VAR = 'test_value';
assert.strictEqual(process.env.TEST_VAR, 'test_value');

// process.argv
assert.ok(Array.isArray(process.argv));
assert.ok(process.argv.length >= 1);

// process.exit and process.exitCode
assert.strictEqual(typeof process.exit, 'function');
assert.strictEqual(process.exitCode, 0);
process.exitCode = 42;
assert.strictEqual(process.exitCode, 42);
process.exitCode = 0; // Reset

// process.stdout
assert.strictEqual(typeof process.stdout, 'object');
assert.strictEqual(typeof process.stdout.write, 'function');

// process.stderr
assert.strictEqual(typeof process.stderr, 'object');
assert.strictEqual(typeof process.stderr.write, 'function');

// process.nextTick
assert.strictEqual(typeof process.nextTick, 'function');
let nextTickCalled = false;
process.nextTick(() => {
  nextTickCalled = true;
});

// process event emitter methods
assert.strictEqual(typeof process.on, 'function');
assert.strictEqual(typeof process.off, 'function');
assert.strictEqual(typeof process.once, 'function');
assert.strictEqual(typeof process.emit, 'function');
assert.strictEqual(typeof process.addListener, 'function');
assert.strictEqual(typeof process.removeListener, 'function');
assert.strictEqual(typeof process.removeAllListeners, 'function');

// Test process event emitter
let eventCalled = false;
const handler = () => { eventCalled = true; };
process.on('test-event', handler);
process.emit('test-event');
assert.strictEqual(eventCalled, true);
process.off('test-event', handler);

// Test once
let onceCalled = 0;
process.once('once-event', () => { onceCalled++; });
process.emit('once-event');
process.emit('once-event'); // Should not increment
assert.strictEqual(onceCalled, 1);

// process.memoryUsage
assert.strictEqual(typeof process.memoryUsage, 'function');
const mem = process.memoryUsage();
assert.strictEqual(typeof mem, 'object');
assert.ok('rss' in mem);
assert.ok('heapTotal' in mem);
assert.ok('heapUsed' in mem);

// process.cpuUsage
assert.strictEqual(typeof process.cpuUsage, 'function');
const cpu = process.cpuUsage();
assert.strictEqual(typeof cpu, 'object');
assert.ok('user' in cpu);
assert.ok('system' in cpu);

// process.uptime
assert.strictEqual(typeof process.uptime, 'function');
assert.strictEqual(typeof process.uptime(), 'number');

// process.hrtime
assert.strictEqual(typeof process.hrtime, 'object');
assert.strictEqual(typeof process.hrtime.bigint, 'function');
const hrtime = process.hrtime.bigint();
assert.strictEqual(typeof hrtime, 'bigint');

// Use setTimeout to verify nextTick was called
setTimeout(() => {
  assert.strictEqual(nextTickCalled, true);
  console.log('All process tests passed!');
}, 10);
