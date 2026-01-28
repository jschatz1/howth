'use strict';
const assert = require('assert');
const url = require('url');

// Test url.parse
const parsed = url.parse('https://user:pass@example.com:8080/path?query=1#hash');
assert.strictEqual(parsed.protocol, 'https:');
assert.strictEqual(parsed.hostname, 'example.com');
assert.strictEqual(parsed.port, '8080');
assert.strictEqual(parsed.pathname, '/path');
assert.strictEqual(parsed.search, '?query=1');
assert.strictEqual(parsed.hash, '#hash');
assert.strictEqual(parsed.auth, 'user:pass');

// Test url.parse with parseQueryString
const parsedQuery = url.parse('https://example.com?foo=bar&baz=qux', true);
assert.deepStrictEqual(parsedQuery.query, { foo: 'bar', baz: 'qux' });

// Test url.format
const formatted = url.format({
  protocol: 'https:',
  hostname: 'example.com',
  port: '8080',
  pathname: '/path',
  search: '?query=1',
  hash: '#hash',
});
assert.ok(formatted.includes('https://'));
assert.ok(formatted.includes('example.com'));
assert.ok(formatted.includes(':8080'));
assert.ok(formatted.includes('/path'));
assert.ok(formatted.includes('?query=1'));
assert.ok(formatted.includes('#hash'));

// Test url.resolve
assert.strictEqual(
  url.resolve('https://example.com/a/b', '/c'),
  'https://example.com/c'
);
assert.strictEqual(
  url.resolve('https://example.com/a/b', 'c'),
  'https://example.com/a/c'
);
assert.strictEqual(
  url.resolve('https://example.com/a/b/', 'c'),
  'https://example.com/a/b/c'
);

// Test URL class is exported
assert.ok(url.URL);
assert.ok(url.URLSearchParams);

// Test pathToFileURL
const fileUrl = url.pathToFileURL('/path/to/file.txt');
assert.strictEqual(fileUrl.protocol, 'file:');
assert.strictEqual(fileUrl.pathname, '/path/to/file.txt');

// Test fileURLToPath
const filePath = url.fileURLToPath('file:///path/to/file.txt');
assert.strictEqual(filePath, '/path/to/file.txt');

// Test fileURLToPath with URL object
const filePath2 = url.fileURLToPath(new URL('file:///another/path.js'));
assert.strictEqual(filePath2, '/another/path.js');

console.log('All url module tests passed!');
