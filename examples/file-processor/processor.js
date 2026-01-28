/**
 * File Processor Example
 *
 * Demonstrates:
 * - Recursive directory traversal
 * - File reading/writing
 * - Stream processing concepts
 * - Transformations (minify, format, etc.)
 * - Progress reporting
 *
 * Run: howth run --native examples/file-processor/processor.js <command> <dir>
 *
 * Commands:
 *   analyze  - Analyze codebase statistics
 *   minify   - Minify JSON files (remove whitespace)
 *   todos    - Extract TODO/FIXME comments
 *   unused   - Find potentially unused exports
 */

const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
const command = args[0];
const targetDir = args[1] || '.';

// Colors
const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

function walk(dir, callback, options = {}) {
  const { extensions = [], exclude = ['node_modules', '.git', 'dist', 'build'] } = options;

  try {
    const entries = fs.readdirSync(dir);
    for (const entry of entries) {
      if (exclude.includes(entry) || entry.startsWith('.')) continue;

      const fullPath = path.join(dir, entry);
      const stat = fs.statSync(fullPath);

      if (stat.isDirectory()) {
        walk(fullPath, callback, options);
      } else if (stat.isFile()) {
        const ext = path.extname(entry);
        if (extensions.length === 0 || extensions.includes(ext)) {
          callback(fullPath, stat);
        }
      }
    }
  } catch (e) {
    console.error(`${c.red}Error reading ${dir}: ${e.message}${c.reset}`);
  }
}

// Command: analyze
function analyzeCodebase(dir) {
  console.log(`\n${c.bold}Analyzing codebase: ${dir}${c.reset}\n`);

  const stats = {
    byExtension: {},
    totalFiles: 0,
    totalLines: 0,
    totalSize: 0,
    largest: { path: '', size: 0 },
    mostLines: { path: '', lines: 0 },
  };

  walk(dir, (filePath, stat) => {
    const ext = path.extname(filePath) || '(no ext)';

    if (!stats.byExtension[ext]) {
      stats.byExtension[ext] = { files: 0, lines: 0, size: 0 };
    }

    stats.byExtension[ext].files++;
    stats.byExtension[ext].size += stat.size;
    stats.totalFiles++;
    stats.totalSize += stat.size;

    if (stat.size > stats.largest.size) {
      stats.largest = { path: filePath, size: stat.size };
    }

    // Count lines for text files
    if (/\.(js|ts|jsx|tsx|json|md|txt|css|html|rs|py|go|toml|yaml|yml)$/.test(ext)) {
      try {
        const content = fs.readFileSync(filePath, 'utf8');
        const lines = content.split('\n').length;
        stats.byExtension[ext].lines += lines;
        stats.totalLines += lines;

        if (lines > stats.mostLines.lines) {
          stats.mostLines = { path: filePath, lines };
        }
      } catch (e) {}
    }
  });

  // Print results
  console.log(`${c.cyan}Files by extension:${c.reset}`);
  const sorted = Object.entries(stats.byExtension)
    .sort((a, b) => b[1].files - a[1].files);

  for (const [ext, data] of sorted) {
    const size = (data.size / 1024).toFixed(1);
    console.log(`  ${c.blue}${ext.padEnd(12)}${c.reset} ${String(data.files).padStart(5)} files  ${String(data.lines).padStart(7)} lines  ${size.padStart(8)} KB`);
  }

  console.log(`\n${c.cyan}Summary:${c.reset}`);
  console.log(`  Total files:  ${stats.totalFiles}`);
  console.log(`  Total lines:  ${stats.totalLines}`);
  console.log(`  Total size:   ${(stats.totalSize / 1024).toFixed(1)} KB`);
  console.log(`\n${c.cyan}Notable files:${c.reset}`);
  console.log(`  Largest:      ${stats.largest.path} (${(stats.largest.size / 1024).toFixed(1)} KB)`);
  console.log(`  Most lines:   ${stats.mostLines.path} (${stats.mostLines.lines} lines)`);
}

// Command: minify
function minifyJsonFiles(dir) {
  console.log(`\n${c.bold}Minifying JSON files in: ${dir}${c.reset}\n`);

  let processed = 0;
  let savedBytes = 0;

  walk(dir, (filePath) => {
    try {
      const original = fs.readFileSync(filePath, 'utf8');
      const json = JSON.parse(original);
      const minified = JSON.stringify(json);

      if (minified.length < original.length) {
        const saved = original.length - minified.length;
        savedBytes += saved;

        // Create backup
        fs.writeFileSync(filePath + '.backup', original);
        fs.writeFileSync(filePath, minified);

        console.log(`  ${c.green}✓${c.reset} ${filePath} (saved ${saved} bytes)`);
        processed++;
      } else {
        console.log(`  ${c.dim}○ ${filePath} (already minified)${c.reset}`);
      }
    } catch (e) {
      console.log(`  ${c.red}✗ ${filePath}: ${e.message}${c.reset}`);
    }
  }, { extensions: ['.json'] });

  console.log(`\n${c.cyan}Summary:${c.reset}`);
  console.log(`  Files processed: ${processed}`);
  console.log(`  Bytes saved:     ${savedBytes}`);
}

// Command: todos
function extractTodos(dir) {
  console.log(`\n${c.bold}Extracting TODOs from: ${dir}${c.reset}\n`);

  const todos = [];
  const patterns = [
    /\/\/\s*(TODO|FIXME|HACK|XXX|BUG)[\s:]+(.+)/gi,
    /\/\*\s*(TODO|FIXME|HACK|XXX|BUG)[\s:]+(.+?)\*\//gi,
    /#\s*(TODO|FIXME|HACK|XXX|BUG)[\s:]+(.+)/gi,
  ];

  walk(dir, (filePath) => {
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      const lines = content.split('\n');

      lines.forEach((line, i) => {
        for (const pattern of patterns) {
          pattern.lastIndex = 0;
          let match;
          while ((match = pattern.exec(line)) !== null) {
            todos.push({
              type: match[1].toUpperCase(),
              message: match[2].trim(),
              file: filePath,
              line: i + 1,
            });
          }
        }
      });
    } catch (e) {}
  }, { extensions: ['.js', '.ts', '.jsx', '.tsx', '.rs', '.py', '.go'] });

  // Group by type
  const byType = {};
  for (const todo of todos) {
    if (!byType[todo.type]) byType[todo.type] = [];
    byType[todo.type].push(todo);
  }

  for (const [type, items] of Object.entries(byType)) {
    const color = type === 'FIXME' || type === 'BUG' ? c.red : type === 'TODO' ? c.yellow : c.cyan;
    console.log(`${color}${c.bold}${type} (${items.length})${c.reset}`);

    for (const item of items.slice(0, 10)) {
      console.log(`  ${c.dim}${item.file}:${item.line}${c.reset}`);
      console.log(`    ${item.message}`);
    }
    if (items.length > 10) {
      console.log(`  ${c.dim}... and ${items.length - 10} more${c.reset}`);
    }
    console.log();
  }

  console.log(`${c.cyan}Total: ${todos.length} items${c.reset}`);
}

// Command: unused (find potentially unused exports)
function findUnusedExports(dir) {
  console.log(`\n${c.bold}Finding potentially unused exports in: ${dir}${c.reset}\n`);

  const exports = new Map(); // name -> { file, line, used: false }
  const imports = new Set();

  // First pass: collect exports
  walk(dir, (filePath) => {
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      const lines = content.split('\n');

      lines.forEach((line, i) => {
        // export const/let/function/class name
        const exportMatch = line.match(/export\s+(?:const|let|var|function|class)\s+(\w+)/);
        if (exportMatch) {
          exports.set(exportMatch[1], { file: filePath, line: i + 1, used: false });
        }

        // export { name, name2 }
        const namedExports = line.match(/export\s*\{([^}]+)\}/);
        if (namedExports) {
          namedExports[1].split(',').forEach(name => {
            const cleanName = name.trim().split(/\s+as\s+/)[0].trim();
            if (cleanName) {
              exports.set(cleanName, { file: filePath, line: i + 1, used: false });
            }
          });
        }
      });
    } catch (e) {}
  }, { extensions: ['.js', '.ts', '.jsx', '.tsx'] });

  // Second pass: collect imports
  walk(dir, (filePath) => {
    try {
      const content = fs.readFileSync(filePath, 'utf8');

      // import { name } from
      const importMatches = content.matchAll(/import\s*\{([^}]+)\}\s*from/g);
      for (const match of importMatches) {
        match[1].split(',').forEach(name => {
          const cleanName = name.trim().split(/\s+as\s+/)[0].trim();
          imports.add(cleanName);
        });
      }

      // import name from (default imports)
      const defaultImports = content.matchAll(/import\s+(\w+)\s+from/g);
      for (const match of defaultImports) {
        imports.add(match[1]);
      }
    } catch (e) {}
  }, { extensions: ['.js', '.ts', '.jsx', '.tsx'] });

  // Mark used exports
  for (const name of imports) {
    if (exports.has(name)) {
      exports.get(name).used = true;
    }
  }

  // Report unused
  const unused = [...exports.entries()].filter(([, data]) => !data.used);

  if (unused.length === 0) {
    console.log(`${c.green}No potentially unused exports found!${c.reset}`);
  } else {
    console.log(`${c.yellow}Potentially unused exports (${unused.length}):${c.reset}\n`);

    for (const [name, data] of unused.slice(0, 20)) {
      console.log(`  ${c.red}${name}${c.reset}`);
      console.log(`    ${c.dim}${data.file}:${data.line}${c.reset}`);
    }

    if (unused.length > 20) {
      console.log(`\n  ${c.dim}... and ${unused.length - 20} more${c.reset}`);
    }
  }
}

// Main
function showHelp() {
  console.log(`
${c.bold}file-processor${c.reset} - Codebase analysis and transformation tool

${c.cyan}USAGE:${c.reset}
  howth run --native processor.js <command> [directory]

${c.cyan}COMMANDS:${c.reset}
  ${c.green}analyze${c.reset}    Analyze codebase statistics
  ${c.green}minify${c.reset}     Minify JSON files
  ${c.green}todos${c.reset}      Extract TODO/FIXME comments
  ${c.green}unused${c.reset}     Find potentially unused exports

${c.cyan}EXAMPLES:${c.reset}
  processor.js analyze ./src
  processor.js todos .
  processor.js minify ./config
`);
}

switch (command) {
  case 'analyze':
    analyzeCodebase(targetDir);
    break;
  case 'minify':
    minifyJsonFiles(targetDir);
    break;
  case 'todos':
    extractTodos(targetDir);
    break;
  case 'unused':
    findUnusedExports(targetDir);
    break;
  case 'help':
  case '--help':
  case '-h':
  case undefined:
    showHelp();
    break;
  default:
    console.log(`${c.red}Unknown command: ${command}${c.reset}`);
    showHelp();
    process.exit(1);
}
