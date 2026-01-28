/**
 * Job Queue Example
 *
 * A simple in-memory job queue with:
 * - Priority queues
 * - Job retries
 * - Delayed jobs
 * - Concurrency control
 * - Job events
 *
 * Run: howth run --native examples/job-queue/queue.js
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

console.log(`\n${c.bold}${c.cyan}Job Queue Demo${c.reset}\n`);

/**
 * Job class
 */
class Job {
  static idCounter = 0;

  constructor(name, data, options = {}) {
    this.id = ++Job.idCounter;
    this.name = name;
    this.data = data;
    this.priority = options.priority || 0;      // Higher = more important
    this.delay = options.delay || 0;            // Delay in ms
    this.maxRetries = options.maxRetries || 3;
    this.retryDelay = options.retryDelay || 1000;
    this.timeout = options.timeout || 30000;

    this.status = 'pending';
    this.attempts = 0;
    this.result = null;
    this.error = null;
    this.createdAt = Date.now();
    this.startedAt = null;
    this.completedAt = null;
    this.runAt = Date.now() + this.delay;
  }

  toJSON() {
    return {
      id: this.id,
      name: this.name,
      status: this.status,
      attempts: this.attempts,
      priority: this.priority,
    };
  }
}

/**
 * Job Queue
 */
class JobQueue extends EventEmitter {
  constructor(options = {}) {
    super();
    this.concurrency = options.concurrency || 2;
    this.handlers = new Map();
    this.jobs = new Map();
    this.pending = [];
    this.active = new Set();
    this.running = false;
    this.processTimer = null;

    this.stats = {
      processed: 0,
      failed: 0,
      retried: 0,
    };
  }

  // Register a job handler
  process(name, handler) {
    this.handlers.set(name, handler);
    return this;
  }

  // Add a job to the queue
  add(name, data, options = {}) {
    const job = new Job(name, data, options);
    this.jobs.set(job.id, job);
    this.pending.push(job);

    // Sort by priority (higher first) then by runAt
    this.pending.sort((a, b) => {
      if (a.priority !== b.priority) return b.priority - a.priority;
      return a.runAt - b.runAt;
    });

    this.emit('added', job);
    this.tick();

    return job;
  }

  // Start processing
  start() {
    if (this.running) return;
    this.running = true;
    this.emit('start');
    this.tick();
  }

  // Stop processing
  stop() {
    this.running = false;
    if (this.processTimer) {
      clearTimeout(this.processTimer);
      this.processTimer = null;
    }
    this.emit('stop');
  }

  // Process next jobs
  tick() {
    if (!this.running) return;

    // Clear existing timer
    if (this.processTimer) {
      clearTimeout(this.processTimer);
      this.processTimer = null;
    }

    const now = Date.now();

    // Find jobs ready to run
    while (this.active.size < this.concurrency && this.pending.length > 0) {
      // Find next ready job
      const index = this.pending.findIndex(j => j.runAt <= now);
      if (index === -1) break;

      const job = this.pending.splice(index, 1)[0];
      this.runJob(job);
    }

    // Schedule next tick for delayed jobs
    const nextJob = this.pending.find(j => j.runAt > now);
    if (nextJob) {
      const delay = Math.max(0, nextJob.runAt - now);
      this.processTimer = setTimeout(() => this.tick(), delay);
    }
  }

  // Run a single job
  async runJob(job) {
    const handler = this.handlers.get(job.name);
    if (!handler) {
      job.status = 'failed';
      job.error = `No handler for job type: ${job.name}`;
      this.emit('failed', job, new Error(job.error));
      return;
    }

    job.status = 'active';
    job.attempts++;
    job.startedAt = Date.now();
    this.active.add(job.id);

    this.emit('active', job);

    try {
      // Run with timeout
      const result = await Promise.race([
        handler(job),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Job timeout')), job.timeout)
        ),
      ]);

      job.status = 'completed';
      job.result = result;
      job.completedAt = Date.now();
      this.stats.processed++;

      this.emit('completed', job, result);

    } catch (error) {
      job.error = error.message;

      if (job.attempts < job.maxRetries) {
        // Retry
        job.status = 'pending';
        job.runAt = Date.now() + job.retryDelay * job.attempts;
        this.pending.push(job);
        this.stats.retried++;

        this.emit('retry', job, error);
      } else {
        // Failed permanently
        job.status = 'failed';
        job.completedAt = Date.now();
        this.stats.failed++;

        this.emit('failed', job, error);
      }
    } finally {
      this.active.delete(job.id);
      this.tick();
    }
  }

  // Get job by ID
  getJob(id) {
    return this.jobs.get(id);
  }

  // Get queue status
  getStatus() {
    return {
      running: this.running,
      pending: this.pending.length,
      active: this.active.size,
      concurrency: this.concurrency,
      stats: { ...this.stats },
    };
  }

  // Get all jobs by status
  getJobs(status = null) {
    const jobs = [...this.jobs.values()];
    if (status) {
      return jobs.filter(j => j.status === status);
    }
    return jobs;
  }
}

// Create queue
const queue = new JobQueue({ concurrency: 2 });

// Register handlers
queue.process('email', async (job) => {
  console.log(`  ${c.blue}[email]${c.reset} Sending email to ${job.data.to}...`);
  await new Promise(r => setTimeout(r, 100));
  return { sent: true, to: job.data.to };
});

queue.process('report', async (job) => {
  console.log(`  ${c.blue}[report]${c.reset} Generating ${job.data.type} report...`);
  await new Promise(r => setTimeout(r, 150));
  return { type: job.data.type, rows: Math.floor(Math.random() * 1000) };
});

queue.process('cleanup', async (job) => {
  console.log(`  ${c.blue}[cleanup]${c.reset} Cleaning up ${job.data.target}...`);
  await new Promise(r => setTimeout(r, 50));
  return { cleaned: true };
});

queue.process('flaky', async (job) => {
  console.log(`  ${c.blue}[flaky]${c.reset} Running flaky job (attempt ${job.attempts})...`);
  await new Promise(r => setTimeout(r, 50));

  // 50% chance of failure
  if (Math.random() < 0.5) {
    throw new Error('Random failure');
  }
  return { success: true };
});

// Event handlers
queue.on('added', (job) => {
  console.log(`  ${c.dim}+ Added job #${job.id} (${job.name})${c.reset}`);
});

queue.on('active', (job) => {
  console.log(`  ${c.yellow}▶${c.reset} Started job #${job.id} (${job.name})`);
});

queue.on('completed', (job, result) => {
  const duration = job.completedAt - job.startedAt;
  console.log(`  ${c.green}✓${c.reset} Completed job #${job.id} in ${duration}ms`);
});

queue.on('retry', (job, error) => {
  console.log(`  ${c.yellow}↻${c.reset} Retrying job #${job.id}: ${error.message}`);
});

queue.on('failed', (job, error) => {
  console.log(`  ${c.red}✗${c.reset} Failed job #${job.id}: ${error.message}`);
});

// Demo
console.log(`${c.bold}1. Adding jobs to queue${c.reset}\n`);

// Add various jobs
queue.add('email', { to: 'alice@example.com', subject: 'Welcome!' }, { priority: 2 });
queue.add('email', { to: 'bob@example.com', subject: 'Newsletter' }, { priority: 1 });
queue.add('report', { type: 'monthly' }, { priority: 3 });
queue.add('cleanup', { target: 'temp files' }, { priority: 0 });
queue.add('flaky', {}, { maxRetries: 3, retryDelay: 200 });
queue.add('email', { to: 'charlie@example.com' }, { delay: 200 }); // Delayed

console.log(`\n${c.bold}2. Starting queue processing${c.reset}\n`);
queue.start();

// Wait for jobs to complete
await new Promise(resolve => {
  const checkDone = setInterval(() => {
    const status = queue.getStatus();
    if (status.pending === 0 && status.active === 0) {
      clearInterval(checkDone);
      resolve();
    }
  }, 100);
});

// Summary
console.log(`\n${c.bold}3. Queue Summary${c.reset}`);
const status = queue.getStatus();
console.log(`  Processed: ${c.green}${status.stats.processed}${c.reset}`);
console.log(`  Failed:    ${c.red}${status.stats.failed}${c.reset}`);
console.log(`  Retried:   ${c.yellow}${status.stats.retried}${c.reset}`);

console.log(`\n${c.bold}4. Job Details${c.reset}`);
const jobs = queue.getJobs();
for (const job of jobs) {
  const icon = job.status === 'completed' ? c.green + '✓' :
               job.status === 'failed' ? c.red + '✗' : c.yellow + '○';
  const duration = job.completedAt && job.startedAt
    ? ` (${job.completedAt - job.startedAt}ms)`
    : '';
  console.log(`  ${icon}${c.reset} #${job.id} ${job.name} - ${job.status}${duration}`);
}

queue.stop();

console.log(`\n${c.green}${c.bold}Job queue demo completed!${c.reset}\n`);
