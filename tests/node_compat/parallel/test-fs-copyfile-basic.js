'use strict';
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const tmpdir = require('../common/tmpdir');
tmpdir.refresh();

const srcFile = path.join(tmpdir.path, 'test-copyfile-src.txt');
const destFile = path.join(tmpdir.path, 'test-copyfile-dest.txt');

// Test copyFileSync
fs.writeFileSync(srcFile, 'copy content');
fs.copyFileSync(srcFile, destFile);
assert.strictEqual(fs.existsSync(destFile), true);
assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'copy content');
// Source should still exist
assert.strictEqual(fs.existsSync(srcFile), true);

// Clean up for next test
fs.unlinkSync(destFile);

// Test async copyFile
fs.copyFile(srcFile, destFile, (err) => {
  assert.strictEqual(err, null);
  assert.strictEqual(fs.existsSync(destFile), true);
  assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'copy content');

  // Clean up for next test
  fs.unlinkSync(destFile);

  // Test promises API
  fs.promises.copyFile(srcFile, destFile).then(() => {
    assert.strictEqual(fs.existsSync(destFile), true);
    assert.strictEqual(fs.readFileSync(destFile, 'utf8'), 'copy content');
    console.log('All copyFile tests passed!');
  });
});
