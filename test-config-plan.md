# Config File Support — Test Plan

## Unit Tests (already passing)

These 12 tests exist in `crates/fastnode-core/src/dev/config.rs`:

- [x] `test_find_config_file` — discovery priority order
- [x] `test_parse_simple_config` — server/resolve/define/base fields
- [x] `test_parse_config_with_comments` — single-line and block comments
- [x] `test_parse_config_double_quotes` — double-quoted strings
- [x] `test_parse_empty_config` — `export default {}`
- [x] `test_parse_config_with_array` — config with only server.port
- [x] `test_parse_define_with_dotted_keys` — `process.env.NODE_ENV` style keys
- [x] `test_no_default_export` — error on missing export default
- [x] `test_load_config_js_file` — end-to-end load from .js file
- [x] `test_load_config_explicit_path` — `--config` explicit path
- [x] `test_load_config_missing_explicit_path` — error on nonexistent path
- [x] `test_strip_comments` — comment stripping utility

## Integration Tests (to run)

### 1. Auto-discovery: howth.config.js

Create a temp project with `howth.config.js` and verify it loads.

```
mkdir /tmp/howth-test-autodiscovery
echo 'export default { server: { port: 4444 } };' > /tmp/howth-test-autodiscovery/howth.config.js
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-autodiscovery/index.html
mkdir -p /tmp/howth-test-autodiscovery/src
echo 'console.log("hello");' > /tmp/howth-test-autodiscovery/src/main.tsx
```

**Expected:** Server prints `Loaded config from howth.config.js` and binds to port 4444.

### 2. Auto-discovery: vite.config.ts

```
mkdir /tmp/howth-test-vite-ts
cat > /tmp/howth-test-vite-ts/vite.config.ts << 'CONF'
export default {
  server: {
    port: 5555,
    host: 'localhost',
  },
  resolve: {
    alias: {
      '@': './src',
    },
  },
  define: {
    'process.env.NODE_ENV': '"development"',
  },
  base: '/',
};
CONF
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-vite-ts/index.html
mkdir -p /tmp/howth-test-vite-ts/src
echo 'console.log("hello");' > /tmp/howth-test-vite-ts/src/main.tsx
```

**Expected:** Server prints `Loaded config from vite.config.ts` and binds to port 5555.

### 3. Priority order: howth.config.ts wins over vite.config.js

```
mkdir /tmp/howth-test-priority
echo 'export default { server: { port: 6001 } };' > /tmp/howth-test-priority/howth.config.ts
echo 'export default { server: { port: 6002 } };' > /tmp/howth-test-priority/vite.config.js
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-priority/index.html
mkdir -p /tmp/howth-test-priority/src
echo 'console.log("hello");' > /tmp/howth-test-priority/src/main.tsx
```

**Expected:** Loads `howth.config.ts` (port 6001), not `vite.config.js`.

### 4. CLI `--config` flag overrides auto-discovery

```
mkdir /tmp/howth-test-cli-config
echo 'export default { server: { port: 7001 } };' > /tmp/howth-test-cli-config/howth.config.js
echo 'export default { server: { port: 7002 } };' > /tmp/howth-test-cli-config/custom.config.js
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-cli-config/index.html
mkdir -p /tmp/howth-test-cli-config/src
echo 'console.log("hello");' > /tmp/howth-test-cli-config/src/main.tsx
```

Run with: `howth dev src/main.tsx --config custom.config.js`

**Expected:** Loads `custom.config.js` (port 7002), ignoring `howth.config.js`.

### 5. CLI `--port` flag overrides config file port

```
mkdir /tmp/howth-test-cli-override
echo 'export default { server: { port: 8001 } };' > /tmp/howth-test-cli-override/howth.config.js
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-cli-override/index.html
mkdir -p /tmp/howth-test-cli-override/src
echo 'console.log("hello");' > /tmp/howth-test-cli-override/src/main.tsx
```

Run with: `howth dev src/main.tsx --port 9999`

**Expected:** Binds to port 9999, not 8001.

### 6. No config file — no error

```
mkdir /tmp/howth-test-noconfig
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-noconfig/index.html
mkdir -p /tmp/howth-test-noconfig/src
echo 'console.log("hello");' > /tmp/howth-test-noconfig/src/main.tsx
```

**Expected:** Server starts on default port 3000, no config-related output.

### 7. Invalid config file — warning, not crash

```
mkdir /tmp/howth-test-badconfig
echo 'this is not valid config' > /tmp/howth-test-badconfig/howth.config.js
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-badconfig/index.html
mkdir -p /tmp/howth-test-badconfig/src
echo 'console.log("hello");' > /tmp/howth-test-badconfig/src/main.tsx
```

**Expected:** Prints warning about failed config load, server still starts on default port 3000.

### 8. Config with comments and trailing commas

```
mkdir /tmp/howth-test-comments
cat > /tmp/howth-test-comments/howth.config.js << 'CONF'
// Howth config
/* block comment */
export default {
  server: {
    port: 4321, // trailing comma
  },
  resolve: {
    alias: {
      '@': './src', // another trailing comma
    },
  },
};
CONF
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-comments/index.html
mkdir -p /tmp/howth-test-comments/src
echo 'console.log("hello");' > /tmp/howth-test-comments/src/main.tsx
```

**Expected:** Loads successfully, port 4321.

### 9. `--config` with nonexistent file — error

```
mkdir /tmp/howth-test-missing
echo '<html><body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>' > /tmp/howth-test-missing/index.html
mkdir -p /tmp/howth-test-missing/src
echo 'console.log("hello");' > /tmp/howth-test-missing/src/main.tsx
```

Run with: `howth dev src/main.tsx --config nonexistent.js`

**Expected:** Prints warning about missing config file, server still starts.

## How to Run

Each integration test:
1. Set up the temp directory (commands above)
2. `cd /tmp/howth-test-*` and run `howth dev src/main.tsx`
3. Check stdout for expected output
4. Ctrl+C to stop
5. Clean up: `rm -rf /tmp/howth-test-*`

For automated testing, each server is started in the background, output is captured for 2 seconds, then killed. We check for expected strings in the output.
