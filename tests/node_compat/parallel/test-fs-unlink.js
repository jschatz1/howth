'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const testFile = path.join(tmpdir.path, 'test-unlink.txt');

// Test unlinkSync
fs.writeFileSync(testFile, 'to delete');
assert.strictEqual(fs.existsSync(testFile), true);
fs.unlinkSync(testFile);
assert.strictEqual(fs.existsSync(testFile), false);

// Test async unlink
fs.writeFileSync(testFile, 'to delete async');
assert.strictEqual(fs.existsSync(testFile), true);
fs.unlink(testFile, (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.existsSync(testFile), false);

  // Test promises API
  fs.writeFileSync(testFile, 'to delete promises');
  assert.strictEqual(fs.existsSync(testFile), true);
  fs.promises.unlink(testFile).then(() => {
    assert.strictEqual(fs.existsSync(testFile), false);
    console.log('All unlink tests passed!');
  });
});
