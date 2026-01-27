'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const testFile = path.join(tmpdir.path, 'test-appendfile.txt');

// Test appendFileSync creates new file
fs.writeFileSync(testFile, 'start');
fs.appendFileSync(testFile, ' middle');
fs.appendFileSync(testFile, ' end');
assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'start middle end');

// Test appendFileSync with Buffer
fs.writeFileSync(testFile, 'buf:');
fs.appendFileSync(testFile, Buffer.from('data'));
assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'buf:data');

// Test async appendFile
fs.writeFileSync(testFile, 'async:');
fs.appendFile(testFile, 'appended', (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'async:appended');

  // Test promises API
  fs.writeFileSync(testFile, 'promise:');
  fs.promises.appendFile(testFile, 'data').then(() => {
    assert.strictEqual(fs.readFileSync(testFile, 'utf8'), 'promise:data');
    console.log('All appendFile tests passed!');
  });
});
