// Standalone tmpdir module for Node.js test compatibility
'use strict';

const fs = require('fs');
const path = require('path');

const tmpDir = process.env.TMPDIR || process.env.TMP || '/tmp';

const tmpdir = {
  path: path.join(tmpDir, `howth-test-${process.pid}`),
  refresh() {
    try {
      fs.rmdirSync(this.path, { recursive: true });
    } catch (e) {
      // Ignore
    }
    fs.mkdirSync(this.path, { recursive: true });
    return this.path;
  },
  resolve(...args) {
    return path.resolve(this.path, ...args);
  },
  hasEnoughSpace() {
    return true;
  },
};

module.exports = tmpdir;
