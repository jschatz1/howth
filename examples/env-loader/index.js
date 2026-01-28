/**
 * Environment & Config Loader Example
 *
 * Demonstrates:
 * - .env file parsing
 * - JSON config loading
 * - Config merging and validation
 * - Environment-specific configs
 * - Secret masking in logs
 *
 * Run: howth run --native examples/env-loader/index.js
 */

const fs = require('fs');
const path = require('path');

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

/**
 * Parse .env file contents
 */
function parseEnvFile(content) {
  const env = {};

  for (const line of content.split('\n')) {
    // Skip empty lines and comments
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;

    // Parse KEY=value or KEY="value" or KEY='value'
    const match = trimmed.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/);
    if (!match) continue;

    let [, key, value] = match;

    // Remove quotes if present
    if ((value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1);
    }

    // Handle escape sequences in double-quoted strings
    if (value.includes('\\')) {
      value = value
        .replace(/\\n/g, '\n')
        .replace(/\\t/g, '\t')
        .replace(/\\r/g, '\r')
        .replace(/\\\\/g, '\\');
    }

    env[key] = value;
  }

  return env;
}

/**
 * Load .env file and merge with process.env
 */
function loadEnv(filepath) {
  const absolutePath = path.resolve(filepath);

  if (!fs.existsSync(absolutePath)) {
    console.log(`${c.yellow}⚠ ${filepath} not found, skipping${c.reset}`);
    return {};
  }

  console.log(`${c.green}✓${c.reset} Loading ${filepath}`);
  const content = fs.readFileSync(absolutePath, 'utf8');
  const env = parseEnvFile(content);

  // Merge into process.env (don't override existing)
  for (const [key, value] of Object.entries(env)) {
    if (!(key in process.env)) {
      process.env[key] = value;
    }
  }

  return env;
}

/**
 * Load JSON config file
 */
function loadJsonConfig(filepath) {
  const absolutePath = path.resolve(filepath);

  if (!fs.existsSync(absolutePath)) {
    console.log(`${c.yellow}⚠ ${filepath} not found, skipping${c.reset}`);
    return {};
  }

  console.log(`${c.green}✓${c.reset} Loading ${filepath}`);
  const content = fs.readFileSync(absolutePath, 'utf8');
  return JSON.parse(content);
}

/**
 * Deep merge objects
 */
function deepMerge(target, source) {
  const result = { ...target };

  for (const [key, value] of Object.entries(source)) {
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      result[key] = deepMerge(result[key] || {}, value);
    } else {
      result[key] = value;
    }
  }

  return result;
}

/**
 * Interpolate environment variables in config values
 */
function interpolateEnv(config, env = process.env) {
  if (typeof config === 'string') {
    return config.replace(/\$\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_, key) => env[key] || '');
  }

  if (Array.isArray(config)) {
    return config.map(item => interpolateEnv(item, env));
  }

  if (config && typeof config === 'object') {
    const result = {};
    for (const [key, value] of Object.entries(config)) {
      result[key] = interpolateEnv(value, env);
    }
    return result;
  }

  return config;
}

/**
 * Mask secrets in output
 */
function maskSecrets(obj, secretKeys = ['password', 'secret', 'token', 'key', 'api_key', 'apiKey']) {
  if (typeof obj === 'string') return obj;

  if (Array.isArray(obj)) {
    return obj.map(item => maskSecrets(item, secretKeys));
  }

  if (obj && typeof obj === 'object') {
    const result = {};
    for (const [key, value] of Object.entries(obj)) {
      const isSecret = secretKeys.some(s =>
        key.toLowerCase().includes(s.toLowerCase())
      );
      result[key] = isSecret ? '***MASKED***' : maskSecrets(value, secretKeys);
    }
    return result;
  }

  return obj;
}

/**
 * Validate config against schema
 */
function validateConfig(config, schema) {
  const errors = [];

  function validate(obj, schemaObj, path = '') {
    for (const [key, def] of Object.entries(schemaObj)) {
      const fullPath = path ? `${path}.${key}` : key;
      const value = obj?.[key];

      if (def.required && (value === undefined || value === null || value === '')) {
        errors.push(`${fullPath} is required`);
        continue;
      }

      if (value !== undefined && def.type) {
        const actualType = Array.isArray(value) ? 'array' : typeof value;
        if (actualType !== def.type) {
          errors.push(`${fullPath} should be ${def.type}, got ${actualType}`);
        }
      }

      if (value !== undefined && def.enum && !def.enum.includes(value)) {
        errors.push(`${fullPath} should be one of: ${def.enum.join(', ')}`);
      }

      if (def.type === 'object' && def.properties && value) {
        validate(value, def.properties, fullPath);
      }
    }
  }

  validate(config, schema);
  return errors;
}

// Demo
console.log(`\n${c.bold}Environment & Config Loader Demo${c.reset}\n`);

// Create sample files for demo
const exampleDir = path.dirname(process.argv[1] || __filename);

// Create sample .env
const sampleEnv = `
# Database configuration
DATABASE_URL=postgres://localhost:5432/myapp
DATABASE_POOL_SIZE=10

# API Keys
API_KEY=sk_test_1234567890
SECRET_TOKEN="my-secret-token"

# Feature flags
ENABLE_CACHE=true
DEBUG_MODE=false
`;

// Create sample config.json
const sampleConfig = {
  app: {
    name: "MyApp",
    port: 3000,
    env: "${NODE_ENV}",
  },
  database: {
    url: "${DATABASE_URL}",
    poolSize: "${DATABASE_POOL_SIZE}",
  },
  features: {
    cache: true,
    logging: true,
  },
};

// Create sample config.production.json
const sampleProdConfig = {
  app: {
    port: 8080,
  },
  features: {
    logging: false,
  },
};

// Write sample files
fs.writeFileSync(path.join(exampleDir, '.env'), sampleEnv);
fs.writeFileSync(path.join(exampleDir, 'config.json'), JSON.stringify(sampleConfig, null, 2));
fs.writeFileSync(path.join(exampleDir, 'config.production.json'), JSON.stringify(sampleProdConfig, null, 2));

console.log(`${c.cyan}Created sample config files${c.reset}\n`);

// Load environment
console.log(`${c.bold}1. Loading environment files:${c.reset}`);
process.env.NODE_ENV = process.env.NODE_ENV || 'development';
loadEnv(path.join(exampleDir, '.env'));

// Load configs
console.log(`\n${c.bold}2. Loading config files:${c.reset}`);
let config = loadJsonConfig(path.join(exampleDir, 'config.json'));

// Load environment-specific config
const envConfig = loadJsonConfig(path.join(exampleDir, `config.${process.env.NODE_ENV}.json`));
config = deepMerge(config, envConfig);

// Interpolate environment variables
console.log(`\n${c.bold}3. Interpolating environment variables...${c.reset}`);
config = interpolateEnv(config);

// Define schema for validation
const configSchema = {
  app: {
    required: true,
    type: 'object',
    properties: {
      name: { required: true, type: 'string' },
      port: { required: true, type: 'number' },
    },
  },
  database: {
    required: true,
    type: 'object',
    properties: {
      url: { required: true, type: 'string' },
    },
  },
};

// Validate
console.log(`\n${c.bold}4. Validating config...${c.reset}`);
// Convert string port to number for validation
config.app.port = parseInt(config.app.port) || config.app.port;
config.database.poolSize = parseInt(config.database.poolSize) || config.database.poolSize;

const errors = validateConfig(config, configSchema);
if (errors.length > 0) {
  console.log(`${c.red}Validation errors:${c.reset}`);
  errors.forEach(e => console.log(`  ${c.red}✗${c.reset} ${e}`));
} else {
  console.log(`${c.green}✓ Config is valid${c.reset}`);
}

// Print final config (with secrets masked)
console.log(`\n${c.bold}5. Final configuration:${c.reset}`);
const maskedConfig = maskSecrets(config);
console.log(JSON.stringify(maskedConfig, null, 2));

// Print loaded env vars
console.log(`\n${c.bold}6. Loaded environment variables:${c.reset}`);
const relevantEnv = Object.entries(process.env)
  .filter(([k]) => ['DATABASE_URL', 'API_KEY', 'SECRET_TOKEN', 'ENABLE_CACHE', 'DEBUG_MODE', 'NODE_ENV'].includes(k));

for (const [key, value] of relevantEnv) {
  const masked = key.toLowerCase().includes('key') || key.toLowerCase().includes('secret') || key.toLowerCase().includes('token')
    ? '***MASKED***'
    : value;
  console.log(`  ${c.blue}${key}${c.reset}=${masked}`);
}

console.log(`\n${c.green}${c.bold}Config loading demo completed!${c.reset}\n`);
