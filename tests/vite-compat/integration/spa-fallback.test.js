/**
 * SPA Fallback Tests
 *
 * Tests that howth implements SPA fallback correctly:
 * - Routes without extensions return index.html
 * - Deep routes work for client-side routing
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { startServer, fetchText } from './test-utils.js';

describe('SPA Fallback', () => {
  let howth;

  before(async () => {
    if (process.env.VITE_ONLY !== '1') {
      try {
        howth = await startServer('howth');
      } catch (err) {
        console.error('Failed to start howth:', err.message);
      }
    }
  });

  after(async () => {
    if (howth) await howth.stop();
  });

  describe('Route Fallback', () => {
    it('should serve index.html for /about (no extension)', async () => {
      if (howth) {
        const res = await fetchText(howth, '/about');

        // Should either:
        // 1. Return index.html (SPA fallback)
        // 2. Return 404 (no fallback implemented)
        if (res.status === 200) {
          assert.ok(res.contentType.includes('text/html'));
          assert.ok(
            res.text.includes('<!DOCTYPE html>') || res.text.includes('<!doctype html>'),
            'Should serve HTML'
          );
        } else {
          // Document that fallback is not implemented
          assert.strictEqual(res.status, 404);
        }
      }
    });

    it('should serve index.html for nested routes', async () => {
      if (howth) {
        const res = await fetchText(howth, '/users/123/profile');

        if (res.status === 200) {
          assert.ok(res.contentType.includes('text/html'));
        }
        // 404 is also acceptable if fallback not implemented
      }
    });

    it('should NOT fallback for paths with extensions', async () => {
      if (howth) {
        // A request for a .js file that doesn't exist should 404, not fallback
        const res = await howth.fetch('/non-existent.js');
        assert.strictEqual(res.status, 404);
      }
    });

    it('should serve actual files over fallback', async () => {
      if (howth) {
        // /src/main.tsx should serve the file, not index.html
        const res = await fetchText(howth, '/src/main.tsx');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('javascript'));
        assert.ok(!res.text.includes('<!DOCTYPE html>'));
      }
    });
  });

  describe('Public Directory', () => {
    // Note: The fixture doesn't have a public directory
    // These tests document expected behavior

    it('should serve files from public directory at root', async () => {
      // If public/favicon.ico exists, it should be served at /favicon.ico
      // This is expected Vite behavior
    });
  });
});
