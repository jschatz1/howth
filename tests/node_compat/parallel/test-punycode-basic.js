'use strict';
const assert = require('assert');
const punycode = require('punycode');

// Test version
assert.strictEqual(typeof punycode.version, 'string');

// Test toASCII with ASCII domain (no conversion needed)
assert.strictEqual(punycode.toASCII('example.com'), 'example.com');

// Test toUnicode with ASCII domain (no conversion needed)
assert.strictEqual(punycode.toUnicode('example.com'), 'example.com');

// Test ucs2.decode
const decoded = punycode.ucs2.decode('abc');
assert.deepStrictEqual(decoded, [97, 98, 99]);

// Test ucs2.encode
const encoded = punycode.ucs2.encode([97, 98, 99]);
assert.strictEqual(encoded, 'abc');

// Test ucs2 roundtrip
const original = 'Hello';
const points = punycode.ucs2.decode(original);
const back = punycode.ucs2.encode(points);
assert.strictEqual(back, original);

// Test encode/decode exist
assert.strictEqual(typeof punycode.encode, 'function');
assert.strictEqual(typeof punycode.decode, 'function');

console.log('All punycode tests passed!');
