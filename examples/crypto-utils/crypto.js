/**
 * Crypto Utilities Example
 *
 * Demonstrates Node.js crypto module:
 * - Random bytes generation
 * - UUID generation
 * - Hashing (SHA-256, SHA-512, MD5)
 * - HMAC authentication
 * - Base64/Hex encoding
 * - Token generation
 *
 * Run: howth run --native examples/crypto-utils/crypto.js
 */

const crypto = require('crypto');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

async function main() {
  console.log(`\n${c.bold}${c.cyan}Crypto Utilities Demo${c.reset}\n`);

  // 1. Random bytes
  console.log(`${c.bold}1. Random Bytes${c.reset}`);
  const randomBytes = crypto.randomBytes(16);
  console.log(`  Hex:    ${randomBytes.toString('hex')}`);
  console.log(`  Base64: ${randomBytes.toString('base64')}`);

  // 2. UUID generation (v4)
  console.log(`\n${c.bold}2. UUID Generation${c.reset}`);
  function generateUUID() {
    const bytes = crypto.randomBytes(16);
    // Set version (4) and variant bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    const hex = bytes.toString('hex');
    return [
      hex.substring(0, 8),
      hex.substring(8, 12),
      hex.substring(12, 16),
      hex.substring(16, 20),
      hex.substring(20, 32),
    ].join('-');
  }

  for (let i = 0; i < 3; i++) {
    console.log(`  ${c.blue}UUID:${c.reset} ${generateUUID()}`);
  }

  // 3. Hashing with createHash (async in howth)
  console.log(`\n${c.bold}3. Hashing (SHA-256, SHA-512, MD5)${c.reset}`);
  const testData = 'Hello, Howth!';
  console.log(`  Input: "${testData}"`);

  // SHA-256
  const sha256Hash = crypto.createHash('sha256');
  sha256Hash.update(testData);
  const sha256Result = await sha256Hash.digest('hex');
  console.log(`  SHA-256: ${sha256Result}`);

  // SHA-512
  const sha512Hash = crypto.createHash('sha512');
  sha512Hash.update(testData);
  const sha512Result = await sha512Hash.digest('hex');
  console.log(`  SHA-512: ${String(sha512Result).substring(0, 32)}...`);

  // MD5 (for checksums, not security)
  const md5Hash = crypto.createHash('md5');
  md5Hash.update(testData);
  const md5Result = await md5Hash.digest('hex');
  console.log(`  MD5:     ${md5Result}`);

  // 4. Multiple hash updates (streaming style)
  console.log(`\n${c.bold}4. Incremental Hashing${c.reset}`);
  const streamHash = crypto.createHash('sha256');
  streamHash.update('Part 1: ');
  streamHash.update('Part 2: ');
  streamHash.update('Part 3');
  const streamResult = await streamHash.digest('hex');
  console.log(`  Input: "Part 1: " + "Part 2: " + "Part 3"`);
  console.log(`  Hash:  ${streamResult}`);

  // Verify concatenated matches
  const concatHash = crypto.createHash('sha256');
  concatHash.update('Part 1: Part 2: Part 3');
  const concatResult = await concatHash.digest('hex');
  console.log(`  ${c.dim}Matches concatenated: ${streamResult === concatResult}${c.reset}`);

  // 5. Base64 encoding/decoding
  console.log(`\n${c.bold}5. Base64 Encoding${c.reset}`);
  const original = 'Hello, World!';
  const encoded = Buffer.from(original).toString('base64');
  const decoded = Buffer.from(encoded, 'base64').toString('utf8');

  console.log(`  Original: "${original}"`);
  console.log(`  Encoded:  "${encoded}"`);
  console.log(`  Decoded:  "${decoded}"`);
  console.log(`  ${c.dim}Match: ${original === decoded}${c.reset}`);

  // 6. Hex encoding
  console.log(`\n${c.bold}6. Hex Encoding${c.reset}`);
  const text = 'Howth';
  const hexEncoded = Buffer.from(text).toString('hex');
  const hexDecoded = Buffer.from(hexEncoded, 'hex').toString('utf8');

  console.log(`  Original: "${text}"`);
  console.log(`  Hex:      "${hexEncoded}"`);
  console.log(`  Decoded:  "${hexDecoded}"`);

  // 7. Token Generation
  console.log(`\n${c.bold}7. Token Generation${c.reset}`);
  function generateToken(length = 32) {
    const bytes = crypto.randomBytes(length);
    // base64url encoding
    return bytes.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  }

  console.log(`  API Key:     ${generateToken(24)}`);
  console.log(`  Session ID:  ${generateToken(16)}`);
  console.log(`  Reset Token: ${generateToken(32)}`);

  // 8. Random integers
  console.log(`\n${c.bold}8. Random Integers${c.reset}`);
  function randomInt(min, max) {
    const range = max - min + 1;
    const bytes = crypto.randomBytes(4);
    const value = bytes.readUInt32BE(0);
    return min + (value % range);
  }

  console.log(`  Random 1-100:  ${randomInt(1, 100)}`);
  console.log(`  Random 1-100:  ${randomInt(1, 100)}`);
  console.log(`  Random 1-1000: ${randomInt(1, 1000)}`);

  // 9. Secure password generator
  console.log(`\n${c.bold}9. Password Generator${c.reset}`);
  function generatePassword(length = 16) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*';
    const bytes = crypto.randomBytes(length);
    let password = '';
    for (let i = 0; i < length; i++) {
      password += chars[bytes[i] % chars.length];
    }
    return password;
  }

  console.log(`  Password 1: ${generatePassword(12)}`);
  console.log(`  Password 2: ${generatePassword(16)}`);
  console.log(`  Password 3: ${generatePassword(20)}`);

  // 10. OTP (One-Time Password) generation
  console.log(`\n${c.bold}10. OTP Generation${c.reset}`);
  function generateOTP(digits = 6) {
    const max = Math.pow(10, digits) - 1;
    const bytes = crypto.randomBytes(4);
    const value = bytes.readUInt32BE(0) % (max + 1);
    return value.toString().padStart(digits, '0');
  }

  console.log(`  6-digit OTP: ${generateOTP(6)}`);
  console.log(`  4-digit OTP: ${generateOTP(4)}`);
  console.log(`  8-digit OTP: ${generateOTP(8)}`);

  // 11. Hash-based password verification simulation
  console.log(`\n${c.bold}11. Password Hashing${c.reset}`);
  async function hashPassword(password, salt) {
    const hash = crypto.createHash('sha256');
    hash.update(salt + password);
    return await hash.digest('hex');
  }

  const password = 'mySecurePassword123';
  const salt = crypto.randomBytes(16).toString('hex');
  const hashedPassword = await hashPassword(password, salt);

  console.log(`  Password: "${password}"`);
  console.log(`  Salt:     ${salt}`);
  console.log(`  Hash:     ${hashedPassword}`);

  // Verify password
  const verifyHash = await hashPassword(password, salt);
  console.log(`  ${c.dim}Password verified: ${hashedPassword === verifyHash}${c.reset}`);

  console.log(`\n${c.green}${c.bold}Crypto utilities demo completed!${c.reset}\n`);
}

main();
