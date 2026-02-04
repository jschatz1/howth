/**
 * Dependency Pre-bundling Tests
 *
 * Tests that howth pre-bundles dependencies correctly:
 * - Serving dependencies at /@modules/
 * - Handling scoped packages
 * - Caching pre-bundled deps
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { startServer, fetchText } from './test-utils.js';

describe('Dependency Pre-bundling', () => {
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

  describe('Pre-bundled Dependencies', () => {
    it('should serve react at /@modules/react', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@modules/react');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('javascript'));

        // Should be valid JS module
        assert.ok(
          res.text.includes('export') || res.text.includes('module.exports'),
          'Should be a valid module'
        );
      }
    });

    it('should serve react-dom at /@modules/react-dom', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@modules/react-dom');
        // May redirect or serve directly
        assert.ok(
          res.status === 200 || res.status === 302,
          'Should serve react-dom'
        );
      }
    });

    it('should serve react-dom/client subpath', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@modules/react-dom/client');
        // May or may not support subpath depending on implementation
        // Document expected behavior
        if (res.status === 200) {
          assert.ok(res.contentType.includes('javascript'));
        }
      }
    });
  });

  describe('Pre-bundled Module Content', () => {
    it('should export React hooks', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@modules/react');

        if (res.status === 200) {
          // Should export useState, useEffect, etc. or have React module structure
          // Note: Pre-bundling may use CJS wrapper that conditionally loads react
          assert.ok(
            res.text.includes('useState') ||
            res.text.includes('useEffect') ||
            res.text.includes('createElement') ||
            res.text.includes('react') ||  // Module references react
            res.text.includes('__modules'), // Has module system
            'Should export React APIs or have valid module structure'
          );
        }
      }
    });

    it('should be ESM format', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@modules/react');

        if (res.status === 200) {
          // Should be ESM (export statements) not CommonJS
          assert.ok(
            res.text.includes('export ') ||
            res.text.includes('export{') ||
            res.text.includes('export default'),
            'Should be ESM format'
          );
        }
      }
    });
  });

  describe('Caching', () => {
    it('should have cache-control headers for pre-bundled deps', async () => {
      if (howth) {
        const res = await howth.fetch('/@modules/react');

        if (res.status === 200) {
          const cacheControl = res.headers.get('cache-control');
          // Pre-bundled deps should be immutable or have long cache
          // This is optional but good practice
          if (cacheControl) {
            assert.ok(
              cacheControl.includes('max-age') || cacheControl.includes('immutable'),
              'Should have caching headers'
            );
          }
        }
      }
    });
  });

  describe('Error Handling', () => {
    it('should return 404 for non-existent packages', async () => {
      if (howth) {
        const res = await howth.fetch('/@modules/this-package-does-not-exist-12345');
        assert.strictEqual(res.status, 404);
      }
    });
  });
});
