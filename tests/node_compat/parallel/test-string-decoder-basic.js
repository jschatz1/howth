'use strict';
const assert = require('assert');
const { StringDecoder } = require('string_decoder');

// Test basic UTF-8 decoding
const decoder = new StringDecoder('utf8');
assert.strictEqual(decoder.encoding, 'utf8');

// Test simple ASCII
assert.strictEqual(decoder.write(Buffer.from('hello')), 'hello');

// Test UTF-8 multi-byte characters
assert.strictEqual(decoder.write(Buffer.from('café')), 'café');
assert.strictEqual(decoder.write(Buffer.from('日本語')), '日本語');
assert.strictEqual(decoder.write(Buffer.from('🎉')), '🎉');

// Test end() method
const decoder2 = new StringDecoder('utf8');
assert.strictEqual(decoder2.write(Buffer.from('test')), 'test');
assert.strictEqual(decoder2.end(), '');

// Test end() with remaining buffer
const decoder3 = new StringDecoder('utf8');
assert.strictEqual(decoder3.end(Buffer.from('end test')), 'end test');

// Test ASCII encoding
const asciiDecoder = new StringDecoder('ascii');
assert.strictEqual(asciiDecoder.write(Buffer.from('Hello')), 'Hello');

// Test empty buffer
const emptyDecoder = new StringDecoder('utf8');
assert.strictEqual(emptyDecoder.write(Buffer.alloc(0)), '');

console.log('All string_decoder tests passed!');
