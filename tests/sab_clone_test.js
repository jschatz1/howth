// Test if structuredClone is available and handles SharedArrayBuffer
console.log('Testing structuredClone with SharedArrayBuffer...');

try {
  const sab = new SharedArrayBuffer(16);
  const view = new Int32Array(sab);
  view[0] = 42;

  // structuredClone with transfer should preserve the SharedArrayBuffer
  const cloned = structuredClone({ buffer: sab });
  const clonedView = new Int32Array(cloned.buffer);

  console.log('Original value:', view[0]);
  console.log('Cloned value:', clonedView[0]);
  console.log('Same buffer?', sab === cloned.buffer);
  console.log('structuredClone available: YES');
} catch (e) {
  console.log('Error:', e.message);
}
