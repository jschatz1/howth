/**
 * Vite Compatibility Test Utilities
 *
 * Helpers for starting/stopping dev servers and making assertions
 */

import { spawn } from 'node:child_process';
import { setTimeout } from 'node:timers/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const FIXTURE_DIR = path.resolve(__dirname, '../fixtures/basic-app');
export const VITE_PORT = 5173;
export const HOWTH_PORT = 5174;

/**
 * Start a dev server (vite or howth)
 */
export async function startServer(type, options = {}) {
  const { port = type === 'vite' ? VITE_PORT : HOWTH_PORT, cwd = FIXTURE_DIR } = options;

  let cmd, args;
  if (type === 'vite') {
    cmd = 'npx';
    args = ['vite', '--port', String(port), '--strictPort'];
  } else if (type === 'howth') {
    cmd = process.env.HOWTH_BIN || 'howth';
    args = ['dev', 'src/main.tsx', '--port', String(port)];
  } else {
    throw new Error(`Unknown server type: ${type}`);
  }

  const proc = spawn(cmd, args, {
    cwd,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, FORCE_COLOR: '0' },
  });

  let stdout = '';
  let stderr = '';

  proc.stdout.on('data', (data) => {
    stdout += data.toString();
  });

  proc.stderr.on('data', (data) => {
    stderr += data.toString();
  });

  // Wait for server to be ready
  const startTime = Date.now();
  const timeout = 30000;

  while (Date.now() - startTime < timeout) {
    try {
      await fetch(`http://localhost:${port}/`);
      break;
    } catch {
      await setTimeout(100);
    }
  }

  // Verify server is actually running
  try {
    const res = await fetch(`http://localhost:${port}/`);
    if (!res.ok) {
      throw new Error(`Server returned ${res.status}`);
    }
  } catch (err) {
    proc.kill();
    throw new Error(`Failed to start ${type} server: ${err.message}\nstdout: ${stdout}\nstderr: ${stderr}`);
  }

  return {
    type,
    port,
    proc,
    stdout: () => stdout,
    stderr: () => stderr,
    async stop() {
      proc.kill('SIGTERM');
      await setTimeout(500);
      if (!proc.killed) {
        proc.kill('SIGKILL');
      }
    },
    async fetch(urlPath, options = {}) {
      const url = `http://localhost:${port}${urlPath}`;
      return fetch(url, options);
    },
  };
}

/**
 * Fetch a URL and return text content
 */
export async function fetchText(server, urlPath) {
  const res = await server.fetch(urlPath);
  return {
    status: res.status,
    contentType: res.headers.get('content-type'),
    text: await res.text(),
  };
}

/**
 * Compare responses from two servers
 */
export function compareResponses(viteRes, howthRes, options = {}) {
  const { ignoreWhitespace = false, ignoreComments = false } = options;

  const result = {
    statusMatch: viteRes.status === howthRes.status,
    contentTypeMatch: normalizeContentType(viteRes.contentType) === normalizeContentType(howthRes.contentType),
    vite: viteRes,
    howth: howthRes,
  };

  let viteText = viteRes.text;
  let howthText = howthRes.text;

  if (ignoreWhitespace) {
    viteText = normalizeWhitespace(viteText);
    howthText = normalizeWhitespace(howthText);
  }

  if (ignoreComments) {
    viteText = removeComments(viteText);
    howthText = removeComments(howthText);
  }

  result.textMatch = viteText === howthText;

  return result;
}

function normalizeContentType(ct) {
  if (!ct) return '';
  return ct.split(';')[0].trim().toLowerCase();
}

function normalizeWhitespace(text) {
  return text.replace(/\s+/g, ' ').trim();
}

function removeComments(text) {
  // Remove single-line comments
  text = text.replace(/\/\/[^\n]*/g, '');
  // Remove multi-line comments
  text = text.replace(/\/\*[\s\S]*?\*\//g, '');
  return text;
}

/**
 * Extract import statements from JS code
 */
export function extractImports(code) {
  const imports = [];

  // Static imports: import X from 'Y'
  const staticImportRe = /import\s+(?:[\w*{}\s,]+\s+from\s+)?['"]([^'"]+)['"]/g;
  let match;
  while ((match = staticImportRe.exec(code)) !== null) {
    imports.push({ type: 'static', specifier: match[1] });
  }

  // Dynamic imports: import('X')
  const dynamicImportRe = /import\s*\(\s*['"]([^'"]+)['"]\s*\)/g;
  while ((match = dynamicImportRe.exec(code)) !== null) {
    imports.push({ type: 'dynamic', specifier: match[1] });
  }

  // Re-exports: export X from 'Y'
  const reExportRe = /export\s+(?:[\w*{}\s,]+\s+)?from\s+['"]([^'"]+)['"]/g;
  while ((match = reExportRe.exec(code)) !== null) {
    imports.push({ type: 'reexport', specifier: match[1] });
  }

  return imports;
}

/**
 * Check if imports are correctly rewritten
 */
export function validateImportRewriting(code, options = {}) {
  const imports = extractImports(code);
  const issues = [];

  for (const imp of imports) {
    const spec = imp.specifier;

    // Bare specifiers should be rewritten to /@modules/
    if (!spec.startsWith('.') && !spec.startsWith('/') && !spec.startsWith('@')) {
      if (!spec.startsWith('/@modules/')) {
        issues.push(`Bare specifier not rewritten: ${spec}`);
      }
    }

    // Scoped packages (@scope/pkg) should be rewritten to /@modules/@scope/pkg
    if (spec.startsWith('@') && !spec.startsWith('/@') && !spec.startsWith('@/')) {
      if (!spec.startsWith('/@modules/@')) {
        issues.push(`Scoped package not rewritten: ${spec}`);
      }
    }

    // Relative imports should be absolute
    if (spec.startsWith('./') || spec.startsWith('../')) {
      issues.push(`Relative import not rewritten to absolute: ${spec}`);
    }
  }

  return { valid: issues.length === 0, issues, imports };
}

/**
 * Check if CSS is properly converted to JS module
 */
export function validateCssModule(code) {
  const checks = {
    hasStyleInjection: code.includes('document.head.appendChild') ||
                        code.includes('createElement("style")') ||
                        code.includes('insertBefore'),
    hasHmrSupport: code.includes('import.meta.hot'),
    hasCssContent: code.includes('.app') || code.includes('.button'),
  };

  return {
    valid: checks.hasStyleInjection && checks.hasCssContent,
    checks,
  };
}

/**
 * WebSocket client for HMR testing
 */
export class HmrClient {
  constructor(port) {
    this.port = port;
    this.messages = [];
    this.ws = null;
    this.connected = false;
  }

  async connect() {
    return new Promise((resolve, reject) => {
      const url = `ws://localhost:${this.port}/__hmr`;
      this.ws = new WebSocket(url);

      this.ws.onopen = () => {
        this.connected = true;
        resolve();
      };

      this.ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          this.messages.push(data);
        } catch {
          this.messages.push({ raw: event.data });
        }
      };

      this.ws.onerror = (err) => {
        reject(err);
      };

      this.ws.onclose = () => {
        this.connected = false;
      };
    });
  }

  async waitForMessage(type, timeout = 5000) {
    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      const msg = this.messages.find(m => m.type === type);
      if (msg) return msg;
      await setTimeout(50);
    }
    throw new Error(`Timeout waiting for HMR message: ${type}`);
  }

  close() {
    if (this.ws) {
      this.ws.close();
    }
  }
}

/**
 * Wait for condition with timeout
 */
export async function waitFor(condition, timeout = 5000, interval = 100) {
  const startTime = Date.now();
  while (Date.now() - startTime < timeout) {
    if (await condition()) {
      return true;
    }
    await setTimeout(interval);
  }
  return false;
}
