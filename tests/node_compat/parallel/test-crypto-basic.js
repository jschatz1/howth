'use strict';
const assert = require('assert');
const crypto = require('crypto');

// Test module exports
assert.strictEqual(typeof crypto.randomBytes, 'function');
assert.strictEqual(typeof crypto.randomUUID, 'function');
assert.strictEqual(typeof crypto.createHash, 'function');
assert.strictEqual(typeof crypto.createHmac, 'function');
assert.strictEqual(typeof crypto.timingSafeEqual, 'function');
assert.strictEqual(typeof crypto.getHashes, 'function');

// Test randomBytes
const bytes = crypto.randomBytes(16);
assert.strictEqual(bytes.length, 16);
assert.ok(Buffer.isBuffer(bytes));

// Test randomBytes produces different values
const bytes2 = crypto.randomBytes(16);
assert.ok(!bytes.equals(bytes2)); // Should be different

// Test randomBytes with callback
let callbackCalled = false;
crypto.randomBytes(8, (err, buf) => {
  assert.strictEqual(err, null);
  assert.strictEqual(buf.length, 8);
  callbackCalled = true;
});

// Test randomUUID
const uuid = crypto.randomUUID();
assert.strictEqual(typeof uuid, 'string');
assert.strictEqual(uuid.length, 36);
assert.ok(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(uuid));

// Test UUIDs are unique
const uuid2 = crypto.randomUUID();
assert.notStrictEqual(uuid, uuid2);

// Test randomInt
const randInt = crypto.randomInt(100);
assert.strictEqual(typeof randInt, 'number');
assert.ok(randInt >= 0 && randInt < 100);

const randIntRange = crypto.randomInt(10, 20);
assert.ok(randIntRange >= 10 && randIntRange < 20);

// Test randomFill
const fillBuf = Buffer.alloc(8);
crypto.randomFillSync(fillBuf);
assert.strictEqual(fillBuf.length, 8);
// Verify it's not all zeros
let hasNonZero = false;
for (let i = 0; i < fillBuf.length; i++) {
  if (fillBuf[i] !== 0) hasNonZero = true;
}
assert.ok(hasNonZero);

// Test getHashes
const hashes = crypto.getHashes();
assert.ok(Array.isArray(hashes));
assert.ok(hashes.includes('sha256'));
assert.ok(hashes.includes('md5'));

// Test createHash with MD5 (sync capable)
const md5Hash = crypto.createHash('md5');
md5Hash.update('hello');
const md5Result = md5Hash.digestSync('hex');
assert.strictEqual(md5Result, '5d41402abc4b2a76b9719d911017c592');

// Test MD5 with multiple updates
const md5Hash2 = crypto.createHash('md5');
md5Hash2.update('hello');
md5Hash2.update(' ');
md5Hash2.update('world');
const md5Result2 = md5Hash2.digestSync('hex');
assert.strictEqual(md5Result2, '5eb63bbbe01eeed093cb22bb8f5acdc3');

// Test copy
const md5Hash3 = crypto.createHash('md5');
md5Hash3.update('hello');
const copy = md5Hash3.copy();
copy.update(' world');
assert.strictEqual(md5Hash3.digestSync('hex'), '5d41402abc4b2a76b9719d911017c592');
assert.strictEqual(copy.digestSync('hex'), '5eb63bbbe01eeed093cb22bb8f5acdc3');

// Test MD5 base64 encoding
const md5Base64 = crypto.createHash('md5');
md5Base64.update('hello');
assert.strictEqual(md5Base64.digestSync('base64'), 'XUFAKrxLKna5cZ2REBfFkg==');

// Test timingSafeEqual
const a = Buffer.from('hello');
const b = Buffer.from('hello');
const c = Buffer.from('world');
assert.strictEqual(crypto.timingSafeEqual(a, b), true);
assert.strictEqual(crypto.timingSafeEqual(a, c), false);

// Test timingSafeEqual throws on different lengths
assert.throws(() => {
  crypto.timingSafeEqual(Buffer.from('hi'), Buffer.from('hello'));
}, RangeError);

// Test webcrypto access
assert.ok(crypto.subtle);
assert.ok(crypto.webcrypto);

// Test getCiphers
const ciphers = crypto.getCiphers();
assert.ok(Array.isArray(ciphers));

// Test constants exist
assert.ok(crypto.constants);

// Allow async operations to complete
setTimeout(() => {
  assert.strictEqual(callbackCalled, true);
  console.log('All crypto tests passed!');
}, 50);
