'use strict';
// Basic buffer tests that don't require node:test or vm
const assert = require('assert');

// Buffer.from string
const buf1 = Buffer.from('hello');
assert.strictEqual(buf1.length, 5);
assert.strictEqual(buf1.toString(), 'hello');

// Buffer.from array
const buf2 = Buffer.from([72, 101, 108, 108, 111]);
assert.strictEqual(buf2.toString(), 'Hello');

// Buffer.alloc
const buf3 = Buffer.alloc(10);
assert.strictEqual(buf3.length, 10);
assert.strictEqual(buf3[0], 0);

// Buffer.alloc with fill
const buf4 = Buffer.alloc(5, 'a');
assert.strictEqual(buf4.toString(), 'aaaaa');

// Buffer.allocUnsafe
const buf5 = Buffer.allocUnsafe(10);
assert.strictEqual(buf5.length, 10);

// Buffer.concat
const buf6 = Buffer.concat([Buffer.from('hello'), Buffer.from(' '), Buffer.from('world')]);
assert.strictEqual(buf6.toString(), 'hello world');

// Buffer.byteLength
assert.strictEqual(Buffer.byteLength('hello'), 5);
assert.strictEqual(Buffer.byteLength('κλμνξο', 'utf8'), 12);

// Buffer.isBuffer
assert.strictEqual(Buffer.isBuffer(buf1), true);
assert.strictEqual(Buffer.isBuffer('hello'), false);
assert.strictEqual(Buffer.isBuffer(new Uint8Array(5)), false);

// Buffer.isEncoding
assert.strictEqual(Buffer.isEncoding('utf8'), true);
assert.strictEqual(Buffer.isEncoding('utf-8'), true);
assert.strictEqual(Buffer.isEncoding('hex'), true);
assert.strictEqual(Buffer.isEncoding('invalid'), false);

// Buffer slice
const buf7 = Buffer.from('hello world');
const sliced = buf7.slice(0, 5);
assert.strictEqual(sliced.toString(), 'hello');

// Buffer subarray
const subarr = buf7.subarray(6, 11);
assert.strictEqual(subarr.toString(), 'world');

// Buffer copy
const target = Buffer.alloc(5);
buf1.copy(target);
assert.strictEqual(target.toString(), 'hello');

// Buffer compare
const a = Buffer.from('abc');
const b = Buffer.from('abc');
const c = Buffer.from('abd');
assert.strictEqual(a.compare(b), 0);
assert.strictEqual(a.compare(c), -1);
assert.strictEqual(c.compare(a), 1);

// Buffer equals
assert.strictEqual(a.equals(b), true);
assert.strictEqual(a.equals(c), false);

// Buffer indexOf
const buf8 = Buffer.from('hello world');
assert.strictEqual(buf8.indexOf('world'), 6);
assert.strictEqual(buf8.indexOf('x'), -1);
assert.strictEqual(buf8.indexOf(111), 4); // 'o'

// Buffer includes
assert.strictEqual(buf8.includes('world'), true);
assert.strictEqual(buf8.includes('xyz'), false);

// Buffer fill
const buf9 = Buffer.alloc(5);
buf9.fill('x');
assert.strictEqual(buf9.toString(), 'xxxxx');

// Buffer write
const buf10 = Buffer.alloc(10);
buf10.write('hello');
assert.strictEqual(buf10.slice(0, 5).toString(), 'hello');

// Buffer readUInt8/writeUInt8
const buf11 = Buffer.alloc(4);
buf11.writeUInt8(255, 0);
buf11.writeUInt8(128, 1);
assert.strictEqual(buf11.readUInt8(0), 255);
assert.strictEqual(buf11.readUInt8(1), 128);

// Buffer readUInt16/writeUInt16
const buf12 = Buffer.alloc(4);
buf12.writeUInt16BE(0x1234, 0);
buf12.writeUInt16LE(0x5678, 2);
assert.strictEqual(buf12.readUInt16BE(0), 0x1234);
assert.strictEqual(buf12.readUInt16LE(2), 0x5678);

// Buffer readUInt32/writeUInt32
const buf13 = Buffer.alloc(8);
buf13.writeUInt32BE(0x12345678, 0);
buf13.writeUInt32LE(0xDEADBEEF, 4);
assert.strictEqual(buf13.readUInt32BE(0), 0x12345678);
assert.strictEqual(buf13.readUInt32LE(4), 0xDEADBEEF >>> 0);

// Buffer hex encoding
const hexBuf = Buffer.from('48656c6c6f', 'hex');
assert.strictEqual(hexBuf.toString(), 'Hello');
assert.strictEqual(hexBuf.toString('hex'), '48656c6c6f');

// Buffer base64 encoding
const b64Buf = Buffer.from('SGVsbG8=', 'base64');
assert.strictEqual(b64Buf.toString(), 'Hello');
assert.strictEqual(Buffer.from('Hello').toString('base64'), 'SGVsbG8=');

// Buffer.from with ArrayBuffer
const ab = new ArrayBuffer(4);
const view = new Uint8Array(ab);
view[0] = 1; view[1] = 2; view[2] = 3; view[3] = 4;
const buf14 = Buffer.from(ab);
assert.strictEqual(buf14[0], 1);
assert.strictEqual(buf14[3], 4);

// Buffer.from with TypedArray
const uint8 = new Uint8Array([5, 6, 7, 8]);
const buf15 = Buffer.from(uint8);
assert.strictEqual(buf15[0], 5);
assert.strictEqual(buf15[3], 8);

console.log('All buffer tests passed!');
