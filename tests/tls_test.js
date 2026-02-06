// Test TLS module
const tls = require('tls');

console.log('Testing TLS module...\n');

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

async function asyncTest(name, fn) {
  try {
    await fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

// Test module existence
test('tls module exists', () => {
  if (typeof tls !== 'object') throw new Error('tls is not an object');
});

test('tls.connect exists', () => {
  if (typeof tls.connect !== 'function') throw new Error('connect is not a function');
});

test('tls.TLSSocket exists', () => {
  if (typeof tls.TLSSocket !== 'function') throw new Error('TLSSocket is not a function');
});

test('tls.createServer exists', () => {
  if (typeof tls.createServer !== 'function') throw new Error('createServer is not a function');
});

test('tls.getCiphers returns array', () => {
  const ciphers = tls.getCiphers();
  if (!Array.isArray(ciphers)) throw new Error('getCiphers did not return array');
  if (ciphers.length === 0) throw new Error('getCiphers returned empty array');
});

test('tls constants exist', () => {
  if (!tls.DEFAULT_MIN_VERSION) throw new Error('missing DEFAULT_MIN_VERSION');
  if (!tls.DEFAULT_MAX_VERSION) throw new Error('missing DEFAULT_MAX_VERSION');
});

// Test real TLS connection
async function runAsyncTests() {
  await asyncTest('tls.connect to google.com:443', async () => {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Connection timeout'));
      }, 10000);

      const socket = tls.connect({
        host: 'google.com',
        port: 443,
        servername: 'google.com'
      });

      socket.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });

      socket.on('secureConnect', () => {
        clearTimeout(timeout);

        // Check connection properties
        if (!socket.encrypted) {
          reject(new Error('Socket not encrypted'));
          return;
        }
        if (!socket.authorized) {
          reject(new Error('Certificate not authorized'));
          return;
        }

        const protocol = socket.getProtocol();
        console.log(`  -> Protocol: ${protocol}`);

        const cipher = socket.getCipher();
        console.log(`  -> Cipher: ${cipher.name}`);

        socket.end();
        resolve();
      });
    });
  });

  await asyncTest('tls.connect and send HTTP request', async () => {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Request timeout'));
      }, 10000);

      const socket = tls.connect({
        host: 'httpbin.org',
        port: 443,
        servername: 'httpbin.org'
      });

      let response = '';

      socket.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });

      socket.on('secureConnect', () => {
        // Send HTTP GET request
        socket.write('GET /get HTTP/1.1\r\nHost: httpbin.org\r\nConnection: close\r\n\r\n');
      });

      socket.on('data', (chunk) => {
        response += chunk.toString();
      });

      socket.on('end', () => {
        clearTimeout(timeout);

        if (!response.includes('HTTP/1.1 200')) {
          reject(new Error(`Expected HTTP 200, got: ${response.substring(0, 50)}`));
          return;
        }

        console.log(`  -> Received ${response.length} bytes`);
        resolve();
      });

      socket.on('close', () => {
        clearTimeout(timeout);
        if (response.length === 0) {
          reject(new Error('No response received'));
        }
      });
    });
  });

  await asyncTest('TLSSocket has correct address info', async () => {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Connection timeout'));
      }, 10000);

      const socket = tls.connect({
        host: 'google.com',
        port: 443
      });

      socket.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });

      socket.on('secureConnect', () => {
        clearTimeout(timeout);

        const addr = socket.address();
        if (!addr.address) {
          reject(new Error('Missing local address'));
          return;
        }
        if (!addr.port) {
          reject(new Error('Missing local port'));
          return;
        }

        console.log(`  -> Local: ${addr.address}:${addr.port}`);
        console.log(`  -> Remote: ${socket.remoteAddress}:${socket.remotePort}`);

        socket.end();
        resolve();
      });
    });
  });

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  process.exit(failed > 0 ? 1 : 0);
}

runAsyncTests().catch(e => {
  console.error('Test error:', e);
  process.exit(1);
});
