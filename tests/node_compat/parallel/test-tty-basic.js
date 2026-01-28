'use strict';
const assert = require('assert');
const tty = require('tty');

// Test isatty function
assert.strictEqual(typeof tty.isatty, 'function');
const isTty0 = tty.isatty(0);
const isTty1 = tty.isatty(1);
const isTty2 = tty.isatty(2);
assert.strictEqual(typeof isTty0, 'boolean');
assert.strictEqual(typeof isTty1, 'boolean');
assert.strictEqual(typeof isTty2, 'boolean');

// Non-TTY file descriptors
assert.strictEqual(tty.isatty(100), false);
assert.strictEqual(tty.isatty(-1), false);

// Test ReadStream class
assert.strictEqual(typeof tty.ReadStream, 'function');
const readStream = new tty.ReadStream(0);
assert.strictEqual(typeof readStream.isTTY, 'boolean');
assert.strictEqual(typeof readStream.setRawMode, 'function');
readStream.setRawMode(true);
assert.strictEqual(readStream.isRaw, true);

// Test WriteStream class
assert.strictEqual(typeof tty.WriteStream, 'function');
const writeStream = new tty.WriteStream(1);
assert.strictEqual(typeof writeStream.isTTY, 'boolean');
assert.strictEqual(typeof writeStream.columns, 'number');
assert.strictEqual(typeof writeStream.rows, 'number');

// Test WriteStream methods
assert.strictEqual(typeof writeStream.clearLine, 'function');
assert.strictEqual(typeof writeStream.clearScreenDown, 'function');
assert.strictEqual(typeof writeStream.cursorTo, 'function');
assert.strictEqual(typeof writeStream.moveCursor, 'function');
assert.strictEqual(typeof writeStream.getColorDepth, 'function');
assert.strictEqual(typeof writeStream.hasColors, 'function');
assert.strictEqual(typeof writeStream.getWindowSize, 'function');

// Test getWindowSize
const size = writeStream.getWindowSize();
assert.ok(Array.isArray(size));
assert.strictEqual(size.length, 2);
assert.strictEqual(typeof size[0], 'number');
assert.strictEqual(typeof size[1], 'number');

// Test getColorDepth
const depth = writeStream.getColorDepth();
assert.strictEqual(typeof depth, 'number');
assert.ok(depth > 0);

// Test hasColors
assert.strictEqual(writeStream.hasColors(16), true);
assert.strictEqual(writeStream.hasColors(256), true);

console.log('All tty tests passed!');
