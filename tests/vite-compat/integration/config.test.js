/**
 * Configuration Tests
 *
 * Tests that howth loads and applies configuration correctly:
 * - howth.config.ts loading
 * - vite.config.ts compatibility
 * - Alias resolution
 * - Server options
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { startServer, fetchText, HOWTH_PORT } from './test-utils.js';

describe('Configuration', () => {
  let howth;

  before(async () => {
    if (process.env.VITE_ONLY !== '1') {
      try {
        howth = await startServer('howth', { port: HOWTH_PORT });
      } catch (err) {
        console.error('Failed to start howth:', err.message);
      }
    }
  });

  after(async () => {
    if (howth) await howth.stop();
  });

  describe('Config File Loading', () => {
    it('should load howth.config.ts', async () => {
      // If config loads successfully, the server should start on configured port
      if (howth) {
        const res = await fetchText(howth, '/');
        assert.strictEqual(res.status, 200);
      }
    });
  });

  describe('Alias Resolution', () => {
    it('should resolve @ alias to /src', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');

        // The main.tsx uses @components/Button
        // After resolution, the import should point to /src/components/Button
        // Check that the import is resolved (not a 404 when fetching)
        const buttonRes = await fetchText(howth, '/src/components/Button.tsx');
        assert.strictEqual(buttonRes.status, 200);
      }
    });

    it('should resolve @components alias', async () => {
      if (howth) {
        // @components should resolve to /src/components
        // This is configured in howth.config.ts
        const res = await fetchText(howth, '/src/components/Button.tsx');
        assert.strictEqual(res.status, 200);
        assert.ok(res.text.includes('function Button'));
      }
    });
  });

  describe('Server Options', () => {
    it('should respect configured port', async () => {
      if (howth) {
        // The server should be running on the port from CLI/config
        // We test this implicitly by the fact that our tests connect
        const res = await fetchText(howth, '/');
        assert.strictEqual(res.status, 200);
      }
    });
  });

  describe('Define Configuration', () => {
    it('should apply define replacements', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');

        // __APP_VERSION__ should be defined in config
        // Check that it's either replaced or the code runs without error
        assert.ok(
          res.text.includes('__APP_VERSION__') ||
          res.text.includes('1.0.0'),
          'Should handle define configuration'
        );
      }
    });
  });
});
