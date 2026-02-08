// Test SharedArrayBuffer and Atomics availability
console.log('Testing SharedArrayBuffer and Atomics...\n');

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

// Test SharedArrayBuffer availability
test('SharedArrayBuffer is available', () => {
  if (typeof SharedArrayBuffer === 'undefined') {
    throw new Error('SharedArrayBuffer is not defined');
  }
});

test('Can create SharedArrayBuffer', () => {
  const sab = new SharedArrayBuffer(16);
  if (sab.byteLength !== 16) {
    throw new Error(`Expected byteLength 16, got ${sab.byteLength}`);
  }
});

test('Can create Int32Array on SharedArrayBuffer', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);
  if (view.length !== 4) {
    throw new Error(`Expected length 4, got ${view.length}`);
  }
});

// Test Atomics availability
test('Atomics is available', () => {
  if (typeof Atomics === 'undefined') {
    throw new Error('Atomics is not defined');
  }
});

test('Atomics.store and Atomics.load', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 42);
  const value = Atomics.load(view, 0);

  if (value !== 42) {
    throw new Error(`Expected 42, got ${value}`);
  }
});

test('Atomics.add', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 10);
  const oldValue = Atomics.add(view, 0, 5);
  const newValue = Atomics.load(view, 0);

  if (oldValue !== 10) throw new Error(`Expected old value 10, got ${oldValue}`);
  if (newValue !== 15) throw new Error(`Expected new value 15, got ${newValue}`);
});

test('Atomics.sub', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 20);
  const oldValue = Atomics.sub(view, 0, 7);
  const newValue = Atomics.load(view, 0);

  if (oldValue !== 20) throw new Error(`Expected old value 20, got ${oldValue}`);
  if (newValue !== 13) throw new Error(`Expected new value 13, got ${newValue}`);
});

test('Atomics.exchange', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 100);
  const oldValue = Atomics.exchange(view, 0, 200);
  const newValue = Atomics.load(view, 0);

  if (oldValue !== 100) throw new Error(`Expected old value 100, got ${oldValue}`);
  if (newValue !== 200) throw new Error(`Expected new value 200, got ${newValue}`);
});

test('Atomics.compareExchange', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 50);

  // Should succeed - expected matches
  const result1 = Atomics.compareExchange(view, 0, 50, 75);
  if (result1 !== 50) throw new Error(`CAS should return old value 50, got ${result1}`);
  if (Atomics.load(view, 0) !== 75) throw new Error('CAS should have set value to 75');

  // Should fail - expected doesn't match
  const result2 = Atomics.compareExchange(view, 0, 50, 100);
  if (result2 !== 75) throw new Error(`CAS should return current value 75, got ${result2}`);
  if (Atomics.load(view, 0) !== 75) throw new Error('CAS should not have changed value');
});

test('Atomics.and / Atomics.or / Atomics.xor', () => {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);

  Atomics.store(view, 0, 0b1111);
  Atomics.and(view, 0, 0b1010);
  if (Atomics.load(view, 0) !== 0b1010) throw new Error('AND failed');

  Atomics.or(view, 0, 0b0101);
  if (Atomics.load(view, 0) !== 0b1111) throw new Error('OR failed');

  Atomics.xor(view, 0, 0b1100);
  if (Atomics.load(view, 0) !== 0b0011) throw new Error('XOR failed');
});

test('Atomics.isLockFree', () => {
  // 4 bytes (Int32) should always be lock-free
  if (!Atomics.isLockFree(4)) {
    throw new Error('Int32 should be lock-free');
  }
  // 1, 2 bytes usually lock-free too
  // 8 bytes (BigInt64) may or may not be
});

// Summary
console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);

if (failed > 0) {
  process.exit(1);
}
