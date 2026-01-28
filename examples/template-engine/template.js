/**
 * Template Engine Example
 *
 * A simple template engine with:
 * - Variable interpolation
 * - Conditionals
 * - Loops
 * - Includes/partials
 * - Filters
 * - Escaping
 *
 * Run: howth run --native examples/template-engine/template.js
 */

const fs = require('fs');
const path = require('path');

const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}Template Engine Demo${c.reset}\n`);

/**
 * Simple Template Engine
 */
class TemplateEngine {
  constructor(options = {}) {
    this.cache = new Map();
    this.filters = new Map();
    this.partials = new Map();
    this.basePath = options.basePath || '.';

    // Register default filters
    this.registerFilter('upper', (val) => String(val).toUpperCase());
    this.registerFilter('lower', (val) => String(val).toLowerCase());
    this.registerFilter('capitalize', (val) => String(val).charAt(0).toUpperCase() + String(val).slice(1));
    this.registerFilter('trim', (val) => String(val).trim());
    this.registerFilter('json', (val) => JSON.stringify(val));
    this.registerFilter('length', (val) => Array.isArray(val) ? val.length : String(val).length);
    this.registerFilter('default', (val, def) => val || def);
    this.registerFilter('date', (val) => new Date(val).toLocaleDateString());
    this.registerFilter('escape', (val) => this.escapeHtml(String(val)));
  }

  // Register a custom filter
  registerFilter(name, fn) {
    this.filters.set(name, fn);
  }

  // Register a partial template
  registerPartial(name, template) {
    this.partials.set(name, template);
  }

  // Escape HTML
  escapeHtml(str) {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // Get value from nested path (e.g., "user.name")
  getValue(obj, path) {
    return path.split('.').reduce((curr, key) => curr?.[key], obj);
  }

  // Apply filters to value
  applyFilters(value, filterStr) {
    const filters = filterStr.split('|').map(f => f.trim());

    for (const filterExpr of filters) {
      const match = filterExpr.match(/^(\w+)(?:\(([^)]*)\))?$/);
      if (!match) continue;

      const [, name, args] = match;
      const filter = this.filters.get(name);

      if (filter) {
        const filterArgs = args ? args.split(',').map(a => a.trim().replace(/^['"]|['"]$/g, '')) : [];
        value = filter(value, ...filterArgs);
      }
    }

    return value;
  }

  // Parse and evaluate expression
  evaluateExpression(expr, context) {
    // Handle filters: {{ value | filter1 | filter2 }}
    const [varPart, ...filterParts] = expr.split('|').map(s => s.trim());

    let value = this.getValue(context, varPart);

    if (filterParts.length > 0) {
      value = this.applyFilters(value, filterParts.join('|'));
    }

    return value;
  }

  // Compile template to function
  compile(template) {
    let code = 'let __output = "";\n';
    let cursor = 0;

    // Regex patterns
    const patterns = {
      variable: /\{\{\s*(.+?)\s*\}\}/g,
      ifStart: /\{%\s*if\s+(.+?)\s*%\}/g,
      elif: /\{%\s*elif\s+(.+?)\s*%\}/g,
      else: /\{%\s*else\s*%\}/g,
      endif: /\{%\s*endif\s*%\}/g,
      forStart: /\{%\s*for\s+(\w+)\s+in\s+(.+?)\s*%\}/g,
      endfor: /\{%\s*endfor\s*%\}/g,
      include: /\{%\s*include\s+['"](.+?)['"]\s*%\}/g,
    };

    // Simple tokenizer
    const tokens = [];
    const combinedPattern = /(\{\{.+?\}\}|\{%.+?%\})/g;
    let match;
    let lastIndex = 0;

    while ((match = combinedPattern.exec(template)) !== null) {
      if (match.index > lastIndex) {
        tokens.push({ type: 'text', value: template.slice(lastIndex, match.index) });
      }
      tokens.push({ type: 'tag', value: match[1] });
      lastIndex = combinedPattern.lastIndex;
    }

    if (lastIndex < template.length) {
      tokens.push({ type: 'text', value: template.slice(lastIndex) });
    }

    // Generate code from tokens
    for (const token of tokens) {
      if (token.type === 'text') {
        code += `__output += ${JSON.stringify(token.value)};\n`;
        continue;
      }

      const tag = token.value;

      // Variable interpolation
      if (tag.startsWith('{{')) {
        const expr = tag.slice(2, -2).trim();
        code += `__output += __engine.evaluateExpression(${JSON.stringify(expr)}, __context) ?? '';\n`;
        continue;
      }

      // Control structures
      let m;

      if ((m = /\{%\s*if\s+(.+?)\s*%\}/.exec(tag))) {
        code += `if (__engine.getValue(__context, ${JSON.stringify(m[1])})) {\n`;
      } else if ((m = /\{%\s*elif\s+(.+?)\s*%\}/.exec(tag))) {
        code += `} else if (__engine.getValue(__context, ${JSON.stringify(m[1])})) {\n`;
      } else if (/\{%\s*else\s*%\}/.test(tag)) {
        code += `} else {\n`;
      } else if (/\{%\s*endif\s*%\}/.test(tag)) {
        code += `}\n`;
      } else if ((m = /\{%\s*for\s+(\w+)\s+in\s+(.+?)\s*%\}/.exec(tag))) {
        const [, varName, arrExpr] = m;
        code += `for (const ${varName} of (__engine.getValue(__context, ${JSON.stringify(arrExpr)}) || [])) {\n`;
        code += `  const __prevContext = __context;\n`;
        code += `  __context = { ...__context, ${varName} };\n`;
      } else if (/\{%\s*endfor\s*%\}/.test(tag)) {
        code += `  __context = __prevContext;\n`;
        code += `}\n`;
      } else if ((m = /\{%\s*include\s+['"](.+?)['"]\s*%\}/.exec(tag))) {
        const partialName = m[1];
        code += `__output += __engine.renderPartial(${JSON.stringify(partialName)}, __context);\n`;
      }
    }

    code += 'return __output;';

    // Create and return render function
    try {
      const fn = new Function('__engine', '__context', code);
      return (context) => fn(this, context);
    } catch (e) {
      throw new Error(`Template compilation error: ${e.message}\n${code}`);
    }
  }

  // Render partial
  renderPartial(name, context) {
    const partial = this.partials.get(name);
    if (!partial) {
      return `<!-- Partial "${name}" not found -->`;
    }
    return this.render(partial, context);
  }

  // Render template with context
  render(template, context = {}) {
    // Check cache
    let renderFn = this.cache.get(template);

    if (!renderFn) {
      renderFn = this.compile(template);
      this.cache.set(template, renderFn);
    }

    return renderFn(context);
  }
}

// Create engine
const engine = new TemplateEngine();

// Register partials
engine.registerPartial('header', `
<header>
  <h1>{{ title | upper }}</h1>
  <nav>{% for item in nav %}<a href="{{ item.url }}">{{ item.text }}</a> {% endfor %}</nav>
</header>
`);

engine.registerPartial('footer', `
<footer>&copy; {{ year }} {{ company }}</footer>
`);

// Demo templates
console.log(`${c.bold}1. Variable Interpolation${c.reset}`);
const t1 = `Hello, {{ name }}! You have {{ count }} messages.`;
console.log(`  Template: ${c.dim}${t1}${c.reset}`);
console.log(`  Output:   ${engine.render(t1, { name: 'Alice', count: 5 })}`);

console.log(`\n${c.bold}2. Filters${c.reset}`);
const t2 = `Name: {{ name | upper }}, Items: {{ items | length }}, Date: {{ date | date }}`;
console.log(`  Template: ${c.dim}${t2}${c.reset}`);
console.log(`  Output:   ${engine.render(t2, { name: 'alice', items: [1, 2, 3], date: '2024-01-15' })}`);

console.log(`\n${c.bold}3. Conditionals${c.reset}`);
const t3 = `{% if admin %}Admin user{% elif member %}Member{% else %}Guest{% endif %}`;
console.log(`  Template: ${c.dim}${t3}${c.reset}`);
console.log(`  admin=true:   ${engine.render(t3, { admin: true })}`);
console.log(`  member=true:  ${engine.render(t3, { member: true })}`);
console.log(`  neither:      ${engine.render(t3, {})}`);

console.log(`\n${c.bold}4. Loops${c.reset}`);
const t4 = `Users: {% for user in users %}{{ user.name }} ({{ user.role }}), {% endfor %}`;
console.log(`  Template: ${c.dim}${t4}${c.reset}`);
console.log(`  Output:   ${engine.render(t4, {
  users: [
    { name: 'Alice', role: 'admin' },
    { name: 'Bob', role: 'user' },
    { name: 'Charlie', role: 'user' },
  ]
})}`);

console.log(`\n${c.bold}5. Nested Data${c.reset}`);
const t5 = `User: {{ user.profile.name }}, City: {{ user.profile.address.city }}`;
console.log(`  Template: ${c.dim}${t5}${c.reset}`);
console.log(`  Output:   ${engine.render(t5, {
  user: {
    profile: {
      name: 'Alice',
      address: { city: 'New York' }
    }
  }
})}`);

console.log(`\n${c.bold}6. Partials / Includes${c.reset}`);
const t6 = `{% include "header" %}
<main>Welcome, {{ user }}!</main>
{% include "footer" %}`;
console.log(`  Template: ${c.dim}(uses header and footer partials)${c.reset}`);
console.log(`  Output:`);
const output = engine.render(t6, {
  title: 'My Site',
  nav: [{ url: '/', text: 'Home' }, { url: '/about', text: 'About' }],
  user: 'Alice',
  year: 2024,
  company: 'Howth Inc.'
});
output.trim().split('\n').forEach(line => console.log(`    ${c.dim}${line}${c.reset}`));

console.log(`\n${c.bold}7. Custom Filters${c.reset}`);
engine.registerFilter('currency', (val, symbol = '$') => `${symbol}${Number(val).toFixed(2)}`);
engine.registerFilter('pluralize', (val, singular, plural) =>
  val === 1 ? singular : (plural || singular + 's')
);

const t7 = `Price: {{ price | currency('$') }}, {{ count }} {{ count | pluralize('item', 'items') }}`;
console.log(`  Template: ${c.dim}${t7}${c.reset}`);
console.log(`  Output:   ${engine.render(t7, { price: 29.99, count: 3 })}`);

console.log(`\n${c.bold}8. Full Page Template${c.reset}`);
const pageTemplate = `<!DOCTYPE html>
<html>
<head><title>{{ title }}</title></head>
<body>
  <h1>{{ title }}</h1>
  {% if showIntro %}<p>{{ intro }}</p>{% endif %}
  <ul>
  {% for item in items %}
    <li>{{ item.name }} - {{ item.price | currency }}</li>
  {% endfor %}
  </ul>
  <p>Total: {{ total | currency }}</p>
</body>
</html>`;

const pageData = {
  title: 'Product List',
  showIntro: true,
  intro: 'Check out our products!',
  items: [
    { name: 'Widget', price: 9.99 },
    { name: 'Gadget', price: 19.99 },
    { name: 'Gizmo', price: 14.99 },
  ],
  total: 44.97,
};

console.log(`  Rendered page preview:`);
const fullPage = engine.render(pageTemplate, pageData);
fullPage.split('\n').slice(0, 12).forEach(line =>
  console.log(`    ${c.dim}${line}${c.reset}`)
);
console.log(`    ${c.dim}...${c.reset}`);

console.log(`\n${c.bold}9. Performance Test${c.reset}`);
const perfTemplate = `{% for i in items %}{{ i }}{% endfor %}`;
const perfData = { items: Array.from({ length: 1000 }, (_, i) => i) };

const start = Date.now();
for (let i = 0; i < 100; i++) {
  engine.render(perfTemplate, perfData);
}
const duration = Date.now() - start;

console.log(`  100 iterations of 1000-item loop: ${duration}ms`);
console.log(`  ${c.dim}(cached compilation)${c.reset}`);

console.log(`\n${c.green}${c.bold}Template engine demo completed!${c.reset}\n`);
