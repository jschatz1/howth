/**
 * Event System Example
 *
 * Demonstrates Node.js EventEmitter:
 * - Basic events
 * - Multiple listeners
 * - Once listeners
 * - Error handling
 * - Event chaining
 * - Custom event classes
 * - Async events
 *
 * Run: howth run --native examples/event-system/events.js
 */

const EventEmitter = require('events');

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

console.log(`\n${c.bold}${c.cyan}Event System Demo${c.reset}\n`);

// 1. Basic EventEmitter
console.log(`${c.bold}1. Basic EventEmitter${c.reset}`);

const emitter = new EventEmitter();
const messages = [];

emitter.on('message', (text) => {
  messages.push(text);
});

emitter.emit('message', 'Hello');
emitter.emit('message', 'World');

console.log(`  Messages received: ${messages.join(', ')}`);

// 2. Multiple listeners
console.log(`\n${c.bold}2. Multiple Listeners${c.reset}`);

const counter = new EventEmitter();
let count1 = 0, count2 = 0;

counter.on('increment', () => count1++);
counter.on('increment', () => count2 += 2);

counter.emit('increment');
counter.emit('increment');

console.log(`  Listener 1 count: ${count1}`);
console.log(`  Listener 2 count: ${count2}`);
console.log(`  Total listeners: ${counter.listenerCount('increment')}`);

// 3. Once listener
console.log(`\n${c.bold}3. Once Listener${c.reset}`);

const oneTime = new EventEmitter();
let onceCount = 0;

oneTime.once('trigger', () => {
  onceCount++;
  console.log(`  ${c.green}Once listener fired!${c.reset}`);
});

oneTime.emit('trigger');
oneTime.emit('trigger'); // Won't fire
oneTime.emit('trigger'); // Won't fire

console.log(`  Once listener fired ${onceCount} time(s)`);

// 4. Event with data
console.log(`\n${c.bold}4. Events with Data${c.reset}`);

const dataEmitter = new EventEmitter();

dataEmitter.on('user:login', (user, timestamp) => {
  console.log(`  ${c.green}Login:${c.reset} ${user.name} at ${timestamp}`);
});

dataEmitter.on('user:logout', (user) => {
  console.log(`  ${c.yellow}Logout:${c.reset} ${user.name}`);
});

dataEmitter.emit('user:login', { name: 'Alice', id: 1 }, new Date().toISOString());
dataEmitter.emit('user:logout', { name: 'Alice', id: 1 });

// 5. Error handling
console.log(`\n${c.bold}5. Error Handling${c.reset}`);

const errorEmitter = new EventEmitter();

errorEmitter.on('error', (err) => {
  console.log(`  ${c.red}Error caught:${c.reset} ${err.message}`);
});

errorEmitter.emit('error', new Error('Something went wrong'));
console.log(`  ${c.dim}(Program continues after error)${c.reset}`);

// 6. Custom Event Class
console.log(`\n${c.bold}6. Custom Event Class${c.reset}`);

class Database extends EventEmitter {
  constructor() {
    super();
    this.connected = false;
    this.data = new Map();
  }

  connect() {
    this.connected = true;
    this.emit('connect', { timestamp: Date.now() });
  }

  disconnect() {
    this.connected = false;
    this.emit('disconnect');
  }

  insert(key, value) {
    this.data.set(key, value);
    this.emit('insert', { key, value });
  }

  get(key) {
    const value = this.data.get(key);
    this.emit('query', { key, found: value !== undefined });
    return value;
  }
}

const db = new Database();

db.on('connect', (info) => console.log(`  ${c.green}Connected${c.reset} at ${info.timestamp}`));
db.on('disconnect', () => console.log(`  ${c.yellow}Disconnected${c.reset}`));
db.on('insert', ({ key, value }) => console.log(`  ${c.blue}Inserted:${c.reset} ${key} = ${JSON.stringify(value)}`));
db.on('query', ({ key, found }) => console.log(`  ${c.dim}Query: ${key} (found: ${found})${c.reset}`));

db.connect();
db.insert('user:1', { name: 'Alice' });
db.insert('user:2', { name: 'Bob' });
db.get('user:1');
db.get('user:3');
db.disconnect();

// 7. Event chaining
console.log(`\n${c.bold}7. Event Chaining / Pipeline${c.reset}`);

class Pipeline extends EventEmitter {
  constructor() {
    super();
    this.stages = [];
  }

  addStage(name, handler) {
    this.stages.push({ name, handler });
    return this; // Allow chaining
  }

  async process(input) {
    let data = input;

    for (const stage of this.stages) {
      this.emit('stage:start', stage.name);
      data = await stage.handler(data);
      this.emit('stage:complete', stage.name, data);
    }

    this.emit('complete', data);
    return data;
  }
}

const pipeline = new Pipeline();

pipeline.on('stage:start', (name) => console.log(`  ${c.dim}Starting: ${name}${c.reset}`));
pipeline.on('stage:complete', (name) => console.log(`  ${c.green}✓${c.reset} ${name} complete`));
pipeline.on('complete', (result) => console.log(`  ${c.bold}Final result: ${result}${c.reset}`));

pipeline
  .addStage('parse', (data) => parseInt(data, 10))
  .addStage('double', (data) => data * 2)
  .addStage('format', (data) => `Result: ${data}`);

await pipeline.process('21');

// 8. Removing listeners
console.log(`\n${c.bold}8. Removing Listeners${c.reset}`);

const removable = new EventEmitter();
let callCount = 0;

const handler = () => callCount++;

removable.on('tick', handler);
removable.emit('tick');
removable.emit('tick');

console.log(`  Before remove: ${callCount} calls`);

removable.removeListener('tick', handler);
removable.emit('tick');
removable.emit('tick');

console.log(`  After remove: ${callCount} calls`);

// 9. Max listeners
console.log(`\n${c.bold}9. Max Listeners${c.reset}`);

const maxEmitter = new EventEmitter();
maxEmitter.setMaxListeners(3);

console.log(`  Default max listeners: 10`);
console.log(`  Set max listeners: ${maxEmitter.getMaxListeners()}`);

// 10. Event names
console.log(`\n${c.bold}10. Event Introspection${c.reset}`);

const introspect = new EventEmitter();
introspect.on('foo', () => {});
introspect.on('bar', () => {});
introspect.on('bar', () => {});
introspect.on('baz', () => {});

console.log(`  Event names: ${introspect.eventNames().join(', ')}`);
console.log(`  'bar' listeners: ${introspect.listenerCount('bar')}`);

// 11. Prepend listener
console.log(`\n${c.bold}11. Prepend Listener${c.reset}`);

const prependEmitter = new EventEmitter();
const order = [];

prependEmitter.on('test', () => order.push('first'));
prependEmitter.on('test', () => order.push('second'));
prependEmitter.prependListener('test', () => order.push('prepended'));

prependEmitter.emit('test');
console.log(`  Execution order: ${order.join(' -> ')}`);

// 12. Async event handling
console.log(`\n${c.bold}12. Async Event Handling${c.reset}`);

class AsyncEmitter extends EventEmitter {
  async emitAsync(event, ...args) {
    const listeners = this.listeners(event);
    for (const listener of listeners) {
      await listener(...args);
    }
  }
}

const asyncEmitter = new AsyncEmitter();
const asyncResults = [];

asyncEmitter.on('process', async (data) => {
  await new Promise(r => setTimeout(r, 50));
  asyncResults.push(`processed: ${data}`);
});

asyncEmitter.on('process', async (data) => {
  await new Promise(r => setTimeout(r, 30));
  asyncResults.push(`logged: ${data}`);
});

await asyncEmitter.emitAsync('process', 'test-data');
console.log(`  Async results: ${asyncResults.join(', ')}`);

// 13. Event-based state machine
console.log(`\n${c.bold}13. State Machine${c.reset}`);

class StateMachine extends EventEmitter {
  constructor(initialState) {
    super();
    this.state = initialState;
    this.transitions = {};
  }

  addTransition(from, event, to) {
    if (!this.transitions[from]) this.transitions[from] = {};
    this.transitions[from][event] = to;
  }

  trigger(event) {
    const nextState = this.transitions[this.state]?.[event];
    if (nextState) {
      const prevState = this.state;
      this.state = nextState;
      this.emit('transition', { from: prevState, to: nextState, event });
      return true;
    }
    return false;
  }
}

const trafficLight = new StateMachine('red');
trafficLight.addTransition('red', 'timer', 'green');
trafficLight.addTransition('green', 'timer', 'yellow');
trafficLight.addTransition('yellow', 'timer', 'red');

trafficLight.on('transition', ({ from, to }) => {
  console.log(`  ${c.dim}${from}${c.reset} -> ${c.bold}${to}${c.reset}`);
});

trafficLight.trigger('timer');
trafficLight.trigger('timer');
trafficLight.trigger('timer');

console.log(`\n${c.green}${c.bold}Event system demo completed!${c.reset}\n`);
