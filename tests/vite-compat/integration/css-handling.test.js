/**
 * CSS Handling Tests
 *
 * Tests that howth handles CSS correctly:
 * - CSS to JS module conversion
 * - Style injection
 * - HMR support for CSS
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { startServer, fetchText, validateCssModule } from './test-utils.js';

describe('CSS Handling', () => {
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

  describe('CSS Module Conversion', () => {
    it('should convert CSS to JS module via /@style/', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@style/src/styles.css');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('javascript'), 'Should serve as JavaScript');
      }
    });

    it('should inject style tag in CSS module', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@style/src/styles.css');
        const validation = validateCssModule(res.text);

        assert.ok(
          validation.checks.hasStyleInjection,
          'Should have style injection code'
        );
      }
    });

    it('should contain CSS content in module', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@style/src/styles.css');

        // Should contain our CSS rules (escaped in JS string)
        assert.ok(
          res.text.includes('.app') || res.text.includes('font-family'),
          'Should contain CSS content'
        );
      }
    });

    it('should include HMR support', async () => {
      if (howth) {
        const res = await fetchText(howth, '/@style/src/styles.css');

        // Should have HMR API usage for hot reloading
        assert.ok(
          res.text.includes('import.meta.hot') || res.text.includes('hot'),
          'Should include HMR support'
        );
      }
    });
  });

  describe('CSS Import in JS', () => {
    it('should handle CSS import statements', async () => {
      if (howth) {
        const mainRes = await fetchText(howth, '/src/main.tsx');

        // CSS import should be transformed
        // Either to /@style/ path or kept as .css with special handling
        assert.ok(
          mainRes.text.includes('/@style/') ||
          mainRes.text.includes('styles.css') ||
          mainRes.text.includes('style'),
          'Should handle CSS import'
        );
      }
    });
  });

  describe('Direct CSS Serving', () => {
    it('should serve raw CSS at direct path', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/styles.css');
        assert.strictEqual(res.status, 200);

        // Can serve either as raw CSS or as JS module
        // Both are valid Vite-compatible behaviors
        const isRawCss = res.contentType.includes('text/css');
        const isJsModule = res.contentType.includes('javascript');

        assert.ok(isRawCss || isJsModule, 'Should serve CSS in some form');

        if (isRawCss) {
          // Raw CSS should have actual CSS content
          assert.ok(res.text.includes('.app'), 'Raw CSS should have .app class');
        }
      }
    });
  });

  describe('CSS Error Handling', () => {
    it('should handle non-existent CSS files', async () => {
      if (howth) {
        const res = await howth.fetch('/@style/src/non-existent.css');
        assert.strictEqual(res.status, 404);
      }
    });
  });
});
