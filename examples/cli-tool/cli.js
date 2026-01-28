#!/usr/bin/env howth run --native
/**
 * CLI Tool Example
 *
 * Demonstrates:
 * - Command-line argument parsing
 * - process.argv handling
 * - File system operations
 * - Colored output (ANSI codes)
 * - Exit codes
 *
 * Run: howth run --native examples/cli-tool/cli.js --help
 *      howth run --native examples/cli-tool/cli.js count ./src
 *      howth run --native examples/cli-tool/cli.js search "TODO" ./src
 */

const fs = require('fs');
const path = require('path');

// ANSI colors
const colors = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

function log(color, ...args) {
  console.log(color + args.join(' ') + colors.reset);
}

// Parse arguments
const args = process.argv.slice(2);
const command = args[0];

function showHelp() {
  console.log(`
${colors.bold}howth-cli${colors.reset} - Example CLI tool

${colors.cyan}USAGE:${colors.reset}
  howth run --native cli.js <command> [options]

${colors.cyan}COMMANDS:${colors.reset}
  ${colors.green}count${colors.reset} <dir>              Count files and lines in directory
  ${colors.green}search${colors.reset} <pattern> <dir>   Search for pattern in files
  ${colors.green}tree${colors.reset} <dir>               Display directory tree
  ${colors.green}stats${colors.reset} <file>             Show file statistics
  ${colors.green}help${colors.reset}                     Show this help message

${colors.cyan}EXAMPLES:${colors.reset}
  cli.js count ./src
  cli.js search "TODO" ./src
  cli.js tree ./examples
  cli.js stats package.json
`);
}

function countFiles(dir, stats = { files: 0, dirs: 0, lines: 0, bytes: 0 }) {
  try {
    const entries = fs.readdirSync(dir);
    for (const entry of entries) {
      if (entry.startsWith('.') || entry === 'node_modules') continue;

      const fullPath = path.join(dir, entry);
      const stat = fs.statSync(fullPath);

      if (stat.isDirectory()) {
        stats.dirs++;
        countFiles(fullPath, stats);
      } else if (stat.isFile()) {
        stats.files++;
        stats.bytes += stat.size;

        // Count lines for text files
        if (/\.(js|ts|json|md|txt|rs|toml)$/.test(entry)) {
          try {
            const content = fs.readFileSync(fullPath, 'utf8');
            stats.lines += content.split('\n').length;
          } catch (e) {}
        }
      }
    }
  } catch (e) {
    log(colors.red, `Error reading ${dir}: ${e.message}`);
  }
  return stats;
}

function searchFiles(pattern, dir, results = []) {
  try {
    const regex = new RegExp(pattern, 'gi');
    const entries = fs.readdirSync(dir);

    for (const entry of entries) {
      if (entry.startsWith('.') || entry === 'node_modules') continue;

      const fullPath = path.join(dir, entry);
      const stat = fs.statSync(fullPath);

      if (stat.isDirectory()) {
        searchFiles(pattern, fullPath, results);
      } else if (stat.isFile() && /\.(js|ts|json|md|txt|rs|toml)$/.test(entry)) {
        try {
          const content = fs.readFileSync(fullPath, 'utf8');
          const lines = content.split('\n');
          lines.forEach((line, i) => {
            if (regex.test(line)) {
              results.push({ file: fullPath, line: i + 1, content: line.trim() });
            }
          });
        } catch (e) {}
      }
    }
  } catch (e) {
    log(colors.red, `Error searching ${dir}: ${e.message}`);
  }
  return results;
}

function showTree(dir, prefix = '', isLast = true) {
  const name = path.basename(dir);
  const connector = isLast ? '└── ' : '├── ';
  console.log(prefix + connector + colors.blue + name + colors.reset);

  try {
    const entries = fs.readdirSync(dir).filter(e => !e.startsWith('.') && e !== 'node_modules');
    entries.forEach((entry, i) => {
      const fullPath = path.join(dir, entry);
      const stat = fs.statSync(fullPath);
      const newPrefix = prefix + (isLast ? '    ' : '│   ');

      if (stat.isDirectory()) {
        showTree(fullPath, newPrefix, i === entries.length - 1);
      } else {
        const fileConnector = i === entries.length - 1 ? '└── ' : '├── ';
        console.log(newPrefix + fileConnector + entry);
      }
    });
  } catch (e) {}
}

function showStats(file) {
  try {
    const stat = fs.statSync(file);
    const content = fs.readFileSync(file, 'utf8');

    console.log(`\n${colors.bold}File: ${file}${colors.reset}\n`);
    console.log(`  Size:       ${stat.size} bytes`);
    console.log(`  Lines:      ${content.split('\n').length}`);
    console.log(`  Characters: ${content.length}`);
    console.log(`  Words:      ${content.split(/\s+/).filter(Boolean).length}`);
    console.log(`  Modified:   ${new Date(stat.mtime).toISOString()}`);

    if (file.endsWith('.json')) {
      try {
        const json = JSON.parse(content);
        console.log(`  JSON keys:  ${Object.keys(json).join(', ')}`);
      } catch (e) {}
    }
  } catch (e) {
    log(colors.red, `Error: ${e.message}`);
    process.exit(1);
  }
}

// Main
switch (command) {
  case 'count': {
    const dir = args[1] || '.';
    log(colors.cyan, `\nCounting files in ${dir}...`);
    const stats = countFiles(dir);
    console.log(`\n  ${colors.green}Files:${colors.reset}       ${stats.files}`);
    console.log(`  ${colors.green}Directories:${colors.reset} ${stats.dirs}`);
    console.log(`  ${colors.green}Lines:${colors.reset}       ${stats.lines}`);
    console.log(`  ${colors.green}Size:${colors.reset}        ${(stats.bytes / 1024).toFixed(2)} KB\n`);
    break;
  }

  case 'search': {
    const pattern = args[1];
    const dir = args[2] || '.';
    if (!pattern) {
      log(colors.red, 'Error: Pattern required');
      process.exit(1);
    }
    log(colors.cyan, `\nSearching for "${pattern}" in ${dir}...`);
    const results = searchFiles(pattern, dir);
    console.log(`\nFound ${colors.yellow}${results.length}${colors.reset} matches:\n`);
    results.slice(0, 20).forEach(r => {
      console.log(`  ${colors.blue}${r.file}${colors.reset}:${colors.yellow}${r.line}${colors.reset}`);
      console.log(`    ${r.content.substring(0, 80)}`);
    });
    if (results.length > 20) {
      console.log(`\n  ... and ${results.length - 20} more matches`);
    }
    break;
  }

  case 'tree': {
    const dir = args[1] || '.';
    console.log();
    showTree(dir);
    console.log();
    break;
  }

  case 'stats': {
    const file = args[1];
    if (!file) {
      log(colors.red, 'Error: File path required');
      process.exit(1);
    }
    showStats(file);
    break;
  }

  case 'help':
  case '--help':
  case '-h':
  case undefined:
    showHelp();
    break;

  default:
    log(colors.red, `Unknown command: ${command}`);
    showHelp();
    process.exit(1);
}
