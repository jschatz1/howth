/**
 * Task Scheduler Example
 *
 * A cron-like task scheduler with:
 * - Interval-based tasks
 * - Cron expression parsing
 * - Task priorities
 * - Retry logic
 * - Task dependencies
 * - Status monitoring
 *
 * Run: howth run --native examples/task-scheduler/scheduler.js
 */

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
 * Task definition
 */
class Task {
  constructor(name, fn, options = {}) {
    this.name = name;
    this.fn = fn;
    this.interval = options.interval || null;  // ms
    this.cron = options.cron || null;          // cron expression
    this.priority = options.priority || 5;      // 1 (highest) - 10 (lowest)
    this.maxRetries = options.maxRetries || 0;
    this.retryDelay = options.retryDelay || 1000;
    this.timeout = options.timeout || 30000;
    this.dependencies = options.dependencies || [];
    this.enabled = true;

    this.lastRun = null;
    this.lastResult = null;
    this.lastError = null;
    this.runCount = 0;
    this.errorCount = 0;
    this.consecutiveErrors = 0;
    this.nextRun = null;
    this.timerId = null;
  }

  // Calculate next run time based on cron expression
  calculateNextRun() {
    if (this.interval) {
      this.nextRun = Date.now() + this.interval;
    } else if (this.cron) {
      this.nextRun = this.parseCron(this.cron);
    }
    return this.nextRun;
  }

  // Simple cron parser (supports: * and specific values for min/hour/day)
  parseCron(expr) {
    const parts = expr.split(' ');
    if (parts.length !== 5) {
      throw new Error(`Invalid cron expression: ${expr}`);
    }

    const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
    const now = new Date();
    const next = new Date(now);

    // Parse minute
    if (minute !== '*') {
      next.setMinutes(parseInt(minute, 10));
      next.setSeconds(0);
      next.setMilliseconds(0);
    }

    // Parse hour
    if (hour !== '*') {
      next.setHours(parseInt(hour, 10));
    }

    // If time is in the past, move to next occurrence
    if (next <= now) {
      if (minute !== '*' && hour === '*') {
        next.setHours(next.getHours() + 1);
      } else {
        next.setDate(next.getDate() + 1);
      }
    }

    return next.getTime();
  }

  getStatus() {
    return {
      name: this.name,
      enabled: this.enabled,
      lastRun: this.lastRun ? new Date(this.lastRun).toISOString() : null,
      nextRun: this.nextRun ? new Date(this.nextRun).toISOString() : null,
      runCount: this.runCount,
      errorCount: this.errorCount,
      lastError: this.lastError,
    };
  }
}

/**
 * Task Scheduler
 */
class Scheduler {
  constructor(options = {}) {
    this.tasks = new Map();
    this.running = false;
    this.tickInterval = options.tickInterval || 1000;
    this.mainTimer = null;
    this.taskHistory = [];
    this.maxHistory = options.maxHistory || 100;

    this.onTaskStart = options.onTaskStart || null;
    this.onTaskComplete = options.onTaskComplete || null;
    this.onTaskError = options.onTaskError || null;
  }

  // Add a task
  addTask(name, fn, options = {}) {
    if (this.tasks.has(name)) {
      throw new Error(`Task "${name}" already exists`);
    }

    const task = new Task(name, fn, options);
    this.tasks.set(name, task);

    if (this.running) {
      this.scheduleTask(task);
    }

    return task;
  }

  // Remove a task
  removeTask(name) {
    const task = this.tasks.get(name);
    if (task) {
      if (task.timerId) {
        clearTimeout(task.timerId);
      }
      this.tasks.delete(name);
    }
  }

  // Enable/disable a task
  setTaskEnabled(name, enabled) {
    const task = this.tasks.get(name);
    if (task) {
      task.enabled = enabled;
      if (!enabled && task.timerId) {
        clearTimeout(task.timerId);
        task.timerId = null;
      } else if (enabled && this.running) {
        this.scheduleTask(task);
      }
    }
  }

  // Get task by name
  getTask(name) {
    return this.tasks.get(name);
  }

  // Run a task immediately
  async runTask(name) {
    const task = this.tasks.get(name);
    if (!task) {
      throw new Error(`Task "${name}" not found`);
    }
    return this.executeTask(task);
  }

  // Execute a task
  async executeTask(task, retryCount = 0) {
    if (!task.enabled) return;

    // Check dependencies
    for (const depName of task.dependencies) {
      const dep = this.tasks.get(depName);
      if (dep && dep.lastError) {
        console.log(`${c.yellow}  Skipping ${task.name}: dependency ${depName} failed${c.reset}`);
        return;
      }
    }

    const startTime = Date.now();
    task.lastRun = startTime;
    task.runCount++;

    if (this.onTaskStart) {
      this.onTaskStart(task);
    }

    try {
      // Run with timeout
      const result = await Promise.race([
        task.fn(),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Task timeout')), task.timeout)
        ),
      ]);

      const duration = Date.now() - startTime;
      task.lastResult = result;
      task.lastError = null;
      task.consecutiveErrors = 0;

      this.recordHistory(task.name, 'success', duration);

      if (this.onTaskComplete) {
        this.onTaskComplete(task, result, duration);
      }

      return result;

    } catch (error) {
      const duration = Date.now() - startTime;
      task.lastError = error.message;
      task.errorCount++;
      task.consecutiveErrors++;

      this.recordHistory(task.name, 'error', duration, error.message);

      if (this.onTaskError) {
        this.onTaskError(task, error);
      }

      // Retry logic
      if (retryCount < task.maxRetries) {
        console.log(`${c.yellow}  Retrying ${task.name} (${retryCount + 1}/${task.maxRetries})...${c.reset}`);
        await this.delay(task.retryDelay);
        return this.executeTask(task, retryCount + 1);
      }

      throw error;
    }
  }

  // Schedule next run for a task
  scheduleTask(task) {
    if (!task.enabled || !this.running) return;

    const nextRun = task.calculateNextRun();
    if (!nextRun) return;

    const delay = Math.max(0, nextRun - Date.now());

    task.timerId = setTimeout(async () => {
      try {
        await this.executeTask(task);
      } catch (e) {
        // Error already handled
      }
      this.scheduleTask(task);
    }, delay);
  }

  // Start the scheduler
  start() {
    if (this.running) return;

    this.running = true;
    console.log(`${c.green}Scheduler started${c.reset}`);

    // Schedule all tasks
    for (const task of this.tasks.values()) {
      this.scheduleTask(task);
    }
  }

  // Stop the scheduler
  stop() {
    if (!this.running) return;

    this.running = false;

    // Clear all timers
    for (const task of this.tasks.values()) {
      if (task.timerId) {
        clearTimeout(task.timerId);
        task.timerId = null;
      }
    }

    console.log(`${c.yellow}Scheduler stopped${c.reset}`);
  }

  // Record task execution in history
  recordHistory(taskName, status, duration, error = null) {
    this.taskHistory.unshift({
      task: taskName,
      status,
      duration,
      error,
      timestamp: new Date().toISOString(),
    });

    // Trim history
    if (this.taskHistory.length > this.maxHistory) {
      this.taskHistory.pop();
    }
  }

  // Get scheduler status
  getStatus() {
    return {
      running: this.running,
      taskCount: this.tasks.size,
      tasks: [...this.tasks.values()].map(t => t.getStatus()),
    };
  }

  // Get execution history
  getHistory(limit = 10) {
    return this.taskHistory.slice(0, limit);
  }

  // Utility: delay
  delay(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Export for module use
if (typeof module !== 'undefined') {
  module.exports = { Scheduler, Task };
}

// Demo
(async () => {
console.log(`\n${c.bold}${c.cyan}Task Scheduler Demo${c.reset}\n`);

const scheduler = new Scheduler({
  onTaskStart: (task) => {
    console.log(`${c.blue}▶${c.reset} Starting: ${task.name}`);
  },
  onTaskComplete: (task, result, duration) => {
    console.log(`${c.green}✓${c.reset} Completed: ${task.name} (${duration}ms)`);
  },
  onTaskError: (task, error) => {
    console.log(`${c.red}✗${c.reset} Failed: ${task.name} - ${error.message}`);
  },
});

// Add tasks
console.log(`${c.bold}1. Adding tasks${c.reset}\n`);

scheduler.addTask('heartbeat', async () => {
  return { status: 'alive', timestamp: Date.now() };
}, {
  interval: 2000,
  priority: 1,
});

scheduler.addTask('cleanup', async () => {
  // Simulate cleanup work
  await scheduler.delay(100);
  return { cleaned: Math.floor(Math.random() * 10) };
}, {
  interval: 5000,
  priority: 3,
});

scheduler.addTask('report', async () => {
  const status = scheduler.getStatus();
  return { taskCount: status.taskCount };
}, {
  interval: 4000,
  priority: 5,
  dependencies: ['heartbeat'],
});

scheduler.addTask('flaky', async () => {
  // 50% chance of failure
  if (Math.random() < 0.5) {
    throw new Error('Random failure');
  }
  return { success: true };
}, {
  interval: 3000,
  maxRetries: 2,
  retryDelay: 500,
});

console.log(`  Added ${scheduler.tasks.size} tasks\n`);

// Start scheduler
console.log(`${c.bold}2. Starting scheduler${c.reset}\n`);
scheduler.start();

// Run for 5 seconds (shorter for testing)
console.log(`${c.dim}Running for 5 seconds...${c.reset}\n`);

await scheduler.delay(5000);

// Stop and show results
console.log(`\n${c.bold}3. Stopping scheduler${c.reset}\n`);
scheduler.stop();

// Show status
console.log(`${c.bold}4. Task status${c.reset}`);
const status = scheduler.getStatus();
for (const task of status.tasks) {
  const statusIcon = task.lastError ? c.red + '✗' : c.green + '✓';
  console.log(`  ${statusIcon}${c.reset} ${task.name}`);
  console.log(`    ${c.dim}Runs: ${task.runCount}, Errors: ${task.errorCount}${c.reset}`);
}

// Show history
console.log(`\n${c.bold}5. Execution history (last 10)${c.reset}`);
const history = scheduler.getHistory(10);
for (const entry of history) {
  const icon = entry.status === 'success' ? c.green + '✓' : c.red + '✗';
  console.log(`  ${icon}${c.reset} ${entry.task} - ${entry.duration}ms ${c.dim}(${entry.timestamp})${c.reset}`);
}

console.log(`\n${c.green}${c.bold}Scheduler demo completed!${c.reset}\n`);
})();
