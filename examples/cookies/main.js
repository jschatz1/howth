/**
 * Cookie API Example
 *
 * Demonstrates Howth.Cookie and Howth.CookieMap APIs for working with HTTP cookies.
 * Similar to Bun.Cookie and Bun.CookieMap.
 *
 * Run: howth run --native examples/cookies/main.js
 */

console.log('=== Cookie API Example ===\n');

// ============================================================================
// Cookie Class - Represents a single HTTP cookie
// ============================================================================

console.log('1. Creating Cookies:');
console.log('─'.repeat(50));

// Basic cookie
const basicCookie = new Howth.Cookie('session', 'abc123');
console.log('Basic cookie:');
console.log(`  ${basicCookie.serialize()}`);

// Cookie with all options
const secureCookie = new Howth.Cookie('auth_token', 'xyz789', {
  domain: 'example.com',
  path: '/api',
  expires: new Date(Date.now() + 86400000), // 1 day
  secure: true,
  httpOnly: true,
  sameSite: 'strict',
});
console.log('\nSecure cookie with options:');
console.log(`  ${secureCookie.serialize()}`);

// Cookie with maxAge
const maxAgeCookie = new Howth.Cookie('preferences', 'dark-mode', {
  maxAge: 3600, // 1 hour in seconds
  sameSite: 'lax',
});
console.log('\nCookie with maxAge:');
console.log(`  ${maxAgeCookie.serialize()}`);

// ============================================================================
// Parsing Cookies
// ============================================================================

console.log('\n2. Parsing Cookie Strings:');
console.log('─'.repeat(50));

const cookieString = 'session=abc123; Path=/; Secure; HttpOnly; SameSite=Strict';
const parsed = Howth.Cookie.parse(cookieString);

console.log(`Input: "${cookieString}"`);
console.log('Parsed:');
console.log(`  name: ${parsed.name}`);
console.log(`  value: ${parsed.value}`);
console.log(`  path: ${parsed.path}`);
console.log(`  secure: ${parsed.secure}`);
console.log(`  httpOnly: ${parsed.httpOnly}`);
console.log(`  sameSite: ${parsed.sameSite}`);

// ============================================================================
// Cookie Expiration
// ============================================================================

console.log('\n3. Cookie Expiration:');
console.log('─'.repeat(50));

const expiredCookie = new Cookie('old', 'data', {
  expires: new Date(Date.now() - 1000), // 1 second ago
});
console.log(`Expired cookie (past date): isExpired = ${expiredCookie.isExpired()}`);

const zeroMaxAge = new Cookie('deleted', '', { maxAge: 0 });
console.log(`Zero maxAge cookie: isExpired = ${zeroMaxAge.isExpired()}`);

const validCookie = new Cookie('valid', 'data', { maxAge: 3600 });
console.log(`Valid cookie (1 hour maxAge): isExpired = ${validCookie.isExpired()}`);

const sessionCookie = new Cookie('session', 'data');
console.log(`Session cookie (no expiry): isExpired = ${sessionCookie.isExpired()}`);

// ============================================================================
// CookieMap - Working with Multiple Cookies
// ============================================================================

console.log('\n4. CookieMap - Multiple Cookies:');
console.log('─'.repeat(50));

// From cookie header string (as received from browser)
const fromHeader = new CookieMap('session=abc123; theme=dark; lang=en');
console.log('From header string:');
console.log(`  size: ${fromHeader.size}`);
console.log(`  session: ${fromHeader.get('session')}`);
console.log(`  theme: ${fromHeader.get('theme')}`);

// From object
const fromObject = new CookieMap({
  user: 'john',
  role: 'admin',
  preferences: 'compact',
});
console.log('\nFrom object:');
console.log(`  ${JSON.stringify(fromObject.toJSON())}`);

// From array of pairs
const fromArray = new CookieMap([
  ['a', '1'],
  ['b', '2'],
  ['c', '3'],
]);
console.log('\nFrom array:');
for (const [name, value] of fromArray) {
  console.log(`  ${name}: ${value}`);
}

// ============================================================================
// CookieMap Operations
// ============================================================================

console.log('\n5. CookieMap Operations:');
console.log('─'.repeat(50));

const cookies = new CookieMap();

// Set cookies
cookies.set('session', 'xyz789');
cookies.set('theme', 'dark');
cookies.set({
  name: 'preferences',
  value: JSON.stringify({ fontSize: 14, compact: true }),
  maxAge: 86400,
});

console.log('After setting cookies:');
console.log(`  ${JSON.stringify(cookies.toJSON())}`);

// Check existence
console.log(`\nhas('session'): ${cookies.has('session')}`);
console.log(`has('missing'): ${cookies.has('missing')}`);

// Get values
console.log(`\nget('session'): ${cookies.get('session')}`);
console.log(`get('missing'): ${cookies.get('missing')}`);

// Delete cookie
cookies.delete('theme');
console.log(`\nAfter deleting 'theme': ${JSON.stringify(cookies.toJSON())}`);

// ============================================================================
// Set-Cookie Headers for HTTP Responses
// ============================================================================

console.log('\n6. Generating Set-Cookie Headers:');
console.log('─'.repeat(50));

const responseCookies = new CookieMap();

// Set new cookies
responseCookies.set('auth', 'token123', {
  httpOnly: true,
  secure: true,
  sameSite: 'strict',
  path: '/',
});

responseCookies.set('user_id', '42', {
  maxAge: 86400,
});

// Delete an old cookie
responseCookies.delete('old_session');
responseCookies.delete({ name: 'legacy_token', domain: 'old.example.com', path: '/v1' });

console.log('Set-Cookie headers to send:');
for (const header of responseCookies.toSetCookieHeaders()) {
  console.log(`  ${header}`);
}

// ============================================================================
// Iteration Methods
// ============================================================================

console.log('\n7. Iteration Methods:');
console.log('─'.repeat(50));

const iterCookies = new CookieMap({ a: '1', b: '2', c: '3' });

console.log('entries():');
for (const [name, value] of iterCookies.entries()) {
  console.log(`  ${name} = ${value}`);
}

console.log('\nkeys():');
console.log(`  [${[...iterCookies.keys()].join(', ')}]`);

console.log('\nvalues():');
console.log(`  [${[...iterCookies.values()].join(', ')}]`);

console.log('\nforEach():');
iterCookies.forEach((value, name) => {
  console.log(`  ${name}: ${value}`);
});

// ============================================================================
// JSON Serialization
// ============================================================================

console.log('\n8. JSON Serialization:');
console.log('─'.repeat(50));

const cookie = new Cookie('session', 'data', {
  secure: true,
  httpOnly: true,
  maxAge: 3600,
});

console.log('Cookie.toJSON():');
console.log(JSON.stringify(cookie.toJSON(), null, 2));

console.log('\nCookieMap.toJSON():');
const map = new CookieMap({ a: '1', b: '2' });
console.log(JSON.stringify(map.toJSON(), null, 2));

// ============================================================================
// Practical Example: Session Management
// ============================================================================

console.log('\n9. Practical Example - Session Management:');
console.log('─'.repeat(50));

function handleRequest(cookieHeader) {
  // Parse incoming cookies
  const cookies = new CookieMap(cookieHeader);

  // Check for existing session
  const sessionId = cookies.get('session_id');

  if (sessionId) {
    console.log(`  Existing session: ${sessionId}`);
    // Update last access time
    cookies.set('last_access', new Date().toISOString(), { maxAge: 3600 });
  } else {
    // Create new session
    const newSessionId = 'sess_' + Math.random().toString(36).slice(2);
    console.log(`  New session: ${newSessionId}`);
    cookies.set('session_id', newSessionId, {
      httpOnly: true,
      secure: true,
      sameSite: 'lax',
      maxAge: 86400 * 7, // 1 week
    });
  }

  // Return Set-Cookie headers for response
  return cookies.toSetCookieHeaders();
}

// Simulate requests
console.log('\nFirst request (no cookies):');
const firstHeaders = handleRequest('');
console.log('  Response headers:', firstHeaders);

console.log('\nSecond request (with session):');
const secondHeaders = handleRequest('session_id=sess_abc123');
console.log('  Response headers:', secondHeaders);

// ============================================================================
// Summary
// ============================================================================

console.log('\n=== API Summary ===\n');

console.log('Howth.Cookie:');
console.log('  new Cookie(name, value, options?)');
console.log('  new Cookie(cookieString)');
console.log('  new Cookie(options)');
console.log('  Cookie.parse(string)');
console.log('  Cookie.from(name, value, options?)');
console.log('  cookie.isExpired()');
console.log('  cookie.serialize() / toString()');
console.log('  cookie.toJSON()');

console.log('\nHowth.CookieMap:');
console.log('  new CookieMap(cookieHeader?)');
console.log('  new CookieMap({ name: value })');
console.log('  new CookieMap([[name, value], ...])');
console.log('  map.get(name), map.set(...), map.delete(...)');
console.log('  map.has(name), map.size');
console.log('  map.toSetCookieHeaders()');
console.log('  map.entries(), keys(), values(), forEach()');

console.log('\nCookie Options:');
console.log('  domain, path, expires, maxAge');
console.log('  secure, httpOnly, sameSite, partitioned');

console.log('\n✓ Cookie API example complete!');
