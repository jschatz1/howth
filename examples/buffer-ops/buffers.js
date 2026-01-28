/**
 * Buffer Operations Example
 *
 * Demonstrates Node.js Buffer API:
 * - Creating buffers
 * - Reading/writing various data types
 * - Encoding conversions
 * - Buffer manipulation
 * - Binary data handling
 *
 * Run: howth run --native examples/buffer-ops/buffers.js
 */

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Buffer Operations Demo${c.reset}\n`);

// 1. Creating Buffers
console.log(`${c.bold}1. Creating Buffers${c.reset}`);

const buf1 = Buffer.alloc(10); // Zero-filled
const buf2 = Buffer.alloc(10, 0xFF); // Filled with 0xFF
const buf3 = Buffer.from([1, 2, 3, 4, 5]);
const buf4 = Buffer.from('Hello, Howth!');
const buf5 = Buffer.from('48656c6c6f', 'hex');

console.log(`  alloc(10):       [${[...buf1].join(', ')}]`);
console.log(`  alloc(10, 0xFF): [${[...buf2].map(b => '0x' + b.toString(16)).join(', ')}]`);
console.log(`  from([1-5]):     [${[...buf3].join(', ')}]`);
console.log(`  from(string):    "${buf4.toString()}"`);
console.log(`  from(hex):       "${buf5.toString()}"`);

// 2. Buffer properties
console.log(`\n${c.bold}2. Buffer Properties${c.reset}`);

const sample = Buffer.from('Testing');
console.log(`  Buffer: "${sample.toString()}"`);
console.log(`  Length: ${sample.length}`);
console.log(`  Byte length: ${Buffer.byteLength('Testing')}`);
console.log(`  Is Buffer: ${Buffer.isBuffer(sample)}`);
console.log(`  Is Buffer (string): ${Buffer.isBuffer('not a buffer')}`);

// 3. Reading/Writing integers
console.log(`\n${c.bold}3. Reading/Writing Integers${c.reset}`);

const intBuf = Buffer.alloc(16);

// Write different integer types
intBuf.writeUInt8(255, 0);
intBuf.writeUInt16LE(65535, 1);
intBuf.writeUInt32LE(4294967295, 3);
intBuf.writeInt8(-128, 7);
intBuf.writeInt16LE(-32768, 8);
intBuf.writeInt32LE(-2147483648, 10);

console.log(`  UInt8:    ${intBuf.readUInt8(0)}`);
console.log(`  UInt16LE: ${intBuf.readUInt16LE(1)}`);
console.log(`  UInt32LE: ${intBuf.readUInt32LE(3)}`);
console.log(`  Int8:     ${intBuf.readInt8(7)}`);
console.log(`  Int16LE:  ${intBuf.readInt16LE(8)}`);
console.log(`  Int32LE:  ${intBuf.readInt32LE(10)}`);

// 4. Big-endian vs Little-endian
console.log(`\n${c.bold}4. Endianness${c.reset}`);

const endianBuf = Buffer.alloc(4);
const value = 0x12345678;

endianBuf.writeUInt32LE(value, 0);
console.log(`  Value: 0x${value.toString(16)}`);
console.log(`  Little-endian bytes: [${[...endianBuf].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);

endianBuf.writeUInt32BE(value, 0);
console.log(`  Big-endian bytes:    [${[...endianBuf].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);

// 5. Buffer from various sources
console.log(`\n${c.bold}5. Buffer From Various Sources${c.reset}`);

const fromString = Buffer.from('Hello');
const fromArray = Buffer.from([72, 101, 108, 108, 111]); // "Hello" in ASCII
const fromHex = Buffer.from('48656c6c6f', 'hex');
const fromBase64 = Buffer.from('SGVsbG8=', 'base64');

console.log(`  from string: "${fromString.toString()}"`);
console.log(`  from array:  "${fromArray.toString()}"`);
console.log(`  from hex:    "${fromHex.toString()}"`);
console.log(`  from base64: "${fromBase64.toString()}"`);
console.log(`  All equal:   ${fromString.equals(fromArray) && fromArray.equals(fromHex)}`);

// 6. String encoding
console.log(`\n${c.bold}6. String Encoding${c.reset}`);

const text = 'Hello, 世界!';
const encodings = ['utf8', 'ascii', 'base64', 'hex'];

console.log(`  Original: "${text}"`);
for (const enc of encodings) {
  try {
    const encoded = Buffer.from(text, 'utf8').toString(enc);
    console.log(`  ${enc.padEnd(7)}: "${encoded}"`);
  } catch (e) {
    console.log(`  ${enc.padEnd(7)}: (not supported)`);
  }
}

// 7. Buffer slicing
console.log(`\n${c.bold}7. Buffer Slicing${c.reset}`);

const original = Buffer.from('Hello, World!');
const slice = original.slice(0, 5);
const subarray = original.subarray(7, 12);

console.log(`  Original: "${original.toString()}"`);
console.log(`  slice(0,5): "${slice.toString()}"`);
console.log(`  subarray(7,12): "${subarray.toString()}"`);

// Slices share memory!
slice[0] = 'J'.charCodeAt(0);
console.log(`  After modifying slice: "${original.toString()}"`);

// 8. Buffer copying
console.log(`\n${c.bold}8. Buffer Copying${c.reset}`);

const src = Buffer.from('Source Buffer');
const dest = Buffer.alloc(20);

src.copy(dest, 0, 0, 6);
console.log(`  Source: "${src.toString()}"`);
console.log(`  Dest after copy: "${dest.toString()}"`);

// 9. Buffer comparison
console.log(`\n${c.bold}9. Buffer Comparison${c.reset}`);

const bufA = Buffer.from('ABC');
const bufB = Buffer.from('ABC');
const bufC = Buffer.from('ABD');

console.log(`  'ABC'.equals('ABC'): ${bufA.equals(bufB)}`);
console.log(`  'ABC'.equals('ABD'): ${bufA.equals(bufC)}`);
// Manual comparison
function compareBuffers(a, b) {
  const minLen = Math.min(a.length, b.length);
  for (let i = 0; i < minLen; i++) {
    if (a[i] < b[i]) return -1;
    if (a[i] > b[i]) return 1;
  }
  return a.length - b.length;
}
console.log(`  compare('ABC', 'ABD'): ${compareBuffers(bufA, bufC)} (negative = A < C)`);

// 10. Buffer concatenation
console.log(`\n${c.bold}10. Buffer Concatenation${c.reset}`);

const parts = [
  Buffer.from('Hello'),
  Buffer.from(', '),
  Buffer.from('World'),
  Buffer.from('!'),
];

const combined = Buffer.concat(parts);
console.log(`  Parts: ${parts.map(p => `"${p}"`).join(' + ')}`);
console.log(`  Combined: "${combined.toString()}"`);
console.log(`  Total length: ${combined.length}`);

// 11. Finding in buffers
console.log(`\n${c.bold}11. Searching Buffers${c.reset}`);

const haystack = Buffer.from('Hello, World! Hello, Howth!');
const needle = Buffer.from('Hello');

console.log(`  Buffer: "${haystack.toString()}"`);
console.log(`  indexOf('Hello'): ${haystack.indexOf(needle)}`);
console.log(`  lastIndexOf('Hello'): ${haystack.lastIndexOf(needle)}`);
console.log(`  includes('World'): ${haystack.includes('World')}`);
console.log(`  includes('Node'): ${haystack.includes('Node')}`);

// 12. Buffer fill
console.log(`\n${c.bold}12. Buffer Fill${c.reset}`);

const fillBuf = Buffer.alloc(10);

fillBuf.fill('a');
console.log(`  fill('a'): "${fillBuf.toString()}"`);

fillBuf.fill('bc');
console.log(`  fill('bc'): "${fillBuf.toString()}"`);

fillBuf.fill(0x00);
console.log(`  fill(0x00): [${[...fillBuf].join(', ')}]`);

// 13. Iteration
console.log(`\n${c.bold}13. Buffer Iteration${c.reset}`);

const iterBuf = Buffer.from('Howth');

const keys = [...iterBuf.keys()];
const values = [...iterBuf.values()];
const entries = [...iterBuf.entries()];

console.log(`  Keys: [${keys.join(', ')}]`);
console.log(`  Values: [${values.join(', ')}] (${values.map(v => String.fromCharCode(v)).join('')})`);
console.log(`  Entries: ${entries.map(([k, v]) => `${k}:${String.fromCharCode(v)}`).join(', ')}`);

// 14. Manual Byte Swapping
console.log(`\n${c.bold}14. Byte Swapping (Manual)${c.reset}`);

const swapBuf = Buffer.from([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);

console.log(`  Original:  [${[...swapBuf].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);

// Manual swap16 implementation
function swap16(buf) {
  const result = Buffer.alloc(buf.length);
  for (let i = 0; i < buf.length; i += 2) {
    result[i] = buf[i + 1];
    result[i + 1] = buf[i];
  }
  return result;
}

// Manual swap32 implementation
function swap32(buf) {
  const result = Buffer.alloc(buf.length);
  for (let i = 0; i < buf.length; i += 4) {
    result[i] = buf[i + 3];
    result[i + 1] = buf[i + 2];
    result[i + 2] = buf[i + 1];
    result[i + 3] = buf[i];
  }
  return result;
}

const swapped16 = swap16(swapBuf);
console.log(`  swap16:    [${[...swapped16].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);

const swapped32 = swap32(swapBuf);
console.log(`  swap32:    [${[...swapped32].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);

// 15. Binary protocol example
console.log(`\n${c.bold}15. Binary Protocol Example${c.reset}`);

// Simple message protocol: [type(1)][length(2)][payload(n)]
function encodeMessage(type, payload) {
  const payloadBuf = Buffer.from(payload);
  const msg = Buffer.alloc(3 + payloadBuf.length);

  msg.writeUInt8(type, 0);
  msg.writeUInt16LE(payloadBuf.length, 1);
  payloadBuf.copy(msg, 3);

  return msg;
}

function decodeMessage(buf) {
  const type = buf.readUInt8(0);
  const length = buf.readUInt16LE(1);
  const payload = buf.slice(3, 3 + length).toString();

  return { type, length, payload };
}

const encoded = encodeMessage(0x01, 'Hello, Protocol!');
const decoded = decodeMessage(encoded);

console.log(`  Encoded: [${[...encoded].map(b => '0x' + b.toString(16).padStart(2, '0')).join(', ')}]`);
console.log(`  Decoded: type=${decoded.type}, length=${decoded.length}, payload="${decoded.payload}"`);

console.log(`\n${c.green}${c.bold}Buffer operations demo completed!${c.reset}\n`);
