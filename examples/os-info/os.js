/**
 * OS Information Example
 *
 * Demonstrates Node.js os module:
 * - System information
 * - CPU details
 * - Memory usage
 * - Network interfaces
 * - User info
 * - Platform detection
 *
 * Run: howth run --native examples/os-info/os.js
 */

const os = require('os');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}OS Information Demo${c.reset}\n`);

// Helper to format bytes
function formatBytes(bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (bytes >= 1024 && i < units.length - 1) {
    bytes /= 1024;
    i++;
  }
  return `${bytes.toFixed(2)} ${units[i]}`;
}

// Helper to format uptime
function formatUptime(seconds) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  const parts = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (mins > 0) parts.push(`${mins}m`);
  parts.push(`${secs}s`);

  return parts.join(' ');
}

// 1. Platform Information
console.log(`${c.bold}1. Platform Information${c.reset}`);
console.log(`  Platform:     ${os.platform()}`);
console.log(`  Architecture: ${os.arch()}`);
console.log(`  OS Type:      ${os.type()}`);
console.log(`  Release:      ${os.release()}`);
console.log(`  Version:      ${os.version ? os.version() : 'N/A'}`);
console.log(`  Endianness:   ${os.endianness()}`);

// 2. Hostname and Paths
console.log(`\n${c.bold}2. Host & Paths${c.reset}`);
console.log(`  Hostname:     ${os.hostname()}`);
console.log(`  Home Dir:     ${os.homedir()}`);
console.log(`  Temp Dir:     ${os.tmpdir()}`);
console.log(`  EOL:          ${JSON.stringify(os.EOL)}`);

// 3. CPU Information
console.log(`\n${c.bold}3. CPU Information${c.reset}`);
const cpus = os.cpus();
console.log(`  Model:        ${cpus[0]?.model || 'Unknown'}`);
console.log(`  Cores:        ${cpus.length}`);
console.log(`  Speed:        ${cpus[0]?.speed || 'Unknown'} MHz`);

// CPU times breakdown
if (cpus[0]?.times) {
  const total = cpus.reduce((acc, cpu) => {
    const t = cpu.times;
    return {
      user: acc.user + t.user,
      nice: acc.nice + t.nice,
      sys: acc.sys + t.sys,
      idle: acc.idle + t.idle,
      irq: acc.irq + t.irq,
    };
  }, { user: 0, nice: 0, sys: 0, idle: 0, irq: 0 });

  const sum = total.user + total.nice + total.sys + total.idle + total.irq;
  console.log(`  ${c.dim}Usage breakdown:${c.reset}`);
  console.log(`    User:  ${((total.user / sum) * 100).toFixed(1)}%`);
  console.log(`    System: ${((total.sys / sum) * 100).toFixed(1)}%`);
  console.log(`    Idle:  ${((total.idle / sum) * 100).toFixed(1)}%`);
}

// 4. Memory Information
console.log(`\n${c.bold}4. Memory Information${c.reset}`);
const totalMem = os.totalmem();
const freeMem = os.freemem();
const usedMem = totalMem - freeMem;
const usedPercent = ((usedMem / totalMem) * 100).toFixed(1);

console.log(`  Total:        ${formatBytes(totalMem)}`);
console.log(`  Used:         ${formatBytes(usedMem)} (${usedPercent}%)`);
console.log(`  Free:         ${formatBytes(freeMem)}`);

// Memory bar
const barWidth = 30;
const filledWidth = Math.round((usedMem / totalMem) * barWidth);
const bar = '█'.repeat(filledWidth) + '░'.repeat(barWidth - filledWidth);
console.log(`  [${c.green}${bar}${c.reset}]`);

// 5. System Uptime
console.log(`\n${c.bold}5. System Uptime${c.reset}`);
const uptime = os.uptime();
console.log(`  Uptime:       ${formatUptime(uptime)}`);
console.log(`  Seconds:      ${uptime.toLocaleString()}`);

// 6. Load Average
console.log(`\n${c.bold}6. Load Average${c.reset}`);
const loadAvg = os.loadavg();
console.log(`  1 min:        ${loadAvg[0].toFixed(2)}`);
console.log(`  5 min:        ${loadAvg[1].toFixed(2)}`);
console.log(`  15 min:       ${loadAvg[2].toFixed(2)}`);

// 7. User Info
console.log(`\n${c.bold}7. User Information${c.reset}`);
try {
  const userInfo = os.userInfo();
  console.log(`  Username:     ${userInfo.username}`);
  console.log(`  UID:          ${userInfo.uid}`);
  console.log(`  GID:          ${userInfo.gid}`);
  console.log(`  Shell:        ${userInfo.shell}`);
  console.log(`  Home:         ${userInfo.homedir}`);
} catch (e) {
  console.log(`  ${c.dim}(User info not available)${c.reset}`);
}

// 8. Network Interfaces
console.log(`\n${c.bold}8. Network Interfaces${c.reset}`);
const interfaces = os.networkInterfaces();
let interfaceCount = 0;

for (const [name, addrs] of Object.entries(interfaces)) {
  if (!addrs) continue;

  for (const addr of addrs) {
    if (addr.internal) continue; // Skip internal interfaces

    interfaceCount++;
    console.log(`  ${c.blue}${name}${c.reset}`);
    console.log(`    Family:   ${addr.family}`);
    console.log(`    Address:  ${addr.address}`);
    console.log(`    Netmask:  ${addr.netmask}`);
    if (addr.mac) console.log(`    MAC:      ${addr.mac}`);
  }
}

if (interfaceCount === 0) {
  console.log(`  ${c.dim}(No external network interfaces found)${c.reset}`);
}

// 9. Constants
console.log(`\n${c.bold}9. OS Constants${c.reset}`);
if (os.constants) {
  console.log(`  Signal constants: ${Object.keys(os.constants.signals || {}).length}`);
  console.log(`  Error constants:  ${Object.keys(os.constants.errno || {}).length}`);

  // Show a few common signals
  const signals = os.constants.signals || {};
  const commonSignals = ['SIGINT', 'SIGTERM', 'SIGKILL', 'SIGHUP'];
  console.log(`  ${c.dim}Common signals:${c.reset}`);
  for (const sig of commonSignals) {
    if (signals[sig]) {
      console.log(`    ${sig}: ${signals[sig]}`);
    }
  }
} else {
  console.log(`  ${c.dim}(Constants not available)${c.reset}`);
}

// 10. Priority (nice value)
console.log(`\n${c.bold}10. Process Priority${c.reset}`);
if (os.getPriority && os.setPriority) {
  try {
    const priority = os.getPriority();
    console.log(`  Current priority: ${priority}`);
    console.log(`  ${c.dim}(0 = normal, negative = higher, positive = lower)${c.reset}`);
  } catch (e) {
    console.log(`  ${c.dim}(Priority not available: ${e.message})${c.reset}`);
  }
} else {
  console.log(`  ${c.dim}(Priority functions not available)${c.reset}`);
}

// 11. System Summary
console.log(`\n${c.bold}11. System Summary${c.reset}`);
console.log(`  ┌─────────────────────────────────────────┐`);
console.log(`  │ ${c.bold}${os.hostname().padEnd(39)}${c.reset} │`);
console.log(`  ├─────────────────────────────────────────┤`);
console.log(`  │ OS: ${(os.type() + ' ' + os.release()).padEnd(35)} │`);
console.log(`  │ Arch: ${os.arch().padEnd(33)} │`);
console.log(`  │ CPUs: ${(cpus.length + ' cores').padEnd(33)} │`);
console.log(`  │ Memory: ${(formatBytes(usedMem) + ' / ' + formatBytes(totalMem)).padEnd(31)} │`);
console.log(`  │ Uptime: ${formatUptime(uptime).padEnd(31)} │`);
console.log(`  └─────────────────────────────────────────┘`);

console.log(`\n${c.green}${c.bold}OS information demo completed!${c.reset}\n`);
