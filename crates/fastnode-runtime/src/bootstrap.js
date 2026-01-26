// Bootstrap JavaScript runtime with Node-like globals
((globalThis) => {
  const core = Deno.core;
  const ops = core.ops;

  // Console implementation
  globalThis.console = {
    log(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    error(...args) {
      ops.op_howth_print_error(args.map(String).join(" ") + "\n");
    },
    warn(...args) {
      ops.op_howth_print_error(args.map(String).join(" ") + "\n");
    },
    info(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    debug(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
  };

  // Process implementation (Node.js compatibility)
  globalThis.process = {
    env: new Proxy({}, {
      get(_, key) {
        return ops.op_howth_env_get(String(key));
      },
      set(_, key, value) {
        // Environment variables are read-only in this runtime
        return false;
      },
    }),
    cwd() {
      return ops.op_howth_cwd();
    },
    exit(code = 0) {
      ops.op_howth_exit(code);
    },
    argv: ops.op_howth_args(),
    platform: Deno.build?.os || "unknown",
    version: "v20.0.0", // Fake Node.js version for compatibility
    versions: {
      node: "20.0.0",
      v8: "11.0.0",
      howth: "0.1.0",
    },
  };

  // Basic fs module (synchronous only for now)
  globalThis.__howth_fs = {
    readFileSync(path, options) {
      const content = ops.op_howth_read_file(path);
      if (options?.encoding === "utf8" || options === "utf8") {
        return content;
      }
      // Return as string for now (Buffer not implemented)
      return content;
    },
    writeFileSync(path, data) {
      ops.op_howth_write_file(path, String(data));
    },
  };

  // Timers (basic implementation)
  const timers = new Map();
  let timerId = 0;

  globalThis.setTimeout = (callback, delay, ...args) => {
    const id = ++timerId;
    const handle = core.queueUserTimer(
      core.getTimerDepth() + 1,
      false,
      delay,
      () => {
        timers.delete(id);
        callback(...args);
      }
    );
    timers.set(id, handle);
    return id;
  };

  globalThis.clearTimeout = (id) => {
    const handle = timers.get(id);
    if (handle !== undefined) {
      // Note: deno_core doesn't expose timer cancellation easily
      timers.delete(id);
    }
  };

  globalThis.setInterval = (callback, delay, ...args) => {
    const id = ++timerId;
    const tick = () => {
      callback(...args);
      if (timers.has(id)) {
        const handle = core.queueUserTimer(
          core.getTimerDepth() + 1,
          false,
          delay,
          tick
        );
        timers.set(id, handle);
      }
    };
    const handle = core.queueUserTimer(
      core.getTimerDepth() + 1,
      false,
      delay,
      tick
    );
    timers.set(id, handle);
    return id;
  };

  globalThis.clearInterval = (id) => {
    timers.delete(id);
  };

  // Mark bootstrap as complete
  globalThis.__howth_ready = true;

})(globalThis);
