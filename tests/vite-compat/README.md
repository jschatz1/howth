# Vite Compatibility Tests

This test suite verifies that howth's dev server is compatible with Vite's behavior and API.

## Structure

```
tests/vite-compat/
├── fixtures/
│   └── basic-app/          # Test application with React, CSS, aliases
│       ├── src/
│       │   ├── main.tsx    # Entry point with HMR, aliases, env vars
│       │   ├── App.tsx     # React component with hooks, JSON import
│       │   ├── components/ # Button, Counter components
│       │   ├── hooks/      # Custom hooks (useToggle)
│       │   ├── styles.css  # CSS styles
│       │   └── data.json   # JSON data
│       ├── .env            # Environment variables
│       ├── .env.development
│       ├── howth.config.ts # howth configuration
│       ├── vite.config.ts  # Vite configuration (for comparison)
│       └── package.json
│
└── integration/
    ├── test-utils.js       # Test utilities (server management, assertions)
    ├── run-tests.js        # Test runner script
    ├── module-serving.test.js    # Module transpilation and serving
    ├── css-handling.test.js      # CSS module conversion
    ├── hmr.test.js               # Hot Module Replacement
    ├── prebundling.test.js       # Dependency pre-bundling
    ├── env-variables.test.js     # Environment variables
    ├── config.test.js            # Configuration loading
    └── spa-fallback.test.js      # SPA routing fallback
```

## Running Tests

### Prerequisites

1. Build howth with the dev server:
   ```bash
   cargo build --release -p fastnode-cli
   ```

2. Install fixture dependencies:
   ```bash
   cd tests/vite-compat/fixtures/basic-app
   npm install
   ```

### Run All Tests

```bash
cd tests/vite-compat/integration

# Run tests against howth only
HOWTH_BIN=../../../target/release/howth HOWTH_ONLY=1 node run-tests.js

# Run tests against vite only (for comparison)
VITE_ONLY=1 node run-tests.js

# Run tests against both (requires vite installed)
HOWTH_BIN=../../../target/release/howth node run-tests.js
```

### Run Individual Test Files

```bash
cd tests/vite-compat/integration

# Test module serving
HOWTH_BIN=../../../target/release/howth HOWTH_ONLY=1 \
  node --test module-serving.test.js

# Test HMR
HOWTH_BIN=../../../target/release/howth HOWTH_ONLY=1 \
  node --test hmr.test.js
```

## Test Coverage

### Module Serving (`module-serving.test.js`)

- [x] Index HTML generation with HMR client injection
- [x] TypeScript/TSX transpilation
- [x] Import rewriting (bare → /@modules/, relative → absolute)
- [x] Alias resolution (@components, @/)
- [x] JSON to ESM conversion
- [x] CSS import handling
- [x] 404 for non-existent files
- [x] Path traversal protection

### CSS Handling (`css-handling.test.js`)

- [x] CSS to JS module conversion at /@style/
- [x] Style tag injection
- [x] CSS content preservation
- [x] HMR support for CSS
- [x] Direct CSS file serving

### HMR (`hmr.test.js`)

- [x] HMR client runtime at /@hmr-client
- [x] import.meta.hot API (accept, dispose)
- [x] WebSocket connection
- [x] Module preamble injection
- [x] React Refresh at /@react-refresh
- [x] React component refresh integration

### Pre-bundling (`prebundling.test.js`)

- [x] Serving deps at /@modules/react
- [x] ESM format output
- [x] React hooks export
- [x] Cache-control headers
- [x] 404 for non-existent packages

### Environment Variables (`env-variables.test.js`)

- [x] import.meta.env.MODE replacement
- [x] import.meta.env.DEV replacement
- [x] VITE_* variable exposure
- [x] HOWTH_* variable exposure
- [x] Define replacements from config
- [x] .env file loading
- [x] .env.development loading

### Configuration (`config.test.js`)

- [x] howth.config.ts loading
- [x] Alias resolution from config
- [x] Server port configuration
- [x] Define configuration

### SPA Fallback (`spa-fallback.test.js`)

- [x] Fallback for routes without extensions
- [x] Nested route fallback
- [x] No fallback for .js/.ts files
- [x] Actual files served over fallback

## Adding New Tests

1. Create a new test file in `integration/` with the pattern `*.test.js`
2. Import utilities from `test-utils.js`
3. Use Node.js built-in test runner (`node:test`)
4. Start servers with `startServer('howth')` or `startServer('vite')`
5. Clean up with `server.stop()` in `after()` hook

Example:

```javascript
import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { startServer, fetchText } from './test-utils.js';

describe('My Feature', () => {
  let howth;

  before(async () => {
    howth = await startServer('howth');
  });

  after(async () => {
    if (howth) await howth.stop();
  });

  it('should do something', async () => {
    const res = await fetchText(howth, '/path');
    assert.strictEqual(res.status, 200);
  });
});
```

## Comparing with Vite

To compare howth's output with Vite's:

1. Run tests without `HOWTH_ONLY` or `VITE_ONLY`
2. Tests in the "Vite Comparison" sections will run against both
3. Use `compareResponses()` utility for detailed comparison

## Known Differences

Some differences from Vite are expected and documented:

1. **React Refresh injection** - Implementation may vary
2. **Import rewriting format** - `/@modules/` vs `/node_modules/.vite/`
3. **CSS module format** - Style injection code may differ
4. **HMR message format** - Protocol compatible but not identical
5. **Error messages** - Format and content may differ

## Troubleshooting

### Server fails to start

- Check that the port is not in use
- Verify howth binary path with `HOWTH_BIN`
- Check fixture has `node_modules` installed

### Tests timeout

- Increase timeout in `test-utils.js` (default 30s for server start)
- Check for hanging processes with `ps aux | grep howth`

### Import not rewritten

- Verify the dependency is in `package.json`
- Check that pre-bundling ran (`.howth/deps/` directory)
