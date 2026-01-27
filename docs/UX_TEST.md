# Howth UX/DX Testing Guide

A comprehensive walkthrough to test the full howth developer experience.

## Prerequisites

```bash
# Build howth
cargo build --release

# Add to PATH (or use full path)
export PATH="$PWD/target/release:$PATH"

# Start the daemon (required for pkg operations)
howth daemon &
```

## 1. Project Initialization

Test `howth init` - bun-style project scaffolding.

```bash
# Create a new project
mkdir /tmp/test-howth && cd /tmp/test-howth
howth init

# Verify files created
ls -la
# Should see: package.json, index.ts, tsconfig.json, .gitignore

# Check package.json contents
cat package.json

# Test with --yes flag (non-interactive)
mkdir /tmp/test-howth-2 && cd /tmp/test-howth-2
howth init -y

# Test JSON output
mkdir /tmp/test-howth-3 && cd /tmp/test-howth-3
howth init -y --json
```

## 2. Running Scripts

Test `howth run` - execute package.json scripts or TypeScript/JavaScript files.

```bash
cd /tmp/test-howth

# Add a script to package.json
cat > package.json << 'EOF'
{
  "name": "test-howth",
  "version": "1.0.0",
  "scripts": {
    "hello": "echo 'Hello from howth!'",
    "greet": "echo 'Greetings, developer!'",
    "test": "echo 'Running tests...'",
    "lint": "echo 'Linting code...'"
  }
}
EOF

# Run a script
howth run hello
# Should print: $ echo 'Hello from howth!'
#               Hello from howth!

# Run another script
howth run greet
```

## 3. Script Shortcuts

Test running scripts without the `run` keyword.

```bash
cd /tmp/test-howth

# These should work without "run"
howth test
howth lint

# Unknown scripts also work as shortcuts
howth hello
howth greet
```

## 4. Running TypeScript/JavaScript Files

```bash
cd /tmp/test-howth

# Create a TypeScript file
cat > app.ts << 'EOF'
const message: string = "Hello from TypeScript!";
console.log(message);

interface User {
  name: string;
  age: number;
}

const user: User = { name: "Alice", age: 30 };
console.log(`User: ${user.name}, Age: ${user.age}`);
EOF

# Run the file directly
howth run app.ts

# Create a JavaScript file
cat > script.js << 'EOF'
console.log("Hello from JavaScript!");
console.log("Process args:", process.argv.slice(2));
EOF

# Run with arguments
howth run script.js arg1 arg2
```

## 5. Package Linking

Test `howth link` - local package development workflow.

```bash
# Create a library package
mkdir -p /tmp/my-lib && cd /tmp/my-lib
cat > package.json << 'EOF'
{
  "name": "my-awesome-lib",
  "version": "1.0.0",
  "main": "index.js"
}
EOF

cat > index.js << 'EOF'
module.exports = {
  greet: (name) => `Hello, ${name}!`
};
EOF

# Register the package globally
howth link
# Should print: Registered my-awesome-lib -> /tmp/my-lib

# List registered packages
howth link --list

# Now use it in another project
cd /tmp/test-howth
howth link my-awesome-lib
# Should print: Linked my-awesome-lib -> /tmp/my-lib

# Verify the symlink
ls -la node_modules/

# Test with --save flag (adds to package.json)
howth link my-awesome-lib --save
cat package.json  # Should have "my-awesome-lib": "link:..."

# Unlink
howth unlink my-awesome-lib
ls node_modules/  # my-awesome-lib should be gone

# Unregister the global link
cd /tmp/my-lib
howth unlink
howth link --list  # Should be empty or not include my-awesome-lib
```

## 6. Workspaces (Monorepo Support)

Test `howth workspaces` - monorepo package management.

```bash
# Create a monorepo structure
mkdir -p /tmp/test-monorepo && cd /tmp/test-monorepo

# Root package.json with workspaces
cat > package.json << 'EOF'
{
  "name": "test-monorepo",
  "private": true,
  "workspaces": ["packages/*", "apps/*"]
}
EOF

# Create workspace packages
mkdir -p packages/ui packages/utils apps/web

# packages/ui
cat > packages/ui/package.json << 'EOF'
{
  "name": "@myorg/ui",
  "version": "1.0.0",
  "main": "index.js"
}
EOF
echo "module.exports = { Button: () => 'Button' };" > packages/ui/index.js

# packages/utils
cat > packages/utils/package.json << 'EOF'
{
  "name": "@myorg/utils",
  "version": "2.0.0",
  "main": "index.js"
}
EOF
echo "module.exports = { formatDate: () => 'formatted' };" > packages/utils/index.js

# apps/web (depends on @myorg/ui)
cat > apps/web/package.json << 'EOF'
{
  "name": "@myorg/web",
  "version": "0.1.0",
  "dependencies": {
    "@myorg/ui": "workspace:*"
  }
}
EOF

# List workspace packages
howth workspaces
# Should list all 3 packages with paths

# JSON output
howth workspaces --json

# Link all workspace packages
howth workspaces link
# Should link @myorg/ui, @myorg/utils into root node_modules
```

## 7. Install with Workspace Integration

Test `howth install` - installing dependencies with workspace package auto-linking.

```bash
cd /tmp/test-monorepo/apps/web

# Create a lockfile
cat > howth.lock << 'EOF'
{
  "lockfile_version": 1,
  "root": {
    "name": "@myorg/web",
    "version": "0.1.0"
  },
  "dependencies": {
    "@myorg/ui": {
      "range": "workspace:*",
      "kind": "dep",
      "resolved": "@myorg/ui@1.0.0"
    }
  },
  "packages": {
    "@myorg/ui@1.0.0": {
      "version": "1.0.0",
      "integrity": "",
      "resolution": {
        "link": {
          "path": "../../packages/ui"
        }
      }
    }
  }
}
EOF

# Clear any existing node_modules
rm -rf node_modules

# Install - should link workspace package locally
howth install
# Should print:
#   howth install
#   packages: 1 total, 0 cached, 0 downloaded, 1 workspace
#   + @myorg/ui@1.0.0 (workspace)
#   note: 1 workspace package(s) linked locally

# Verify symlink
ls -la node_modules/@myorg/
# ui should be a symlink to ../../packages/ui

# JSON output
rm -rf node_modules
howth install --json
```

## 8. Full Workflow Test

Test a complete development workflow.

```bash
# Start fresh
rm -rf /tmp/full-test && mkdir /tmp/full-test && cd /tmp/full-test

# Initialize project
howth init -y

# Add scripts
cat > package.json << 'EOF'
{
  "name": "full-test",
  "version": "1.0.0",
  "scripts": {
    "start": "echo 'Starting app...'",
    "test": "echo 'All tests passed!'",
    "build": "echo 'Building...'",
    "dev": "echo 'Dev server running...'"
  }
}
EOF

# Create app code
cat > index.ts << 'EOF'
interface Config {
  port: number;
  host: string;
}

const config: Config = {
  port: 3000,
  host: "localhost"
};

console.log(`Server config: ${config.host}:${config.port}`);
export default config;
EOF

# Run various commands
howth start      # Script shortcut
howth test       # Script shortcut
howth build      # Script shortcut
howth run dev    # Explicit run
howth run index.ts  # Run TypeScript file

echo "All UX tests completed successfully!"
```

## 9. JSON Output Verification

Test that all commands support `--json` for machine-readable output.

```bash
cd /tmp/test-howth

# Init
howth init -y --json 2>/dev/null | jq .

# Workspaces
cd /tmp/test-monorepo
howth workspaces --json | jq .

# Install
cd /tmp/test-monorepo/apps/web
howth install --json | jq .

# Link list
howth link --list --json | jq .
```

## 10. Error Handling

Test error cases are handled gracefully.

```bash
# Run non-existent script
cd /tmp/test-howth
howth run nonexistent
# Should error gracefully

# Run non-existent file
howth run missing.ts
# Should error gracefully

# Install without lockfile (non-frozen)
mkdir /tmp/no-lock && cd /tmp/no-lock
echo '{"name": "test"}' > package.json
howth install
# Should indicate no lockfile found

# Link unregistered package
cd /tmp/test-howth
howth link some-package-that-doesnt-exist
# Should error gracefully
```

## Cleanup

```bash
# Stop the daemon
pkill -f "howth daemon"

# Remove test directories
rm -rf /tmp/test-howth /tmp/test-howth-2 /tmp/test-howth-3
rm -rf /tmp/my-lib /tmp/test-monorepo /tmp/full-test /tmp/no-lock
```

## Summary

| Feature | Command | Status |
|---------|---------|--------|
| Project init | `howth init` | |
| Run scripts | `howth run <script>` | |
| Run files | `howth run <file.ts>` | |
| Script shortcuts | `howth test`, `howth build` | |
| Link package | `howth link` | |
| Unlink package | `howth unlink` | |
| List links | `howth link --list` | |
| Workspaces list | `howth workspaces` | |
| Workspaces link | `howth workspaces link` | |
| Install | `howth install` | |
| Workspace install | `howth install` (auto-links) | |
| JSON output | `--json` flag | |
