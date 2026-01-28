/**
 * Data Validator Example
 *
 * A schema-based validation library:
 * - Type validation
 * - Required fields
 * - Custom validators
 * - Nested objects
 * - Arrays
 * - Error messages
 *
 * Run: howth run --native examples/validator/validate.js
 */

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Data Validator Demo${c.reset}\n`);

/**
 * Validation Error
 */
class ValidationError extends Error {
  constructor(errors) {
    super('Validation failed');
    this.name = 'ValidationError';
    this.errors = errors;
  }
}

/**
 * Schema Builder
 */
class Schema {
  constructor() {
    this.rules = [];
    this._optional = false;
    this._default = undefined;
  }

  // Mark field as optional
  optional() {
    this._optional = true;
    return this;
  }

  // Set default value
  default(value) {
    this._default = value;
    this._optional = true;
    return this;
  }

  // Add custom validation rule
  custom(fn, message = 'Custom validation failed') {
    this.rules.push({ type: 'custom', fn, message });
    return this;
  }

  // Validate value
  validate(value, path = '') {
    const errors = [];

    // Handle undefined/null
    if (value === undefined || value === null) {
      if (this._default !== undefined) {
        return { value: this._default, errors: [] };
      }
      if (this._optional) {
        return { value, errors: [] };
      }
      return { value, errors: [{ path, message: 'Value is required' }] };
    }

    // Run all rules
    for (const rule of this.rules) {
      const error = this.checkRule(rule, value, path);
      if (error) {
        errors.push(error);
      }
    }

    return { value, errors };
  }

  checkRule(rule, value, path) {
    switch (rule.type) {
      case 'custom':
        if (!rule.fn(value)) {
          return { path, message: rule.message };
        }
        break;
    }
    return null;
  }
}

/**
 * String Schema
 */
class StringSchema extends Schema {
  constructor() {
    super();
    this.rules.push({ type: 'type', expected: 'string' });
  }

  min(length) {
    this.rules.push({ type: 'minLength', length });
    return this;
  }

  max(length) {
    this.rules.push({ type: 'maxLength', length });
    return this;
  }

  length(length) {
    this.rules.push({ type: 'exactLength', length });
    return this;
  }

  email() {
    this.rules.push({ type: 'email' });
    return this;
  }

  url() {
    this.rules.push({ type: 'url' });
    return this;
  }

  pattern(regex, message = 'Invalid format') {
    this.rules.push({ type: 'pattern', regex, message });
    return this;
  }

  enum(values) {
    this.rules.push({ type: 'enum', values });
    return this;
  }

  checkRule(rule, value, path) {
    switch (rule.type) {
      case 'type':
        if (typeof value !== 'string') {
          return { path, message: `Expected string, got ${typeof value}` };
        }
        break;
      case 'minLength':
        if (value.length < rule.length) {
          return { path, message: `Must be at least ${rule.length} characters` };
        }
        break;
      case 'maxLength':
        if (value.length > rule.length) {
          return { path, message: `Must be at most ${rule.length} characters` };
        }
        break;
      case 'exactLength':
        if (value.length !== rule.length) {
          return { path, message: `Must be exactly ${rule.length} characters` };
        }
        break;
      case 'email':
        if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) {
          return { path, message: 'Invalid email address' };
        }
        break;
      case 'url':
        if (!/^https?:\/\/.+/.test(value)) {
          return { path, message: 'Invalid URL' };
        }
        break;
      case 'pattern':
        if (!rule.regex.test(value)) {
          return { path, message: rule.message };
        }
        break;
      case 'enum':
        if (!rule.values.includes(value)) {
          return { path, message: `Must be one of: ${rule.values.join(', ')}` };
        }
        break;
      default:
        return super.checkRule(rule, value, path);
    }
    return null;
  }
}

/**
 * Number Schema
 */
class NumberSchema extends Schema {
  constructor() {
    super();
    this.rules.push({ type: 'type', expected: 'number' });
  }

  min(value) {
    this.rules.push({ type: 'min', value });
    return this;
  }

  max(value) {
    this.rules.push({ type: 'max', value });
    return this;
  }

  integer() {
    this.rules.push({ type: 'integer' });
    return this;
  }

  positive() {
    this.rules.push({ type: 'positive' });
    return this;
  }

  negative() {
    this.rules.push({ type: 'negative' });
    return this;
  }

  checkRule(rule, value, path) {
    switch (rule.type) {
      case 'type':
        if (typeof value !== 'number' || isNaN(value)) {
          return { path, message: `Expected number, got ${typeof value}` };
        }
        break;
      case 'min':
        if (value < rule.value) {
          return { path, message: `Must be at least ${rule.value}` };
        }
        break;
      case 'max':
        if (value > rule.value) {
          return { path, message: `Must be at most ${rule.value}` };
        }
        break;
      case 'integer':
        if (!Number.isInteger(value)) {
          return { path, message: 'Must be an integer' };
        }
        break;
      case 'positive':
        if (value <= 0) {
          return { path, message: 'Must be positive' };
        }
        break;
      case 'negative':
        if (value >= 0) {
          return { path, message: 'Must be negative' };
        }
        break;
      default:
        return super.checkRule(rule, value, path);
    }
    return null;
  }
}

/**
 * Boolean Schema
 */
class BooleanSchema extends Schema {
  constructor() {
    super();
    this.rules.push({ type: 'type', expected: 'boolean' });
  }

  checkRule(rule, value, path) {
    if (rule.type === 'type' && typeof value !== 'boolean') {
      return { path, message: `Expected boolean, got ${typeof value}` };
    }
    return super.checkRule(rule, value, path);
  }
}

/**
 * Array Schema
 */
class ArraySchema extends Schema {
  constructor(itemSchema = null) {
    super();
    this.itemSchema = itemSchema;
  }

  min(length) {
    this.rules.push({ type: 'minLength', length });
    return this;
  }

  max(length) {
    this.rules.push({ type: 'maxLength', length });
    return this;
  }

  validate(value, path = '') {
    const errors = [];

    if (value === undefined || value === null) {
      if (this._optional) return { value, errors: [] };
      return { value, errors: [{ path, message: 'Value is required' }] };
    }

    if (!Array.isArray(value)) {
      return { value, errors: [{ path, message: `Expected array, got ${typeof value}` }] };
    }

    // Check rules
    for (const rule of this.rules) {
      if (rule.type === 'minLength' && value.length < rule.length) {
        errors.push({ path, message: `Array must have at least ${rule.length} items` });
      }
      if (rule.type === 'maxLength' && value.length > rule.length) {
        errors.push({ path, message: `Array must have at most ${rule.length} items` });
      }
    }

    // Validate items
    if (this.itemSchema) {
      value.forEach((item, index) => {
        const result = this.itemSchema.validate(item, `${path}[${index}]`);
        errors.push(...result.errors);
      });
    }

    return { value, errors };
  }
}

/**
 * Object Schema
 */
class ObjectSchema extends Schema {
  constructor(shape = {}) {
    super();
    this.shape = shape;
  }

  validate(value, path = '') {
    const errors = [];

    if (value === undefined || value === null) {
      if (this._optional) return { value, errors: [] };
      return { value, errors: [{ path, message: 'Value is required' }] };
    }

    if (typeof value !== 'object' || Array.isArray(value)) {
      return { value, errors: [{ path, message: `Expected object, got ${Array.isArray(value) ? 'array' : typeof value}` }] };
    }

    // Validate each field
    for (const [key, schema] of Object.entries(this.shape)) {
      const fieldPath = path ? `${path}.${key}` : key;
      const result = schema.validate(value[key], fieldPath);
      errors.push(...result.errors);
    }

    return { value, errors };
  }
}

/**
 * Validator factory
 */
const v = {
  string: () => new StringSchema(),
  number: () => new NumberSchema(),
  boolean: () => new BooleanSchema(),
  array: (itemSchema) => new ArraySchema(itemSchema),
  object: (shape) => new ObjectSchema(shape),
};

/**
 * Validate data against schema
 */
function validate(schema, data) {
  const result = schema.validate(data);

  if (result.errors.length > 0) {
    throw new ValidationError(result.errors);
  }

  return result.value;
}

// Demo
console.log(`${c.bold}1. String Validation${c.reset}`);

const emailSchema = v.string().email();
const tests1 = ['test@example.com', 'invalid-email', ''];

for (const test of tests1) {
  const result = emailSchema.validate(test, 'email');
  const status = result.errors.length === 0 ? c.green + '✓' : c.red + '✗';
  const error = result.errors[0]?.message || '';
  console.log(`  ${status}${c.reset} "${test}" ${error ? c.dim + '- ' + error + c.reset : ''}`);
}

console.log(`\n${c.bold}2. Number Validation${c.reset}`);

const ageSchema = v.number().integer().min(0).max(150);
const tests2 = [25, -5, 200, 3.14, 'not a number'];

for (const test of tests2) {
  const result = ageSchema.validate(test, 'age');
  const status = result.errors.length === 0 ? c.green + '✓' : c.red + '✗';
  const error = result.errors[0]?.message || '';
  console.log(`  ${status}${c.reset} ${JSON.stringify(test)} ${error ? c.dim + '- ' + error + c.reset : ''}`);
}

console.log(`\n${c.bold}3. Object Validation${c.reset}`);

const userSchema = v.object({
  name: v.string().min(2).max(50),
  email: v.string().email(),
  age: v.number().integer().min(0).optional(),
  role: v.string().enum(['admin', 'user', 'guest']).default('user'),
});

const validUser = { name: 'Alice', email: 'alice@example.com', age: 30 };
const invalidUser = { name: 'A', email: 'not-an-email', age: -5 };

console.log(`  Valid user:`);
try {
  validate(userSchema, validUser);
  console.log(`    ${c.green}✓ Validation passed${c.reset}`);
} catch (e) {
  console.log(`    ${c.red}✗ ${e.errors.map(e => e.message).join(', ')}${c.reset}`);
}

console.log(`  Invalid user:`);
try {
  validate(userSchema, invalidUser);
  console.log(`    ${c.green}✓ Validation passed${c.reset}`);
} catch (e) {
  for (const err of e.errors) {
    console.log(`    ${c.red}✗${c.reset} ${err.path}: ${err.message}`);
  }
}

console.log(`\n${c.bold}4. Array Validation${c.reset}`);

const tagsSchema = v.array(v.string().min(1).max(20)).min(1).max(5);

const validTags = ['javascript', 'nodejs', 'howth'];
const invalidTags = ['', 'this-tag-is-way-too-long-for-validation', 'ok'];
const emptyTags = [];

for (const test of [validTags, invalidTags, emptyTags]) {
  const result = tagsSchema.validate(test, 'tags');
  const status = result.errors.length === 0 ? c.green + '✓' : c.red + '✗';
  console.log(`  ${status}${c.reset} ${JSON.stringify(test).slice(0, 50)}`);
  for (const err of result.errors.slice(0, 2)) {
    console.log(`    ${c.dim}${err.path}: ${err.message}${c.reset}`);
  }
}

console.log(`\n${c.bold}5. Nested Object Validation${c.reset}`);

const orderSchema = v.object({
  id: v.string().pattern(/^ORD-\d+$/, 'Invalid order ID format'),
  customer: v.object({
    name: v.string().min(1),
    email: v.string().email(),
    address: v.object({
      street: v.string(),
      city: v.string(),
      zip: v.string().pattern(/^\d{5}$/, 'ZIP must be 5 digits'),
    }),
  }),
  items: v.array(v.object({
    sku: v.string(),
    quantity: v.number().integer().positive(),
    price: v.number().positive(),
  })).min(1),
  total: v.number().positive(),
});

const order = {
  id: 'ORD-12345',
  customer: {
    name: 'Bob',
    email: 'bob@example.com',
    address: {
      street: '123 Main St',
      city: 'New York',
      zip: '10001',
    },
  },
  items: [
    { sku: 'WIDGET-1', quantity: 2, price: 9.99 },
    { sku: 'GADGET-2', quantity: 1, price: 19.99 },
  ],
  total: 39.97,
};

const orderResult = orderSchema.validate(order);
console.log(`  Order validation: ${orderResult.errors.length === 0 ? c.green + '✓ passed' : c.red + '✗ failed'}${c.reset}`);

console.log(`\n${c.bold}6. Custom Validators${c.reset}`);

const passwordSchema = v.string()
  .min(8)
  .custom(v => /[A-Z]/.test(v), 'Must contain uppercase letter')
  .custom(v => /[a-z]/.test(v), 'Must contain lowercase letter')
  .custom(v => /[0-9]/.test(v), 'Must contain number')
  .custom(v => /[!@#$%^&*]/.test(v), 'Must contain special character');

const passwords = ['weak', 'StrongPass1!', 'NoSpecial1'];

for (const pwd of passwords) {
  const result = passwordSchema.validate(pwd, 'password');
  const status = result.errors.length === 0 ? c.green + '✓' : c.red + '✗';
  console.log(`  ${status}${c.reset} "${pwd}"`);
  for (const err of result.errors) {
    console.log(`    ${c.dim}${err.message}${c.reset}`);
  }
}

console.log(`\n${c.green}${c.bold}Validator demo completed!${c.reset}\n`);
