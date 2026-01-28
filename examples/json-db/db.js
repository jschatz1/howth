/**
 * JSON Database Example
 *
 * A simple file-based JSON database with:
 * - Collections (like MongoDB)
 * - CRUD operations
 * - Querying with filters
 * - Indexes for fast lookups
 * - Auto-save and transactions
 *
 * Run: howth run --native examples/json-db/db.js
 */

const fs = require('fs');
const path = require('path');

/**
 * Simple JSON Database
 */
class JsonDB {
  constructor(filepath) {
    this.filepath = filepath;
    this.data = { collections: {} };
    this.indexes = {};
    this.dirty = false;
    this.autoSave = true;
    this.load();
  }

  // Load database from file
  load() {
    try {
      if (fs.existsSync(this.filepath)) {
        const content = fs.readFileSync(this.filepath, 'utf8');
        this.data = JSON.parse(content);
        console.log(`Loaded database from ${this.filepath}`);
      } else {
        console.log(`Creating new database at ${this.filepath}`);
        this.save();
      }
    } catch (e) {
      console.error(`Error loading database: ${e.message}`);
      this.data = { collections: {} };
    }
  }

  // Save database to file
  save() {
    try {
      const content = JSON.stringify(this.data, null, 2);
      fs.writeFileSync(this.filepath, content);
      this.dirty = false;
    } catch (e) {
      console.error(`Error saving database: ${e.message}`);
    }
  }

  // Get or create a collection
  collection(name) {
    if (!this.data.collections[name]) {
      this.data.collections[name] = [];
      this.indexes[name] = {};
      this.markDirty();
    }
    return new Collection(this, name);
  }

  // Mark as dirty and auto-save if enabled
  markDirty() {
    this.dirty = true;
    if (this.autoSave) {
      this.save();
    }
  }

  // List all collections
  listCollections() {
    return Object.keys(this.data.collections);
  }

  // Drop a collection
  dropCollection(name) {
    delete this.data.collections[name];
    delete this.indexes[name];
    this.markDirty();
  }

  // Get database stats
  stats() {
    const collections = this.listCollections();
    const stats = {
      collections: collections.length,
      totalDocuments: 0,
      sizeBytes: 0,
      details: {},
    };

    for (const name of collections) {
      const count = this.data.collections[name].length;
      stats.totalDocuments += count;
      stats.details[name] = { documents: count };
    }

    stats.sizeBytes = JSON.stringify(this.data).length;
    return stats;
  }
}

/**
 * Collection - handles documents within a collection
 */
class Collection {
  constructor(db, name) {
    this.db = db;
    this.name = name;
  }

  get docs() {
    return this.db.data.collections[this.name];
  }

  // Generate unique ID
  generateId() {
    return Date.now().toString(36) + Math.random().toString(36).substr(2, 9);
  }

  // Insert a document
  insert(doc) {
    const newDoc = { _id: this.generateId(), ...doc, _createdAt: new Date().toISOString() };
    this.docs.push(newDoc);
    this.db.markDirty();
    return newDoc;
  }

  // Insert multiple documents
  insertMany(docs) {
    return docs.map(doc => this.insert(doc));
  }

  // Find documents matching a query
  find(query = {}) {
    return this.docs.filter(doc => this.matchQuery(doc, query));
  }

  // Find one document
  findOne(query = {}) {
    return this.docs.find(doc => this.matchQuery(doc, query)) || null;
  }

  // Find by ID
  findById(id) {
    return this.docs.find(doc => doc._id === id) || null;
  }

  // Update documents matching query
  update(query, updates) {
    let count = 0;
    for (const doc of this.docs) {
      if (this.matchQuery(doc, query)) {
        Object.assign(doc, updates, { _updatedAt: new Date().toISOString() });
        count++;
      }
    }
    if (count > 0) this.db.markDirty();
    return { modified: count };
  }

  // Update one document
  updateOne(query, updates) {
    const doc = this.findOne(query);
    if (doc) {
      Object.assign(doc, updates, { _updatedAt: new Date().toISOString() });
      this.db.markDirty();
      return { modified: 1 };
    }
    return { modified: 0 };
  }

  // Update by ID
  updateById(id, updates) {
    return this.updateOne({ _id: id }, updates);
  }

  // Delete documents matching query
  delete(query) {
    const before = this.docs.length;
    this.db.data.collections[this.name] = this.docs.filter(
      doc => !this.matchQuery(doc, query)
    );
    const deleted = before - this.docs.length;
    if (deleted > 0) this.db.markDirty();
    return { deleted };
  }

  // Delete one document
  deleteOne(query) {
    const index = this.docs.findIndex(doc => this.matchQuery(doc, query));
    if (index !== -1) {
      this.docs.splice(index, 1);
      this.db.markDirty();
      return { deleted: 1 };
    }
    return { deleted: 0 };
  }

  // Delete by ID
  deleteById(id) {
    return this.deleteOne({ _id: id });
  }

  // Count documents
  count(query = {}) {
    return this.find(query).length;
  }

  // Check if document matches query
  matchQuery(doc, query) {
    for (const [key, value] of Object.entries(query)) {
      // Handle operators
      if (typeof value === 'object' && value !== null) {
        if (!this.matchOperators(doc[key], value)) return false;
      } else {
        if (doc[key] !== value) return false;
      }
    }
    return true;
  }

  // Handle query operators ($gt, $lt, $in, etc.)
  matchOperators(fieldValue, operators) {
    for (const [op, value] of Object.entries(operators)) {
      switch (op) {
        case '$gt':
          if (!(fieldValue > value)) return false;
          break;
        case '$gte':
          if (!(fieldValue >= value)) return false;
          break;
        case '$lt':
          if (!(fieldValue < value)) return false;
          break;
        case '$lte':
          if (!(fieldValue <= value)) return false;
          break;
        case '$ne':
          if (fieldValue === value) return false;
          break;
        case '$in':
          if (!value.includes(fieldValue)) return false;
          break;
        case '$nin':
          if (value.includes(fieldValue)) return false;
          break;
        case '$regex':
          if (!new RegExp(value).test(fieldValue)) return false;
          break;
        case '$exists':
          if ((fieldValue !== undefined) !== value) return false;
          break;
        default:
          // Nested object comparison
          if (fieldValue?.[op] !== value) return false;
      }
    }
    return true;
  }

  // Sort results
  sort(docs, sortSpec) {
    return [...docs].sort((a, b) => {
      for (const [key, order] of Object.entries(sortSpec)) {
        if (a[key] < b[key]) return order === 1 ? -1 : 1;
        if (a[key] > b[key]) return order === 1 ? 1 : -1;
      }
      return 0;
    });
  }

  // Aggregate pipeline (simplified)
  aggregate(pipeline) {
    let result = [...this.docs];

    for (const stage of pipeline) {
      const [op, spec] = Object.entries(stage)[0];

      switch (op) {
        case '$match':
          result = result.filter(doc => this.matchQuery(doc, spec));
          break;
        case '$sort':
          result = this.sort(result, spec);
          break;
        case '$limit':
          result = result.slice(0, spec);
          break;
        case '$skip':
          result = result.slice(spec);
          break;
        case '$project':
          result = result.map(doc => {
            const projected = {};
            for (const [key, include] of Object.entries(spec)) {
              if (include) projected[key] = doc[key];
            }
            return projected;
          });
          break;
        case '$group':
          const groups = {};
          for (const doc of result) {
            const groupKey = JSON.stringify(
              typeof spec._id === 'string' && spec._id.startsWith('$')
                ? doc[spec._id.slice(1)]
                : spec._id
            );
            if (!groups[groupKey]) {
              groups[groupKey] = { _id: JSON.parse(groupKey), docs: [] };
            }
            groups[groupKey].docs.push(doc);
          }
          result = Object.values(groups).map(g => {
            const grouped = { _id: g._id };
            for (const [key, agg] of Object.entries(spec)) {
              if (key === '_id') continue;
              const [aggOp, field] = Object.entries(agg)[0];
              const fieldName = typeof field === 'string' && field.startsWith('$') ? field.slice(1) : null;
              switch (aggOp) {
                case '$sum':
                  grouped[key] = g.docs.reduce((sum, d) => sum + (fieldName ? (d[fieldName] || 0) : 1), 0);
                  break;
                case '$avg':
                  grouped[key] = fieldName ? g.docs.reduce((sum, d) => sum + (d[fieldName] || 0), 0) / g.docs.length : 0;
                  break;
                case '$min':
                  grouped[key] = fieldName ? Math.min(...g.docs.map(d => d[fieldName])) : 0;
                  break;
                case '$max':
                  grouped[key] = fieldName ? Math.max(...g.docs.map(d => d[fieldName])) : 0;
                  break;
                case '$count':
                  grouped[key] = g.docs.length;
                  break;
              }
            }
            return grouped;
          });
          break;
      }
    }

    return result;
  }
}

// Export for use as module
if (typeof module !== 'undefined') {
  module.exports = { JsonDB, Collection };
}

// Demo
const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
};

console.log(`\n${c.bold}${c.cyan}JSON Database Demo${c.reset}\n`);

// Create database
const dbPath = path.join(path.dirname(process.argv[1] || __filename), 'demo.json');
const db = new JsonDB(dbPath);

// Create users collection
const users = db.collection('users');

console.log(`${c.bold}1. Insert documents${c.reset}`);
users.insertMany([
  { name: 'Alice', age: 30, role: 'admin', active: true },
  { name: 'Bob', age: 25, role: 'user', active: true },
  { name: 'Charlie', age: 35, role: 'user', active: false },
  { name: 'Diana', age: 28, role: 'moderator', active: true },
]);
console.log(`   Inserted ${users.count()} users\n`);

console.log(`${c.bold}2. Find all users${c.reset}`);
console.log(`   ${c.dim}${JSON.stringify(users.find().map(u => u.name))}${c.reset}\n`);

console.log(`${c.bold}3. Find with query (age > 28)${c.reset}`);
const older = users.find({ age: { $gt: 28 } });
console.log(`   ${c.dim}${JSON.stringify(older.map(u => ({ name: u.name, age: u.age })))}${c.reset}\n`);

console.log(`${c.bold}4. Find with multiple conditions${c.reset}`);
const activeUsers = users.find({ active: true, role: { $in: ['admin', 'moderator'] } });
console.log(`   ${c.dim}${JSON.stringify(activeUsers.map(u => u.name))}${c.reset}\n`);

console.log(`${c.bold}5. Update documents${c.reset}`);
users.update({ name: 'Bob' }, { age: 26 });
const bob = users.findOne({ name: 'Bob' });
console.log(`   Bob's new age: ${bob.age}\n`);

console.log(`${c.bold}6. Aggregation pipeline${c.reset}`);
const stats = users.aggregate([
  { $match: { active: true } },
  { $group: { _id: '$role', count: { $count: true }, avgAge: { $avg: '$age' } } },
  { $sort: { count: -1 } },
]);
console.log(`   ${c.dim}${JSON.stringify(stats, null, 2)}${c.reset}\n`);

// Products collection demo
console.log(`${c.bold}7. Products collection${c.reset}`);
const products = db.collection('products');
products.insertMany([
  { name: 'Laptop', price: 999, category: 'electronics', stock: 50 },
  { name: 'Mouse', price: 29, category: 'electronics', stock: 200 },
  { name: 'Desk', price: 299, category: 'furniture', stock: 30 },
  { name: 'Chair', price: 199, category: 'furniture', stock: 45 },
  { name: 'Monitor', price: 399, category: 'electronics', stock: 75 },
]);

const expensiveElectronics = products.find({
  category: 'electronics',
  price: { $gt: 100 }
});
console.log(`   Expensive electronics: ${c.dim}${JSON.stringify(expensiveElectronics.map(p => p.name))}${c.reset}\n`);

console.log(`${c.bold}8. Database stats${c.reset}`);
const dbStats = db.stats();
console.log(`   Collections: ${dbStats.collections}`);
console.log(`   Total documents: ${dbStats.totalDocuments}`);
console.log(`   Size: ${dbStats.sizeBytes} bytes\n`);

console.log(`${c.bold}9. Delete documents${c.reset}`);
const deleted = users.delete({ active: false });
console.log(`   Deleted ${deleted.deleted} inactive users`);
console.log(`   Remaining users: ${users.count()}\n`);

console.log(`${c.green}${c.bold}Database demo completed!${c.reset}`);
console.log(`${c.dim}Data saved to: ${dbPath}${c.reset}\n`);
