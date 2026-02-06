// Test crypto.scrypt
const crypto = require('crypto');

console.log('Testing crypto.scrypt...\n');

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

async function asyncTest(name, fn) {
  try {
    await fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

// Test function existence
test('crypto.scrypt exists', () => {
  if (typeof crypto.scrypt !== 'function') throw new Error('scrypt is not a function');
});

test('crypto.scryptSync exists', () => {
  if (typeof crypto.scryptSync !== 'function') throw new Error('scryptSync is not a function');
});

// Test scryptSync basic functionality
test('scryptSync returns Buffer', () => {
  const result = crypto.scryptSync('password', 'salt', 32);
  if (!Buffer.isBuffer(result)) throw new Error('Expected Buffer');
  if (result.length !== 32) throw new Error(`Expected 32 bytes, got ${result.length}`);
});

test('scryptSync with string inputs', () => {
  const result = crypto.scryptSync('password', 'salt', 64);
  if (result.length !== 64) throw new Error(`Expected 64 bytes, got ${result.length}`);
});

test('scryptSync with Buffer inputs', () => {
  const password = Buffer.from('password');
  const salt = Buffer.from('salt');
  const result = crypto.scryptSync(password, salt, 32);
  if (result.length !== 32) throw new Error(`Expected 32 bytes, got ${result.length}`);
});

test('scryptSync deterministic output', () => {
  const result1 = crypto.scryptSync('password', 'salt', 32);
  const result2 = crypto.scryptSync('password', 'salt', 32);
  if (!result1.equals(result2)) throw new Error('Results should be equal');
});

test('scryptSync different passwords produce different output', () => {
  const result1 = crypto.scryptSync('password1', 'salt', 32);
  const result2 = crypto.scryptSync('password2', 'salt', 32);
  if (result1.equals(result2)) throw new Error('Different passwords should produce different output');
});

test('scryptSync different salts produce different output', () => {
  const result1 = crypto.scryptSync('password', 'salt1', 32);
  const result2 = crypto.scryptSync('password', 'salt2', 32);
  if (result1.equals(result2)) throw new Error('Different salts should produce different output');
});

test('scryptSync with custom N parameter', () => {
  const result = crypto.scryptSync('password', 'salt', 32, { N: 1024 });
  if (result.length !== 32) throw new Error(`Expected 32 bytes, got ${result.length}`);
});

test('scryptSync with custom r and p parameters', () => {
  const result = crypto.scryptSync('password', 'salt', 32, { N: 1024, r: 8, p: 1 });
  if (result.length !== 32) throw new Error(`Expected 32 bytes, got ${result.length}`);
});

test('scryptSync throws for non-power-of-2 N', () => {
  try {
    crypto.scryptSync('password', 'salt', 32, { N: 1000 });
    throw new Error('Should have thrown');
  } catch (e) {
    if (!e.message.includes('power of 2')) throw new Error('Wrong error message');
  }
});

// Test known test vector (RFC 7914)
test('scryptSync matches RFC 7914 test vector 1', () => {
  // Test vector: password="", salt="", N=16, r=1, p=1, dkLen=64
  const result = crypto.scryptSync('', '', 64, { N: 16, r: 1, p: 1 });
  const expected = Buffer.from([
    0x77, 0xd6, 0x57, 0x62, 0x38, 0x65, 0x7b, 0x20,
    0x3b, 0x19, 0xca, 0x42, 0xc1, 0x8a, 0x04, 0x97,
    0xf1, 0x6b, 0x48, 0x44, 0xe3, 0x07, 0x4a, 0xe8,
    0xdf, 0xdf, 0xfa, 0x3f, 0xed, 0xe2, 0x14, 0x42,
    0xfc, 0xd0, 0x06, 0x9d, 0xed, 0x09, 0x48, 0xf8,
    0x32, 0x6a, 0x75, 0x3a, 0x0f, 0xc8, 0x1f, 0x17,
    0xe8, 0xd3, 0xe0, 0xfb, 0x2e, 0x0d, 0x36, 0x28,
    0xcf, 0x35, 0xe2, 0x0c, 0x38, 0xd1, 0x89, 0x06
  ]);
  if (!result.equals(expected)) {
    console.log('  Got:     ', result.toString('hex').substring(0, 32) + '...');
    console.log('  Expected:', expected.toString('hex').substring(0, 32) + '...');
    throw new Error('Does not match RFC 7914 test vector');
  }
});

test('scryptSync matches RFC 7914 test vector 2', () => {
  // Test vector: password="password", salt="NaCl", N=1024, r=8, p=16, dkLen=64
  const result = crypto.scryptSync('password', 'NaCl', 64, { N: 1024, r: 8, p: 16 });
  const expected = Buffer.from([
    0xfd, 0xba, 0xbe, 0x1c, 0x9d, 0x34, 0x72, 0x00,
    0x78, 0x56, 0xe7, 0x19, 0x0d, 0x01, 0xe9, 0xfe,
    0x7c, 0x6a, 0xd7, 0xcb, 0xc8, 0x23, 0x78, 0x30,
    0xe7, 0x73, 0x76, 0x63, 0x4b, 0x37, 0x31, 0x62,
    0x2e, 0xaf, 0x30, 0xd9, 0x2e, 0x22, 0xa3, 0x88,
    0x6f, 0xf1, 0x09, 0x27, 0x9d, 0x98, 0x30, 0xda,
    0xc7, 0x27, 0xaf, 0xb9, 0x4a, 0x83, 0xee, 0x6d,
    0x83, 0x60, 0xcb, 0xdf, 0xa2, 0xcc, 0x06, 0x40
  ]);
  if (!result.equals(expected)) {
    console.log('  Got:     ', result.toString('hex').substring(0, 32) + '...');
    console.log('  Expected:', expected.toString('hex').substring(0, 32) + '...');
    throw new Error('Does not match RFC 7914 test vector');
  }
});

// Test async scrypt
async function runAsyncTests() {
  await asyncTest('scrypt async basic', async () => {
    return new Promise((resolve, reject) => {
      crypto.scrypt('password', 'salt', 32, (err, derivedKey) => {
        if (err) return reject(err);
        if (!Buffer.isBuffer(derivedKey)) return reject(new Error('Expected Buffer'));
        if (derivedKey.length !== 32) return reject(new Error(`Expected 32 bytes, got ${derivedKey.length}`));
        resolve();
      });
    });
  });

  await asyncTest('scrypt async with options', async () => {
    return new Promise((resolve, reject) => {
      crypto.scrypt('password', 'salt', 32, { N: 1024, r: 8, p: 1 }, (err, derivedKey) => {
        if (err) return reject(err);
        if (derivedKey.length !== 32) return reject(new Error(`Expected 32 bytes`));
        resolve();
      });
    });
  });

  await asyncTest('scrypt async matches sync', async () => {
    const syncResult = crypto.scryptSync('password', 'salt', 32, { N: 1024 });
    return new Promise((resolve, reject) => {
      crypto.scrypt('password', 'salt', 32, { N: 1024 }, (err, asyncResult) => {
        if (err) return reject(err);
        if (!syncResult.equals(asyncResult)) return reject(new Error('Async and sync results differ'));
        resolve();
      });
    });
  });

  await asyncTest('scrypt async error handling', async () => {
    return new Promise((resolve, reject) => {
      crypto.scrypt('password', 'salt', 32, { N: 1000 }, (err, derivedKey) => {
        if (!err) return reject(new Error('Should have errored'));
        if (!err.message.includes('power of 2')) return reject(new Error('Wrong error message'));
        resolve();
      });
    });
  });

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  process.exit(failed > 0 ? 1 : 0);
}

runAsyncTests().catch(e => {
  console.error('Test error:', e);
  process.exit(1);
});
