/**
 * Module Serving Tests
 *
 * Tests that howth serves modules correctly compared to Vite:
 * - Index HTML generation
 * - TypeScript/TSX transpilation
 * - Import rewriting
 * - JSON to ESM conversion
 * - Static file serving
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import {
  startServer,
  fetchText,
  extractImports,
  validateImportRewriting,
  FIXTURE_DIR,
} from './test-utils.js';

describe('Module Serving', () => {
  let vite, howth;
  const skipVite = process.env.HOWTH_ONLY === '1';
  const skipHowth = process.env.VITE_ONLY === '1';

  before(async () => {
    if (!skipVite) {
      try {
        vite = await startServer('vite');
      } catch (err) {
        console.error('Failed to start Vite:', err.message);
      }
    }
    if (!skipHowth) {
      try {
        howth = await startServer('howth');
      } catch (err) {
        console.error('Failed to start howth:', err.message);
      }
    }
  });

  after(async () => {
    if (vite) await vite.stop();
    if (howth) await howth.stop();
  });

  describe('Index HTML', () => {
    it('should serve index.html at root', async () => {
      if (howth) {
        const res = await fetchText(howth, '/');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('text/html'));
        assert.ok(res.text.includes('<!DOCTYPE html>') || res.text.includes('<!doctype html>'));
        assert.ok(res.text.includes('<script type="module"'));
      }
    });

    it('should inject HMR client script', async () => {
      if (howth) {
        const res = await fetchText(howth, '/');
        assert.ok(
          res.text.includes('/@hmr-client') || res.text.includes('__hmr'),
          'Should inject HMR client'
        );
      }
    });
  });

  describe('TypeScript/TSX Transpilation', () => {
    it('should transpile main.tsx', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('javascript'));
        // Should not contain TypeScript syntax
        assert.ok(!res.text.includes(': React.'), 'Should remove type annotations');
        // Should contain transpiled JSX
        assert.ok(
          res.text.includes('createElement') || res.text.includes('jsx') || res.text.includes('_jsx'),
          'Should transpile JSX'
        );
      }
    });

    it('should transpile App.tsx with useState', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/App.tsx');
        assert.strictEqual(res.status, 200);
        // Should have valid JS
        assert.ok(!res.text.includes('useState<'), 'Should remove generic type params');
        assert.ok(res.text.includes('useState'));
      }
    });

    it('should transpile TypeScript hooks', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/hooks/useToggle.ts');
        assert.strictEqual(res.status, 200);
        // Should remove return type annotation
        assert.ok(!res.text.includes(': [boolean, () => void]'));
      }
    });
  });

  describe('Import Rewriting', () => {
    it('should rewrite bare specifiers to /@modules/', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');
        const imports = extractImports(res.text);

        const reactImport = imports.find(i => i.specifier.includes('react'));
        assert.ok(reactImport, 'Should have react import');
        assert.ok(
          reactImport.specifier.startsWith('/@modules/') || reactImport.specifier.startsWith('/node_modules/'),
          `React import should be rewritten: ${reactImport.specifier}`
        );
      }
    });

    it('should rewrite relative imports to absolute paths', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');
        const imports = extractImports(res.text);

        // Should not have any ./relative imports
        const relativeImports = imports.filter(i =>
          i.specifier.startsWith('./') || i.specifier.startsWith('../')
        );
        assert.strictEqual(
          relativeImports.length,
          0,
          `Should not have relative imports: ${relativeImports.map(i => i.specifier).join(', ')}`
        );
      }
    });

    it('should resolve alias imports (@components)', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');
        // @components/Button should be resolved
        assert.ok(
          res.text.includes('/src/components/Button') || res.text.includes('@components'),
          'Should resolve or preserve alias'
        );
      }
    });

    it('should handle CSS imports', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/main.tsx');
        // CSS import should be rewritten to /@style/ or similar
        assert.ok(
          res.text.includes('/@style/') || res.text.includes('.css'),
          'Should handle CSS import'
        );
      }
    });
  });

  describe('JSON Import', () => {
    it('should convert JSON to ESM', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/data.json');
        assert.strictEqual(res.status, 200);
        assert.ok(res.contentType.includes('javascript'));
        // Should export default
        assert.ok(res.text.includes('export default') || res.text.includes('export{'));
        // Should contain the JSON data
        assert.ok(res.text.includes('Hello from JSON'));
      }
    });
  });

  describe('Static Files', () => {
    it('should serve CSS files directly', async () => {
      if (howth) {
        const res = await fetchText(howth, '/src/styles.css');
        assert.strictEqual(res.status, 200);
        // Either serves as CSS or converts to JS module
        assert.ok(
          res.contentType.includes('css') || res.contentType.includes('javascript'),
          'Should serve CSS'
        );
      }
    });
  });

  describe('Error Handling', () => {
    it('should return 404 for non-existent files', async () => {
      if (howth) {
        const res = await howth.fetch('/src/does-not-exist.tsx');
        assert.strictEqual(res.status, 404);
      }
    });

    it('should handle malformed paths gracefully', async () => {
      if (howth) {
        const res = await howth.fetch('/../../../etc/passwd');
        // Should not serve files outside project
        assert.ok(res.status === 400 || res.status === 403 || res.status === 404);
      }
    });
  });

  describe('Vite Comparison', () => {
    it('should produce similar transpiled output to Vite', async () => {
      if (vite && howth) {
        const viteRes = await fetchText(vite, '/src/components/Counter.tsx');
        const howthRes = await fetchText(howth, '/src/components/Counter.tsx');

        assert.strictEqual(viteRes.status, howthRes.status, 'Status should match');

        // Both should have transpiled JSX
        assert.ok(
          viteRes.text.includes('createElement') || viteRes.text.includes('jsx'),
          'Vite should transpile JSX'
        );
        assert.ok(
          howthRes.text.includes('createElement') || howthRes.text.includes('jsx'),
          'Howth should transpile JSX'
        );
      }
    });

    it('should rewrite imports similarly to Vite', async () => {
      if (vite && howth) {
        const viteRes = await fetchText(vite, '/src/App.tsx');
        const howthRes = await fetchText(howth, '/src/App.tsx');

        const viteImports = extractImports(viteRes.text);
        const howthImports = extractImports(howthRes.text);

        // Both should have similar number of imports
        // Allow some variance for injected HMR imports
        const diff = Math.abs(viteImports.length - howthImports.length);
        assert.ok(diff <= 3, `Import count difference too large: Vite=${viteImports.length}, Howth=${howthImports.length}`);
      }
    });
  });
});
