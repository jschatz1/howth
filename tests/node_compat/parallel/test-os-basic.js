'use strict';
const assert = require('assert');
const os = require('os');

// Test arch
const arch = os.arch();
assert.strictEqual(typeof arch, 'string');
assert.ok(['x64', 'arm64', 'ia32', 'arm'].includes(arch));

// Test platform
const platform = os.platform();
assert.strictEqual(typeof platform, 'string');
assert.ok(['darwin', 'linux', 'win32', 'freebsd'].includes(platform));

// Test type
const type = os.type();
assert.strictEqual(typeof type, 'string');
assert.ok(['Darwin', 'Linux', 'Windows_NT', 'FreeBSD', 'Unknown'].includes(type));

// Test endianness
const endianness = os.endianness();
assert.ok(endianness === 'LE' || endianness === 'BE');

// Test EOL
assert.ok(os.EOL === '\n' || os.EOL === '\r\n');

// Test homedir
const home = os.homedir();
assert.strictEqual(typeof home, 'string');
assert.ok(home.length > 0);

// Test tmpdir
const tmp = os.tmpdir();
assert.strictEqual(typeof tmp, 'string');
assert.ok(tmp.length > 0);

// Test hostname
const hostname = os.hostname();
assert.strictEqual(typeof hostname, 'string');

// Test cpus
const cpus = os.cpus();
assert.ok(Array.isArray(cpus));
assert.ok(cpus.length > 0);
assert.strictEqual(typeof cpus[0].model, 'string');
assert.strictEqual(typeof cpus[0].speed, 'number');
assert.ok(cpus[0].times);

// Test freemem and totalmem
const freemem = os.freemem();
const totalmem = os.totalmem();
assert.strictEqual(typeof freemem, 'number');
assert.strictEqual(typeof totalmem, 'number');
assert.ok(freemem > 0);
assert.ok(totalmem > 0);

// Test loadavg
const loadavg = os.loadavg();
assert.ok(Array.isArray(loadavg));
assert.strictEqual(loadavg.length, 3);

// Test networkInterfaces
const netifs = os.networkInterfaces();
assert.strictEqual(typeof netifs, 'object');

// Test uptime
const uptime = os.uptime();
assert.strictEqual(typeof uptime, 'number');

// Test userInfo
const userInfo = os.userInfo();
assert.strictEqual(typeof userInfo, 'object');
assert.strictEqual(typeof userInfo.username, 'string');
assert.strictEqual(typeof userInfo.homedir, 'string');

// Test constants
assert.ok(os.constants);
assert.ok(os.constants.signals);
assert.ok(os.constants.errno);
assert.strictEqual(os.constants.signals.SIGKILL, 9);
assert.strictEqual(os.constants.errno.ENOENT, 2);

// Test devNull
assert.ok(os.devNull === '/dev/null' || os.devNull === '\\\\.\\nul');

// Test machine
const machine = os.machine();
assert.strictEqual(typeof machine, 'string');

// Test release
const release = os.release();
assert.strictEqual(typeof release, 'string');

// Test version
const version = os.version();
assert.strictEqual(typeof version, 'string');

console.log('All os module tests passed!');
