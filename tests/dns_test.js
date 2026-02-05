// Test DNS module
const dns = require('dns');
const dnsPromises = require('dns/promises');

console.log('Testing DNS module...\n');

let passed = 0;
let failed = 0;

async function test(name, fn) {
  try {
    await fn();
    console.log(`✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`✗ ${name}: ${e.message}`);
    failed++;
  }
}

async function runTests() {
  // Test dns.promises.lookup
  await test('dns.promises.lookup resolves google.com', async () => {
    const result = await dnsPromises.lookup('google.com');
    if (!result.address) throw new Error('No address returned');
    if (result.family !== 4 && result.family !== 6) throw new Error('Invalid family');
    console.log(`  -> ${result.address} (IPv${result.family})`);
  });

  // Test dns.promises.resolve4
  await test('dns.promises.resolve4 returns IPv4 addresses', async () => {
    const addresses = await dnsPromises.resolve4('google.com');
    if (!Array.isArray(addresses)) throw new Error('Expected array');
    if (addresses.length === 0) throw new Error('No addresses returned');
    console.log(`  -> ${addresses.join(', ')}`);
  });

  // Test dns.promises.resolve6
  await test('dns.promises.resolve6 returns IPv6 addresses', async () => {
    const addresses = await dnsPromises.resolve6('google.com');
    if (!Array.isArray(addresses)) throw new Error('Expected array');
    // IPv6 might not be available everywhere, so just check it's an array
    console.log(`  -> ${addresses.length} addresses`);
  });

  // Test dns.promises.resolveMx
  await test('dns.promises.resolveMx returns MX records', async () => {
    const records = await dnsPromises.resolveMx('google.com');
    if (!Array.isArray(records)) throw new Error('Expected array');
    if (records.length === 0) throw new Error('No MX records');
    if (!records[0].exchange) throw new Error('Missing exchange');
    if (typeof records[0].priority !== 'number') throw new Error('Missing priority');
    console.log(`  -> ${records[0].exchange} (priority ${records[0].priority})`);
  });

  // Test dns.promises.resolveTxt
  await test('dns.promises.resolveTxt returns TXT records', async () => {
    const records = await dnsPromises.resolveTxt('google.com');
    if (!Array.isArray(records)) throw new Error('Expected array');
    console.log(`  -> ${records.length} TXT records`);
  });

  // Test dns.promises.resolveNs
  await test('dns.promises.resolveNs returns NS records', async () => {
    const records = await dnsPromises.resolveNs('google.com');
    if (!Array.isArray(records)) throw new Error('Expected array');
    if (records.length === 0) throw new Error('No NS records');
    console.log(`  -> ${records.join(', ')}`);
  });

  // Test dns.promises.resolveSoa
  await test('dns.promises.resolveSoa returns SOA record', async () => {
    const record = await dnsPromises.resolveSoa('google.com');
    if (!record.nsname) throw new Error('Missing nsname');
    if (!record.hostmaster) throw new Error('Missing hostmaster');
    console.log(`  -> ${record.nsname}`);
  });

  // Test dns.promises.resolve with rrtype
  await test('dns.promises.resolve with MX rrtype', async () => {
    const records = await dnsPromises.resolve('google.com', 'MX');
    if (!Array.isArray(records)) throw new Error('Expected array');
    if (records.length === 0) throw new Error('No records');
  });

  // Test callback API
  await test('dns.lookup callback API', async () => {
    return new Promise((resolve, reject) => {
      dns.lookup('google.com', (err, address, family) => {
        if (err) return reject(err);
        if (!address) return reject(new Error('No address'));
        console.log(`  -> ${address} (IPv${family})`);
        resolve();
      });
    });
  });

  // Test dns.resolve4 callback API
  await test('dns.resolve4 callback API', async () => {
    return new Promise((resolve, reject) => {
      dns.resolve4('google.com', (err, addresses) => {
        if (err) return reject(err);
        if (!addresses || addresses.length === 0) return reject(new Error('No addresses'));
        console.log(`  -> ${addresses[0]}`);
        resolve();
      });
    });
  });

  // Test Resolver class
  await test('dns.Resolver class', async () => {
    const resolver = new dns.Resolver();
    return new Promise((resolve, reject) => {
      resolver.resolve4('google.com', (err, addresses) => {
        if (err) return reject(err);
        if (!addresses || addresses.length === 0) return reject(new Error('No addresses'));
        console.log(`  -> ${addresses[0]}`);
        resolve();
      });
    });
  });

  // Test error constants
  await test('DNS error constants exist', async () => {
    if (dns.NOTFOUND !== 'ENOTFOUND') throw new Error('Missing NOTFOUND');
    if (dns.SERVFAIL !== 'ESERVFAIL') throw new Error('Missing SERVFAIL');
  });

  // Test setServers/getServers
  await test('dns.setServers and getServers', async () => {
    const original = dns.getServers();
    dns.setServers(['1.1.1.1', '1.0.0.1']);
    const updated = dns.getServers();
    if (updated[0] !== '1.1.1.1') throw new Error('setServers did not work');
    dns.setServers(original); // restore
  });

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch(e => {
  console.error('Test error:', e);
  process.exit(1);
});
