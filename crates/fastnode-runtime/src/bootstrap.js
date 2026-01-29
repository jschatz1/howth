// Bootstrap JavaScript runtime with Node-like globals
((globalThis) => {
  const core = Deno.core;
  const ops = core.ops;

  // Node.js uses 'global' as an alias for globalThis
  globalThis.global = globalThis;

  Error.stackTraceLimit = 50;

  // Console implementation
  const consoleTimers = new Map();
  const consoleCounts = new Map();

  globalThis.console = {
    log(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    error(...args) {
      ops.op_howth_print_error(args.map(a => a instanceof Error ? (a.stack || String(a)) : String(a)).join(" ") + "\n");
    },
    warn(...args) {
      ops.op_howth_print_error(args.map(a => a instanceof Error ? (a.stack || String(a)) : String(a)).join(" ") + "\n");
    },
    info(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    debug(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    assert(condition, ...args) {
      if (!condition) {
        const message = args.length > 0 ? args.map(String).join(" ") : "Assertion failed";
        ops.op_howth_print_error("Assertion failed: " + message + "\n");
      }
    },
    time(label = "default") {
      consoleTimers.set(label, Date.now());
    },
    timeEnd(label = "default") {
      const start = consoleTimers.get(label);
      if (start !== undefined) {
        const duration = Date.now() - start;
        ops.op_howth_print(`${label}: ${duration}ms\n`);
        consoleTimers.delete(label);
      } else {
        ops.op_howth_print_error(`Timer '${label}' does not exist\n`);
      }
    },
    timeLog(label = "default", ...args) {
      const start = consoleTimers.get(label);
      if (start !== undefined) {
        const duration = Date.now() - start;
        const extra = args.length > 0 ? " " + args.map(String).join(" ") : "";
        ops.op_howth_print(`${label}: ${duration}ms${extra}\n`);
      } else {
        ops.op_howth_print_error(`Timer '${label}' does not exist\n`);
      }
    },
    count(label = "default") {
      const count = (consoleCounts.get(label) || 0) + 1;
      consoleCounts.set(label, count);
      ops.op_howth_print(`${label}: ${count}\n`);
    },
    countReset(label = "default") {
      consoleCounts.delete(label);
    },
    trace(...args) {
      const err = new Error();
      const stack = err.stack || "";
      const message = args.length > 0 ? args.map(String).join(" ") : "Trace";
      ops.op_howth_print(`${message}\n${stack}\n`);
    },
    dir(obj, options) {
      // Simple dir implementation
      ops.op_howth_print(JSON.stringify(obj, null, 2) + "\n");
    },
    dirxml(...args) {
      // Alias to dir for non-browser environments
      this.dir(...args);
    },
    table(data) {
      // Simple table implementation
      ops.op_howth_print(JSON.stringify(data, null, 2) + "\n");
    },
    clear() {
      // Clear console (no-op in most non-TTY environments)
    },
    group(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    groupCollapsed(...args) {
      ops.op_howth_print(args.map(String).join(" ") + "\n");
    },
    groupEnd() {
      // No-op
    },
  };

  // Process implementation (Node.js compatibility)
  // Event emitter functionality for process
  const processListeners = new Map();

  globalThis.process = {
    // Event emitter methods
    on(event, listener) {
      if (!processListeners.has(event)) {
        processListeners.set(event, []);
      }
      processListeners.get(event).push(listener);
      return this;
    },
    addListener(event, listener) {
      return this.on(event, listener);
    },
    once(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener(...args);
      };
      wrapper.listener = listener;
      return this.on(event, wrapper);
    },
    off(event, listener) {
      const listeners = processListeners.get(event);
      if (listeners) {
        const index = listeners.findIndex(l => l === listener || l.listener === listener);
        if (index !== -1) {
          listeners.splice(index, 1);
        }
      }
      return this;
    },
    removeListener(event, listener) {
      return this.off(event, listener);
    },
    removeAllListeners(event) {
      if (event) {
        processListeners.delete(event);
      } else {
        processListeners.clear();
      }
      return this;
    },
    emit(event, ...args) {
      const listeners = processListeners.get(event);
      if (!listeners || listeners.length === 0) return false;
      for (const listener of [...listeners]) {
        listener(...args);
      }
      return true;
    },
    listeners(event) {
      return processListeners.get(event) || [];
    },
    listenerCount(event) {
      return (processListeners.get(event) || []).length;
    },
    prependListener(event, listener) {
      if (!processListeners.has(event)) {
        processListeners.set(event, []);
      }
      processListeners.get(event).unshift(listener);
      return this;
    },
    prependOnceListener(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener(...args);
      };
      wrapper.listener = listener;
      return this.prependListener(event, wrapper);
    },

    // Environment
    env: new Proxy({}, {
      get(_, key) {
        return ops.op_howth_env_get(String(key));
      },
      set(_, key, value) {
        ops.op_howth_env_set(String(key), String(value));
        return true;
      },
      deleteProperty(_, key) {
        ops.op_howth_env_set(String(key), "");
        return true;
      },
    }),
    cwd() {
      return ops.op_howth_cwd();
    },
    chdir(directory) {
      if (typeof directory !== 'string') {
        throw new TypeError('The "directory" argument must be of type string');
      }
      ops.op_howth_chdir(directory);
    },
    exit(code = 0) {
      // Emit exit event before exiting
      globalThis.process.exitCode = code;
      globalThis.process.emit("exit", code);
      ops.op_howth_exit(code);
    },
    exitCode: 0,
    argv: ops.op_howth_args(),
    argv0: ops.op_howth_args()[0] || "howth",
    execArgv: [], // Command-line options passed to Node.js executable
    execPath: ops.op_howth_args()[0] || "/usr/bin/howth",
    platform: ops.op_howth_platform(),
    version: "v20.18.0", // Fake Node.js version for compatibility
    versions: {
      node: "20.18.0",
      v8: "11.0.0",
      howth: "0.1.0",
    },
    features: {
      inspector: false,
      debug: false,
      uv: true,
      ipv6: true,
      tls_alpn: true,
      tls_sni: true,
      tls_ocsp: true,
      tls: true,
      typescript: false, // Node.js 22+ has native TypeScript support
    },
    pid: 1,
    ppid: 0,
    arch: ops.op_howth_arch(),
    title: "howth",
    hrtime: Object.assign(
      function hrtime(time) {
        const nowBigInt = BigInt(ops.op_howth_hrtime());
        const seconds = Number(nowBigInt / 1000000000n);
        const nanoseconds = Number(nowBigInt % 1000000000n);
        if (time) {
          // Return diff from provided time
          const diffSec = seconds - time[0];
          const diffNsec = nanoseconds - time[1];
          if (diffNsec < 0) {
            return [diffSec - 1, 1000000000 + diffNsec];
          }
          return [diffSec, diffNsec];
        }
        return [seconds, nanoseconds];
      },
      {
        bigint() {
          return BigInt(ops.op_howth_hrtime());
        },
      }
    ),
    nextTick(callback, ...args) {
      queueMicrotask(() => {
        try {
          callback(...args);
        } catch (err) {
          // Emit uncaughtException if handlers exist, otherwise rethrow
          const listeners = processListeners.get("uncaughtException");
          if (listeners && listeners.length > 0) {
            for (const listener of [...listeners]) {
              listener(err);
            }
          } else {
            console.error("[howth] Uncaught exception in nextTick:", err instanceof Error ? err.stack || err.message : err);
            throw err;
          }
        }
      });
    },
    // process.binding - deprecated Node.js internal API for native bindings
    binding(name) {
      const bindings = {
        // fs binding - used by some native modules
        fs: {
          FSReqCallback: class FSReqCallback {
            constructor() {
              this.oncomplete = null;
              this.context = null;
            }
          },
          StatWatcher: class StatWatcher {},
          writeBuffer() {},
          read() {},
          open() {},
          close() {},
          fstat() {},
          stat() {},
          lstat() {},
          fchmod() {},
          chmod() {},
          fchown() {},
          chown() {},
          access() {},
          rename() {},
          unlink() {},
          rmdir() {},
          mkdir() {},
          readdir() {},
          internalModuleReadJSON() { return undefined; },
          internalModuleStat() { return 0; },
        },
        // buffer binding
        buffer: {
          setupBufferJS() {},
          createFromString() {},
          byteLengthUtf8() {},
        },
        // constants binding
        constants: {
          fs: {
            F_OK: 0,
            R_OK: 4,
            W_OK: 2,
            X_OK: 1,
            O_RDONLY: 0,
            O_WRONLY: 1,
            O_RDWR: 2,
            O_CREAT: 64,
            O_EXCL: 128,
            O_TRUNC: 512,
            O_APPEND: 1024,
          },
          os: {},
          crypto: {},
          zlib: {},
        },
        // util binding
        util: {
          isDate(val) { return val instanceof Date; },
          isRegExp(val) { return val instanceof RegExp; },
          isMap(val) { return val instanceof Map; },
          isSet(val) { return val instanceof Set; },
          isArrayBuffer(val) { return val instanceof ArrayBuffer; },
          isDataView(val) { return val instanceof DataView; },
          isNativeError(val) { return val instanceof Error; },
          isPromise(val) { return val instanceof Promise; },
        },
        // natives binding - for internal module source code
        natives: {},
        // config binding
        config: {
          hasOpenSSL: false,
          fipsMode: false,
          hasIntl: false,
          hasTracing: false,
        },
      };
      if (bindings[name]) {
        return bindings[name];
      }
      throw new Error(`No such module: ${name}`);
    },
    // Standard streams (minimal implementation)
    stdout: {
      write(data) {
        ops.op_howth_print(String(data));
        return true;
      },
      isTTY: false,
    },
    stderr: {
      write(data) {
        ops.op_howth_print_error(String(data));
        return true;
      },
      isTTY: false,
    },
    stdin: {
      isTTY: false,
    },
    // Memory usage stub
    memoryUsage() {
      return {
        rss: 0,
        heapTotal: 0,
        heapUsed: 0,
        external: 0,
        arrayBuffers: 0,
      };
    },
    // CPU usage stub
    cpuUsage() {
      return { user: 0, system: 0 };
    },
    // uptime stub
    uptime() {
      return 0;
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
        // Only call callback if timer wasn't cleared
        if (timers.has(id)) {
          timers.delete(id);
          callback(...args);
        }
      }
    );
    timers.set(id, handle);
    return id;
  };

  globalThis.clearTimeout = (id) => {
    // Just delete from map - the callback will check if still present
    timers.delete(id);
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

  // TextEncoder implementation
  globalThis.TextEncoder = class TextEncoder {
    constructor() {
      this.encoding = "utf-8";
    }
    encode(text) {
      const bytes = ops.op_howth_encode_utf8(String(text));
      return new Uint8Array(bytes);
    }
  };

  // TextDecoder implementation
  globalThis.TextDecoder = class TextDecoder {
    constructor(encoding = "utf-8") {
      this.encoding = encoding.toLowerCase();
      if (this.encoding !== "utf-8" && this.encoding !== "utf8") {
        throw new Error("Only UTF-8 encoding is supported");
      }
    }
    decode(buffer) {
      if (!buffer) return "";
      const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
      return ops.op_howth_decode_utf8(bytes);
    }
  };

  // TextEncoderStream - Web Streams API
  globalThis.TextEncoderStream = class TextEncoderStream {
    constructor() {
      this.encoding = "utf-8";
      const encoder = new TextEncoder();
      this.readable = new ReadableStream({
        start() {},
      });
      this.writable = new WritableStream({
        write(chunk) {
          // Transform string to Uint8Array
          return encoder.encode(chunk);
        },
      });
      // Create a simple transform
      const { readable, writable } = new TransformStream({
        transform(chunk, controller) {
          controller.enqueue(encoder.encode(chunk));
        },
      });
      this.readable = readable;
      this.writable = writable;
    }
  };

  // TextDecoderStream - Web Streams API
  globalThis.TextDecoderStream = class TextDecoderStream {
    constructor(encoding = "utf-8") {
      this.encoding = encoding.toLowerCase();
      const decoder = new TextDecoder(encoding);
      const { readable, writable } = new TransformStream({
        transform(chunk, controller) {
          const text = decoder.decode(chunk, { stream: true });
          if (text) controller.enqueue(text);
        },
        flush(controller) {
          const text = decoder.decode();
          if (text) controller.enqueue(text);
        },
      });
      this.readable = readable;
      this.writable = writable;
    }
  };

  // URL implementation (basic)
  globalThis.URL = class URL {
    constructor(url, base) {
      // Convert URL objects or other types to string
      if (url && typeof url === "object" && url.href) {
        url = url.href;
      } else if (typeof url !== "string") {
        url = String(url);
      }
      if (base && typeof base === "object" && base.href) {
        base = base.href;
      } else if (base !== undefined && typeof base !== "string") {
        base = String(base);
      }
      let fullUrl = url;
      if (base) {
        // Simple base URL handling
        if (!url.match(/^[a-z]+:\/\//i)) {
          const baseUrl = new URL(base);
          if (url.startsWith("/")) {
            fullUrl = baseUrl.origin + url;
          } else {
            fullUrl = baseUrl.href.replace(/[^/]*$/, "") + url;
          }
        }
      }

      // Parse URL with optional auth (user:pass@)
      // Also handle file:// URLs which may have empty hostname (file:///path)
      const match = fullUrl.match(/^([a-z]+):\/\/(?:([^:@\/]+)(?::([^@\/]*))?@)?([^\/:?#]*)(?::(\d+))?(\/[^?#]*)?(\?[^#]*)?(#.*)?$/i);
      if (!match) {
        throw new TypeError("Invalid URL: " + url);
      }

      this.protocol = match[1].toLowerCase() + ":";
      this.username = match[2] || "";
      this.password = match[3] || "";
      this.hostname = match[4];
      this.port = match[5] || "";
      this.pathname = match[6] || "/";
      this.search = match[7] || "";
      this.hash = match[8] || "";
      this.host = this.port ? this.hostname + ":" + this.port : this.hostname;
      this.origin = this.protocol + "//" + this.host;
      // Build href with auth if present
      let authPart = "";
      if (this.username) {
        authPart = this.username + (this.password ? ":" + this.password : "") + "@";
      }
      this.href = this.protocol + "//" + authPart + this.host + this.pathname + this.search + this.hash;
      this.searchParams = new URLSearchParams(this.search);
    }

    toString() {
      return this.href;
    }

    toJSON() {
      return this.href;
    }
  };

  // URLSearchParams implementation
  globalThis.URLSearchParams = class URLSearchParams {
    #params = new Map();

    constructor(init) {
      if (typeof init === "string") {
        const search = init.startsWith("?") ? init.slice(1) : init;
        for (const pair of search.split("&")) {
          if (!pair) continue;
          const [key, value = ""] = pair.split("=").map(decodeURIComponent);
          this.append(key, value);
        }
      } else if (init instanceof URLSearchParams) {
        for (const [key, value] of init) {
          this.append(key, value);
        }
      } else if (init && typeof init === "object") {
        for (const [key, value] of Object.entries(init)) {
          this.append(key, String(value));
        }
      }
    }

    append(key, value) {
      const existing = this.#params.get(key) || [];
      existing.push(String(value));
      this.#params.set(key, existing);
    }

    delete(key) {
      this.#params.delete(key);
    }

    get(key) {
      const values = this.#params.get(key);
      return values ? values[0] : null;
    }

    getAll(key) {
      return this.#params.get(key) || [];
    }

    has(key) {
      return this.#params.has(key);
    }

    set(key, value) {
      this.#params.set(key, [String(value)]);
    }

    *entries() {
      for (const [key, values] of this.#params) {
        for (const value of values) {
          yield [key, value];
        }
      }
    }

    *keys() {
      for (const [key] of this.entries()) {
        yield key;
      }
    }

    *values() {
      for (const [, value] of this.entries()) {
        yield value;
      }
    }

    [Symbol.iterator]() {
      return this.entries();
    }

    toString() {
      const parts = [];
      for (const [key, value] of this.entries()) {
        parts.push(encodeURIComponent(key) + "=" + encodeURIComponent(value));
      }
      return parts.join("&");
    }
  };

  // Headers implementation
  globalThis.Headers = class Headers {
    #headers = new Map();

    constructor(init) {
      if (init instanceof Headers) {
        for (const [key, value] of init) {
          this.set(key, value);
        }
      } else if (Array.isArray(init)) {
        for (const [key, value] of init) {
          this.append(key, value);
        }
      } else if (init && typeof init === "object") {
        for (const [key, value] of Object.entries(init)) {
          this.set(key, String(value));
        }
      }
    }

    append(name, value) {
      const key = name.toLowerCase();
      const existing = this.#headers.get(key);
      if (existing) {
        this.#headers.set(key, existing + ", " + value);
      } else {
        this.#headers.set(key, String(value));
      }
    }

    delete(name) {
      this.#headers.delete(name.toLowerCase());
    }

    get(name) {
      return this.#headers.get(name.toLowerCase()) || null;
    }

    has(name) {
      return this.#headers.has(name.toLowerCase());
    }

    set(name, value) {
      this.#headers.set(name.toLowerCase(), String(value));
    }

    *entries() {
      yield* this.#headers.entries();
    }

    *keys() {
      yield* this.#headers.keys();
    }

    *values() {
      yield* this.#headers.values();
    }

    [Symbol.iterator]() {
      return this.entries();
    }

    forEach(callback, thisArg) {
      for (const [key, value] of this) {
        callback.call(thisArg, value, key, this);
      }
    }
  };

  // Response implementation (simplified)
  globalThis.Response = class Response {
    constructor(body, init = {}) {
      this._body = body;
      this.status = init.status || 200;
      this.statusText = init.statusText || "";
      this.ok = this.status >= 200 && this.status < 300;
      this.headers = new Headers(init.headers);
      this.url = init.url || "";
      this._bodyUsed = false;
    }

    get bodyUsed() {
      return this._bodyUsed;
    }

    async text() {
      this._bodyUsed = true;
      return String(this._body || "");
    }

    async json() {
      const text = await this.text();
      return JSON.parse(text);
    }

    async arrayBuffer() {
      const text = await this.text();
      const encoder = new TextEncoder();
      return encoder.encode(text).buffer;
    }
  };

  // Request implementation (simplified)
  globalThis.Request = class Request {
    constructor(input, init = {}) {
      if (input instanceof Request) {
        this.url = input.url;
        this.method = init.method || input.method;
        this.headers = new Headers(init.headers || input.headers);
        this._body = init.body !== undefined ? init.body : input._body;
      } else {
        this.url = String(input);
        this.method = init.method || "GET";
        this.headers = new Headers(init.headers);
        this._body = init.body;
      }
    }

    async text() {
      return String(this._body || "");
    }

    async json() {
      const text = await this.text();
      return JSON.parse(text);
    }
  };

  // fetch implementation
  globalThis.fetch = async (input, init = {}) => {
    let url, options;

    if (input instanceof Request) {
      url = input.url;
      options = {
        method: init.method || input.method,
        headers: {},
        body: init.body !== undefined ? init.body : input._body,
      };
      // Copy headers
      for (const [key, value] of input.headers) {
        options.headers[key] = value;
      }
      // Override with init headers
      if (init.headers) {
        const initHeaders = new Headers(init.headers);
        for (const [key, value] of initHeaders) {
          options.headers[key] = value;
        }
      }
    } else {
      url = String(input);
      options = {
        method: init.method,
        headers: {},
        body: init.body,
      };
      if (init.headers) {
        const headers = new Headers(init.headers);
        for (const [key, value] of headers) {
          options.headers[key] = value;
        }
      }
    }

    const result = await core.ops.op_howth_fetch(url, options);

    return new Response(result.body, {
      status: result.status,
      statusText: result.status_text,
      headers: result.headers,
      url: result.url,
    });
  };

  // atob/btoa
  globalThis.atob = (encoded) => {
    // Simple base64 decode
    const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let result = "";
    encoded = encoded.replace(/=+$/, "");
    for (let i = 0; i < encoded.length; i += 4) {
      const a = chars.indexOf(encoded[i]);
      const b = chars.indexOf(encoded[i + 1]);
      const c = chars.indexOf(encoded[i + 2]);
      const d = chars.indexOf(encoded[i + 3]);
      result += String.fromCharCode((a << 2) | (b >> 4));
      if (c !== -1) result += String.fromCharCode(((b & 15) << 4) | (c >> 2));
      if (d !== -1) result += String.fromCharCode(((c & 3) << 6) | d);
    }
    return result;
  };

  globalThis.btoa = (text) => {
    // Simple base64 encode
    const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let result = "";
    for (let i = 0; i < text.length; i += 3) {
      const a = text.charCodeAt(i);
      const b = text.charCodeAt(i + 1);
      const c = text.charCodeAt(i + 2);
      result += chars[a >> 2];
      result += chars[((a & 3) << 4) | (b >> 4)];
      result += isNaN(b) ? "=" : chars[((b & 15) << 2) | (c >> 6)];
      result += isNaN(c) ? "=" : chars[c & 63];
    }
    return result;
  };

  // Event / EventTarget (minimal implementation) - must be before AbortSignal
  globalThis.Event = class Event {
    constructor(type, options = {}) {
      this.type = type;
      this.bubbles = options.bubbles || false;
      this.cancelable = options.cancelable || false;
      this.composed = options.composed || false;
      this.defaultPrevented = false;
      this.timeStamp = Date.now();
    }

    preventDefault() {
      if (this.cancelable) {
        this.defaultPrevented = true;
      }
    }

    stopPropagation() {}
    stopImmediatePropagation() {}
  };

  globalThis.EventTarget = class EventTarget {
    #listeners = new Map();

    addEventListener(type, callback, options) {
      if (!this.#listeners.has(type)) {
        this.#listeners.set(type, []);
      }
      this.#listeners.get(type).push({ callback, options });
    }

    removeEventListener(type, callback) {
      const listeners = this.#listeners.get(type);
      if (listeners) {
        const index = listeners.findIndex(l => l.callback === callback);
        if (index !== -1) {
          listeners.splice(index, 1);
        }
      }
    }

    dispatchEvent(event) {
      const listeners = this.#listeners.get(event.type);
      if (listeners) {
        for (const { callback, options } of [...listeners]) {
          callback.call(this, event);
          if (options?.once) {
            this.removeEventListener(event.type, callback);
          }
        }
      }
      return !event.defaultPrevented;
    }
  };

  // DOMException - must be before AbortSignal
  globalThis.DOMException = class DOMException extends Error {
    constructor(message, name = "Error") {
      super(message);
      this.name = name;
    }
  };

  // crypto implementation - Web Crypto API
  globalThis.crypto = {
    getRandomValues(array) {
      const bytes = ops.op_howth_random_bytes(array.length);
      for (let i = 0; i < array.length; i++) {
        array[i] = bytes[i];
      }
      return array;
    },
    randomUUID() {
      return ops.op_howth_random_uuid();
    },
    subtle: {
      async digest(algorithm, data) {
        const algo = typeof algorithm === "string" ? algorithm : algorithm.name;
        const bytes = data instanceof ArrayBuffer
          ? new Uint8Array(data)
          : data instanceof Uint8Array
            ? data
            : new TextEncoder().encode(String(data));
        const result = ops.op_howth_hash(algo, bytes);
        return new Uint8Array(result).buffer;
      },

      async generateKey(algorithm, extractable, keyUsages) {
        const algo = typeof algorithm === 'string' ? { name: algorithm } : algorithm;
        const keyBytes = ops.op_howth_random_bytes(32); // 256 bits

        const key = {
          type: 'secret',
          extractable,
          algorithm: algo,
          usages: keyUsages,
          _keyData: keyBytes,
        };

        if (algo.name === 'RSA-OAEP' || algo.name === 'RSA-PSS' || algo.name === 'RSASSA-PKCS1-v1_5') {
          // Return a key pair for asymmetric algorithms
          return {
            publicKey: { ...key, type: 'public', _keyData: ops.op_howth_random_bytes(256) },
            privateKey: { ...key, type: 'private', _keyData: ops.op_howth_random_bytes(256) },
          };
        }

        return key;
      },

      async importKey(format, keyData, algorithm, extractable, keyUsages) {
        const algo = typeof algorithm === 'string' ? { name: algorithm } : algorithm;
        let data;
        if (format === 'raw') {
          data = keyData instanceof ArrayBuffer ? new Uint8Array(keyData) : keyData;
        } else if (format === 'jwk') {
          // JWK format - simplified handling
          data = keyData.k ? new TextEncoder().encode(keyData.k) : new Uint8Array(32);
        } else {
          data = new Uint8Array(32);
        }

        return {
          type: 'secret',
          extractable,
          algorithm: algo,
          usages: keyUsages,
          _keyData: data,
        };
      },

      async exportKey(format, key) {
        if (format === 'raw') {
          return key._keyData.buffer || key._keyData;
        } else if (format === 'jwk') {
          return {
            kty: 'oct',
            k: btoa(String.fromCharCode(...key._keyData)),
            alg: key.algorithm.name,
            ext: key.extractable,
            key_ops: key.usages,
          };
        }
        throw new Error(`Unsupported export format: ${format}`);
      },

      async sign(algorithm, key, data) {
        const algo = typeof algorithm === 'string' ? { name: algorithm } : algorithm;
        const dataBytes = data instanceof ArrayBuffer ? new Uint8Array(data) : data;

        // Simple HMAC-like signature using hash
        const keyBytes = key._keyData || new Uint8Array(32);
        const combined = new Uint8Array(keyBytes.length + dataBytes.length);
        combined.set(keyBytes);
        combined.set(dataBytes, keyBytes.length);

        const result = ops.op_howth_hash('SHA-256', combined);
        return new Uint8Array(result).buffer;
      },

      async verify(algorithm, key, signature, data) {
        const newSig = await this.sign(algorithm, key, data);
        const sigBytes = new Uint8Array(signature);
        const newSigBytes = new Uint8Array(newSig);

        if (sigBytes.length !== newSigBytes.length) return false;
        for (let i = 0; i < sigBytes.length; i++) {
          if (sigBytes[i] !== newSigBytes[i]) return false;
        }
        return true;
      },

      async encrypt(algorithm, key, data) {
        // Stub: just XOR with key for minimal implementation
        const algo = typeof algorithm === 'string' ? { name: algorithm } : algorithm;
        const dataBytes = data instanceof ArrayBuffer ? new Uint8Array(data) : data;
        const keyBytes = key._keyData || new Uint8Array(32);

        const result = new Uint8Array(dataBytes.length);
        for (let i = 0; i < dataBytes.length; i++) {
          result[i] = dataBytes[i] ^ keyBytes[i % keyBytes.length];
        }
        return result.buffer;
      },

      async decrypt(algorithm, key, data) {
        // XOR is symmetric, so decrypt is same as encrypt
        return this.encrypt(algorithm, key, data);
      },

      async deriveBits(algorithm, baseKey, length) {
        const algo = typeof algorithm === 'string' ? { name: algorithm } : algorithm;
        const keyBytes = baseKey._keyData || new Uint8Array(32);
        const numBytes = Math.ceil(length / 8);

        // Simple derivation using hash iterations
        let result = keyBytes;
        const salt = algo.salt ? new Uint8Array(algo.salt) : new Uint8Array(16);
        for (let i = 0; i < (algo.iterations || 1); i++) {
          const combined = new Uint8Array(result.length + salt.length);
          combined.set(result);
          combined.set(salt, result.length);
          result = new Uint8Array(ops.op_howth_hash('SHA-256', combined));
        }

        return result.slice(0, numBytes).buffer;
      },

      async deriveKey(algorithm, baseKey, derivedKeyAlgorithm, extractable, keyUsages) {
        const bits = await this.deriveBits(algorithm, baseKey, derivedKeyAlgorithm.length || 256);
        return {
          type: 'secret',
          extractable,
          algorithm: derivedKeyAlgorithm,
          usages: keyUsages,
          _keyData: new Uint8Array(bits),
        };
      },

      async wrapKey(format, key, wrappingKey, wrapAlgorithm) {
        const exported = await this.exportKey(format, key);
        const data = format === 'raw' ? new Uint8Array(exported) : new TextEncoder().encode(JSON.stringify(exported));
        return this.encrypt(wrapAlgorithm, wrappingKey, data);
      },

      async unwrapKey(format, wrappedKey, unwrappingKey, unwrapAlgorithm, unwrappedKeyAlgorithm, extractable, keyUsages) {
        const decrypted = await this.decrypt(unwrapAlgorithm, unwrappingKey, wrappedKey);
        const data = format === 'raw' ? decrypted : JSON.parse(new TextDecoder().decode(new Uint8Array(decrypted)));
        return this.importKey(format, data, unwrappedKeyAlgorithm, extractable, keyUsages);
      },
    },
  };

  // AbortController / AbortSignal
  globalThis.AbortSignal = class AbortSignal extends EventTarget {
    #aborted = false;
    #reason = undefined;

    get aborted() {
      return this.#aborted;
    }

    get reason() {
      return this.#reason;
    }

    throwIfAborted() {
      if (this.#aborted) {
        throw this.#reason;
      }
    }

    static abort(reason) {
      const signal = new AbortSignal();
      signal.#aborted = true;
      signal.#reason = reason ?? new DOMException("signal is aborted without reason", "AbortError");
      return signal;
    }

    static timeout(ms) {
      const signal = new AbortSignal();
      setTimeout(() => {
        signal.#aborted = true;
        signal.#reason = new DOMException("signal timed out", "TimeoutError");
        signal.dispatchEvent(new Event("abort"));
      }, ms);
      return signal;
    }

    // Internal method for AbortController
    _abort(reason) {
      if (this.#aborted) return;
      this.#aborted = true;
      this.#reason = reason ?? new DOMException("signal is aborted without reason", "AbortError");
      this.dispatchEvent(new Event("abort"));
    }
  };

  globalThis.AbortController = class AbortController {
    #signal = new AbortSignal();

    get signal() {
      return this.#signal;
    }

    abort(reason) {
      this.#signal._abort(reason);
    }
  };

  // Blob implementation
  globalThis.Blob = class Blob {
    #parts = [];
    #type = "";

    constructor(parts = [], options = {}) {
      this.#type = options.type || "";
      for (const part of parts) {
        if (part instanceof Blob) {
          this.#parts.push(...part.#parts);
        } else if (part instanceof ArrayBuffer) {
          this.#parts.push(new Uint8Array(part));
        } else if (ArrayBuffer.isView(part)) {
          this.#parts.push(new Uint8Array(part.buffer, part.byteOffset, part.byteLength));
        } else {
          this.#parts.push(new TextEncoder().encode(String(part)));
        }
      }
    }

    get size() {
      return this.#parts.reduce((acc, part) => acc + part.length, 0);
    }

    get type() {
      return this.#type;
    }

    async text() {
      const decoder = new TextDecoder();
      return this.#parts.map(p => decoder.decode(p)).join("");
    }

    async arrayBuffer() {
      const size = this.size;
      const buffer = new ArrayBuffer(size);
      const view = new Uint8Array(buffer);
      let offset = 0;
      for (const part of this.#parts) {
        view.set(part, offset);
        offset += part.length;
      }
      return buffer;
    }

    slice(start = 0, end = this.size, type = "") {
      const buffer = new Uint8Array(this.size);
      let offset = 0;
      for (const part of this.#parts) {
        buffer.set(part, offset);
        offset += part.length;
      }
      return new Blob([buffer.slice(start, end)], { type });
    }

    stream() {
      const parts = this.#parts;
      return new ReadableStream({
        start(controller) {
          for (const part of parts) {
            controller.enqueue(part);
          }
          controller.close();
        },
      });
    }
  };

  // File extends Blob
  globalThis.File = class File extends Blob {
    #name;
    #lastModified;

    constructor(parts, name, options = {}) {
      super(parts, options);
      this.#name = name;
      this.#lastModified = options.lastModified || Date.now();
    }

    get name() {
      return this.#name;
    }

    get lastModified() {
      return this.#lastModified;
    }
  };

  // FormData implementation
  globalThis.FormData = class FormData {
    #entries = [];

    append(name, value, filename) {
      if (value instanceof Blob && filename === undefined) {
        filename = value instanceof File ? value.name : "blob";
      }
      this.#entries.push([name, value, filename]);
    }

    delete(name) {
      this.#entries = this.#entries.filter(([n]) => n !== name);
    }

    get(name) {
      const entry = this.#entries.find(([n]) => n === name);
      return entry ? entry[1] : null;
    }

    getAll(name) {
      return this.#entries.filter(([n]) => n === name).map(([, v]) => v);
    }

    has(name) {
      return this.#entries.some(([n]) => n === name);
    }

    set(name, value, filename) {
      this.delete(name);
      this.append(name, value, filename);
    }

    *entries() {
      for (const [name, value] of this.#entries) {
        yield [name, value];
      }
    }

    *keys() {
      for (const [name] of this.#entries) {
        yield name;
      }
    }

    *values() {
      for (const [, value] of this.#entries) {
        yield value;
      }
    }

    [Symbol.iterator]() {
      return this.entries();
    }

    forEach(callback, thisArg) {
      for (const [name, value] of this) {
        callback.call(thisArg, value, name, this);
      }
    }
  };

  // ReadableStream (minimal implementation)
  globalThis.ReadableStream = class ReadableStream {
    #source;
    #reader = null;

    constructor(source = {}) {
      this.#source = source;
    }

    getReader() {
      if (this.#reader) {
        throw new TypeError("ReadableStream is locked");
      }
      this.#reader = new ReadableStreamDefaultReader(this, this.#source);
      return this.#reader;
    }

    async *[Symbol.asyncIterator]() {
      const reader = this.getReader();
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          yield value;
        }
      } finally {
        reader.releaseLock();
      }
    }
  };

  class ReadableStreamDefaultReader {
    #stream;
    #source;
    #controller;
    #closed = false;

    constructor(stream, source) {
      this.#stream = stream;
      this.#source = source;
      this.#controller = {
        enqueue: (chunk) => this._queue.push(chunk),
        close: () => { this.#closed = true; },
        error: (e) => { this._error = e; this.#closed = true; },
      };
      this._queue = [];
      this._error = null;

      if (source.start) {
        source.start(this.#controller);
      }
    }

    async read() {
      if (this._error) {
        throw this._error;
      }
      if (this._queue.length > 0) {
        return { done: false, value: this._queue.shift() };
      }
      if (this.#closed) {
        return { done: true, value: undefined };
      }
      if (this.#source.pull) {
        await this.#source.pull(this.#controller);
        if (this._queue.length > 0) {
          return { done: false, value: this._queue.shift() };
        }
      }
      return { done: true, value: undefined };
    }

    releaseLock() {
      this.#stream._reader = null;
    }
  }

  // WritableStream implementation
  globalThis.WritableStream = class WritableStream {
    #sink;
    #writer = null;
    #closed = false;

    constructor(sink = {}) {
      this.#sink = sink;
    }

    getWriter() {
      if (this.#writer) {
        throw new TypeError("WritableStream is locked");
      }
      this.#writer = new WritableStreamDefaultWriter(this, this.#sink);
      return this.#writer;
    }

    get locked() {
      return this.#writer !== null;
    }

    async close() {
      if (this.#sink.close) {
        await this.#sink.close();
      }
      this.#closed = true;
    }

    abort(reason) {
      if (this.#sink.abort) {
        return this.#sink.abort(reason);
      }
      this.#closed = true;
    }
  };

  class WritableStreamDefaultWriter {
    #stream;
    #sink;
    #controller;
    #readyPromise;
    #closedPromise;
    #resolveReady;
    #resolveClose;

    constructor(stream, sink) {
      this.#stream = stream;
      this.#sink = sink;
      this.#controller = {
        error: (e) => { throw e; },
      };

      this.#readyPromise = Promise.resolve();
      this.#closedPromise = new Promise(resolve => {
        this.#resolveClose = resolve;
      });

      if (sink.start) {
        sink.start(this.#controller);
      }
    }

    get ready() {
      return this.#readyPromise;
    }

    get closed() {
      return this.#closedPromise;
    }

    async write(chunk) {
      if (this.#sink.write) {
        await this.#sink.write(chunk, this.#controller);
      }
    }

    async close() {
      if (this.#sink.close) {
        await this.#sink.close();
      }
      this.#resolveClose();
    }

    abort(reason) {
      if (this.#sink.abort) {
        return this.#sink.abort(reason);
      }
    }

    releaseLock() {
      this.#stream._writer = null;
    }
  }

  // TransformStream implementation
  globalThis.TransformStream = class TransformStream {
    #readable;
    #writable;
    #transformer;
    #controller;
    #queue = [];

    constructor(transformer = {}) {
      this.#transformer = transformer;

      const self = this;
      this.#controller = {
        enqueue(chunk) {
          self.#queue.push(chunk);
        },
        error(reason) {
          throw reason;
        },
        terminate() {
          // No-op for basic implementation
        },
      };

      if (transformer.start) {
        transformer.start(this.#controller);
      }

      this.#readable = new ReadableStream({
        pull: async (controller) => {
          while (this.#queue.length > 0) {
            controller.enqueue(this.#queue.shift());
          }
        },
      });

      this.#writable = new WritableStream({
        write: async (chunk) => {
          if (this.#transformer.transform) {
            await this.#transformer.transform(chunk, this.#controller);
          } else {
            this.#controller.enqueue(chunk);
          }
        },
        close: async () => {
          if (this.#transformer.flush) {
            await this.#transformer.flush(this.#controller);
          }
        },
      });
    }

    get readable() {
      return this.#readable;
    }

    get writable() {
      return this.#writable;
    }
  };

  // Buffer implementation (Node.js compatibility)
  globalThis.Buffer = class Buffer extends Uint8Array {
    static alloc(size, fill = 0) {
      const buf = new Buffer(size);
      buf.fill(fill);
      return buf;
    }

    static allocUnsafe(size) {
      return new Buffer(size);
    }

    static allocUnsafeSlow(size) {
      // In Node.js, allocUnsafeSlow is similar to allocUnsafe but uses
      // a slower allocation path that doesn't use the pool
      return new Buffer(size);
    }

    static from(data, encoding = "utf8") {
      if (typeof data === "string") {
        if (encoding === "base64") {
          const binary = atob(data);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
          }
          return new Buffer(bytes);
        } else if (encoding === "hex") {
          const bytes = new Uint8Array(data.length / 2);
          for (let i = 0; i < data.length; i += 2) {
            bytes[i / 2] = parseInt(data.substr(i, 2), 16);
          }
          return new Buffer(bytes);
        } else {
          // Default to UTF-8
          const encoder = new TextEncoder();
          return new Buffer(encoder.encode(data));
        }
      } else if (Array.isArray(data)) {
        return new Buffer(new Uint8Array(data));
      } else if (data instanceof ArrayBuffer) {
        return new Buffer(new Uint8Array(data));
      } else if (ArrayBuffer.isView(data)) {
        return new Buffer(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
      }
      throw new TypeError("Invalid data type for Buffer.from");
    }

    static isBuffer(obj) {
      return obj instanceof Buffer;
    }

    static isEncoding(encoding) {
      if (typeof encoding !== "string") return false;
      const enc = encoding.toLowerCase();
      return ["utf8", "utf-8", "hex", "base64", "ascii", "latin1", "binary", "ucs2", "ucs-2", "utf16le", "utf-16le"].includes(enc);
    }

    static concat(list, totalLength) {
      if (totalLength === undefined) {
        totalLength = list.reduce((acc, buf) => acc + buf.length, 0);
      }
      const result = new Buffer(totalLength);
      let offset = 0;
      for (const buf of list) {
        result.set(buf, offset);
        offset += buf.length;
      }
      return result;
    }

    static byteLength(string, encoding = "utf8") {
      if (encoding === "utf8" || encoding === "utf-8") {
        return new TextEncoder().encode(string).length;
      }
      return string.length;
    }

    toString(encoding = "utf8") {
      if (encoding === "base64") {
        let binary = "";
        for (let i = 0; i < this.length; i++) {
          binary += String.fromCharCode(this[i]);
        }
        return btoa(binary);
      } else if (encoding === "hex") {
        return Array.from(this).map(b => b.toString(16).padStart(2, "0")).join("");
      } else {
        // Default to UTF-8
        const decoder = new TextDecoder();
        return decoder.decode(this);
      }
    }

    write(string, offset = 0, length, encoding = "utf8") {
      const bytes = Buffer.from(string, encoding);
      const writeLength = Math.min(bytes.length, length ?? bytes.length, this.length - offset);
      this.set(bytes.subarray(0, writeLength), offset);
      return writeLength;
    }

    copy(target, targetStart = 0, sourceStart = 0, sourceEnd = this.length) {
      const slice = this.subarray(sourceStart, sourceEnd);
      target.set(slice, targetStart);
      return slice.length;
    }

    slice(start, end) {
      return new Buffer(this.subarray(start, end));
    }

    equals(other) {
      if (this.length !== other.length) return false;
      for (let i = 0; i < this.length; i++) {
        if (this[i] !== other[i]) return false;
      }
      return true;
    }

    compare(other) {
      const len = Math.min(this.length, other.length);
      for (let i = 0; i < len; i++) {
        if (this[i] < other[i]) return -1;
        if (this[i] > other[i]) return 1;
      }
      if (this.length < other.length) return -1;
      if (this.length > other.length) return 1;
      return 0;
    }

    indexOf(value, byteOffset = 0) {
      if (typeof value === "string") {
        value = Buffer.from(value);
      }
      if (typeof value === "number") {
        for (let i = byteOffset; i < this.length; i++) {
          if (this[i] === value) return i;
        }
        return -1;
      }
      // Search for buffer
      outer: for (let i = byteOffset; i <= this.length - value.length; i++) {
        for (let j = 0; j < value.length; j++) {
          if (this[i + j] !== value[j]) continue outer;
        }
        return i;
      }
      return -1;
    }

    includes(value, byteOffset = 0) {
      return this.indexOf(value, byteOffset) !== -1;
    }

    fill(value, offset = 0, end = this.length, encoding = "utf8") {
      if (typeof value === "string") {
        if (value.length === 0) {
          value = 0;
        } else {
          const fillBuf = Buffer.from(value, encoding);
          for (let i = offset; i < end; i++) {
            this[i] = fillBuf[i % fillBuf.length];
          }
          return this;
        }
      }
      // Numeric fill
      for (let i = offset; i < end; i++) {
        this[i] = value & 0xff;
      }
      return this;
    }

    // Read methods
    readUInt8(offset = 0) { return this[offset]; }
    readUInt16LE(offset = 0) { return this[offset] | (this[offset + 1] << 8); }
    readUInt16BE(offset = 0) { return (this[offset] << 8) | this[offset + 1]; }
    readUInt32LE(offset = 0) {
      return (this[offset] | (this[offset + 1] << 8) | (this[offset + 2] << 16) | (this[offset + 3] << 24)) >>> 0;
    }
    readUInt32BE(offset = 0) {
      return ((this[offset] << 24) | (this[offset + 1] << 16) | (this[offset + 2] << 8) | this[offset + 3]) >>> 0;
    }
    readInt8(offset = 0) {
      const val = this[offset];
      return val > 127 ? val - 256 : val;
    }
    readInt16LE(offset = 0) {
      const val = this.readUInt16LE(offset);
      return val > 32767 ? val - 65536 : val;
    }
    readInt16BE(offset = 0) {
      const val = this.readUInt16BE(offset);
      return val > 32767 ? val - 65536 : val;
    }
    readInt32LE(offset = 0) {
      return this[offset] | (this[offset + 1] << 8) | (this[offset + 2] << 16) | (this[offset + 3] << 24);
    }
    readInt32BE(offset = 0) {
      return (this[offset] << 24) | (this[offset + 1] << 16) | (this[offset + 2] << 8) | this[offset + 3];
    }

    // Write methods
    writeUInt8(value, offset = 0) { this[offset] = value & 0xff; return offset + 1; }
    writeUInt16LE(value, offset = 0) {
      this[offset] = value & 0xff;
      this[offset + 1] = (value >> 8) & 0xff;
      return offset + 2;
    }
    writeUInt16BE(value, offset = 0) {
      this[offset] = (value >> 8) & 0xff;
      this[offset + 1] = value & 0xff;
      return offset + 2;
    }
    writeUInt32LE(value, offset = 0) {
      this[offset] = value & 0xff;
      this[offset + 1] = (value >> 8) & 0xff;
      this[offset + 2] = (value >> 16) & 0xff;
      this[offset + 3] = (value >> 24) & 0xff;
      return offset + 4;
    }
    writeUInt32BE(value, offset = 0) {
      this[offset] = (value >> 24) & 0xff;
      this[offset + 1] = (value >> 16) & 0xff;
      this[offset + 2] = (value >> 8) & 0xff;
      this[offset + 3] = value & 0xff;
      return offset + 4;
    }
    writeInt8(value, offset = 0) { return this.writeUInt8(value < 0 ? value + 256 : value, offset); }
    writeInt16LE(value, offset = 0) { return this.writeUInt16LE(value < 0 ? value + 65536 : value, offset); }
    writeInt16BE(value, offset = 0) { return this.writeUInt16BE(value < 0 ? value + 65536 : value, offset); }
    writeInt32LE(value, offset = 0) { return this.writeUInt32LE(value >>> 0, offset); }
    writeInt32BE(value, offset = 0) { return this.writeUInt32BE(value >>> 0, offset); }

    // Float and Double methods using DataView
    readFloatLE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 4);
      return view.getFloat32(0, true);
    }
    readFloatBE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 4);
      return view.getFloat32(0, false);
    }
    readDoubleLE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getFloat64(0, true);
    }
    readDoubleBE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getFloat64(0, false);
    }
    writeFloatLE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 4);
      view.setFloat32(0, value, true);
      return offset + 4;
    }
    writeFloatBE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 4);
      view.setFloat32(0, value, false);
      return offset + 4;
    }
    writeDoubleLE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setFloat64(0, value, true);
      return offset + 8;
    }
    writeDoubleBE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setFloat64(0, value, false);
      return offset + 8;
    }

    // BigInt methods
    readBigInt64LE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getBigInt64(0, true);
    }
    readBigInt64BE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getBigInt64(0, false);
    }
    readBigUInt64LE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getBigUint64(0, true);
    }
    readBigUInt64BE(offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      return view.getBigUint64(0, false);
    }
    writeBigInt64LE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setBigInt64(0, value, true);
      return offset + 8;
    }
    writeBigInt64BE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setBigInt64(0, value, false);
      return offset + 8;
    }
    writeBigUInt64LE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setBigUint64(0, value, true);
      return offset + 8;
    }
    writeBigUInt64BE(value, offset = 0) {
      const view = new DataView(this.buffer, this.byteOffset + offset, 8);
      view.setBigUint64(0, value, false);
      return offset + 8;
    }

    toJSON() {
      return { type: "Buffer", data: Array.from(this) };
    }
  };

  // performance API
  const performanceStart = Date.now();
  const performanceMarks = new Map();
  const performanceMeasures = [];
  const performanceEntries = [];

  globalThis.performance = {
    now() {
      return Date.now() - performanceStart;
    },
    timeOrigin: performanceStart,

    mark(name, options = {}) {
      const startTime = options.startTime ?? this.now();
      const entry = {
        name,
        entryType: "mark",
        startTime,
        duration: 0,
        detail: options.detail,
        toJSON() {
          return { name: this.name, entryType: this.entryType, startTime: this.startTime, duration: this.duration };
        },
      };
      performanceMarks.set(name, entry);
      performanceEntries.push(entry);
      return entry;
    },

    measure(name, startMarkOrOptions, endMark) {
      let startTime = 0;
      let endTime = this.now();
      let detail;

      if (typeof startMarkOrOptions === "string") {
        const startMarkEntry = performanceMarks.get(startMarkOrOptions);
        if (startMarkEntry) startTime = startMarkEntry.startTime;
        if (endMark) {
          const endMarkEntry = performanceMarks.get(endMark);
          if (endMarkEntry) endTime = endMarkEntry.startTime;
        }
      } else if (startMarkOrOptions && typeof startMarkOrOptions === "object") {
        if (startMarkOrOptions.start !== undefined) {
          if (typeof startMarkOrOptions.start === "string") {
            const m = performanceMarks.get(startMarkOrOptions.start);
            startTime = m ? m.startTime : 0;
          } else {
            startTime = startMarkOrOptions.start;
          }
        }
        if (startMarkOrOptions.end !== undefined) {
          if (typeof startMarkOrOptions.end === "string") {
            const m = performanceMarks.get(startMarkOrOptions.end);
            endTime = m ? m.startTime : this.now();
          } else {
            endTime = startMarkOrOptions.end;
          }
        }
        if (startMarkOrOptions.duration !== undefined) {
          endTime = startTime + startMarkOrOptions.duration;
        }
        detail = startMarkOrOptions.detail;
      }

      const entry = {
        name,
        entryType: "measure",
        startTime,
        duration: endTime - startTime,
        detail,
        toJSON() {
          return { name: this.name, entryType: this.entryType, startTime: this.startTime, duration: this.duration };
        },
      };
      performanceMeasures.push(entry);
      performanceEntries.push(entry);
      return entry;
    },

    clearMarks(name) {
      if (name) {
        performanceMarks.delete(name);
        const idx = performanceEntries.findIndex((e) => e.name === name && e.entryType === "mark");
        if (idx !== -1) performanceEntries.splice(idx, 1);
      } else {
        performanceMarks.clear();
        for (let i = performanceEntries.length - 1; i >= 0; i--) {
          if (performanceEntries[i].entryType === "mark") performanceEntries.splice(i, 1);
        }
      }
    },

    clearMeasures(name) {
      if (name) {
        const idx = performanceMeasures.findIndex((m) => m.name === name);
        if (idx !== -1) performanceMeasures.splice(idx, 1);
        const idx2 = performanceEntries.findIndex((e) => e.name === name && e.entryType === "measure");
        if (idx2 !== -1) performanceEntries.splice(idx2, 1);
      } else {
        performanceMeasures.length = 0;
        for (let i = performanceEntries.length - 1; i >= 0; i--) {
          if (performanceEntries[i].entryType === "measure") performanceEntries.splice(i, 1);
        }
      }
    },

    clearResourceTimings() {
      // Clear resource timing entries
      for (let i = performanceEntries.length - 1; i >= 0; i--) {
        if (performanceEntries[i].entryType === "resource") performanceEntries.splice(i, 1);
      }
    },

    getEntries() {
      return [...performanceEntries];
    },

    getEntriesByName(name, type) {
      return performanceEntries.filter(
        (e) => e.name === name && (!type || e.entryType === type)
      );
    },

    getEntriesByType(type) {
      return performanceEntries.filter((e) => e.entryType === type);
    },

    setResourceTimingBufferSize(maxSize) {
      // No-op for now
    },

    toJSON() {
      return {
        timeOrigin: this.timeOrigin,
      };
    },
  };

  // structuredClone
  globalThis.structuredClone = (value) => {
    // Simple implementation using JSON (doesn't handle all cases)
    return JSON.parse(JSON.stringify(value));
  };

  // sleep helper (non-standard but useful)
  globalThis.sleep = (ms) => ops.op_howth_sleep(ms);

  // ============================================
  // Node.js built-in modules
  // ============================================

  // Detect platform
  const isWindows = (Deno.build?.os || "").toLowerCase() === "windows" ||
                    globalThis.process?.platform === "win32";

  // Character constants (matching Node.js internal/constants)
  const CHAR_UPPERCASE_A = 65;
  const CHAR_LOWERCASE_A = 97;
  const CHAR_UPPERCASE_Z = 90;
  const CHAR_LOWERCASE_Z = 122;
  const CHAR_DOT = 46;
  const CHAR_FORWARD_SLASH = 47;
  const CHAR_BACKWARD_SLASH = 92;
  const CHAR_COLON = 58;

  // Argument validation helper (matching Node.js)
  function validateString(value, name) {
    if (typeof value !== 'string') {
      const err = new TypeError(`The "${name}" argument must be of type string. Received ${typeof value === 'object' ? (value === null ? 'null' : 'object') : typeof value}`);
      err.code = 'ERR_INVALID_ARG_TYPE';
      throw err;
    }
  }

  // Helper functions (matching Node.js lib/path.js)
  function isPathSeparator(code) {
    return code === CHAR_FORWARD_SLASH || code === CHAR_BACKWARD_SLASH;
  }

  function isPosixPathSeparator(code) {
    return code === CHAR_FORWARD_SLASH;
  }

  function isWindowsDeviceRoot(code) {
    return (code >= CHAR_UPPERCASE_A && code <= CHAR_UPPERCASE_Z) ||
           (code >= CHAR_LOWERCASE_A && code <= CHAR_LOWERCASE_Z);
  }

  // Windows reserved device names
  const WINDOWS_RESERVED_NAMES = [
    'CON', 'PRN', 'AUX', 'NUL',
    'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
    'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
  ];

  function isWindowsReservedName(path, colonIndex) {
    const devicePart = path.slice(0, colonIndex).toUpperCase();
    return WINDOWS_RESERVED_NAMES.includes(devicePart);
  }

  // Windows path case-insensitive lowercase that preserves length
  // Handles Turkish İ by first removing combining marks, then lowercasing
  function toLowerCasePreservingLength(str) {
    // First remove combining dot above (U+0307) which is part of i̇
    // This converts i̇ (2 chars: i + combining dot) to just i (1 char)
    const withoutCombining = str.replace(/\u0307/g, '');
    // Now lowercase - İ (U+0130) becomes i̇ but we need to handle it specially
    let result = '';
    for (let i = 0; i < withoutCombining.length; i++) {
      const code = withoutCombining.charCodeAt(i);
      if (code === 0x0130) {
        // İ -> i (not i̇, to preserve length)
        result += 'i';
      } else if (code >= CHAR_UPPERCASE_A && code <= CHAR_UPPERCASE_Z) {
        result += String.fromCharCode(code + 32);
      } else {
        result += withoutCombining[i];
      }
    }
    return result;
  }

  // Core path normalization function (from Node.js)
  function normalizeString(path, allowAboveRoot, separator, isPathSeparatorFn) {
    let res = '';
    let lastSegmentLength = 0;
    let lastSlash = -1;
    let dots = 0;
    let code = 0;

    for (let i = 0; i <= path.length; ++i) {
      if (i < path.length)
        code = path.charCodeAt(i);
      else if (isPathSeparatorFn(code))
        break;
      else
        code = CHAR_FORWARD_SLASH;

      if (isPathSeparatorFn(code)) {
        if (lastSlash === i - 1 || dots === 1) {
          // NOOP
        } else if (dots === 2) {
          if (res.length < 2 || lastSegmentLength !== 2 ||
              res.charCodeAt(res.length - 1) !== CHAR_DOT ||
              res.charCodeAt(res.length - 2) !== CHAR_DOT) {
            if (res.length > 2) {
              const lastSlashIndex = res.lastIndexOf(separator);
              if (lastSlashIndex === -1) {
                res = '';
                lastSegmentLength = 0;
              } else {
                res = res.slice(0, lastSlashIndex);
                lastSegmentLength = res.length - 1 - res.lastIndexOf(separator);
              }
              lastSlash = i;
              dots = 0;
              continue;
            } else if (res.length !== 0) {
              res = '';
              lastSegmentLength = 0;
              lastSlash = i;
              dots = 0;
              continue;
            }
          }
          if (allowAboveRoot) {
            res += res.length > 0 ? `${separator}..` : '..';
            lastSegmentLength = 2;
          }
        } else {
          if (res.length > 0)
            res += `${separator}${path.slice(lastSlash + 1, i)}`;
          else
            res = path.slice(lastSlash + 1, i);
          lastSegmentLength = i - lastSlash - 1;
        }
        lastSlash = i;
        dots = 0;
      } else if (code === CHAR_DOT && dots !== -1) {
        ++dots;
      } else {
        dots = -1;
      }
    }
    return res;
  }

  // ============================================
  // POSIX path implementation
  // ============================================
  const posixPath = {};

  posixPath.sep = "/";
  posixPath.delimiter = ":";

  posixPath.normalize = function normalize(path) {
    validateString(path, 'path');
    if (path.length === 0) return '.';

    const isAbsolute = path.charCodeAt(0) === CHAR_FORWARD_SLASH;
    const trailingSeparator = path.charCodeAt(path.length - 1) === CHAR_FORWARD_SLASH;

    path = normalizeString(path, !isAbsolute, '/', isPosixPathSeparator);

    if (path.length === 0) {
      if (isAbsolute) return '/';
      return trailingSeparator ? './' : '.';
    }
    if (trailingSeparator) path += '/';

    return isAbsolute ? `/${path}` : path;
  };

  posixPath.join = function join(...args) {
    if (args.length === 0) return '.';

    let joined;
    for (let i = 0; i < args.length; ++i) {
      const arg = args[i];
      validateString(arg, 'path');
      if (arg.length > 0) {
        if (joined === undefined) joined = arg;
        else joined += `/${arg}`;
      }
    }

    if (joined === undefined) return '.';

    return posixPath.normalize(joined);
  };

  posixPath.resolve = function resolve(...args) {
    let resolvedPath = '';
    let resolvedAbsolute = false;

    for (let i = args.length - 1; i >= -1 && !resolvedAbsolute; i--) {
      let path;
      if (i >= 0) {
        path = args[i];
        validateString(path, 'path');
      } else {
        // Use process.cwd() to allow tests to override it
        const cwd = globalThis.process?.cwd?.();
        path = cwd !== undefined ? cwd : ops.op_howth_cwd();
      }

      if (path.length === 0) continue;

      resolvedPath = `${path}/${resolvedPath}`;
      resolvedAbsolute = path.charCodeAt(0) === CHAR_FORWARD_SLASH;
    }

    resolvedPath = normalizeString(resolvedPath, !resolvedAbsolute, '/', isPosixPathSeparator);

    if (resolvedAbsolute) {
      return `/${resolvedPath}`;
    }
    return resolvedPath.length > 0 ? resolvedPath : '.';
  };

  posixPath.isAbsolute = function isAbsolute(path) {
    validateString(path, 'path');
    return path.length > 0 && path.charCodeAt(0) === CHAR_FORWARD_SLASH;
  };

  posixPath.dirname = function dirname(path) {
    validateString(path, 'path');
    if (path.length === 0) return '.';

    const hasRoot = path.charCodeAt(0) === CHAR_FORWARD_SLASH;
    let end = -1;
    let matchedSlash = true;

    for (let i = path.length - 1; i >= 1; --i) {
      if (path.charCodeAt(i) === CHAR_FORWARD_SLASH) {
        if (!matchedSlash) {
          end = i;
          break;
        }
      } else {
        matchedSlash = false;
      }
    }

    if (end === -1) return hasRoot ? '/' : '.';
    if (hasRoot && end === 1) return '//';
    return path.slice(0, end);
  };

  posixPath.basename = function basename(path, ext) {
    validateString(path, 'path');
    if (ext !== undefined) validateString(ext, 'ext');

    let start = 0;
    let end = -1;
    let matchedSlash = true;

    if (ext !== undefined && ext.length > 0 && ext.length <= path.length) {
      if (ext === path) return '';
      let extIdx = ext.length - 1;
      let firstNonSlashEnd = -1;

      for (let i = path.length - 1; i >= 0; --i) {
        const code = path.charCodeAt(i);
        if (code === CHAR_FORWARD_SLASH) {
          if (!matchedSlash) {
            start = i + 1;
            break;
          }
        } else {
          if (firstNonSlashEnd === -1) {
            matchedSlash = false;
            firstNonSlashEnd = i + 1;
          }
          if (extIdx >= 0) {
            if (code === ext.charCodeAt(extIdx)) {
              if (--extIdx === -1) {
                end = i;
              }
            } else {
              extIdx = -1;
              end = firstNonSlashEnd;
            }
          }
        }
      }

      if (start === end) end = firstNonSlashEnd;
      else if (end === -1) end = path.length;
      return path.slice(start, end);
    }

    for (let i = path.length - 1; i >= 0; --i) {
      if (path.charCodeAt(i) === CHAR_FORWARD_SLASH) {
        if (!matchedSlash) {
          start = i + 1;
          break;
        }
      } else if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
    }

    if (end === -1) return '';
    return path.slice(start, end);
  };

  posixPath.extname = function extname(path) {
    validateString(path, 'path');

    let startDot = -1;
    let startPart = 0;
    let end = -1;
    let matchedSlash = true;
    let preDotState = 0;

    for (let i = path.length - 1; i >= 0; --i) {
      const code = path.charCodeAt(i);
      if (code === CHAR_FORWARD_SLASH) {
        if (!matchedSlash) {
          startPart = i + 1;
          break;
        }
        continue;
      }
      if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
      if (code === CHAR_DOT) {
        if (startDot === -1) startDot = i;
        else if (preDotState !== 1) preDotState = 1;
      } else if (startDot !== -1) {
        preDotState = -1;
      }
    }

    if (startDot === -1 || end === -1 ||
        preDotState === 0 ||
        (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
      return '';
    }
    return path.slice(startDot, end);
  };

  posixPath.relative = function relative(from, to) {
    validateString(from, 'from');
    validateString(to, 'to');

    if (from === to) return '';

    from = posixPath.resolve(from);
    to = posixPath.resolve(to);

    if (from === to) return '';

    const fromStart = 1;
    const fromEnd = from.length;
    const fromLen = fromEnd - fromStart;
    const toStart = 1;
    const toLen = to.length - toStart;

    const length = fromLen < toLen ? fromLen : toLen;
    let lastCommonSep = -1;
    let i = 0;

    for (; i < length; i++) {
      const fromCode = from.charCodeAt(fromStart + i);
      if (fromCode !== to.charCodeAt(toStart + i)) break;
      else if (fromCode === CHAR_FORWARD_SLASH) lastCommonSep = i;
    }

    if (i === length) {
      if (toLen > length) {
        if (to.charCodeAt(toStart + i) === CHAR_FORWARD_SLASH) {
          return to.slice(toStart + i + 1);
        }
        if (i === 0) {
          return to.slice(toStart + i);
        }
      } else if (fromLen > length) {
        if (from.charCodeAt(fromStart + i) === CHAR_FORWARD_SLASH) {
          lastCommonSep = i;
        } else if (i === 0) {
          lastCommonSep = 0;
        }
      }
    }

    let out = '';
    for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
      if (i === fromEnd || from.charCodeAt(i) === CHAR_FORWARD_SLASH) {
        out += out.length === 0 ? '..' : '/..';
      }
    }

    return `${out}${to.slice(toStart + lastCommonSep)}`;
  };

  posixPath.parse = function parse(path) {
    validateString(path, 'path');

    const ret = { root: '', dir: '', base: '', ext: '', name: '' };
    if (path.length === 0) return ret;

    const isAbsolute = path.charCodeAt(0) === CHAR_FORWARD_SLASH;
    let start;
    if (isAbsolute) {
      ret.root = '/';
      start = 1;
    } else {
      start = 0;
    }

    let startDot = -1;
    let startPart = 0;
    let end = -1;
    let matchedSlash = true;
    let i = path.length - 1;
    let preDotState = 0;

    for (; i >= start; --i) {
      const code = path.charCodeAt(i);
      if (code === CHAR_FORWARD_SLASH) {
        if (!matchedSlash) {
          startPart = i + 1;
          break;
        }
        continue;
      }
      if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
      if (code === CHAR_DOT) {
        if (startDot === -1) startDot = i;
        else if (preDotState !== 1) preDotState = 1;
      } else if (startDot !== -1) {
        preDotState = -1;
      }
    }

    if (end !== -1) {
      const start2 = startPart === 0 && isAbsolute ? 1 : startPart;
      if (startDot === -1 || preDotState === 0 ||
          (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
        ret.base = ret.name = path.slice(start2, end);
      } else {
        ret.name = path.slice(start2, startDot);
        ret.base = path.slice(start2, end);
        ret.ext = path.slice(startDot, end);
      }
    }

    if (startPart > 0) ret.dir = path.slice(0, startPart - 1);
    else if (isAbsolute) ret.dir = '/';

    return ret;
  };

  posixPath.format = function format(pathObject) {
    if (pathObject === null || typeof pathObject !== 'object') {
      let received;
      if (pathObject == null) {
        received = `Received ${pathObject}`;
      } else {
        const inspected = JSON.stringify(pathObject);
        received = `Received type ${typeof pathObject} (${inspected})`;
      }
      const err = new TypeError(`The "pathObject" argument must be of type object. ${received}`);
      err.code = 'ERR_INVALID_ARG_TYPE';
      throw err;
    }
    const dir = pathObject.dir || pathObject.root;
    let base = pathObject.base;
    if (!base) {
      const name = pathObject.name || '';
      let ext = pathObject.ext || '';
      // Normalize ext to ensure it starts with a dot (if non-empty)
      if (ext && ext.charCodeAt(0) !== CHAR_DOT) {
        ext = `.${ext}`;
      }
      base = `${name}${ext}`;
    }
    if (!dir) return base;
    return dir === pathObject.root ? `${dir}${base}` : `${dir}/${base}`;
  };

  posixPath.toNamespacedPath = function toNamespacedPath(path) {
    return path;
  };

  // ============================================
  // Windows path implementation
  // ============================================
  const win32Path = {};

  win32Path.sep = "\\";
  win32Path.delimiter = ";";

  win32Path.normalize = function normalize(path) {
    validateString(path, 'path');
    const len = path.length;
    if (len === 0) return '.';

    let rootEnd = 0;
    let device;
    let isAbsolute = false;
    const code = path.charCodeAt(0);

    if (len === 1) {
      return isPosixPathSeparator(code) ? '\\' : path;
    }

    if (isPathSeparator(code)) {
      isAbsolute = true;

      if (isPathSeparator(path.charCodeAt(1))) {
        let j = 2;
        let last = j;
        while (j < len && !isPathSeparator(path.charCodeAt(j))) {
          j++;
        }
        if (j < len && j !== last) {
          const firstPart = path.slice(last, j);
          last = j;
          while (j < len && isPathSeparator(path.charCodeAt(j))) {
            j++;
          }
          if (j < len && j !== last) {
            last = j;
            while (j < len && !isPathSeparator(path.charCodeAt(j))) {
              j++;
            }
            if (j === len || j !== last) {
              if (firstPart === '.' || firstPart === '?') {
                device = `\\\\${firstPart}`;
                rootEnd = 4;
              } else if (j === len) {
                return `\\\\${firstPart}\\${path.slice(last)}\\`;
              } else {
                device = `\\\\${firstPart}\\${path.slice(last, j)}`;
                rootEnd = j;
              }
            }
          }
        }
      }
      if (device === undefined) {
        rootEnd = 1;
      }
    } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === CHAR_COLON) {
      device = path.slice(0, 2);
      rootEnd = 2;
      if (len > 2 && isPathSeparator(path.charCodeAt(2))) {
        isAbsolute = true;
        rootEnd = 3;
      }
    }

    let tail = rootEnd < len ?
      normalizeString(path.slice(rootEnd), !isAbsolute, '\\', isPathSeparator) : '';

    if (tail.length === 0 && !isAbsolute) tail = '.';
    if (tail.length > 0 && isPathSeparator(path.charCodeAt(len - 1))) tail += '\\';

    if (device === undefined) {
      return isAbsolute ? `\\${tail}` : tail;
    }
    return isAbsolute ? `${device}\\${tail}` : `${device}${tail}`;
  };

  win32Path.join = function join(...args) {
    if (args.length === 0) return '.';

    let joined;
    let firstPart;
    for (let i = 0; i < args.length; ++i) {
      const arg = args[i];
      validateString(arg, 'path');
      if (arg.length > 0) {
        if (joined === undefined) {
          joined = firstPart = arg;
        } else {
          joined += `\\${arg}`;
        }
      }
    }

    if (joined === undefined) return '.';

    let needsReplace = true;
    let slashCount = 0;

    if (isPathSeparator(firstPart.charCodeAt(0))) {
      ++slashCount;
      const firstLen = firstPart.length;
      if (firstLen > 1 && isPathSeparator(firstPart.charCodeAt(1))) {
        ++slashCount;
        if (firstLen > 2) {
          if (isPathSeparator(firstPart.charCodeAt(2))) ++slashCount;
          else needsReplace = false;
        }
      }
    }

    if (needsReplace) {
      while (slashCount < joined.length && isPathSeparator(joined.charCodeAt(slashCount))) {
        slashCount++;
      }
      if (slashCount >= 2) {
        joined = `\\${joined.slice(slashCount)}`;
      }
    }

    return win32Path.normalize(joined);
  };

  win32Path.resolve = function resolve(...args) {
    let resolvedDevice = '';
    let resolvedTail = '';
    let resolvedAbsolute = false;

    for (let i = args.length - 1; i >= -1; i--) {
      let path;
      if (i >= 0) {
        path = args[i];
        validateString(path, 'path');
        if (path.length === 0) continue;
      } else if (resolvedDevice.length === 0) {
        // Use process.cwd() to allow tests to override it
        const cwd = globalThis.process?.cwd?.();
        path = cwd !== undefined ? cwd : ops.op_howth_cwd();
      } else {
        path = `${resolvedDevice}\\`;
      }

      const len = path.length;
      let rootEnd = 0;
      let device = '';
      let isAbsolute = false;
      const code = path.charCodeAt(0);

      if (len === 1) {
        if (isPathSeparator(code)) {
          rootEnd = 1;
          isAbsolute = true;
        }
      } else if (isPathSeparator(code)) {
        isAbsolute = true;

        if (isPathSeparator(path.charCodeAt(1))) {
          let j = 2;
          let last = j;
          while (j < len && !isPathSeparator(path.charCodeAt(j))) {
            j++;
          }
          if (j < len && j !== last) {
            const firstPart = path.slice(last, j);
            last = j;
            while (j < len && isPathSeparator(path.charCodeAt(j))) {
              j++;
            }
            if (j < len && j !== last) {
              last = j;
              while (j < len && !isPathSeparator(path.charCodeAt(j))) {
                j++;
              }
              if (j === len || j !== last) {
                if (firstPart !== '.' && firstPart !== '?') {
                  device = `\\\\${firstPart}\\${path.slice(last, j)}`;
                  rootEnd = j;
                } else {
                  device = `\\\\${firstPart}`;
                  rootEnd = 4;
                }
              }
            }
          }
        } else {
          rootEnd = 1;
        }
      } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === CHAR_COLON) {
        device = path.slice(0, 2);
        rootEnd = 2;
        if (len > 2 && isPathSeparator(path.charCodeAt(2))) {
          isAbsolute = true;
          rootEnd = 3;
        }
      }

      if (device.length > 0) {
        if (resolvedDevice.length > 0) {
          if (device.toLowerCase() !== resolvedDevice.toLowerCase()) continue;
        } else {
          resolvedDevice = device;
        }
      }

      if (resolvedAbsolute) {
        if (resolvedDevice.length > 0) break;
      } else {
        resolvedTail = `${path.slice(rootEnd)}\\${resolvedTail}`;
        resolvedAbsolute = isAbsolute;
        if (isAbsolute && resolvedDevice.length > 0) {
          break;
        }
      }
    }

    resolvedTail = normalizeString(resolvedTail, !resolvedAbsolute, '\\', isPathSeparator);

    return resolvedAbsolute ?
      `${resolvedDevice}\\${resolvedTail}` :
      `${resolvedDevice}${resolvedTail}` || '.';
  };

  win32Path.isAbsolute = function isAbsolute(path) {
    validateString(path, 'path');
    if (path.length === 0) return false;

    const code = path.charCodeAt(0);
    return isPathSeparator(code) ||
      (path.length > 2 && isWindowsDeviceRoot(code) &&
       path.charCodeAt(1) === CHAR_COLON && isPathSeparator(path.charCodeAt(2)));
  };

  win32Path.dirname = function dirname(path) {
    validateString(path, 'path');
    const len = path.length;
    if (len === 0) return '.';

    let rootEnd = -1;
    let offset = 0;
    const code = path.charCodeAt(0);

    if (len === 1) {
      return isPathSeparator(code) ? path : '.';
    }

    if (isPathSeparator(code)) {
      rootEnd = offset = 1;
      if (isPathSeparator(path.charCodeAt(1))) {
        let j = 2;
        let last = j;
        while (j < len && !isPathSeparator(path.charCodeAt(j))) {
          j++;
        }
        if (j < len && j !== last) {
          last = j;
          while (j < len && isPathSeparator(path.charCodeAt(j))) {
            j++;
          }
          if (j < len && j !== last) {
            last = j;
            while (j < len && !isPathSeparator(path.charCodeAt(j))) {
              j++;
            }
            if (j === len) {
              return path;
            }
            if (j !== last) {
              rootEnd = offset = j + 1;
            }
          }
        }
      }
    } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === CHAR_COLON) {
      rootEnd = len > 2 && isPathSeparator(path.charCodeAt(2)) ? 3 : 2;
      offset = rootEnd;
    }

    let end = -1;
    let matchedSlash = true;
    for (let i = len - 1; i >= offset; --i) {
      if (isPathSeparator(path.charCodeAt(i))) {
        if (!matchedSlash) {
          end = i;
          break;
        }
      } else {
        matchedSlash = false;
      }
    }

    if (end === -1) {
      if (rootEnd === -1) return '.';
      end = rootEnd;
    }
    return path.slice(0, end);
  };

  win32Path.basename = function basename(path, ext) {
    validateString(path, 'path');
    if (ext !== undefined) validateString(ext, 'ext');

    let start = 0;
    let end = -1;
    let matchedSlash = true;

    if (path.length >= 2 && isWindowsDeviceRoot(path.charCodeAt(0)) &&
        path.charCodeAt(1) === CHAR_COLON) {
      start = 2;
    }

    if (ext !== undefined && ext.length > 0 && ext.length <= path.length) {
      if (ext === path) return '';
      let extIdx = ext.length - 1;
      let firstNonSlashEnd = -1;

      for (let i = path.length - 1; i >= start; --i) {
        const code = path.charCodeAt(i);
        if (isPathSeparator(code)) {
          if (!matchedSlash) {
            start = i + 1;
            break;
          }
        } else {
          if (firstNonSlashEnd === -1) {
            matchedSlash = false;
            firstNonSlashEnd = i + 1;
          }
          if (extIdx >= 0) {
            if (code === ext.charCodeAt(extIdx)) {
              if (--extIdx === -1) {
                end = i;
              }
            } else {
              extIdx = -1;
              end = firstNonSlashEnd;
            }
          }
        }
      }

      if (start === end) end = firstNonSlashEnd;
      else if (end === -1) end = path.length;
      return path.slice(start, end);
    }

    for (let i = path.length - 1; i >= start; --i) {
      if (isPathSeparator(path.charCodeAt(i))) {
        if (!matchedSlash) {
          start = i + 1;
          break;
        }
      } else if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
    }

    if (end === -1) return '';
    return path.slice(start, end);
  };

  win32Path.extname = function extname(path) {
    validateString(path, 'path');

    let start = 0;
    let startDot = -1;
    let startPart = 0;
    let end = -1;
    let matchedSlash = true;
    let preDotState = 0;

    if (path.length >= 2 && path.charCodeAt(1) === CHAR_COLON &&
        isWindowsDeviceRoot(path.charCodeAt(0))) {
      start = startPart = 2;
    }

    for (let i = path.length - 1; i >= start; --i) {
      const code = path.charCodeAt(i);
      if (isPathSeparator(code)) {
        if (!matchedSlash) {
          startPart = i + 1;
          break;
        }
        continue;
      }
      if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
      if (code === CHAR_DOT) {
        if (startDot === -1) startDot = i;
        else if (preDotState !== 1) preDotState = 1;
      } else if (startDot !== -1) {
        preDotState = -1;
      }
    }

    if (startDot === -1 || end === -1 ||
        preDotState === 0 ||
        (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
      return '';
    }
    return path.slice(startDot, end);
  };

  win32Path.relative = function relative(from, to) {
    validateString(from, 'from');
    validateString(to, 'to');

    if (from === to) return '';

    const fromOrig = win32Path.resolve(from);
    const toOrig = win32Path.resolve(to);

    if (fromOrig === toOrig) return '';

    from = toLowerCasePreservingLength(fromOrig);
    to = toLowerCasePreservingLength(toOrig);

    if (from === to) return '';

    let fromStart = 0;
    while (fromStart < from.length && from.charCodeAt(fromStart) === CHAR_BACKWARD_SLASH) {
      fromStart++;
    }
    let fromEnd = from.length;
    while (fromEnd - 1 > fromStart && from.charCodeAt(fromEnd - 1) === CHAR_BACKWARD_SLASH) {
      fromEnd--;
    }
    const fromLen = fromEnd - fromStart;

    let toStart = 0;
    while (toStart < to.length && to.charCodeAt(toStart) === CHAR_BACKWARD_SLASH) {
      toStart++;
    }
    let toEnd = to.length;
    while (toEnd - 1 > toStart && to.charCodeAt(toEnd - 1) === CHAR_BACKWARD_SLASH) {
      toEnd--;
    }
    const toLen = toEnd - toStart;

    const length = fromLen < toLen ? fromLen : toLen;
    let lastCommonSep = -1;
    let i = 0;

    for (; i < length; i++) {
      const fromCode = from.charCodeAt(fromStart + i);
      if (fromCode !== to.charCodeAt(toStart + i)) break;
      else if (fromCode === CHAR_BACKWARD_SLASH) lastCommonSep = i;
    }

    if (i !== length) {
      if (lastCommonSep === -1) return toOrig;
    } else {
      if (toLen > length) {
        if (to.charCodeAt(toStart + i) === CHAR_BACKWARD_SLASH) {
          return toOrig.slice(toStart + i + 1);
        }
        if (i === 2) {
          return toOrig.slice(toStart + i);
        }
      }
      if (fromLen > length) {
        if (from.charCodeAt(fromStart + i) === CHAR_BACKWARD_SLASH) {
          lastCommonSep = i;
        } else if (i === 2) {
          lastCommonSep = 3;
        }
      }
      if (lastCommonSep === -1) lastCommonSep = 0;
    }

    let out = '';
    for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
      if (i === fromEnd || from.charCodeAt(i) === CHAR_BACKWARD_SLASH) {
        out += out.length === 0 ? '..' : '\\..';
      }
    }

    toStart += lastCommonSep;

    if (out.length > 0) return `${out}${toOrig.slice(toStart, toEnd)}`;

    if (toOrig.charCodeAt(toStart) === CHAR_BACKWARD_SLASH) ++toStart;
    return toOrig.slice(toStart, toEnd);
  };

  win32Path.parse = function parse(path) {
    validateString(path, 'path');

    const ret = { root: '', dir: '', base: '', ext: '', name: '' };
    if (path.length === 0) return ret;

    const len = path.length;
    let rootEnd = 0;
    let code = path.charCodeAt(0);

    if (len === 1) {
      if (isPathSeparator(code)) {
        ret.root = ret.dir = path;
        return ret;
      }
      ret.base = ret.name = path;
      return ret;
    }

    if (isPathSeparator(code)) {
      rootEnd = 1;
      if (isPathSeparator(path.charCodeAt(1))) {
        let j = 2;
        let last = j;
        while (j < len && !isPathSeparator(path.charCodeAt(j))) {
          j++;
        }
        if (j < len && j !== last) {
          last = j;
          while (j < len && isPathSeparator(path.charCodeAt(j))) {
            j++;
          }
          if (j < len && j !== last) {
            last = j;
            while (j < len && !isPathSeparator(path.charCodeAt(j))) {
              j++;
            }
            if (j === len) {
              rootEnd = j;
            } else if (j !== last) {
              rootEnd = j + 1;
            }
          }
        }
      }
    } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === CHAR_COLON) {
      if (len <= 2) {
        ret.root = ret.dir = path;
        return ret;
      }
      rootEnd = 2;
      if (isPathSeparator(path.charCodeAt(2))) {
        if (len === 3) {
          ret.root = ret.dir = path;
          return ret;
        }
        rootEnd = 3;
      }
    }

    if (rootEnd > 0) ret.root = path.slice(0, rootEnd);

    let startDot = -1;
    let startPart = rootEnd;
    let end = -1;
    let matchedSlash = true;
    let i = path.length - 1;
    let preDotState = 0;

    for (; i >= rootEnd; --i) {
      code = path.charCodeAt(i);
      if (isPathSeparator(code)) {
        if (!matchedSlash) {
          startPart = i + 1;
          break;
        }
        continue;
      }
      if (end === -1) {
        matchedSlash = false;
        end = i + 1;
      }
      if (code === CHAR_DOT) {
        if (startDot === -1) startDot = i;
        else if (preDotState !== 1) preDotState = 1;
      } else if (startDot !== -1) {
        preDotState = -1;
      }
    }

    if (end !== -1) {
      if (startDot === -1 || preDotState === 0 ||
          (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
        ret.base = ret.name = path.slice(startPart, end);
      } else {
        ret.name = path.slice(startPart, startDot);
        ret.base = path.slice(startPart, end);
        ret.ext = path.slice(startDot, end);
      }
    }

    if (startPart > 0 && startPart !== rootEnd) {
      ret.dir = path.slice(0, startPart - 1);
    } else {
      ret.dir = ret.root;
    }

    return ret;
  };

  win32Path.format = function format(pathObject) {
    if (pathObject === null || typeof pathObject !== 'object') {
      let received;
      if (pathObject == null) {
        received = `Received ${pathObject}`;
      } else {
        const inspected = JSON.stringify(pathObject);
        received = `Received type ${typeof pathObject} (${inspected})`;
      }
      const err = new TypeError(`The "pathObject" argument must be of type object. ${received}`);
      err.code = 'ERR_INVALID_ARG_TYPE';
      throw err;
    }
    const dir = pathObject.dir || pathObject.root;
    let base = pathObject.base;
    if (!base) {
      const name = pathObject.name || '';
      let ext = pathObject.ext || '';
      // Normalize ext to ensure it starts with a dot (if non-empty)
      if (ext && ext.charCodeAt(0) !== CHAR_DOT) {
        ext = `.${ext}`;
      }
      base = `${name}${ext}`;
    }
    if (!dir) return base;
    return dir === pathObject.root ? `${dir}${base}` : `${dir}\\${base}`;
  };

  win32Path.toNamespacedPath = function toNamespacedPath(path) {
    if (typeof path !== 'string' || path.length === 0) return path;

    const resolvedPath = win32Path.resolve(path);

    if (resolvedPath.length <= 2) return path;

    if (resolvedPath.charCodeAt(0) === CHAR_BACKWARD_SLASH) {
      if (resolvedPath.charCodeAt(1) === CHAR_BACKWARD_SLASH) {
        const code = resolvedPath.charCodeAt(2);
        if (code !== 63 && code !== 46) {
          return `\\\\?\\UNC\\${resolvedPath.slice(2)}`;
        }
      }
    } else if (isWindowsDeviceRoot(resolvedPath.charCodeAt(0)) &&
               resolvedPath.charCodeAt(1) === CHAR_COLON &&
               resolvedPath.charCodeAt(2) === CHAR_BACKWARD_SLASH) {
      return `\\\\?\\${resolvedPath}`;
    }

    return path;
  };

  // Create the main path module (platform-specific)
  // Note: On POSIX, path === path.posix; on Windows, path === path.win32
  // We add the cross-platform references to the objects themselves
  posixPath.posix = posixPath;
  posixPath.win32 = win32Path;
  win32Path.posix = posixPath;
  win32Path.win32 = win32Path;

  const pathModule = isWindows ? win32Path : posixPath;

  // Register the path module
  globalThis.__howth_modules = globalThis.__howth_modules || {};

  // Lazy module registration system.
  // Factories are called on first access and the result is cached.
  const __lazy_factories = {};
  function __registerLazy(names, factory) {
    for (const name of names) {
      __lazy_factories[name] = factory;
    }
  }
  // __howth_modules is a Proxy that triggers lazy init on first access.
  const __eager_modules = {};
  globalThis.__howth_modules = new Proxy(__eager_modules, {
    get(target, prop) {
      if (prop in target) return target[prop];
      const factory = __lazy_factories[prop];
      if (factory) {
        const mod = factory();
        // The factory registers all its aliases into __eager_modules directly.
        // If it didn't set this key, set it now.
        if (!(prop in target)) target[prop] = mod;
        return target[prop];
      }
      return undefined;
    },
    has(target, prop) {
      return (prop in target) || (prop in __lazy_factories);
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
  });

  // Core modules (eager — needed by everything)
  globalThis.__howth_modules["node:process"] = globalThis.process;
  globalThis.__howth_modules["process"] = globalThis.process;

  const bufferModule = {
    Buffer: globalThis.Buffer,
    kMaxLength: 2147483647,
    kStringMaxLength: 536870888,
    constants: {
      MAX_LENGTH: 2147483647,
      MAX_STRING_LENGTH: 536870888,
    },
    SlowBuffer: globalThis.Buffer,
    transcode: () => { throw new Error('transcode not implemented'); },
    isEncoding(enc) {
      return ['utf8', 'utf-8', 'ascii', 'latin1', 'binary', 'base64', 'hex', 'ucs2', 'ucs-2', 'utf16le', 'utf-16le'].includes(String(enc).toLowerCase());
    },
    isBuffer(obj) {
      return obj instanceof globalThis.Buffer;
    },
  };
  globalThis.__howth_modules["node:buffer"] = bufferModule;
  globalThis.__howth_modules["buffer"] = bufferModule;

  globalThis.__howth_modules["node:path"] = pathModule;
  globalThis.__howth_modules["path"] = pathModule;
  globalThis.__howth_modules["node:path/posix"] = posixPath;
  globalThis.__howth_modules["path/posix"] = posixPath;
  globalThis.__howth_modules["node:path/win32"] = win32Path;
  globalThis.__howth_modules["path/win32"] = win32Path;
  // ============================================
  // node:fs module (lazy)
  // ============================================
  __registerLazy(["node:fs", "fs", "node:fs/promises", "fs/promises"], () => {

  // Stats class to mimic Node.js fs.Stats
  class Stats {
    constructor(stat) {
      this.dev = stat.dev;
      this.ino = stat.ino;
      this.mode = stat.mode;
      this.nlink = stat.nlink;
      this.uid = stat.uid;
      this.gid = stat.gid;
      this.rdev = 0;
      this.size = stat.size;
      this.blksize = 4096;
      this.blocks = Math.ceil(stat.size / 512);
      this.atimeMs = stat.atime_ms;
      this.mtimeMs = stat.mtime_ms;
      this.ctimeMs = stat.ctime_ms;
      this.birthtimeMs = stat.birthtime_ms;
      this.atime = new Date(stat.atime_ms);
      this.mtime = new Date(stat.mtime_ms);
      this.ctime = new Date(stat.ctime_ms);
      this.birthtime = new Date(stat.birthtime_ms);
      this._isFile = stat.is_file;
      this._isDirectory = stat.is_directory;
      this._isSymlink = stat.is_symlink;
    }

    isFile() { return this._isFile; }
    isDirectory() { return this._isDirectory; }
    isSymbolicLink() { return this._isSymlink; }
    isBlockDevice() { return false; }
    isCharacterDevice() { return false; }
    isFIFO() { return false; }
    isSocket() { return false; }
  }

  // Dirent class for readdir with withFileTypes
  class Dirent {
    constructor(entry, parentPath) {
      this.name = entry.name;
      this.parentPath = parentPath;
      this.path = parentPath; // Node 20+ compatibility
      this._isFile = entry.is_file;
      this._isDirectory = entry.is_directory;
      this._isSymlink = entry.is_symlink;
    }

    isFile() { return this._isFile; }
    isDirectory() { return this._isDirectory; }
    isSymbolicLink() { return this._isSymlink; }
    isBlockDevice() { return false; }
    isCharacterDevice() { return false; }
    isFIFO() { return false; }
    isSocket() { return false; }
  }

  // Helper to normalize encoding option
  function normalizeEncoding(options) {
    if (typeof options === "string") return options;
    if (options && options.encoding) return options.encoding;
    return null;
  }

  // Normalize fs errors to have Node.js-compatible properties
  function normalizeFsError(err, syscall, path) {
    if (err && err.message) {
      // Extract error code from message format "CODE: message, syscall 'path'"
      const match = err.message.match(/^([A-Z]+):/);
      if (match) {
        err.code = match[1];
        err.syscall = syscall;
        err.path = path;
        // Set errno based on code
        const errnoMap = {
          ENOENT: -2,
          EACCES: -13,
          EEXIST: -17,
          ENOTDIR: -20,
          EISDIR: -21,
          EINVAL: -22,
          ENOTEMPTY: -39,
          ELOOP: -40,
        };
        err.errno = errnoMap[err.code] || -1;
      }
    }
    return err;
  }

  // Validate path argument for fs functions
  function validatePath(path, name = "path") {
    if (typeof path !== "string" && !Buffer.isBuffer(path)) {
      const err = new TypeError(`The "${name}" argument must be of type string or an instance of Buffer or URL. Received ${path === null ? "null" : typeof path}`);
      err.code = "ERR_INVALID_ARG_TYPE";
      throw err;
    }
  }

  // Maximum file size that can be read (2GB - 1)
  const kIoMaxLength = 2 ** 31 - 1;

  // Synchronous file system functions
  const fsSync = {
    readFileSync(path, options) {
      try {
        const pathStr = String(path);
        // Check file size before reading
        const stat = ops.op_howth_fs_stat(pathStr, true);
        if (stat.size > kIoMaxLength) {
          const err = new RangeError(
            `File size (${stat.size}) is greater than 2 GiB`
          );
          err.code = "ERR_FS_FILE_TOO_LARGE";
          throw err;
        }
        const encoding = normalizeEncoding(options);
        if (encoding === "utf8" || encoding === "utf-8") {
          return ops.op_howth_read_file(pathStr);
        }
        // Return Buffer for binary reads
        const base64 = ops.op_howth_fs_read_bytes(pathStr);
        const buf = Buffer.from(base64, "base64");
        if (encoding) {
          return buf.toString(encoding);
        }
        return buf;
      } catch (e) {
        // Don't wrap ERR_FS_FILE_TOO_LARGE errors
        if (e.code === "ERR_FS_FILE_TOO_LARGE") {
          throw e;
        }
        throw normalizeFsError(e, "open", String(path));
      }
    },

    writeFileSync(path, data, options) {
      const p = String(path);
      try {
        if (typeof data === "string") {
          ops.op_howth_write_file(p, data);
        } else if (data instanceof Uint8Array || Buffer.isBuffer(data)) {
          const base64 = Buffer.from(data).toString("base64");
          ops.op_howth_fs_write_bytes(p, base64);
        } else {
          ops.op_howth_write_file(p, String(data));
        }
      } catch (e) {
        throw normalizeFsError(e, 'open', p);
      }
    },

    appendFileSync(path, data, options) {
      if (typeof data === "string") {
        ops.op_howth_fs_append(String(path), data);
      } else {
        // For binary data, read existing, concat, and write
        const existing = this.existsSync(path) ? this.readFileSync(path) : Buffer.alloc(0);
        const newData = Buffer.concat([existing, Buffer.from(data)]);
        this.writeFileSync(path, newData);
      }
    },

    existsSync(path) {
      return ops.op_howth_fs_exists(String(path));
    },

    mkdirSync(path, options) {
      const p = String(path);
      const recursive = typeof options === 'object' ? (options?.recursive || false) : false;
      try {
        ops.op_howth_fs_mkdir(p, recursive);
        // Node.js returns the first created directory path when recursive is true
        if (recursive) return p;
        return undefined;
      } catch (e) {
        throw normalizeFsError(e, 'mkdir', p);
      }
    },

    rmdirSync(path, options) {
      const p = String(path);
      const recursive = options?.recursive || false;
      try {
        ops.op_howth_fs_rmdir(p, recursive);
      } catch (e) {
        throw normalizeFsError(e, 'rmdir', p);
      }
    },

    rmSync(path, options) {
      const recursive = options?.recursive || false;
      const force = options?.force || false;

      try {
        const stat = ops.op_howth_fs_stat(String(path), true);
        if (stat.is_directory) {
          ops.op_howth_fs_rmdir(String(path), recursive);
        } else {
          ops.op_howth_fs_unlink(String(path));
        }
      } catch (e) {
        if (!force) throw e;
      }
    },

    unlinkSync(path) {
      const p = String(path);
      try { ops.op_howth_fs_unlink(p); } catch (e) { throw normalizeFsError(e, 'unlink', p); }
    },

    renameSync(oldPath, newPath) {
      try { ops.op_howth_fs_rename(String(oldPath), String(newPath)); } catch (e) { throw normalizeFsError(e, 'rename', String(oldPath)); }
    },

    copyFileSync(src, dest, mode) {
      try { ops.op_howth_fs_copy(String(src), String(dest)); } catch (e) { throw normalizeFsError(e, 'copyfile', String(src)); }
    },

    readdirSync(path, options) {
      validatePath(path);
      try {
        const entries = ops.op_howth_fs_readdir(String(path));
        const withFileTypes = options?.withFileTypes || false;
        const encoding = normalizeEncoding(options) || "utf8";

        if (withFileTypes) {
          return entries.map(e => new Dirent(e, String(path)));
        }

        return entries.map(e => e.name);
      } catch (e) {
        throw normalizeFsError(e, "scandir", String(path));
      }
    },

    statSync(path, options) {
      const throwIfNoEntry = options?.throwIfNoEntry !== false;
      try {
        const stat = ops.op_howth_fs_stat(String(path), true);
        return new Stats(stat);
      } catch (e) {
        if (!throwIfNoEntry) return undefined;
        throw normalizeFsError(e, "stat", String(path));
      }
    },

    lstatSync(path, options) {
      const throwIfNoEntry = options?.throwIfNoEntry !== false;
      try {
        const stat = ops.op_howth_fs_stat(String(path), false);
        return new Stats(stat);
      } catch (e) {
        if (!throwIfNoEntry) return undefined;
        throw normalizeFsError(e, "lstat", String(path));
      }
    },

    realpathSync: Object.assign(
      function realpathSync(path, options) {
        return ops.op_howth_fs_realpath(String(path));
      },
      {
        // native is the same as realpathSync on Unix
        native(path, options) {
          return ops.op_howth_fs_realpath(String(path));
        }
      }
    ),

    readlinkSync(path, options) {
      return ops.op_howth_fs_readlink(String(path));
    },

    symlinkSync(target, path, type) {
      // type can be 'file', 'dir', or 'junction' (Windows only)
      ops.op_howth_fs_symlink(String(target), String(path), type || 'file');
    },

    chownSync(path, uid, gid) {
      ops.op_howth_fs_chown(String(path), uid, gid);
    },

    lchownSync(path, uid, gid) {
      // For symlinks - same as chown on most systems
      ops.op_howth_fs_chown(String(path), uid, gid);
    },

    chmodSync(path, mode) {
      ops.op_howth_fs_chmod(String(path), mode);
    },

    accessSync(path, mode) {
      // mode: F_OK=0, R_OK=4, W_OK=2, X_OK=1
      const m = mode === undefined ? 0 : mode;
      ops.op_howth_fs_access(String(path), m);
    },

    // File descriptor-based operations using Rust ops
    // Track fd to path mapping for fstatSync (Rust doesn't have fstat_fd yet)
    _fdPaths: new Map(),

    openSync(path, flags, mode) {
      const strPath = String(path);
      // Convert flags to string format if numeric
      let flagStr = typeof flags === 'string' ? flags : 'r';
      if (typeof flags === 'number') {
        // Convert numeric flags to string flags
        // O_RDONLY=0, O_WRONLY=1, O_RDWR=2, O_CREAT=64, O_TRUNC=512, O_APPEND=1024
        const O_WRONLY = 1, O_RDWR = 2, O_CREAT = 64, O_TRUNC = 512, O_APPEND = 1024;
        if ((flags & O_RDWR) && (flags & O_CREAT) && (flags & O_TRUNC)) {
          flagStr = 'w+';
        } else if ((flags & O_WRONLY) && (flags & O_CREAT) && (flags & O_TRUNC)) {
          flagStr = 'w';
        } else if ((flags & O_WRONLY) && (flags & O_CREAT) && (flags & O_APPEND)) {
          flagStr = 'a';
        } else if ((flags & O_RDWR) && (flags & O_CREAT) && (flags & O_APPEND)) {
          flagStr = 'a+';
        } else if (flags & O_RDWR) {
          flagStr = 'r+';
        } else {
          flagStr = 'r';
        }
      }
      const m = mode || 0o666;
      const result = ops.op_howth_fs_open_fd(strPath, flagStr, m);
      if (result.error) {
        const err = new Error(result.error);
        err.code = 'ENOENT';
        err.syscall = 'open';
        err.path = strPath;
        throw err;
      }
      // Track fd to path for fstatSync
      this._fdPaths.set(result.fd, strPath);
      return result.fd;
    },

    closeSync(fd) {
      ops.op_howth_fs_close_fd(fd);
      this._fdPaths.delete(fd);
    },

    readSync(fd, buffer, offset, length, position) {
      // Handle overloaded signatures
      if (buffer instanceof Buffer || ArrayBuffer.isView(buffer)) {
        offset = offset || 0;
        length = length || buffer.length - offset;
      } else {
        // options object style
        const options = buffer || {};
        buffer = options.buffer || Buffer.alloc(16384);
        offset = options.offset || 0;
        length = options.length || buffer.length;
        position = options.position;
      }

      // Seek to position if specified
      if (position !== null && position !== undefined && position >= 0) {
        ops.op_howth_fs_seek_fd(fd, position, 0); // SEEK_SET = 0
      }

      const result = ops.op_howth_fs_read_fd(fd, length);
      if (result === null) {
        return 0; // EOF
      }

      // Copy data to buffer
      const data = new Uint8Array(result);
      for (let i = 0; i < data.length && i + offset < buffer.length; i++) {
        buffer[offset + i] = data[i];
      }
      return data.length;
    },

    writeSync(fd, buffer, offset, length, position) {
      let data;
      if (typeof buffer === 'string') {
        data = new TextEncoder().encode(buffer);
        if (position !== null && position !== undefined && position >= 0) {
          ops.op_howth_fs_seek_fd(fd, position, 0);
        }
      } else {
        offset = offset || 0;
        length = length || buffer.length - offset;
        data = buffer.slice(offset, offset + length);
        if (position !== null && position !== undefined && position >= 0) {
          ops.op_howth_fs_seek_fd(fd, position, 0);
        }
      }
      return ops.op_howth_fs_write_fd(fd, Array.from(data));
    },

    fstatSync(fd, options) {
      // Use the tracked fd to path mapping
      if (!this._fdPaths.has(fd)) {
        const err = new Error(`EBADF: bad file descriptor, fstat`);
        err.code = 'EBADF';
        err.syscall = 'fstat';
        throw err;
      }
      const path = this._fdPaths.get(fd);
      return this.statSync(path, options);
    },

    truncateSync(path, len) {
      ops.op_howth_fs_truncate(String(path), BigInt(len || 0));
    },

    ftruncateSync(fd, len) {
      if (!globalThis.__howth_fd_map || !globalThis.__howth_fd_map.has(fd)) {
        const err = new Error(`EBADF: bad file descriptor, ftruncate`);
        err.code = 'EBADF';
        err.syscall = 'ftruncate';
        throw err;
      }
      const info = globalThis.__howth_fd_map.get(fd);
      fsSync.truncateSync(info.path, len);
    },
  };

  // Promise-based file system functions
  const fsPromises = {
    async readFile(path, options) {
      return fsSync.readFileSync(path, options);
    },

    async writeFile(path, data, options) {
      return fsSync.writeFileSync(path, data, options);
    },

    async appendFile(path, data, options) {
      return fsSync.appendFileSync(path, data, options);
    },

    async mkdir(path, options) {
      return fsSync.mkdirSync(path, options);
    },

    async rmdir(path, options) {
      return fsSync.rmdirSync(path, options);
    },

    async rm(path, options) {
      return fsSync.rmSync(path, options);
    },

    async unlink(path) {
      return fsSync.unlinkSync(path);
    },

    async rename(oldPath, newPath) {
      return fsSync.renameSync(oldPath, newPath);
    },

    async copyFile(src, dest, mode) {
      return fsSync.copyFileSync(src, dest, mode);
    },

    async readdir(path, options) {
      return fsSync.readdirSync(path, options);
    },

    async stat(path, options) {
      return fsSync.statSync(path, options);
    },

    async lstat(path, options) {
      return fsSync.lstatSync(path, options);
    },

    async realpath(path, options) {
      return fsSync.realpathSync(path, options);
    },

    async readlink(path, options) {
      return fsSync.readlinkSync(path, options);
    },

    async symlink(target, path, type) {
      return fsSync.symlinkSync(target, path, type);
    },

    async chown(path, uid, gid) {
      return fsSync.chownSync(path, uid, gid);
    },

    async lchown(path, uid, gid) {
      return fsSync.lchownSync(path, uid, gid);
    },

    async chmod(path, mode) {
      return fsSync.chmodSync(path, mode);
    },

    async access(path, mode) {
      return fsSync.accessSync(path, mode);
    },
  };

  // Constants
  const fsConstants = {
    F_OK: 0,
    R_OK: 4,
    W_OK: 2,
    X_OK: 1,
    COPYFILE_EXCL: 1,
    COPYFILE_FICLONE: 2,
    COPYFILE_FICLONE_FORCE: 4,
    O_RDONLY: 0,
    O_WRONLY: 1,
    O_RDWR: 2,
    O_CREAT: 64,
    O_EXCL: 128,
    O_TRUNC: 512,
    O_APPEND: 1024,
  };

  // Build the fs module
  const fsModule = {
    // Sync methods
    ...fsSync,

    // Promises API
    promises: fsPromises,

    // Constants
    constants: fsConstants,
    ...fsConstants,

    // Classes
    Stats,
    Dirent,

    // Callback-based methods (wrap sync with nextTick for compatibility)
    readFile(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      // Handle AbortSignal
      const signal = options?.signal;
      if (signal !== undefined) {
        // Validate signal type
        if (
          typeof signal !== "object" ||
          signal === null ||
          typeof signal.aborted !== "boolean"
        ) {
          const err = new TypeError(
            'The "options.signal" property must be an instance of AbortSignal. ' +
              `Received ${signal === null ? "null" : typeof signal}`
          );
          err.code = "ERR_INVALID_ARG_TYPE";
          throw err;
        }
        // Check if already aborted
        if (signal.aborted) {
          const err = new Error("The operation was aborted");
          err.name = "AbortError";
          err.code = "ABORT_ERR";
          queueMicrotask(() => callback(err));
          return;
        }
      }
      try {
        const result = fsSync.readFileSync(path, options);
        // Schedule another nextTick after the test's nextTick to check abort status
        process.nextTick(() => {
          process.nextTick(() => {
            if (signal?.aborted) {
              const err = new Error("The operation was aborted");
              err.name = "AbortError";
              err.code = "ABORT_ERR";
              callback(err);
              return;
            }
            callback(null, result);
          });
        });
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    writeFile(path, data, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        fsSync.writeFileSync(path, data, options);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    appendFile(path, data, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        fsSync.appendFileSync(path, data, options);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    mkdir(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        fsSync.mkdirSync(path, options);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    rmdir(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        fsSync.rmdirSync(path, options);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    rm(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        fsSync.rmSync(path, options);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    unlink(path, callback) {
      try {
        fsSync.unlinkSync(path);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    rename(oldPath, newPath, callback) {
      try {
        fsSync.renameSync(oldPath, newPath);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    copyFile(src, dest, mode, callback) {
      if (typeof mode === "function") {
        callback = mode;
        mode = 0;
      }
      try {
        fsSync.copyFileSync(src, dest, mode);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    readdir(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      // Validate path synchronously (throws ERR_INVALID_ARG_TYPE before calling callback)
      validatePath(path);
      try {
        const result = fsSync.readdirSync(path, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    stat(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        const result = fsSync.statSync(path, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    lstat(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        const result = fsSync.lstatSync(path, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    realpath(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        const result = fsSync.realpathSync(path, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    readlink(path, options, callback) {
      if (typeof options === "function") {
        callback = options;
        options = undefined;
      }
      try {
        const result = fsSync.readlinkSync(path, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    symlink(target, path, type, callback) {
      if (typeof type === "function") {
        callback = type;
        type = undefined;
      }
      try {
        fsSync.symlinkSync(target, path, type);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    chown(path, uid, gid, callback) {
      try {
        fsSync.chownSync(path, uid, gid);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    lchown(path, uid, gid, callback) {
      try {
        fsSync.lchownSync(path, uid, gid);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    chmod(path, mode, callback) {
      try {
        fsSync.chmodSync(path, mode);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    access(path, mode, callback) {
      if (typeof mode === "function") {
        callback = mode;
        mode = 0;
      }
      try {
        fsSync.accessSync(path, mode);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    exists(path, callback) {
      // Node.js throws if callback is not a function
      if (typeof callback !== 'function') {
        const err = new TypeError('The "callback" argument must be of type function. Received ' + typeof callback);
        err.code = 'ERR_INVALID_ARG_TYPE';
        throw err;
      }
      // For invalid path types, call callback with false instead of throwing
      if (typeof path !== 'string' && !(path instanceof URL)) {
        queueMicrotask(() => callback(false));
        return;
      }
      try {
        const exists = fsSync.existsSync(path);
        queueMicrotask(() => callback(exists));
      } catch (e) {
        queueMicrotask(() => callback(false));
      }
    },

    open(path, flags, mode, callback) {
      if (typeof flags === 'function') {
        callback = flags;
        flags = 'r';
        mode = 0o666;
      } else if (typeof mode === 'function') {
        callback = mode;
        mode = 0o666;
      }
      try {
        const fd = fsSync.openSync(path, flags, mode);
        queueMicrotask(() => callback(null, fd));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    close(fd, callback) {
      if (typeof callback !== 'function') {
        callback = () => {};
      }
      try {
        fsSync.closeSync(fd);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    read(fd, buffer, offset, length, position, callback) {
      // Handle overloaded signatures
      if (typeof offset === 'function') {
        callback = offset;
        offset = 0;
        length = buffer.length;
        position = null;
      } else if (typeof length === 'function') {
        callback = length;
        length = buffer.length - offset;
        position = null;
      } else if (typeof position === 'function') {
        callback = position;
        position = null;
      }

      try {
        const bytesRead = fsSync.readSync(fd, buffer, offset, length, position);
        queueMicrotask(() => callback(null, bytesRead, buffer));
      } catch (e) {
        queueMicrotask(() => callback(e, 0, buffer));
      }
    },

    write(fd, buffer, offset, length, position, callback) {
      // Handle overloaded signatures
      if (typeof buffer === 'string') {
        // fs.write(fd, string, position, encoding, callback)
        const str = buffer;
        position = offset;
        const encoding = length || 'utf8';
        callback = position || (() => {});
        if (typeof position === 'function') {
          callback = position;
          position = null;
        }
        try {
          const bytesWritten = fsSync.writeSync(fd, str, null, null, position);
          queueMicrotask(() => callback(null, bytesWritten, str));
        } catch (e) {
          queueMicrotask(() => callback(e, 0, str));
        }
      } else {
        // fs.write(fd, buffer, offset, length, position, callback)
        if (typeof offset === 'function') {
          callback = offset;
          offset = 0;
          length = buffer.length;
          position = null;
        } else if (typeof length === 'function') {
          callback = length;
          length = buffer.length - offset;
          position = null;
        } else if (typeof position === 'function') {
          callback = position;
          position = null;
        }
        try {
          const bytesWritten = fsSync.writeSync(fd, buffer, offset, length, position);
          queueMicrotask(() => callback(null, bytesWritten, buffer));
        } catch (e) {
          queueMicrotask(() => callback(e, 0, buffer));
        }
      }
    },

    fstat(fd, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = undefined;
      }
      // Validate fd - must be a number
      if (typeof fd !== 'number' || Number.isNaN(fd)) {
        const err = new TypeError(`The "fd" argument must be of type number. Received ${fd === null ? 'null' : typeof fd}`);
        err.code = 'ERR_INVALID_ARG_TYPE';
        throw err;
      }
      // If no callback, just call sync and let it throw
      if (typeof callback !== 'function') {
        return fsSync.fstatSync(fd, options);
      }
      try {
        const result = fsSync.fstatSync(fd, options);
        queueMicrotask(() => callback(null, result));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    truncate(path, len, callback) {
      if (typeof len === 'function') {
        callback = len;
        len = 0;
      }
      try {
        fsSync.truncateSync(path, len);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    ftruncate(fd, len, callback) {
      if (typeof len === 'function') {
        callback = len;
        len = 0;
      }
      try {
        fsSync.ftruncateSync(fd, len);
        queueMicrotask(() => callback(null));
      } catch (e) {
        queueMicrotask(() => callback(e));
      }
    },

    // createReadStream - returns a Readable stream using file descriptors
    createReadStream(path, options = {}) {
      const listeners = new Map();
      let destroyed = false;
      let started = false;
      let fd = null;
      let paused = false;

      const highWaterMark = options.highWaterMark || 64 * 1024;
      const encoding = options.encoding;
      const start = options.start || 0;
      const end = options.end;
      const flags = options.flags || 'r';
      const mode = options.mode || 0o666;

      const stream = {
        readable: true,
        path,
        bytesRead: 0,
        fd: null,

        on(event, listener) {
          if (!listeners.has(event)) {
            listeners.set(event, []);
          }
          listeners.get(event).push(listener);

          // Auto-start reading when 'data' listener is added
          if (event === 'data' && !started && !destroyed) {
            started = true;
            openAndRead();
          }
          return this;
        },

        once(event, listener) {
          const wrapper = (...args) => {
            this.off(event, wrapper);
            listener(...args);
          };
          return this.on(event, wrapper);
        },

        off(event, listener) {
          const arr = listeners.get(event);
          if (arr) {
            const idx = arr.indexOf(listener);
            if (idx !== -1) arr.splice(idx, 1);
          }
          return this;
        },

        removeListener(event, listener) {
          return this.off(event, listener);
        },

        emit(event, ...args) {
          const arr = listeners.get(event);
          if (arr) {
            for (const fn of [...arr]) {
              fn(...args);
            }
          }
        },

        pipe(dest) {
          this.on('data', (chunk) => dest.write(chunk));
          this.on('end', () => dest.end && dest.end());
          this.on('error', (err) => dest.emit && dest.emit('error', err));
          return dest;
        },

        destroy(err) {
          if (destroyed) return;
          destroyed = true;
          if (fd !== null) {
            try { ops.op_howth_fs_close_fd(fd); } catch (e) {}
            fd = null;
          }
          if (err) this.emit('error', err);
          this.emit('close');
        },

        setEncoding(enc) {
          return this;
        },

        pause() {
          paused = true;
          return this;
        },

        resume() {
          if (paused) {
            paused = false;
            if (fd !== null) readLoop();
          } else if (!started && !destroyed) {
            started = true;
            openAndRead();
          }
          return this;
        },
      };

      function openAndRead() {
        process.nextTick(() => {
          if (destroyed) return;

          try {
            const result = ops.op_howth_fs_open_fd(path, flags, mode);
            if (result.error) {
              const err = new Error(result.error);
              err.code = 'ENOENT';
              stream.emit('error', err);
              stream.emit('close');
              return;
            }

            fd = result.fd;
            stream.fd = fd;

            // Seek to start position if specified
            if (start > 0) {
              ops.op_howth_fs_seek_fd(fd, start, 0);
            }

            stream.emit('open', fd);
            stream.emit('ready');
            readLoop();
          } catch (e) {
            stream.emit('error', e);
            stream.emit('close');
          }
        });
      }

      function readLoop() {
        if (destroyed || paused || fd === null) return;

        // Calculate how much to read
        let toRead = highWaterMark;
        if (end !== undefined) {
          const remaining = end - start - stream.bytesRead + 1;
          if (remaining <= 0) {
            finishStream();
            return;
          }
          toRead = Math.min(toRead, remaining);
        }

        const data = ops.op_howth_fs_read_fd(fd, toRead);

        if (data === null || data === undefined || data.length === 0) {
          finishStream();
          return;
        }

        stream.bytesRead += data.length;
        let chunk = Buffer.from(data);

        if (encoding) {
          chunk = chunk.toString(encoding);
        }

        stream.emit('data', chunk);

        // Schedule next read
        process.nextTick(readLoop);
      }

      function finishStream() {
        if (fd !== null) {
          try { ops.op_howth_fs_close_fd(fd); } catch (e) {}
          fd = null;
        }
        stream.emit('end');
        stream.emit('close');
      }

      return stream;
    },

    // createWriteStream - returns a Writable stream using file descriptors
    createWriteStream(path, options = {}) {
      const listeners = new Map();
      let destroyed = false;
      let finished = false;
      let fd = null;
      let opened = false;

      const flags = options.flags || 'w';
      const mode = options.mode || 0o666;
      const encoding = options.encoding || 'utf8';
      const start = options.start;

      const stream = {
        writable: true,
        path,
        bytesWritten: 0,
        fd: null,

        on(event, listener) {
          if (!listeners.has(event)) {
            listeners.set(event, []);
          }
          listeners.get(event).push(listener);
          return this;
        },

        once(event, listener) {
          const wrapper = (...args) => {
            this.off(event, wrapper);
            listener(...args);
          };
          return this.on(event, wrapper);
        },

        off(event, listener) {
          const arr = listeners.get(event);
          if (arr) {
            const idx = arr.indexOf(listener);
            if (idx !== -1) arr.splice(idx, 1);
          }
          return this;
        },

        removeListener(event, listener) {
          return this.off(event, listener);
        },

        emit(event, ...args) {
          const arr = listeners.get(event);
          if (arr) {
            for (const fn of [...arr]) {
              fn(...args);
            }
          }
        },

        write(chunk, enc, callback) {
          if (typeof enc === 'function') {
            callback = enc;
            enc = undefined;
          }
          if (destroyed || finished) {
            const err = new Error('write after end');
            if (callback) callback(err);
            return false;
          }

          // Ensure file is open
          if (!opened) {
            openFile();
          }

          let buf;
          if (typeof chunk === 'string') {
            buf = Buffer.from(chunk, enc || encoding);
          } else if (chunk instanceof Buffer) {
            buf = chunk;
          } else {
            buf = Buffer.from(chunk);
          }

          // Write to file descriptor
          try {
            if (fd !== null) {
              const written = ops.op_howth_fs_write_fd(fd, [...buf]);
              stream.bytesWritten += written;
            }
            if (callback) process.nextTick(callback);
            stream.emit('drain');
            return true;
          } catch (e) {
            if (callback) callback(e);
            stream.emit('error', e);
            return false;
          }
        },

        end(chunk, enc, callback) {
          if (typeof chunk === 'function') {
            callback = chunk;
            chunk = undefined;
          } else if (typeof enc === 'function') {
            callback = enc;
            enc = undefined;
          }

          if (chunk) {
            this.write(chunk, enc);
          }

          finished = true;

          process.nextTick(() => {
            if (fd !== null) {
              try { ops.op_howth_fs_close_fd(fd); } catch (e) {}
              fd = null;
            }
            stream.emit('finish');
            stream.emit('close');
            if (callback) callback();
          });
        },

        destroy(err) {
          if (destroyed) return;
          destroyed = true;
          if (fd !== null) {
            try { ops.op_howth_fs_close_fd(fd); } catch (e) {}
            fd = null;
          }
          if (err) this.emit('error', err);
          this.emit('close');
        },

        cork() {},
        uncork() {},
      };

      function openFile() {
        if (opened) return;
        opened = true;

        const result = ops.op_howth_fs_open_fd(path, flags, mode);
        if (result.error) {
          const err = new Error(result.error);
          err.code = 'ENOENT';
          process.nextTick(() => {
            stream.emit('error', err);
            stream.emit('close');
          });
          return;
        }

        fd = result.fd;
        stream.fd = fd;

        // Seek to start position if specified
        if (start !== undefined) {
          ops.op_howth_fs_seek_fd(fd, start, 0);
        }

        process.nextTick(() => {
          stream.emit('open', fd);
          stream.emit('ready');
        });
      }

      // Open file immediately
      process.nextTick(openFile);

      return stream;
    },

    // fs.watch - placeholder, will be patched after fsModule is defined
    watch: null,

    // fs.watchFile - Watch for changes using polling
    watchFile(filename, options = {}, listener) {
      if (typeof options === 'function') {
        listener = options;
        options = {};
      }

      const resolvedPath = posixPath.resolve(filename);
      const interval = options.interval || 5007;
      const persistent = options.persistent !== false;

      let prevStat = null;
      let timerId = null;
      let stopped = false;

      // Get initial stat
      try {
        prevStat = fsModule.statSync(resolvedPath);
      } catch (e) {
        prevStat = {
          dev: 0, ino: 0, mode: 0, nlink: 0, uid: 0, gid: 0,
          rdev: 0, size: 0, blksize: 0, blocks: 0,
          atimeMs: 0, mtimeMs: 0, ctimeMs: 0, birthtimeMs: 0,
          isFile: () => false, isDirectory: () => false, isSymbolicLink: () => false,
        };
      }

      function checkForChanges() {
        if (stopped) return;

        let currStat;
        try {
          currStat = fsModule.statSync(resolvedPath);
        } catch (e) {
          currStat = {
            dev: 0, ino: 0, mode: 0, nlink: 0, uid: 0, gid: 0,
            rdev: 0, size: 0, blksize: 0, blocks: 0,
            atimeMs: 0, mtimeMs: 0, ctimeMs: 0, birthtimeMs: 0,
            isFile: () => false, isDirectory: () => false, isSymbolicLink: () => false,
          };
        }

        // Check if file changed
        if (currStat.mtimeMs !== prevStat.mtimeMs || currStat.size !== prevStat.size) {
          if (listener) {
            listener(currStat, prevStat);
          }
          prevStat = currStat;
        }

        timerId = setTimeout(checkForChanges, interval);
      }

      // Start polling
      timerId = setTimeout(checkForChanges, interval);

      // Store watcher for unwatchFile
      if (!fsModule._watchedFiles) {
        fsModule._watchedFiles = new Map();
      }

      const watcherInfo = { timerId, listener, stop: () => { stopped = true; clearTimeout(timerId); } };

      if (!fsModule._watchedFiles.has(resolvedPath)) {
        fsModule._watchedFiles.set(resolvedPath, []);
      }
      fsModule._watchedFiles.get(resolvedPath).push(watcherInfo);

      return { filename: resolvedPath };
    },

    // fs.unwatchFile - Stop watching a file
    unwatchFile(filename, listener) {
      const resolvedPath = posixPath.resolve(filename);

      if (!fsModule._watchedFiles || !fsModule._watchedFiles.has(resolvedPath)) {
        return;
      }

      const watchers = fsModule._watchedFiles.get(resolvedPath);

      if (listener) {
        // Remove specific listener
        const idx = watchers.findIndex(w => w.listener === listener);
        if (idx !== -1) {
          watchers[idx].stop();
          watchers.splice(idx, 1);
        }
      } else {
        // Remove all listeners for this file
        watchers.forEach(w => w.stop());
        fsModule._watchedFiles.delete(resolvedPath);
      }
    },

    // FSWatcher constructor - will be set after EventEmitter is defined
    FSWatcher: null,
  };

  // Create a simple event emitter mixin for fs.watch before full EventEmitter is available
  function createSimpleEmitter() {
    const listeners = new Map();
    return {
      on(event, fn) {
        if (!listeners.has(event)) listeners.set(event, []);
        listeners.get(event).push(fn);
        return this;
      },
      addListener(event, fn) { return this.on(event, fn); },
      once(event, fn) {
        const wrapper = (...args) => { this.off(event, wrapper); fn.apply(this, args); };
        return this.on(event, wrapper);
      },
      off(event, fn) {
        const list = listeners.get(event);
        if (list) {
          const idx = list.indexOf(fn);
          if (idx !== -1) list.splice(idx, 1);
        }
        return this;
      },
      removeListener(event, fn) { return this.off(event, fn); },
      emit(event, ...args) {
        const list = listeners.get(event);
        if (!list || list.length === 0) return false;
        for (const fn of [...list]) fn.apply(this, args);
        return true;
      },
      listeners(event) { return listeners.get(event) || []; },
      listenerCount(event) { const l = listeners.get(event); return l ? l.length : 0; },
    };
  }

  // Patch fs.watch to use simple emitter since EventEmitter isn't defined yet
  const originalWatch = fsModule.watch;
  fsModule.watch = function(filename, options, listener) {
    if (typeof options === 'function') {
      listener = options;
      options = {};
    }

    const resolvedPath = posixPath.resolve(filename);
    const recursive = options && options.recursive === true;

    // Create watcher with simple emitter
    const watcher = createSimpleEmitter();

    let watchId = null;
    let closed = false;
    let polling = false;

    // Start the native watcher
    const result = ops.op_howth_fs_watch_start(resolvedPath, recursive);
    if (result.error) {
      process.nextTick(() => {
        watcher.emit('error', new Error(result.error));
      });
      return watcher;
    }

    watchId = result.id;

    // Poll for events
    function pollEvents() {
      if (closed || polling) return;
      polling = true;

      const event = ops.op_howth_fs_watch_poll(watchId);
      if (event) {
        const eventType = event.event_type;
        const eventFilename = event.filename || null;

        watcher.emit('change', eventType, eventFilename);
        if (listener) {
          listener(eventType, eventFilename);
        }
      }

      polling = false;
      if (!closed) {
        setTimeout(pollEvents, 50);
      }
    }

    process.nextTick(pollEvents);

    watcher.close = function() {
      if (closed) return;
      closed = true;
      if (watchId !== null) {
        ops.op_howth_fs_watch_close(watchId);
        watchId = null;
      }
      watcher.emit('close');
    };

    watcher.ref = function() { return this; };
    watcher.unref = function() { return this; };

    return watcher;
  };

  // Register the fs module
  globalThis.__howth_modules["node:fs"] = fsModule;
  globalThis.__howth_modules["fs"] = fsModule;
  globalThis.__howth_modules["node:fs/promises"] = fsPromises;
  globalThis.__howth_modules["fs/promises"] = fsPromises;
  return fsModule;
  }); // end lazy node:fs

  // ============================================
  // CommonJS Module System
  // ============================================

  // Module cache to prevent re-loading and handle circular deps
  const moduleCache = new Map();

  // The Module class (similar to Node.js Module)
  class Module {
    constructor(id, parent) {
      this.id = id;
      this.filename = id;
      this.dirname = posixPath.dirname(id);
      this.parent = parent;
      this.children = [];
      this.exports = {};
      this.loaded = false;
      this.paths = Module._nodeModulePaths(this.dirname);
    }

    static _nodeModulePaths(from) {
      // Generate node_modules lookup paths
      const paths = [];
      const seen = new Set();

      // Add paths from the 'from' directory
      let current = from;
      while (current !== "/") {
        const nodeModules = posixPath.join(current, "node_modules");
        if (!seen.has(nodeModules)) {
          paths.push(nodeModules);
          seen.add(nodeModules);
        }
        const parent = posixPath.dirname(current);
        if (parent === current) break;
        current = parent;
      }

      // Also add paths from the current working directory
      // This allows scripts run from outside a project to resolve modules from that project
      const cwd = ops.op_howth_cwd();
      if (cwd && cwd !== from) {
        current = cwd;
        while (current !== "/") {
          const nodeModules = posixPath.join(current, "node_modules");
          if (!seen.has(nodeModules)) {
            paths.push(nodeModules);
            seen.add(nodeModules);
          }
          const parent = posixPath.dirname(current);
          if (parent === current) break;
          current = parent;
        }
      }

      if (!seen.has("/node_modules")) {
        paths.push("/node_modules");
      }
      return paths;
    }

    static _resolveFilename(request, parent) {
      // Handle built-in modules
      if (request.startsWith("node:") || globalThis.__howth_modules[request]) {
        return request;
      }

      // Helper to create MODULE_NOT_FOUND error
      const createModuleNotFoundError = (request, paths) => {
        const err = new Error(`Cannot find module '${request}'`);
        err.code = 'MODULE_NOT_FOUND';
        err.requireStack = paths || [];
        return err;
      };

      // Handle relative and absolute paths
      if (request.startsWith("./") || request.startsWith("../") || request.startsWith("/")) {
        const basePath = request.startsWith("/")
          ? request
          : posixPath.resolve(parent ? parent.dirname : ops.op_howth_cwd(), request);
        const resolved = Module._resolveAsFile(basePath) || Module._resolveAsDirectory(basePath);
        if (resolved) return resolved;
        throw createModuleNotFoundError(request, parent ? [parent.filename] : []);
      }

      // Handle bare specifiers (node_modules)
      const paths = parent ? parent.paths : Module._nodeModulePaths(ops.op_howth_cwd());
      for (const modulesPath of paths) {
        const modulePath = posixPath.join(modulesPath, request);
        const resolved = Module._resolveAsFile(modulePath) || Module._resolveAsDirectory(modulePath);
        if (resolved) return resolved;
      }

      throw createModuleNotFoundError(request, parent ? [parent.filename] : []);
    }

    static _resolveAsFile(path) {
      // Check exact path
      if (ops.op_howth_fs_exists(path) && !Module._isDirectory(path)) {
        return path;
      }
      // Try extensions
      const extensions = [".js", ".cjs", ".json", ".node"];
      for (const ext of extensions) {
        const withExt = path + ext;
        if (ops.op_howth_fs_exists(withExt) && !Module._isDirectory(withExt)) {
          return withExt;
        }
      }
      return null;
    }

    static _resolveAsDirectory(path) {
      // Check for package.json
      const pkgPath = posixPath.join(path, "package.json");
      if (ops.op_howth_fs_exists(pkgPath)) {
        try {
          const pkg = JSON.parse(ops.op_howth_read_file(pkgPath));
          const main = pkg.main || "index.js";
          const mainPath = posixPath.resolve(path, main);
          return Module._resolveAsFile(mainPath) || Module._resolveAsFile(posixPath.join(mainPath, "index"));
        } catch (e) {
          // Fall through to index.js
        }
      }
      // Try index files
      return Module._resolveAsFile(posixPath.join(path, "index"));
    }

    static _isDirectory(path) {
      try {
        const stat = ops.op_howth_fs_stat(path, true);
        return stat.is_directory;
      } catch (e) {
        return false;
      }
    }

    static _load(request, parent) {
      const filename = Module._resolveFilename(request, parent);

      // Check cache
      if (moduleCache.has(filename)) {
        return moduleCache.get(filename).exports;
      }

      // Handle built-in modules
      if (globalThis.__howth_modules[filename]) {
        return globalThis.__howth_modules[filename];
      }

      // Create new module
      const module = new Module(filename, parent);
      moduleCache.set(filename, module);

      if (parent) {
        parent.children.push(module);
      }

      // Load the module
      module.load(filename);

      return module.exports;
    }

    load(filename) {
      const extension = posixPath.extname(filename) || ".js";

      if (extension === ".json") {
        // JSON files
        const content = ops.op_howth_read_file(filename);
        this.exports = JSON.parse(content);
      } else if (extension === ".node") {
        // Native addons not supported
        throw new Error("Native addons (.node) are not supported");
      } else {
        // JavaScript files
        this._compile(filename);
      }

      this.loaded = true;
    }

    _compile(filename) {
      // DEBUG: Log which file is being compiled
      if (process.env.DEBUG_MODULES) {
        console.log('[DEBUG] Compiling:', filename);
      }

      let content = ops.op_howth_read_file(filename);

      // Strip shebang line if present (like #!/usr/bin/env node)
      if (content.startsWith('#!')) {
        const newlineIndex = content.indexOf('\n');
        if (newlineIndex !== -1) {
          content = content.slice(newlineIndex + 1);
        }
      }

      // Create a custom dynamic import function that passes the correct referrer
      // We use a special URL scheme to encode the referrer in the specifier
      const __dynamicImport = (specifier) => {
        // Resolve relative specifiers against this module
        if (specifier.startsWith('./') || specifier.startsWith('../')) {
          const resolved = posixPath.resolve(this.dirname, specifier);
          return import(resolved);
        }
        // For other specifiers, just use the normal import
        return import(specifier);
      };

      // The Node.js module wrapper with __import__ provided
      const wrapper = [
        "(function (exports, require, module, __filename, __dirname, __import__) { ",
        "\n});"
      ];

      // Transform import() calls to use __import__() for relative paths
      // This is a simple transformation - won't handle all edge cases
      let transformedContent = content.replace(/\bimport\s*\(/g, '__import__(');

      const wrappedCode = wrapper[0] + transformedContent + wrapper[1];

      // Create a require function for this module
      const self = this;
      function require(id) {
        return Module._load(id, self);
      }
      require.resolve = (id) => Module._resolveFilename(id, self);
      require.resolve.paths = (id) => Module._nodeModulePaths(self.dirname);
      require.cache = Object.fromEntries(moduleCache);
      require.main = globalThis.__howth_main_module;
      require.extensions = Module._extensions;

      // Execute the wrapped code
      try {
        const compiledWrapper = (0, eval)(wrappedCode);
        compiledWrapper.call(
          this.exports,
          this.exports,
          require,
          this,
          this.filename,
          this.dirname,
          __dynamicImport
        );
      } catch (e) {
        // Remove from cache on error
        moduleCache.delete(filename);
        // Add context to the error
        if (e instanceof Error) {
          e.message = `Error in ${filename}: ${e.message}`;
        }
        throw e;
      }
    }

    // Instance require method (Module.prototype.require)
    require(id) {
      return Module._load(id, this);
    }
  }

  // Add static methods that Next.js expects
  Module._extensions = {
    '.js': (module, filename) => module._compile(filename),
    '.json': (module, filename) => {
      const content = ops.op_howth_read_file(filename);
      module.exports = JSON.parse(content);
    },
    '.node': () => { throw new Error('Native addons not supported'); },
  };

  Module._cache = moduleCache;

  Module.prototype.require = function(id) {
    return Module._load(id, this);
  };

  // The main require function
  function createRequire(parentFilename) {
    const parent = new Module(parentFilename, null);

    function require(id) {
      return Module._load(id, parent);
    }

    require.resolve = (id) => Module._resolveFilename(id, parent);
    require.cache = Object.fromEntries(moduleCache);
    require.main = globalThis.__howth_main_module;

    return require;
  }

  // Helper to extract the caller's file path from stack trace
  function getCallerFilePath() {
    const err = new Error();
    const stack = err.stack || "";
    const lines = stack.split("\n");

    // Look for first file:// URL that's not in bootstrap
    for (const line of lines) {
      // Match file:// URLs in the stack
      const fileMatch = line.match(/file:\/\/([^\s:)]+)/);
      if (fileMatch) {
        let filePath = fileMatch[1];
        // Decode URL encoding
        try { filePath = decodeURIComponent(filePath); } catch {}
        return filePath;
      }
      // Also try matching paths in "at module (path:line:col)" format
      const pathMatch = line.match(/at\s+(?:[^\s]+\s+\()?(\/[^\s:)]+)/);
      if (pathMatch && !pathMatch[1].includes("<howth:bootstrap>")) {
        return pathMatch[1];
      }
    }
    return null;
  }

  // Global require (uses caller's path or main module path as parent)
  function globalRequire(id) {
    // Try to determine parent path from call stack
    const callerPath = getCallerFilePath();
    const parentPath = callerPath ||
                       globalThis.__howth_main_module_path ||
                       posixPath.join(ops.op_howth_cwd(), "__entrypoint__");
    const parentModule = new Module(parentPath, null);
    return Module._load(id, parentModule);
  }
  globalRequire.resolve = (id) => {
    const callerPath = getCallerFilePath();
    const parentPath = callerPath ||
                       globalThis.__howth_main_module_path ||
                       posixPath.join(ops.op_howth_cwd(), "__entrypoint__");
    const parentModule = new Module(parentPath, null);
    return Module._resolveFilename(id, parentModule);
  };
  globalRequire.cache = {};
  Object.defineProperty(globalRequire, "cache", {
    get() { return Object.fromEntries(moduleCache); }
  });

  // Legacy require.extensions (deprecated but still used by some packages)
  globalRequire.extensions = Module._extensions;

  // Export require globally
  globalThis.require = globalRequire;
  globalThis.module = { exports: {} };
  globalThis.exports = globalThis.module.exports;

  // createRequire for ESM interop
  // Node.js exports the Module class itself with extra properties attached
  Module.createRequire = createRequire;
  Module.builtinModules = Object.keys(globalThis.__howth_modules);
  Module.syncBuiltinESMExports = () => {}; // No-op for compatibility
  Module.findSourceMap = () => undefined; // No-op
  Module.SourceMap = class SourceMap {}; // Stub
  Module.wrap = (script) => `(function (exports, require, module, __filename, __dirname) { ${script}\n});`;
  Module.wrapper = [
    '(function (exports, require, module, __filename, __dirname) { ',
    '\n});'
  ];
  Module.runMain = () => {}; // No-op
  Module.isBuiltin = (name) => Object.keys(globalThis.__howth_modules).includes(name);

  globalThis.__howth_modules["node:module"] = Module;
  globalThis.__howth_modules["module"] = Module;

  // ============================================
  // node:assert module (lazy)
  // ============================================
  __registerLazy(["node:assert", "assert", "node:assert/strict", "assert/strict"], () => {

  class AssertionError extends Error {
    constructor(options = {}) {
      const { message, actual, expected, operator, stackStartFn } = options;

      let msg = message;
      if (!msg) {
        if (operator === "strictEqual" || operator === "deepStrictEqual") {
          msg = `Expected values to be strictly ${operator === "deepStrictEqual" ? "deep-" : ""}equal:\n` +
                `+ actual - expected\n\n` +
                `+ ${JSON.stringify(actual)}\n- ${JSON.stringify(expected)}`;
        } else if (operator === "notStrictEqual" || operator === "notDeepStrictEqual") {
          msg = `Expected values not to be strictly ${operator === "notDeepStrictEqual" ? "deep-" : ""}equal:\n` +
                `${JSON.stringify(actual)}`;
        } else {
          msg = `${JSON.stringify(actual)} ${operator} ${JSON.stringify(expected)}`;
        }
      }

      super(msg);
      this.name = "AssertionError";
      this.code = "ERR_ASSERTION";
      this.actual = actual;
      this.expected = expected;
      this.operator = operator;
      this.generatedMessage = !message;

      if (Error.captureStackTrace) {
        Error.captureStackTrace(this, stackStartFn || this.constructor);
      }
    }
  }

  // Deep equality check
  function deepEqual(actual, expected, strict) {
    if (actual === expected) return true;

    if (actual === null || expected === null) return actual === expected;
    if (typeof actual !== typeof expected) return false;

    if (typeof actual !== "object") {
      if (strict) return actual === expected;
      // eslint-disable-next-line eqeqeq
      return actual == expected;
    }

    // Handle Date
    if (actual instanceof Date && expected instanceof Date) {
      return actual.getTime() === expected.getTime();
    }

    // Handle RegExp
    if (actual instanceof RegExp && expected instanceof RegExp) {
      return actual.source === expected.source && actual.flags === expected.flags;
    }

    // Handle arrays
    if (Array.isArray(actual) && Array.isArray(expected)) {
      if (actual.length !== expected.length) return false;
      for (let i = 0; i < actual.length; i++) {
        if (!deepEqual(actual[i], expected[i], strict)) return false;
      }
      return true;
    }

    // Handle typed arrays and buffers
    if (ArrayBuffer.isView(actual) && ArrayBuffer.isView(expected)) {
      if (actual.length !== expected.length) return false;
      for (let i = 0; i < actual.length; i++) {
        if (actual[i] !== expected[i]) return false;
      }
      return true;
    }

    // Handle plain objects
    const actualKeys = Object.keys(actual);
    const expectedKeys = Object.keys(expected);

    if (actualKeys.length !== expectedKeys.length) return false;

    for (const key of actualKeys) {
      if (!Object.prototype.hasOwnProperty.call(expected, key)) return false;
      if (!deepEqual(actual[key], expected[key], strict)) return false;
    }

    return true;
  }

  // Check if error matches expectation
  function checkError(actual, expected) {
    if (expected === undefined) return true;

    if (typeof expected === "function") {
      // Expected is a constructor
      if (expected.prototype !== undefined && actual instanceof expected) {
        return true;
      }
      // Expected is a validation function
      if (expected.call({}, actual) === true) {
        return true;
      }
      return false;
    }

    if (expected instanceof RegExp) {
      return expected.test(String(actual));
    }

    if (typeof expected === "object" && expected !== null) {
      // Match error properties
      for (const key of Object.keys(expected)) {
        if (typeof actual[key] === "string" && expected[key] instanceof RegExp) {
          if (!expected[key].test(actual[key])) return false;
        } else if (!deepEqual(actual[key], expected[key], true)) {
          return false;
        }
      }
      return true;
    }

    return false;
  }

  // Main assert function
  function assert(value, message) {
    if (!value) {
      throw new AssertionError({
        message: message || "The expression evaluated to a falsy value",
        actual: value,
        expected: true,
        operator: "==",
        stackStartFn: assert,
      });
    }
  }

  assert.ok = function ok(value, message) {
    if (!value) {
      throw new AssertionError({
        message: message || "The expression evaluated to a falsy value",
        actual: value,
        expected: true,
        operator: "ok",
        stackStartFn: ok,
      });
    }
  };

  assert.equal = function equal(actual, expected, message) {
    // eslint-disable-next-line eqeqeq
    if (actual != expected) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "==",
        stackStartFn: equal,
      });
    }
  };

  assert.notEqual = function notEqual(actual, expected, message) {
    // eslint-disable-next-line eqeqeq
    if (actual == expected) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "!=",
        stackStartFn: notEqual,
      });
    }
  };

  assert.strictEqual = function strictEqual(actual, expected, message) {
    if (actual !== expected) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "strictEqual",
        stackStartFn: strictEqual,
      });
    }
  };

  assert.notStrictEqual = function notStrictEqual(actual, expected, message) {
    if (actual === expected) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "notStrictEqual",
        stackStartFn: notStrictEqual,
      });
    }
  };

  assert.deepEqual = function deepEqualFn(actual, expected, message) {
    if (!deepEqual(actual, expected, false)) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "deepEqual",
        stackStartFn: deepEqualFn,
      });
    }
  };

  assert.notDeepEqual = function notDeepEqual(actual, expected, message) {
    if (deepEqual(actual, expected, false)) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "notDeepEqual",
        stackStartFn: notDeepEqual,
      });
    }
  };

  assert.deepStrictEqual = function deepStrictEqual(actual, expected, message) {
    if (!deepEqual(actual, expected, true)) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "deepStrictEqual",
        stackStartFn: deepStrictEqual,
      });
    }
  };

  assert.notDeepStrictEqual = function notDeepStrictEqual(actual, expected, message) {
    if (deepEqual(actual, expected, true)) {
      throw new AssertionError({
        message,
        actual,
        expected,
        operator: "notDeepStrictEqual",
        stackStartFn: notDeepStrictEqual,
      });
    }
  };

  assert.throws = function throws(fn, expected, message) {
    if (typeof expected === "string") {
      message = expected;
      expected = undefined;
    }

    let thrown = false;
    let actual;

    try {
      fn();
    } catch (e) {
      thrown = true;
      actual = e;
    }

    if (!thrown) {
      throw new AssertionError({
        message: message || "Missing expected exception",
        actual: undefined,
        expected,
        operator: "throws",
        stackStartFn: throws,
      });
    }

    if (expected !== undefined && !checkError(actual, expected)) {
      throw new AssertionError({
        message: message || `The error did not match the expected`,
        actual,
        expected,
        operator: "throws",
        stackStartFn: throws,
      });
    }
  };

  assert.doesNotThrow = function doesNotThrow(fn, expected, message) {
    if (typeof expected === "string") {
      message = expected;
      expected = undefined;
    }

    try {
      fn();
    } catch (e) {
      if (expected === undefined || checkError(e, expected)) {
        throw new AssertionError({
          message: message || `Got unwanted exception: ${e.message}`,
          actual: e,
          expected,
          operator: "doesNotThrow",
          stackStartFn: doesNotThrow,
        });
      }
      throw e;
    }
  };

  assert.rejects = async function rejects(asyncFn, expected, message) {
    if (typeof expected === "string") {
      message = expected;
      expected = undefined;
    }

    let thrown = false;
    let actual;

    try {
      const promise = typeof asyncFn === "function" ? asyncFn() : asyncFn;
      await promise;
    } catch (e) {
      thrown = true;
      actual = e;
    }

    if (!thrown) {
      throw new AssertionError({
        message: message || "Missing expected rejection",
        actual: undefined,
        expected,
        operator: "rejects",
        stackStartFn: rejects,
      });
    }

    if (expected !== undefined && !checkError(actual, expected)) {
      throw new AssertionError({
        message: message || "The rejection did not match the expected",
        actual,
        expected,
        operator: "rejects",
        stackStartFn: rejects,
      });
    }
  };

  assert.doesNotReject = async function doesNotReject(asyncFn, expected, message) {
    if (typeof expected === "string") {
      message = expected;
      expected = undefined;
    }

    try {
      const promise = typeof asyncFn === "function" ? asyncFn() : asyncFn;
      await promise;
    } catch (e) {
      if (expected === undefined || checkError(e, expected)) {
        throw new AssertionError({
          message: message || `Got unwanted rejection: ${e.message}`,
          actual: e,
          expected,
          operator: "doesNotReject",
          stackStartFn: doesNotReject,
        });
      }
      throw e;
    }
  };

  assert.fail = function fail(message) {
    if (arguments.length === 0) {
      message = "Failed";
    } else if (arguments.length === 2) {
      // Legacy: assert.fail(actual, expected)
      message = `${arguments[0]} undefined ${arguments[1]}`;
    } else if (arguments.length >= 3) {
      // Legacy: assert.fail(actual, expected, message, operator)
      message = arguments[2] || `${arguments[0]} ${arguments[3] || "!="} ${arguments[1]}`;
    }

    throw new AssertionError({
      message,
      operator: "fail",
      stackStartFn: fail,
    });
  };

  assert.ifError = function ifError(value) {
    if (value !== null && value !== undefined) {
      throw value instanceof Error ? value : new AssertionError({
        message: `ifError got unwanted exception: ${value}`,
        actual: value,
        expected: null,
        operator: "ifError",
        stackStartFn: ifError,
      });
    }
  };

  assert.match = function match(string, regexp, message) {
    if (!(regexp instanceof RegExp)) {
      throw new TypeError("The 'regexp' argument must be a RegExp");
    }
    if (typeof string !== "string") {
      throw new TypeError("The 'string' argument must be a string");
    }

    if (!regexp.test(string)) {
      throw new AssertionError({
        message: message || `The input did not match the regular expression ${regexp}`,
        actual: string,
        expected: regexp,
        operator: "match",
        stackStartFn: match,
      });
    }
  };

  assert.doesNotMatch = function doesNotMatch(string, regexp, message) {
    if (!(regexp instanceof RegExp)) {
      throw new TypeError("The 'regexp' argument must be a RegExp");
    }
    if (typeof string !== "string") {
      throw new TypeError("The 'string' argument must be a string");
    }

    if (regexp.test(string)) {
      throw new AssertionError({
        message: message || `The input was expected to not match the regular expression ${regexp}`,
        actual: string,
        expected: regexp,
        operator: "doesNotMatch",
        stackStartFn: doesNotMatch,
      });
    }
  };

  // Strict mode - all functions use strict equality
  assert.strict = Object.assign(
    function strictAssert(value, message) {
      if (!value) {
        throw new AssertionError({
          message: message || "The expression evaluated to a falsy value",
          actual: value,
          expected: true,
          operator: "==",
          stackStartFn: strictAssert,
        });
      }
    },
    {
      ok: assert.ok,
      equal: assert.strictEqual,
      notEqual: assert.notStrictEqual,
      deepEqual: assert.deepStrictEqual,
      notDeepEqual: assert.notDeepStrictEqual,
      strictEqual: assert.strictEqual,
      notStrictEqual: assert.notStrictEqual,
      deepStrictEqual: assert.deepStrictEqual,
      notDeepStrictEqual: assert.notDeepStrictEqual,
      throws: assert.throws,
      doesNotThrow: assert.doesNotThrow,
      rejects: assert.rejects,
      doesNotReject: assert.doesNotReject,
      fail: assert.fail,
      ifError: assert.ifError,
      match: assert.match,
      doesNotMatch: assert.doesNotMatch,
      AssertionError,
    }
  );

  assert.AssertionError = AssertionError;

  // Register the assert module
  globalThis.__howth_modules["node:assert"] = assert;
  globalThis.__howth_modules["assert"] = assert;
  globalThis.__howth_modules["node:assert/strict"] = assert.strict;
  globalThis.__howth_modules["assert/strict"] = assert.strict;
  return assert;
  }); // end lazy node:assert

  // ============================================================================
  // events module (EventEmitter) — start of heavy lazy block
  // All modules from here through node:tls are lazily initialized together.
  // ============================================================================
  __registerLazy([
    "node:events", "events",
    "node:util", "util", "node:util/types", "util/types",
    "node:stream", "stream", "node:stream/promises", "stream/promises", "node:stream/web", "stream/web",
    "node:crypto", "crypto",
    "node:child_process", "child_process",
    "node:os", "os",
    "node:querystring", "querystring",
    "node:timers", "timers", "node:timers/promises", "timers/promises",
    "node:string_decoder", "string_decoder",
    "node:url", "url",
    "node:punycode", "punycode",
    "node:console", "console",
    "node:constants", "constants",
    "node:perf_hooks", "perf_hooks",
    "node:tty", "tty",
    "node:v8", "v8",
    "node:domain", "domain",
    "node:async_hooks", "async_hooks",
    "node:zlib", "zlib",
    "node:worker_threads", "worker_threads",
    "node:http", "http",
    "node:https", "https",
    "node:net", "net",
    "node:dns", "dns", "node:dns/promises", "dns/promises",
    "node:diagnostics_channel", "diagnostics_channel",
    "node:inspector", "inspector",
    "node:vm", "vm",
    "node:tls", "tls",
  ], () => {

  class EventEmitter {
    constructor() {
      if (!this._events) {
        this._events = new Map();
      }
    }

    _initEvents() {
      if (!this._events) {
        this._events = new Map();
      }
    }

    on(event, listener) {
      this._initEvents();
      if (!this._events.has(event)) {
        this._events.set(event, []);
      }
      this._events.get(event).push(listener);
      return this;
    }

    addListener(event, listener) {
      return this.on(event, listener);
    }

    once(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener.apply(this, args);
      };
      wrapper.listener = listener;
      return this.on(event, wrapper);
    }

    off(event, listener) {
      this._initEvents();
      const listeners = this._events.get(event);
      if (listeners) {
        const index = listeners.findIndex(
          (l) => l === listener || l.listener === listener
        );
        if (index !== -1) {
          listeners.splice(index, 1);
        }
      }
      return this;
    }

    removeListener(event, listener) {
      return this.off(event, listener);
    }

    removeAllListeners(event) {
      this._initEvents();
      if (event !== undefined) {
        this._events.delete(event);
      } else {
        this._events.clear();
      }
      return this;
    }

    emit(event, ...args) {
      this._initEvents();
      const listeners = this._events.get(event);
      if (!listeners || listeners.length === 0) return false;
      for (const listener of [...listeners]) {
        listener.apply(this, args);
      }
      return true;
    }

    listeners(event) {
      this._initEvents();
      const list = this._events.get(event);
      if (!list) return [];
      return list.map((l) => l.listener || l);
    }

    listenerCount(event) {
      this._initEvents();
      const list = this._events.get(event);
      return list ? list.length : 0;
    }

    eventNames() {
      this._initEvents();
      return [...this._events.keys()];
    }

    prependListener(event, listener) {
      this._initEvents();
      if (!this._events.has(event)) {
        this._events.set(event, []);
      }
      this._events.get(event).unshift(listener);
      return this;
    }

    prependOnceListener(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener.apply(this, args);
      };
      wrapper.listener = listener;
      return this.prependListener(event, wrapper);
    }

    setMaxListeners(n) {
      // No-op for compatibility
      return this;
    }

    getMaxListeners() {
      return 10; // Default Node.js value
    }

    rawListeners(event) {
      this._initEvents();
      return this._events.get(event) || [];
    }
  }

  // Static methods
  EventEmitter.listenerCount = function (emitter, event) {
    return emitter.listenerCount(event);
  };

  const eventsModule = EventEmitter;
  eventsModule.EventEmitter = EventEmitter;

  globalThis.__howth_modules["node:events"] = eventsModule;
  globalThis.__howth_modules["events"] = eventsModule;

  // ============================================================================
  // util module
  // ============================================================================

  /**
   * Inspect a value and return a string representation.
   */
  function inspect(obj, options = {}) {
    const {
      depth = 2,
      colors = false,
      showHidden = false,
      maxArrayLength = 100,
      maxStringLength = 10000,
      sorted = false,
      getters = false,
    } = typeof options === "boolean" ? { showHidden: options } : options;

    const seen = new WeakSet();

    function inspectValue(value, currentDepth) {
      if (value === null) return colors ? "\x1b[1mnull\x1b[22m" : "null";
      if (value === undefined) return colors ? "\x1b[90mundefined\x1b[39m" : "undefined";

      const type = typeof value;

      if (type === "string") {
        const truncated = value.length > maxStringLength ? value.slice(0, maxStringLength) + "..." : value;
        const escaped = JSON.stringify(truncated);
        return colors ? `\x1b[32m${escaped}\x1b[39m` : escaped;
      }
      if (type === "number") {
        const str = Object.is(value, -0) ? "-0" : String(value);
        return colors ? `\x1b[33m${str}\x1b[39m` : str;
      }
      if (type === "bigint") {
        const str = `${value}n`;
        return colors ? `\x1b[33m${str}\x1b[39m` : str;
      }
      if (type === "boolean") {
        return colors ? `\x1b[33m${value}\x1b[39m` : String(value);
      }
      if (type === "symbol") {
        const str = value.toString();
        return colors ? `\x1b[32m${str}\x1b[39m` : str;
      }
      if (type === "function") {
        const name = value.name || "(anonymous)";
        const str = `[Function: ${name}]`;
        return colors ? `\x1b[36m${str}\x1b[39m` : str;
      }

      if (type === "object") {
        if (seen.has(value)) return colors ? "\x1b[36m[Circular]\x1b[39m" : "[Circular]";
        if (currentDepth > depth) return Array.isArray(value) ? "[Array]" : "[Object]";
        seen.add(value);

        if (value instanceof Date) return colors ? `\x1b[35m${value.toISOString()}\x1b[39m` : value.toISOString();
        if (value instanceof RegExp) return colors ? `\x1b[31m${value}\x1b[39m` : value.toString();
        if (value instanceof Error) return value.stack || value.toString();
        if (value instanceof Map) {
          if (value.size === 0) return "Map(0) {}";
          const entries = [...value.entries()].slice(0, maxArrayLength)
            .map(([k, v]) => `${inspectValue(k, currentDepth + 1)} => ${inspectValue(v, currentDepth + 1)}`).join(", ");
          return `Map(${value.size}) { ${entries} }`;
        }
        if (value instanceof Set) {
          if (value.size === 0) return "Set(0) {}";
          const entries = [...value.values()].slice(0, maxArrayLength).map((v) => inspectValue(v, currentDepth + 1)).join(", ");
          return `Set(${value.size}) { ${entries} }`;
        }
        if (value instanceof WeakMap) return "WeakMap { <items unknown> }";
        if (value instanceof WeakSet) return "WeakSet { <items unknown> }";
        if (ArrayBuffer.isView(value) && !(value instanceof DataView)) {
          const name = value.constructor.name;
          const len = value.length;
          if (len === 0) return `${name}(0) []`;
          const items = [...value].slice(0, maxArrayLength).map(String).join(", ");
          const suffix = len > maxArrayLength ? `, ... ${len - maxArrayLength} more items` : "";
          return `${name}(${len}) [ ${items}${suffix} ]`;
        }
        if (value instanceof ArrayBuffer) return `ArrayBuffer { byteLength: ${value.byteLength} }`;
        if (value instanceof Promise) return "Promise { <pending> }";

        if (Array.isArray(value)) {
          if (value.length === 0) return "[]";
          const items = value.slice(0, maxArrayLength).map((v) => inspectValue(v, currentDepth + 1));
          const suffix = value.length > maxArrayLength ? `, ... ${value.length - maxArrayLength} more items` : "";
          return `[ ${items.join(", ")}${suffix} ]`;
        }

        let keys = Object.keys(value);
        if (sorted) keys = keys.sort();
        if (showHidden) keys = keys.concat(Object.getOwnPropertySymbols(value));
        if (keys.length === 0) return "{}";

        const entries = keys.map((key) => {
          const desc = Object.getOwnPropertyDescriptor(value, key);
          let val;
          if (desc.get && !getters) val = "[Getter]";
          else if (desc.set && !desc.get) val = "[Setter]";
          else val = inspectValue(value[key], currentDepth + 1);
          return `${typeof key === "symbol" ? key.toString() : key}: ${val}`;
        });
        return `{ ${entries.join(", ")} }`;
      }
      return String(value);
    }
    return inspectValue(obj, 0);
  }

  inspect.custom = Symbol.for("nodejs.util.inspect.custom");
  inspect.defaultOptions = { showHidden: false, depth: 2, colors: false, maxArrayLength: 100 };

  /**
   * Format a string with printf-style formatting.
   */
  function format(fmt, ...args) {
    if (typeof fmt !== "string") return [fmt, ...args].map((v) => inspect(v)).join(" ");
    let i = 0;
    let str = fmt.replace(/%([sdifjoOc%])/g, (match, spec) => {
      if (spec === "%") return "%";
      if (i >= args.length) return match;
      const arg = args[i++];
      switch (spec) {
        case "s": return String(arg);
        case "d": case "i": return Number.parseInt(arg, 10).toString();
        case "f": return Number.parseFloat(arg).toString();
        case "j": try { return JSON.stringify(arg); } catch { return "[Circular]"; }
        case "o": case "O": return inspect(arg);
        case "c": return "";
        default: return match;
      }
    });
    while (i < args.length) str += " " + inspect(args[i++]);
    return str;
  }

  function formatWithOptions(inspectOptions, fmt, ...args) {
    if (typeof fmt !== "string") return [fmt, ...args].map((v) => inspect(v, inspectOptions)).join(" ");
    let i = 0;
    let str = fmt.replace(/%([sdifjoOc%])/g, (match, spec) => {
      if (spec === "%") return "%";
      if (i >= args.length) return match;
      const arg = args[i++];
      switch (spec) {
        case "s": return String(arg);
        case "d": case "i": return Number.parseInt(arg, 10).toString();
        case "f": return Number.parseFloat(arg).toString();
        case "j": try { return JSON.stringify(arg); } catch { return "[Circular]"; }
        case "o": case "O": return inspect(arg, inspectOptions);
        case "c": return "";
        default: return match;
      }
    });
    while (i < args.length) str += " " + inspect(args[i++], inspectOptions);
    return str;
  }

  /**
   * Convert a callback-style function to a Promise-returning function.
   */
  function promisify(original) {
    if (typeof original !== "function") throw new TypeError('The "original" argument must be of type Function');
    if (original[promisify.custom]) {
      const fn = original[promisify.custom];
      if (typeof fn !== "function") throw new TypeError('The "util.promisify.custom" property must be of type Function');
      return fn;
    }
    function fn(...args) {
      return new Promise((resolve, reject) => {
        original.call(this, ...args, (err, ...values) => {
          if (err) reject(err);
          else if (values.length === 1) resolve(values[0]);
          else resolve(values);
        });
      });
    }
    Object.setPrototypeOf(fn, Object.getPrototypeOf(original));
    return Object.defineProperties(fn, Object.getOwnPropertyDescriptors(original));
  }
  promisify.custom = Symbol.for("nodejs.util.promisify.custom");

  /**
   * Convert a Promise-returning function to a callback-style function.
   */
  function callbackify(original) {
    if (typeof original !== "function") throw new TypeError('The "original" argument must be of type Function');
    function callbackified(...args) {
      const callback = args.pop();
      if (typeof callback !== "function") throw new TypeError("The last argument must be of type Function");
      Promise.resolve(original.apply(this, args)).then(
        (value) => process.nextTick(callback, null, value),
        (err) => { if (!err) { err = new Error("Promise rejected with falsy value"); err.reason = err; } process.nextTick(callback, err); }
      );
    }
    Object.setPrototypeOf(callbackified, Object.getPrototypeOf(original));
    return Object.defineProperties(callbackified, Object.getOwnPropertyDescriptors(original));
  }

  /**
   * Mark a function as deprecated.
   */
  function deprecate(fn, msg, code) {
    if (typeof fn !== "function") throw new TypeError('The "fn" argument must be of type Function');
    let warned = false;
    function deprecated(...args) {
      if (!warned) { warned = true; console.warn(`DeprecationWarning: ${code ? `[${code}] ` : ""}${msg}`); }
      return fn.apply(this, args);
    }
    Object.setPrototypeOf(deprecated, fn);
    return Object.defineProperties(deprecated, Object.getOwnPropertyDescriptors(fn));
  }

  /**
   * Inherit prototype methods from a constructor.
   */
  function inherits(ctor, superCtor) {
    if (ctor === undefined || ctor === null) throw new TypeError("The constructor must not be null or undefined");
    if (superCtor === undefined || superCtor === null) throw new TypeError("The super constructor must not be null or undefined");
    if (superCtor.prototype === undefined) {
      console.error("[DEBUG inherits] superCtor has no prototype:");
      console.error("  superCtor:", superCtor);
      console.error("  superCtor type:", typeof superCtor);
      console.error("  superCtor.name:", superCtor?.name);
      console.error("  superCtor keys:", Object.keys(superCtor || {}));
      throw new TypeError("The super constructor must have a prototype");
    }
    Object.defineProperty(ctor, "super_", { value: superCtor, writable: true, configurable: true });
    Object.setPrototypeOf(ctor.prototype, superCtor.prototype);
  }

  /**
   * Create a debug logger for a specific section.
   */
  function debuglog(section, callback) {
    const envDebug = process.env.NODE_DEBUG || "";
    const enabled = new RegExp(`\\b${section}\\b`, "i").test(envDebug);
    const fn = enabled
      ? function (...args) { console.error("%s %d: %s", section.toUpperCase(), process.pid, format(...args)); }
      : function () {};
    fn.enabled = enabled;
    if (typeof callback === "function") callback(fn);
    return fn;
  }

  // Type checking utilities
  const types = {
    isAnyArrayBuffer: (v) => v instanceof ArrayBuffer || (typeof SharedArrayBuffer !== "undefined" && v instanceof SharedArrayBuffer),
    isArrayBuffer: (v) => v instanceof ArrayBuffer,
    isArrayBufferView: (v) => ArrayBuffer.isView(v),
    isAsyncFunction: (v) => v?.constructor?.name === "AsyncFunction",
    isBigInt64Array: (v) => v instanceof BigInt64Array,
    isBigUint64Array: (v) => v instanceof BigUint64Array,
    isBooleanObject: (v) => v instanceof Boolean,
    isBoxedPrimitive: (v) => v instanceof Boolean || v instanceof Number || v instanceof String,
    isDataView: (v) => v instanceof DataView,
    isDate: (v) => v instanceof Date,
    isFloat32Array: (v) => v instanceof Float32Array,
    isFloat64Array: (v) => v instanceof Float64Array,
    isGeneratorFunction: (v) => v?.constructor?.name === "GeneratorFunction",
    isGeneratorObject: (v) => v?.[Symbol.toStringTag] === "Generator",
    isInt8Array: (v) => v instanceof Int8Array,
    isInt16Array: (v) => v instanceof Int16Array,
    isInt32Array: (v) => v instanceof Int32Array,
    isMap: (v) => v instanceof Map,
    isMapIterator: (v) => v?.[Symbol.toStringTag] === "Map Iterator",
    isNativeError: (v) => v instanceof Error,
    isNumberObject: (v) => v instanceof Number,
    isPromise: (v) => v instanceof Promise,
    isProxy: () => false,
    isRegExp: (v) => v instanceof RegExp,
    isSet: (v) => v instanceof Set,
    isSetIterator: (v) => v?.[Symbol.toStringTag] === "Set Iterator",
    isSharedArrayBuffer: (v) => typeof SharedArrayBuffer !== "undefined" && v instanceof SharedArrayBuffer,
    isStringObject: (v) => v instanceof String,
    isSymbolObject: (v) => typeof v === "object" && Object.prototype.toString.call(v) === "[object Symbol]",
    isTypedArray: (v) => ArrayBuffer.isView(v) && !(v instanceof DataView),
    isUint8Array: (v) => v instanceof Uint8Array,
    isUint8ClampedArray: (v) => v instanceof Uint8ClampedArray,
    isUint16Array: (v) => v instanceof Uint16Array,
    isUint32Array: (v) => v instanceof Uint32Array,
    isWeakMap: (v) => v instanceof WeakMap,
    isWeakSet: (v) => v instanceof WeakSet,
  };

  /**
   * Deep strict equality check.
   */
  function isDeepStrictEqual(a, b, seen = new Map()) {
    if (Object.is(a, b)) return true;
    if (typeof a !== typeof b) return false;
    if (typeof a !== "object" || a === null || b === null) return false;
    if (seen.has(a)) return seen.get(a) === b;
    seen.set(a, b);
    if (Array.isArray(a)) {
      if (!Array.isArray(b) || a.length !== b.length) return false;
      for (let i = 0; i < a.length; i++) if (!isDeepStrictEqual(a[i], b[i], seen)) return false;
      return true;
    }
    if (a instanceof Date) return b instanceof Date && a.getTime() === b.getTime();
    if (a instanceof RegExp) return b instanceof RegExp && a.toString() === b.toString();
    if (a instanceof Map) {
      if (!(b instanceof Map) || a.size !== b.size) return false;
      for (const [key, val] of a) if (!b.has(key) || !isDeepStrictEqual(val, b.get(key), seen)) return false;
      return true;
    }
    if (a instanceof Set) {
      if (!(b instanceof Set) || a.size !== b.size) return false;
      for (const val of a) if (!b.has(val)) return false;
      return true;
    }
    const keysA = Object.keys(a), keysB = Object.keys(b);
    if (keysA.length !== keysB.length) return false;
    for (const key of keysA) {
      if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
      if (!isDeepStrictEqual(a[key], b[key], seen)) return false;
    }
    return true;
  }

  const utilModule = {
    format, formatWithOptions, inspect, promisify, callbackify, deprecate, inherits, debuglog,
    debug: debuglog, isDeepStrictEqual, types,
    // Legacy type checking
    isArray: Array.isArray,
    isBoolean: (v) => typeof v === "boolean",
    isNull: (v) => v === null,
    isNullOrUndefined: (v) => v == null,
    isNumber: (v) => typeof v === "number",
    isString: (v) => typeof v === "string",
    isSymbol: (v) => typeof v === "symbol",
    isUndefined: (v) => v === undefined,
    isRegExp: (v) => v instanceof RegExp,
    isObject: (v) => typeof v === "object" && v !== null,
    isDate: (v) => v instanceof Date,
    isError: (v) => v instanceof Error,
    isFunction: (v) => typeof v === "function",
    isPrimitive: (v) => v === null || (typeof v !== "object" && typeof v !== "function"),
    isBuffer: Buffer.isBuffer,
    TextDecoder: globalThis.TextDecoder,
    TextEncoder: globalThis.TextEncoder,
  };
  utilModule.promisify.custom = promisify.custom;

  globalThis.__howth_modules["node:util"] = utilModule;
  globalThis.__howth_modules["util"] = utilModule;
  globalThis.__howth_modules["node:util/types"] = types;
  globalThis.__howth_modules["util/types"] = types;

  // ============================================================================
  // stream module
  // ============================================================================

  // Stream base class extends EventEmitter
  class Stream extends EventEmitter {
    constructor(options = {}) {
      super();
      this.readable = false;
      this.writable = false;
      this._readableState = null;
      this._writableState = null;
    }

    pipe(dest, options = {}) {
      const source = this;
      const ondata = (chunk) => {
        if (dest.writable && dest.write(chunk) === false && source.pause) {
          source.pause();
        }
      };
      source.on("data", ondata);

      const ondrain = () => {
        if (source.readable && source.resume) {
          source.resume();
        }
      };
      dest.on("drain", ondrain);

      const onend = () => {
        if (options.end !== false) {
          dest.end();
        }
      };
      source.on("end", onend);

      const onerror = (err) => {
        cleanup();
        if (this.listenerCount("error") === 0) {
          throw err;
        }
      };
      source.on("error", onerror);
      dest.on("error", onerror);

      const onclose = () => {
        cleanup();
      };
      source.on("close", onclose);
      dest.on("close", onclose);

      const cleanup = () => {
        source.off("data", ondata);
        dest.off("drain", ondrain);
        source.off("end", onend);
        source.off("error", onerror);
        dest.off("error", onerror);
        source.off("close", onclose);
        dest.off("close", onclose);
      };

      dest.emit("pipe", source);
      return dest;
    }

    unpipe(dest) {
      // Simplified unpipe
      return this;
    }
  }

  // Readable stream
  class Readable extends Stream {
    constructor(options = {}) {
      super(options);
      this.readable = true;
      this._readableState = {
        buffer: [],
        flowing: null,
        ended: false,
        endEmitted: false,
        reading: false,
        highWaterMark: options.highWaterMark || 16384,
        objectMode: options.objectMode || false,
        encoding: options.encoding || null,
      };

      if (typeof options.read === "function") {
        this._read = options.read;
      }
      if (typeof options.destroy === "function") {
        this._destroy = options.destroy;
      }
    }

    on(event, listener) {
      const result = super.on(event, listener);
      if (event === "data") {
        // Start flowing when data listener is added
        if (this._readableState.flowing !== false) {
          this.resume();
        }
      }
      return result;
    }

    _read(size) {
      // Override this in subclasses
    }

    read(size) {
      const state = this._readableState;
      if (state.buffer.length === 0) {
        if (state.ended) {
          if (!state.endEmitted) {
            state.endEmitted = true;
            process.nextTick(() => this.emit("end"));
          }
          return null;
        }
        state.reading = true;
        this._read(state.highWaterMark);
        state.reading = false;
      }

      if (state.buffer.length === 0) return null;

      let chunk;
      if (size === undefined || size >= state.buffer.length) {
        chunk = state.objectMode ? state.buffer.shift() : Buffer.concat(state.buffer);
        state.buffer = state.objectMode ? state.buffer : [];
      } else {
        chunk = state.buffer[0].slice(0, size);
        state.buffer[0] = state.buffer[0].slice(size);
        if (state.buffer[0].length === 0) state.buffer.shift();
      }

      if (state.encoding && Buffer.isBuffer(chunk)) {
        chunk = chunk.toString(state.encoding);
      }

      return chunk;
    }

    push(chunk, encoding) {
      const state = this._readableState;
      if (chunk === null) {
        state.ended = true;
        if (state.flowing) {
          process.nextTick(() => this.emit("end"));
        }
        return false;
      }

      if (typeof chunk === "string") {
        chunk = Buffer.from(chunk, encoding || state.encoding || "utf8");
      }

      if (state.objectMode || chunk.length > 0) {
        if (state.flowing) {
          // Emit data directly when flowing
          if (state.encoding && Buffer.isBuffer(chunk)) {
            chunk = chunk.toString(state.encoding);
          }
          this.emit("data", chunk);
        } else {
          state.buffer.push(chunk);
        }
      }

      return state.buffer.length < state.highWaterMark;
    }

    unshift(chunk) {
      const state = this._readableState;
      if (typeof chunk === "string") {
        chunk = Buffer.from(chunk);
      }
      state.buffer.unshift(chunk);
    }

    resume() {
      const state = this._readableState;
      if (!state.flowing) {
        state.flowing = true;
        process.nextTick(() => this._flow());
      }
      return this;
    }

    pause() {
      const state = this._readableState;
      if (state.flowing !== false) {
        state.flowing = false;
      }
      return this;
    }

    _flow() {
      const state = this._readableState;
      while (state.flowing && !state.ended) {
        const chunk = this.read();
        if (chunk === null) break;
        this.emit("data", chunk);
      }
    }

    isPaused() {
      return this._readableState.flowing === false;
    }

    setEncoding(encoding) {
      this._readableState.encoding = encoding;
      return this;
    }

    destroy(err) {
      if (this._readableState.destroyed) return this;
      this._readableState.destroyed = true;

      process.nextTick(() => {
        if (err) this.emit("error", err);
        this.emit("close");
      });
      return this;
    }

    [Symbol.asyncIterator]() {
      const stream = this;
      const state = this._readableState;
      return {
        async next() {
          if (state.ended && state.buffer.length === 0) {
            return { done: true, value: undefined };
          }
          return new Promise((resolve, reject) => {
            const onReadable = () => {
              cleanup();
              const chunk = stream.read();
              if (chunk !== null) {
                resolve({ done: false, value: chunk });
              } else if (state.ended) {
                resolve({ done: true, value: undefined });
              }
            };
            const onEnd = () => {
              cleanup();
              resolve({ done: true, value: undefined });
            };
            const onError = (err) => {
              cleanup();
              reject(err);
            };
            const cleanup = () => {
              stream.off("readable", onReadable);
              stream.off("end", onEnd);
              stream.off("error", onError);
            };
            stream.on("readable", onReadable);
            stream.on("end", onEnd);
            stream.on("error", onError);
            // Try immediate read
            const chunk = stream.read();
            if (chunk !== null) {
              cleanup();
              resolve({ done: false, value: chunk });
            }
          });
        },
      };
    }

    static from(iterable, options = {}) {
      const readable = new Readable(options);
      (async () => {
        try {
          for await (const chunk of iterable) {
            if (!readable.push(chunk)) {
              await new Promise((r) => readable.once("drain", r));
            }
          }
          readable.push(null);
        } catch (err) {
          readable.destroy(err);
        }
      })();
      return readable;
    }
  }

  // Writable stream
  class Writable extends Stream {
    constructor(options = {}) {
      super(options);
      this.writable = true;
      this._writableState = {
        buffer: [],
        writing: false,
        ended: false,
        finished: false,
        destroyed: false,
        highWaterMark: options.highWaterMark || 16384,
        objectMode: options.objectMode || false,
        needDrain: false,
        finalCalled: false,
        corked: 0,
        defaultEncoding: options.defaultEncoding || "utf8",
      };

      if (typeof options.write === "function") {
        this._write = options.write;
      }
      if (typeof options.writev === "function") {
        this._writev = options.writev;
      }
      if (typeof options.destroy === "function") {
        this._destroy = options.destroy;
      }
      if (typeof options.final === "function") {
        this._final = options.final;
      }
    }

    _write(chunk, encoding, callback) {
      callback();
    }

    write(chunk, encoding, callback) {
      const state = this._writableState;
      if (typeof encoding === "function") {
        callback = encoding;
        encoding = state.defaultEncoding;
      }
      if (!callback) callback = () => {};
      if (state.ended) {
        const err = new Error("write after end");
        process.nextTick(() => callback(err));
        this.emit("error", err);
        return false;
      }

      if (typeof chunk === "string") {
        chunk = Buffer.from(chunk, encoding);
      }

      const len = state.objectMode ? 1 : chunk.length;
      state.buffer.push({ chunk, encoding, callback });

      const ret = state.buffer.length < state.highWaterMark;
      if (!ret) state.needDrain = true;

      if (!state.writing) {
        this._doWrite();
      }

      return ret;
    }

    _doWrite() {
      const state = this._writableState;
      if (state.buffer.length === 0) {
        if (state.needDrain) {
          state.needDrain = false;
          this.emit("drain");
        }
        if (state.ended && !state.finished) {
          this._finish();
        }
        return;
      }

      state.writing = true;
      const { chunk, encoding, callback } = state.buffer.shift();

      this._write(chunk, encoding, (err) => {
        state.writing = false;
        if (err) {
          callback(err);
          this.emit("error", err);
          return;
        }
        callback();
        process.nextTick(() => this._doWrite());
      });
    }

    _finish() {
      const state = this._writableState;
      if (state.finished) return;

      const doFinish = () => {
        state.finished = true;
        this.emit("finish");
      };

      if (this._final && !state.finalCalled) {
        state.finalCalled = true;
        this._final((err) => {
          if (err) this.emit("error", err);
          doFinish();
        });
      } else {
        doFinish();
      }
    }

    end(chunk, encoding, callback) {
      const state = this._writableState;
      if (typeof chunk === "function") {
        callback = chunk;
        chunk = null;
      } else if (typeof encoding === "function") {
        callback = encoding;
        encoding = null;
      }

      if (chunk !== null && chunk !== undefined) {
        this.write(chunk, encoding);
      }

      state.ended = true;
      if (callback) this.once("finish", callback);

      if (!state.writing && state.buffer.length === 0) {
        process.nextTick(() => this._finish());
      }

      return this;
    }

    cork() {
      this._writableState.corked++;
    }

    uncork() {
      const state = this._writableState;
      if (state.corked > 0) {
        state.corked--;
        if (state.corked === 0 && !state.writing) {
          this._doWrite();
        }
      }
    }

    destroy(err) {
      const state = this._writableState;
      if (state.destroyed) return this;
      state.destroyed = true;

      process.nextTick(() => {
        if (err) this.emit("error", err);
        this.emit("close");
      });
      return this;
    }

    setDefaultEncoding(encoding) {
      this._writableState.defaultEncoding = encoding;
      return this;
    }
  }

  // Duplex stream (both readable and writable)
  class Duplex extends Readable {
    constructor(options = {}) {
      super(options);
      this.writable = true;
      this._writableState = {
        buffer: [],
        writing: false,
        ended: false,
        finished: false,
        destroyed: false,
        highWaterMark: options.writableHighWaterMark || options.highWaterMark || 16384,
        objectMode: options.writableObjectMode || options.objectMode || false,
        needDrain: false,
        finalCalled: false,
        corked: 0,
        defaultEncoding: options.defaultEncoding || "utf8",
      };

      if (typeof options.write === "function") this._write = options.write;
      if (typeof options.writev === "function") this._writev = options.writev;
      if (typeof options.final === "function") this._final = options.final;

      if (options.allowHalfOpen === false) {
        this.once("end", () => this.end());
      }
    }
  }
  // Mix in Writable methods
  Object.assign(Duplex.prototype, {
    write: Writable.prototype.write,
    end: Writable.prototype.end,
    cork: Writable.prototype.cork,
    uncork: Writable.prototype.uncork,
    setDefaultEncoding: Writable.prototype.setDefaultEncoding,
    _write: Writable.prototype._write,
    _doWrite: Writable.prototype._doWrite,
    _finish: Writable.prototype._finish,
  });

  // Transform stream
  class Transform extends Duplex {
    constructor(options = {}) {
      super(options);
      this._transformState = {
        transforming: false,
        pendingCallback: null,
      };

      if (typeof options.transform === "function") {
        this._transform = options.transform;
      }
      if (typeof options.flush === "function") {
        this._flush = options.flush;
      }
    }

    _transform(chunk, encoding, callback) {
      callback(null, chunk);
    }

    _write(chunk, encoding, callback) {
      const state = this._transformState;
      state.transforming = true;
      this._transform(chunk, encoding, (err, data) => {
        state.transforming = false;
        if (err) {
          callback(err);
          return;
        }
        if (data !== null && data !== undefined) {
          this.push(data);
        }
        callback();
      });
    }

    _read(size) {
      const state = this._transformState;
      if (state.pendingCallback) {
        const cb = state.pendingCallback;
        state.pendingCallback = null;
        cb();
      }
    }

    _final(callback) {
      if (this._flush) {
        this._flush((err, data) => {
          if (err) {
            callback(err);
            return;
          }
          if (data !== null && data !== undefined) {
            this.push(data);
          }
          this.push(null);
          callback();
        });
      } else {
        this.push(null);
        callback();
      }
    }
  }

  // PassThrough - simple transform that passes data through unchanged
  class PassThrough extends Transform {
    constructor(options) {
      super(options);
    }

    _transform(chunk, encoding, callback) {
      callback(null, chunk);
    }
  }

  /**
   * Pipeline multiple streams together.
   */
  function pipeline(...streams) {
    let callback = streams[streams.length - 1];
    if (typeof callback !== "function") {
      callback = () => {};
    } else {
      streams = streams.slice(0, -1);
    }

    if (streams.length < 2) {
      throw new Error("pipeline requires at least 2 streams");
    }

    let error;
    const destroyAll = (err) => {
      if (error) return;
      error = err;
      for (const stream of streams) {
        if (stream.destroy) stream.destroy(err);
      }
    };

    // Pipe streams together
    let current = streams[0];
    for (let i = 1; i < streams.length; i++) {
      const next = streams[i];
      current.on("error", destroyAll);
      current = current.pipe(next);
    }

    // Handle completion
    const last = streams[streams.length - 1];
    last.on("error", (err) => {
      destroyAll(err);
      callback(err);
    });
    last.on("finish", () => {
      if (!error) callback();
    });
    last.on("close", () => {
      if (!error) callback();
    });

    return last;
  }

  /**
   * Detect when a stream is finished.
   */
  function finished(stream, options, callback) {
    if (typeof options === "function") {
      callback = options;
      options = {};
    }
    options = options || {};

    const readable = options.readable !== false && stream.readable;
    const writable = options.writable !== false && stream.writable;

    let readableFinished = !readable;
    let writableFinished = !writable;

    const onFinish = () => {
      writableFinished = true;
      if (readableFinished) callback();
    };

    const onEnd = () => {
      readableFinished = true;
      if (writableFinished) callback();
    };

    const onError = (err) => {
      callback(err);
    };

    const onClose = () => {
      if (readable && !readableFinished) {
        callback(new Error("premature close"));
      } else if (writable && !writableFinished) {
        callback(new Error("premature close"));
      }
    };

    if (writable) stream.on("finish", onFinish);
    if (readable) stream.on("end", onEnd);
    stream.on("error", onError);
    stream.on("close", onClose);

    return () => {
      stream.off("finish", onFinish);
      stream.off("end", onEnd);
      stream.off("error", onError);
      stream.off("close", onClose);
    };
  }

  // Promisified versions
  const streamPromises = {
    pipeline(...streams) {
      return new Promise((resolve, reject) => {
        pipeline(...streams, (err) => {
          if (err) reject(err);
          else resolve();
        });
      });
    },
    finished(stream, options) {
      return new Promise((resolve, reject) => {
        finished(stream, options, (err) => {
          if (err) reject(err);
          else resolve();
        });
      });
    },
  };

  // Node.js's require('stream') returns the Stream constructor directly
  // with other classes as properties attached to it
  Stream.Stream = Stream;
  Stream.Readable = Readable;
  Stream.Writable = Writable;
  Stream.Duplex = Duplex;
  Stream.Transform = Transform;
  Stream.PassThrough = PassThrough;
  Stream.pipeline = pipeline;
  Stream.finished = finished;
  Stream.promises = streamPromises;

  globalThis.__howth_modules["node:stream"] = Stream;
  globalThis.__howth_modules["stream"] = Stream;
  globalThis.__howth_modules["node:stream/promises"] = streamPromises;
  globalThis.__howth_modules["stream/promises"] = streamPromises;

  // stream/web - Web Streams API for Node.js compatibility
  const streamWebModule = {
    ReadableStream: globalThis.ReadableStream,
    ReadableStreamDefaultReader: class ReadableStreamDefaultReader {},
    ReadableStreamBYOBReader: class ReadableStreamBYOBReader {},
    ReadableStreamDefaultController: class ReadableStreamDefaultController {},
    ReadableByteStreamController: class ReadableByteStreamController {},
    WritableStream: globalThis.WritableStream,
    WritableStreamDefaultWriter: class WritableStreamDefaultWriter {},
    WritableStreamDefaultController: class WritableStreamDefaultController {},
    TransformStream: globalThis.TransformStream,
    TransformStreamDefaultController: class TransformStreamDefaultController {},
    ByteLengthQueuingStrategy: globalThis.ByteLengthQueuingStrategy || class ByteLengthQueuingStrategy {
      constructor(init) { this.highWaterMark = init.highWaterMark; }
      size(chunk) { return chunk.byteLength; }
    },
    CountQueuingStrategy: globalThis.CountQueuingStrategy || class CountQueuingStrategy {
      constructor(init) { this.highWaterMark = init.highWaterMark; }
      size() { return 1; }
    },
    TextEncoderStream: globalThis.TextEncoderStream,
    TextDecoderStream: globalThis.TextDecoderStream,
  };

  globalThis.__howth_modules["node:stream/web"] = streamWebModule;
  globalThis.__howth_modules["stream/web"] = streamWebModule;

  // ============================================================================
  // crypto module
  // ============================================================================

  // Map Node.js algorithm names to Web Crypto names
  const hashAlgorithms = {
    md5: "MD5", // Note: MD5 not in Web Crypto, we'll implement it
    sha1: "SHA-1",
    "sha-1": "SHA-1",
    sha256: "SHA-256",
    "sha-256": "SHA-256",
    sha384: "SHA-384",
    "sha-384": "SHA-384",
    sha512: "SHA-512",
    "sha-512": "SHA-512",
  };

  /**
   * Generate random bytes.
   */
  function randomBytes(size, callback) {
    const bytes = new Uint8Array(size);
    crypto.getRandomValues(bytes);
    const buf = Buffer.from(bytes);
    if (callback) {
      process.nextTick(() => callback(null, buf));
      return;
    }
    return buf;
  }

  /**
   * Generate random bytes synchronously.
   */
  function randomBytesSync(size) {
    const bytes = new Uint8Array(size);
    crypto.getRandomValues(bytes);
    return Buffer.from(bytes);
  }

  /**
   * Generate a random UUID.
   */
  function randomUUID() {
    return crypto.randomUUID();
  }

  /**
   * Generate random integer in range.
   */
  function randomInt(min, max, callback) {
    if (typeof min === "function") {
      callback = min;
      min = 0;
      max = 2147483647;
    } else if (typeof max === "function") {
      callback = max;
      max = min;
      min = 0;
    } else if (max === undefined) {
      // randomInt(max) - single argument means 0 to max
      max = min;
      min = 0;
    }
    const range = max - min;
    const bytes = new Uint8Array(4);
    crypto.getRandomValues(bytes);
    const value = (bytes[0] | (bytes[1] << 8) | (bytes[2] << 16) | (bytes[3] << 24)) >>> 0;
    const result = min + (value % range);
    if (callback) {
      process.nextTick(() => callback(null, result));
      return;
    }
    return result;
  }

  /**
   * Fill buffer with random bytes.
   */
  function randomFill(buffer, offset, size, callback) {
    if (typeof offset === "function") {
      callback = offset;
      offset = 0;
      size = buffer.length;
    } else if (typeof size === "function") {
      callback = size;
      size = buffer.length - offset;
    }
    const bytes = new Uint8Array(size);
    crypto.getRandomValues(bytes);
    for (let i = 0; i < size; i++) {
      buffer[offset + i] = bytes[i];
    }
    if (callback) {
      process.nextTick(() => callback(null, buffer));
      return;
    }
    return buffer;
  }

  function randomFillSync(buffer, offset = 0, size = buffer.length - offset) {
    const bytes = new Uint8Array(size);
    crypto.getRandomValues(bytes);
    for (let i = 0; i < size; i++) {
      buffer[offset + i] = bytes[i];
    }
    return buffer;
  }

  /**
   * Simple MD5 implementation (not in Web Crypto).
   */
  function md5(data) {
    const bytes = typeof data === "string" ? new TextEncoder().encode(data) : new Uint8Array(data);

    function rotateLeft(x, n) { return ((x << n) | (x >>> (32 - n))) >>> 0; }
    function add32(a, b) { return (a + b) >>> 0; }

    const K = new Uint32Array([
      0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
      0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
      0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
      0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
      0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
      0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
      0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
      0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391
    ]);
    const S = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];

    // Padding
    const bitLen = bytes.length * 8;
    const padLen = (bytes.length % 64 < 56 ? 56 : 120) - (bytes.length % 64);
    const padded = new Uint8Array(bytes.length + padLen + 8);
    padded.set(bytes);
    padded[bytes.length] = 0x80;
    const view = new DataView(padded.buffer);
    view.setUint32(padded.length - 8, bitLen >>> 0, true);
    view.setUint32(padded.length - 4, Math.floor(bitLen / 0x100000000), true);

    let a0 = 0x67452301, b0 = 0xefcdab89, c0 = 0x98badcfe, d0 = 0x10325476;

    for (let i = 0; i < padded.length; i += 64) {
      const M = new Uint32Array(16);
      for (let j = 0; j < 16; j++) {
        M[j] = view.getUint32(i + j * 4, true);
      }

      let A = a0, B = b0, C = c0, D = d0;

      for (let j = 0; j < 64; j++) {
        let F, g;
        if (j < 16) { F = (B & C) | (~B & D); g = j; }
        else if (j < 32) { F = (D & B) | (~D & C); g = (5 * j + 1) % 16; }
        else if (j < 48) { F = B ^ C ^ D; g = (3 * j + 5) % 16; }
        else { F = C ^ (B | ~D); g = (7 * j) % 16; }

        F = add32(add32(add32(F >>> 0, A), K[j]), M[g]);
        A = D; D = C; C = B;
        B = add32(B, rotateLeft(F, S[Math.floor(j / 16) * 4 + (j % 4)]));
      }

      a0 = add32(a0, A); b0 = add32(b0, B); c0 = add32(c0, C); d0 = add32(d0, D);
    }

    const result = new Uint8Array(16);
    const resultView = new DataView(result.buffer);
    resultView.setUint32(0, a0, true);
    resultView.setUint32(4, b0, true);
    resultView.setUint32(8, c0, true);
    resultView.setUint32(12, d0, true);
    return result;
  }

  /**
   * Hash class for streaming hash computation.
   */
  class Hash {
    #algorithm;
    #data = [];

    constructor(algorithm) {
      this.#algorithm = algorithm.toLowerCase();
      if (!hashAlgorithms[this.#algorithm] && this.#algorithm !== "md5") {
        throw new Error(`Digest method not supported: ${algorithm}`);
      }
    }

    update(data, encoding) {
      if (typeof data === "string") {
        data = Buffer.from(data, encoding || "utf8");
      }
      this.#data.push(data);
      return this;
    }

    digest(encoding) {
      const combined = Buffer.concat(this.#data);
      let hash;

      if (this.#algorithm === "md5") {
        hash = md5(combined);
      } else {
        // Use native op for synchronous hashing
        const webAlgo = hashAlgorithms[this.#algorithm] || this.#algorithm;
        const algoName = typeof webAlgo === 'object' ? webAlgo.name : webAlgo;
        try {
          hash = ops.op_howth_hash(algoName, combined);
        } catch (e) {
          // Fallback to a simple hash if the algorithm isn't supported
          hash = md5(combined);
        }
      }

      const result = Buffer.from(hash);
      if (encoding === "hex") return result.toString("hex");
      if (encoding === "base64") return result.toString("base64");
      return result;
    }

    copy() {
      const copy = new Hash(this.#algorithm);
      copy.#data = [...this.#data];
      return copy;
    }
  }

  /**
   * Create a hash object.
   */
  function createHash(algorithm) {
    return new Hash(algorithm);
  }

  /**
   * Hmac class for streaming HMAC computation.
   */
  class Hmac {
    #algorithm;
    #key;
    #data = [];

    constructor(algorithm, key) {
      this.#algorithm = algorithm.toLowerCase();
      if (!hashAlgorithms[this.#algorithm]) {
        throw new Error(`Digest method not supported: ${algorithm}`);
      }
      this.#key = typeof key === "string" ? Buffer.from(key) : key;
    }

    update(data, encoding) {
      if (typeof data === "string") {
        data = Buffer.from(data, encoding || "utf8");
      }
      this.#data.push(data);
      return this;
    }

    digest(encoding) {
      const combined = Buffer.concat(this.#data);
      const algoName = hashAlgorithms[this.#algorithm] || this.#algorithm;
      const hash = ops.op_howth_hmac(algoName, this.#key, combined);
      const result = Buffer.from(hash);
      if (encoding === "hex") return result.toString("hex");
      if (encoding === "base64") return result.toString("base64");
      if (encoding === "base64url") {
        return result.toString("base64").replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
      }
      return result;
    }
  }

  /**
   * Create an HMAC object.
   */
  function createHmac(algorithm, key) {
    return new Hmac(algorithm, key);
  }

  /**
   * Constant-time comparison of two buffers.
   */
  function timingSafeEqual(a, b) {
    if (a.length !== b.length) {
      throw new RangeError("Input buffers must have the same byte length");
    }
    let result = 0;
    for (let i = 0; i < a.length; i++) {
      result |= a[i] ^ b[i];
    }
    return result === 0;
  }

  /**
   * Get list of supported hash algorithms.
   */
  function getHashes() {
    return ["md5", "sha1", "sha256", "sha384", "sha512"];
  }

  /**
   * Get list of supported ciphers (stub).
   */
  function getCiphers() {
    return ["aes-128-cbc", "aes-256-cbc", "aes-128-ctr", "aes-256-ctr", "aes-128-gcm", "aes-256-gcm"];
  }

  /**
   * Cipher class for symmetric encryption.
   */
  class Cipher {
    #algorithm;
    #key;
    #iv;
    #chunks = [];
    #aad = null;
    #authTag = null;
    #isGcm;

    constructor(algorithm, key, iv) {
      this.#algorithm = algorithm.toLowerCase();
      this.#key = typeof key === "string" ? Buffer.from(key) : key;
      this.#iv = typeof iv === "string" ? Buffer.from(iv) : iv;
      this.#isGcm = this.#algorithm.includes("gcm");
    }

    setAAD(buffer) {
      this.#aad = typeof buffer === "string" ? Buffer.from(buffer) : buffer;
      return this;
    }

    getAuthTag() {
      if (!this.#authTag) {
        throw new Error("Auth tag not available. Has final() been called?");
      }
      return Buffer.from(this.#authTag);
    }

    update(data, inputEncoding, outputEncoding) {
      if (typeof data === "string") {
        data = Buffer.from(data, inputEncoding || "utf8");
      }
      this.#chunks.push(data);
      // For streaming compat, return empty buffer on update (data is accumulated)
      const empty = Buffer.alloc(0);
      if (outputEncoding === "hex") return "";
      if (outputEncoding === "base64") return "";
      return empty;
    }

    final(outputEncoding) {
      const combined = Buffer.concat(this.#chunks);
      let result;

      if (this.#isGcm) {
        const aad = this.#aad || Buffer.alloc(0);
        const tag = Buffer.alloc(0);
        const [ct, authTag] = ops.op_howth_cipher_gcm(
          this.#algorithm, this.#key, this.#iv, combined, aad, tag, true
        );
        result = Buffer.from(ct);
        this.#authTag = authTag;
      } else {
        result = Buffer.from(ops.op_howth_cipher(
          this.#algorithm, this.#key, this.#iv, combined, true
        ));
      }

      if (outputEncoding === "hex") return result.toString("hex");
      if (outputEncoding === "base64") return result.toString("base64");
      return result;
    }
  }

  /**
   * Decipher class for symmetric decryption.
   */
  class Decipher {
    #algorithm;
    #key;
    #iv;
    #chunks = [];
    #aad = null;
    #authTag = null;
    #isGcm;

    constructor(algorithm, key, iv) {
      this.#algorithm = algorithm.toLowerCase();
      this.#key = typeof key === "string" ? Buffer.from(key) : key;
      this.#iv = typeof iv === "string" ? Buffer.from(iv) : iv;
      this.#isGcm = this.#algorithm.includes("gcm");
    }

    setAAD(buffer) {
      this.#aad = typeof buffer === "string" ? Buffer.from(buffer) : buffer;
      return this;
    }

    setAuthTag(tag) {
      this.#authTag = typeof tag === "string" ? Buffer.from(tag) : tag;
      return this;
    }

    update(data, inputEncoding, outputEncoding) {
      if (typeof data === "string") {
        data = Buffer.from(data, inputEncoding || "hex");
      }
      this.#chunks.push(data);
      const empty = Buffer.alloc(0);
      if (outputEncoding === "hex") return "";
      if (outputEncoding === "utf8" || outputEncoding === "utf-8") return "";
      return empty;
    }

    final(outputEncoding) {
      const combined = Buffer.concat(this.#chunks);
      let result;

      if (this.#isGcm) {
        const aad = this.#aad || Buffer.alloc(0);
        const tag = this.#authTag || Buffer.alloc(16);
        const [pt] = ops.op_howth_cipher_gcm(
          this.#algorithm, this.#key, this.#iv, combined, aad, tag, false
        );
        result = Buffer.from(pt);
      } else {
        result = Buffer.from(ops.op_howth_cipher(
          this.#algorithm, this.#key, this.#iv, combined, false
        ));
      }

      if (outputEncoding === "hex") return result.toString("hex");
      if (outputEncoding === "utf8" || outputEncoding === "utf-8") return result.toString("utf8");
      if (outputEncoding === "base64") return result.toString("base64");
      return result;
    }
  }

  function createCipheriv(algorithm, key, iv) {
    return new Cipher(algorithm, key, iv);
  }

  function createDecipheriv(algorithm, key, iv) {
    return new Decipher(algorithm, key, iv);
  }

  /**
   * RSA sign.
   */
  function sign(algorithm, data, key) {
    if (typeof data === "string") data = Buffer.from(data);
    let keyPem;
    if (typeof key === "string") {
      keyPem = key;
    } else if (key && typeof key === "object" && key.key) {
      keyPem = typeof key.key === "string" ? key.key : key.key.export({ type: "pkcs8", format: "pem" });
    } else if (Buffer.isBuffer(key)) {
      keyPem = key.toString("utf8");
    } else if (key && typeof key.export === "function") {
      keyPem = key.export({ type: "pkcs8", format: "pem" });
    } else {
      throw new TypeError("Invalid key argument");
    }
    const algo = algorithm ? algorithm.toLowerCase().replace("-", "") : "sha256";
    const sig = ops.op_howth_sign(algo, keyPem, data);
    return Buffer.from(sig);
  }

  /**
   * RSA verify.
   */
  function verify(algorithm, data, key, signature) {
    if (typeof data === "string") data = Buffer.from(data);
    let keyPem;
    if (typeof key === "string") {
      keyPem = key;
    } else if (key && typeof key === "object" && key.key) {
      keyPem = typeof key.key === "string" ? key.key : key.key.export({ type: "spki", format: "pem" });
    } else if (Buffer.isBuffer(key)) {
      keyPem = key.toString("utf8");
    } else if (key && typeof key.export === "function") {
      keyPem = key.export({ type: "spki", format: "pem" });
    } else {
      throw new TypeError("Invalid key argument");
    }
    const algo = algorithm ? algorithm.toLowerCase().replace("-", "") : "sha256";
    return ops.op_howth_verify(algo, keyPem, signature, data);
  }

  /**
   * RSA public encrypt (OAEP with SHA-1).
   */
  function publicEncrypt(key, buffer) {
    let keyPem;
    if (typeof key === "string") {
      keyPem = key;
    } else if (key && typeof key === "object" && key.key) {
      keyPem = typeof key.key === "string" ? key.key : key.key.toString("utf8");
    } else if (Buffer.isBuffer(key)) {
      keyPem = key.toString("utf8");
    } else if (key && typeof key.export === "function") {
      keyPem = key.export({ type: "spki", format: "pem" });
    } else {
      throw new TypeError("Invalid key argument");
    }
    return Buffer.from(ops.op_howth_public_encrypt(keyPem, buffer));
  }

  /**
   * RSA private decrypt (OAEP with SHA-1).
   */
  function privateDecrypt(key, buffer) {
    let keyPem;
    if (typeof key === "string") {
      keyPem = key;
    } else if (key && typeof key === "object" && key.key) {
      keyPem = typeof key.key === "string" ? key.key : key.key.toString("utf8");
    } else if (Buffer.isBuffer(key)) {
      keyPem = key.toString("utf8");
    } else if (key && typeof key.export === "function") {
      keyPem = key.export({ type: "pkcs8", format: "pem" });
    } else {
      throw new TypeError("Invalid key argument");
    }
    return Buffer.from(ops.op_howth_private_decrypt(keyPem, buffer));
  }

  /**
   * Generate an RSA key pair synchronously.
   */
  function generateKeyPairSync(type, options = {}) {
    if (type !== "rsa") {
      throw new Error(`Unsupported key type: ${type}. Only 'rsa' is supported.`);
    }
    const modulusLength = options.modulusLength || 2048;
    const [publicPem, privatePem] = ops.op_howth_generate_rsa_keypair(modulusLength);

    const pubKeyObj = {
      type: "public",
      asymmetricKeyType: "rsa",
      export(opts = {}) {
        if (opts.format === "pem") return publicPem;
        return Buffer.from(publicPem);
      },
      toString() { return publicPem; },
    };
    const privKeyObj = {
      type: "private",
      asymmetricKeyType: "rsa",
      export(opts = {}) {
        if (opts.format === "pem") return privatePem;
        return Buffer.from(privatePem);
      },
      toString() { return privatePem; },
    };

    if (options.publicKeyEncoding && options.privateKeyEncoding) {
      return {
        publicKey: publicPem,
        privateKey: privatePem,
      };
    }

    return {
      publicKey: pubKeyObj,
      privateKey: privKeyObj,
    };
  }

  /**
   * PBKDF2 key derivation.
   */
  async function pbkdf2(password, salt, iterations, keylen, digest, callback) {
    try {
      const enc = new TextEncoder();
      const pwKey = await crypto.subtle.importKey(
        "raw",
        typeof password === "string" ? enc.encode(password) : password,
        "PBKDF2",
        false,
        ["deriveBits"]
      );
      const webDigest = hashAlgorithms[digest.toLowerCase()] || "SHA-256";
      const bits = await crypto.subtle.deriveBits(
        {
          name: "PBKDF2",
          salt: typeof salt === "string" ? enc.encode(salt) : salt,
          iterations,
          hash: webDigest,
        },
        pwKey,
        keylen * 8
      );
      const result = Buffer.from(new Uint8Array(bits));
      if (callback) callback(null, result);
      return result;
    } catch (err) {
      if (callback) callback(err);
      throw err;
    }
  }

  function pbkdf2Sync(password, salt, iterations, keylen, digest) {
    // Sync version - would need native implementation for true sync
    throw new Error("pbkdf2Sync requires async. Use pbkdf2() instead.");
  }

  /**
   * Scrypt key derivation (stub).
   */
  function scrypt(password, salt, keylen, options, callback) {
    if (typeof options === "function") {
      callback = options;
      options = {};
    }
    callback(new Error("scrypt not implemented"));
  }

  function scryptSync() {
    throw new Error("scryptSync not implemented");
  }

  const cryptoModule = {
    // Random
    randomBytes,
    randomBytesSync,
    randomUUID,
    randomInt,
    randomFill,
    randomFillSync,
    getRandomValues: (buffer) => crypto.getRandomValues(buffer),

    // Hashing
    createHash,
    createHmac,
    getHashes,
    Hash,
    Hmac,

    // Utility
    timingSafeEqual,

    // Key derivation
    pbkdf2,
    pbkdf2Sync,
    scrypt,
    scryptSync,

    // Ciphers
    getCiphers,
    createCipheriv,
    createDecipheriv,
    Cipher,
    Decipher,

    // Signing
    sign,
    verify,

    // RSA encrypt/decrypt
    publicEncrypt,
    privateDecrypt,

    // Key generation
    generateKeyPairSync,

    // Web Crypto access
    subtle: crypto.subtle,
    webcrypto: crypto,

    // Constants
    constants: {
      OPENSSL_VERSION_NUMBER: 0,
      SSL_OP_ALL: 0,
      RSA_PKCS1_PADDING: 1,
      RSA_PKCS1_OAEP_PADDING: 4,
    },
  };

  globalThis.__howth_modules["node:crypto"] = cryptoModule;
  globalThis.__howth_modules["crypto"] = cryptoModule;

  // ============================================================================
  // child_process module
  // ============================================================================

  /**
   * Execute a command synchronously in a shell.
   * @param {string} command - The command to run
   * @param {Object} options - Options object
   * @returns {Buffer|string} - stdout output
   */
  function execSync(command, options = {}) {
    const result = ops.op_howth_exec_sync(command, options);

    if (result.error) {
      const err = new Error(result.error);
      err.status = result.status;
      err.stdout = Buffer.from(result.stdout);
      err.stderr = Buffer.from(result.stderr);
      throw err;
    }

    if (result.status !== 0) {
      const err = new Error(
        `Command failed: ${command}\n${result.stderr}`
      );
      err.status = result.status;
      err.stdout = Buffer.from(result.stdout);
      err.stderr = Buffer.from(result.stderr);
      throw err;
    }

    if (options.encoding === "buffer" || options.encoding === undefined) {
      return Buffer.from(result.stdout);
    }
    return result.stdout;
  }

  /**
   * Execute a file synchronously.
   * @param {string} file - The file to execute
   * @param {string[]} args - Arguments
   * @param {Object} options - Options object
   * @returns {Buffer|string} - stdout output
   */
  function execFileSync(file, args = [], options = {}) {
    if (typeof args === "object" && !Array.isArray(args)) {
      options = args;
      args = [];
    }

    const result = ops.op_howth_spawn_sync(file, args, {
      ...options,
      shell: false,
    });

    if (result.error) {
      const err = new Error(result.error);
      err.status = result.status;
      err.stdout = Buffer.from(result.stdout);
      err.stderr = Buffer.from(result.stderr);
      throw err;
    }

    if (result.status !== 0) {
      const err = new Error(
        `Command failed: ${file} ${args.join(" ")}\n${result.stderr}`
      );
      err.status = result.status;
      err.stdout = Buffer.from(result.stdout);
      err.stderr = Buffer.from(result.stderr);
      throw err;
    }

    if (options.encoding === "buffer" || options.encoding === undefined) {
      return Buffer.from(result.stdout);
    }
    return result.stdout;
  }

  /**
   * Spawn a process synchronously.
   * @param {string} command - The command to run
   * @param {string[]} args - Arguments
   * @param {Object} options - Options object
   * @returns {Object} - Result object with status, stdout, stderr
   */
  function spawnSync(command, args = [], options = {}) {
    if (typeof args === "object" && !Array.isArray(args)) {
      options = args;
      args = [];
    }

    const result = ops.op_howth_spawn_sync(command, args, options);

    return {
      pid: 0, // We don't have the real PID in sync mode
      output: [null, Buffer.from(result.stdout), Buffer.from(result.stderr)],
      stdout: Buffer.from(result.stdout),
      stderr: Buffer.from(result.stderr),
      status: result.status,
      signal: null,
      error: result.error ? new Error(result.error) : undefined,
    };
  }

  /**
   * Execute a command asynchronously in a shell (callback-based).
   * @param {string} command - The command to run
   * @param {Object} options - Options object
   * @param {Function} callback - Callback function(error, stdout, stderr)
   * @returns {ChildProcess} - ChildProcess instance
   */
  function exec(command, options, callback) {
    if (typeof options === "function") {
      callback = options;
      options = {};
    }

    // Run synchronously but call callback asynchronously to match Node.js behavior
    process.nextTick(() => {
      try {
        const result = ops.op_howth_exec_sync(command, options || {});

        if (result.error) {
          const err = new Error(result.error);
          err.killed = false;
          err.code = result.status;
          err.signal = null;
          err.cmd = command;
          if (callback) callback(err, result.stdout, result.stderr);
          return;
        }

        if (result.status !== 0) {
          const err = new Error(`Command failed: ${command}\n${result.stderr}`);
          err.killed = false;
          err.code = result.status;
          err.signal = null;
          err.cmd = command;
          if (callback) callback(err, result.stdout, result.stderr);
          return;
        }

        if (callback) callback(null, result.stdout, result.stderr);
      } catch (e) {
        if (callback) callback(e, "", "");
      }
    });

    // Return a minimal ChildProcess-like object
    return {
      pid: 0,
      stdin: null,
      stdout: null,
      stderr: null,
      kill() {},
    };
  }

  /**
   * Execute a file asynchronously (callback-based).
   * @param {string} file - The file to execute
   * @param {string[]} args - Arguments
   * @param {Object} options - Options object
   * @param {Function} callback - Callback function(error, stdout, stderr)
   * @returns {ChildProcess} - ChildProcess instance
   */
  function execFile(file, args, options, callback) {
    if (typeof args === "function") {
      callback = args;
      args = [];
      options = {};
    } else if (typeof options === "function") {
      callback = options;
      options = {};
    }

    if (typeof args === "object" && !Array.isArray(args)) {
      options = args;
      args = [];
    }

    process.nextTick(() => {
      try {
        const result = ops.op_howth_spawn_sync(file, args || [], {
          ...(options || {}),
          shell: false,
        });

        if (result.error) {
          const err = new Error(result.error);
          err.killed = false;
          err.code = result.status;
          err.signal = null;
          err.cmd = file;
          if (callback) callback(err, result.stdout, result.stderr);
          return;
        }

        if (result.status !== 0) {
          const err = new Error(
            `Command failed: ${file} ${(args || []).join(" ")}\n${result.stderr}`
          );
          err.killed = false;
          err.code = result.status;
          err.signal = null;
          err.cmd = file;
          if (callback) callback(err, result.stdout, result.stderr);
          return;
        }

        if (callback) callback(null, result.stdout, result.stderr);
      } catch (e) {
        if (callback) callback(e, "", "");
      }
    });

    return {
      pid: 0,
      stdin: null,
      stdout: null,
      stderr: null,
      kill() {},
    };
  }

  /**
   * Spawn a new process (simplified - runs synchronously internally).
   * @param {string} command - The command to run
   * @param {string[]} args - Arguments
   * @param {Object} options - Options object
   * @returns {ChildProcess} - ChildProcess instance
   */
  function spawn(command, args = [], options = {}) {
    if (typeof args === "object" && !Array.isArray(args)) {
      options = args;
      args = [];
    }

    // Shared state for deferred operations
    let childId = null;
    let childReady = null;
    const childReadyPromise = new Promise((resolve) => {
      childReady = resolve;
    });

    // Create event emitter functionality
    const listeners = new Map();

    function emit(event, ...eventArgs) {
      const eventListeners = listeners.get(event);
      if (eventListeners) {
        for (const listener of [...eventListeners]) {
          listener(...eventArgs);
        }
      }
    }

    // Create a Readable stream for stdout/stderr (deferred until child is ready)
    function createReadableStream(readOp) {
      const streamListeners = new Map();
      let reading = false;
      let ended = false;

      const stream = {
        readable: true,

        on(event, listener) {
          if (!streamListeners.has(event)) {
            streamListeners.set(event, []);
          }
          streamListeners.get(event).push(listener);

          // Start reading when someone listens for 'data'
          if (event === "data" && !reading) {
            reading = true;
            // Wait for child to be ready before starting read loop
            childReadyPromise.then(() => {
              if (childId !== null) readLoop();
            });
          }
          return this;
        },

        once(event, listener) {
          const onceWrapper = (...args) => {
            this.off(event, onceWrapper);
            listener(...args);
          };
          return this.on(event, onceWrapper);
        },

        off(event, listener) {
          const evListeners = streamListeners.get(event);
          if (evListeners) {
            const idx = evListeners.indexOf(listener);
            if (idx !== -1) evListeners.splice(idx, 1);
          }
          return this;
        },

        emit(event, ...args) {
          const evListeners = streamListeners.get(event);
          if (evListeners) {
            for (const l of [...evListeners]) {
              l(...args);
            }
          }
        },

        pipe(dest) {
          this.on("data", (chunk) => dest.write(chunk));
          this.on("end", () => dest.end && dest.end());
          return dest;
        },

        setEncoding(enc) {
          // TODO: implement encoding
          return this;
        },
      };

      async function readLoop() {
        while (!ended) {
          try {
            const data = await readOp(childId);
            if (data === null || data === undefined) {
              ended = true;
              stream.emit("end");
              break;
            }
            // Convert to Buffer
            const buf = Buffer.from(data);
            stream.emit("data", buf);
          } catch (e) {
            stream.emit("error", e);
            break;
          }
        }
      }

      return stream;
    }

    // Create a Writable stream for stdin (uses closure childId)
    function createWritableStream() {
      const streamListeners = new Map();
      let ended = false;
      const pendingWrites = [];

      const stream = {
        writable: true,

        on(event, listener) {
          if (!streamListeners.has(event)) {
            streamListeners.set(event, []);
          }
          streamListeners.get(event).push(listener);
          return this;
        },

        off(event, listener) {
          const evListeners = streamListeners.get(event);
          if (evListeners) {
            const idx = evListeners.indexOf(listener);
            if (idx !== -1) evListeners.splice(idx, 1);
          }
          return this;
        },

        emit(event, ...args) {
          const evListeners = streamListeners.get(event);
          if (evListeners) {
            for (const l of [...evListeners]) {
              l(...args);
            }
          }
        },

        write(chunk, encoding, callback) {
          if (ended) return false;
          if (typeof encoding === "function") {
            callback = encoding;
            encoding = undefined;
          }

          const data = typeof chunk === "string" ? [...Buffer.from(chunk)] : [...chunk];

          // Wait for child to be ready
          childReadyPromise.then(() => {
            if (childId === null) {
              if (callback) callback(new Error("Process not started"));
              return;
            }
            ops.op_howth_spawn_write_stdin(childId, data)
              .then((n) => {
                if (callback) callback();
              })
              .catch((e) => {
                this.emit("error", e);
                if (callback) callback(e);
              });
          });

          return true;
        },

        end(chunk, encoding, callback) {
          if (chunk) {
            this.write(chunk, encoding);
          }
          ended = true;
          childReadyPromise.then(() => {
            if (childId === null) {
              if (callback) callback();
              return;
            }
            ops.op_howth_spawn_close_stdin(childId)
              .then(() => {
                this.emit("finish");
                if (callback) callback();
              })
              .catch((e) => {
                if (callback) callback(e);
              });
          });
        },
      };

      return stream;
    }

    // Create streams immediately (they will wait for childId via promise)
    const stdoutStream = createReadableStream(ops.op_howth_spawn_read_stdout);
    const stderrStream = createReadableStream(ops.op_howth_spawn_read_stderr);
    const stdinStream = createWritableStream();

    // The ChildProcess object
    const child = {
      _id: null,
      pid: 0,
      stdin: stdinStream,
      stdout: stdoutStream,
      stderr: stderrStream,
      killed: false,
      exitCode: null,
      signalCode: null,
      connected: false,
      spawnfile: command,
      spawnargs: [command, ...args],

      on(event, listener) {
        if (!listeners.has(event)) {
          listeners.set(event, []);
        }
        listeners.get(event).push(listener);
        return this;
      },

      once(event, listener) {
        const onceListener = (...eventArgs) => {
          this.off(event, onceListener);
          listener(...eventArgs);
        };
        return this.on(event, onceListener);
      },

      off(event, listener) {
        const eventListeners = listeners.get(event);
        if (eventListeners) {
          const idx = eventListeners.indexOf(listener);
          if (idx !== -1) eventListeners.splice(idx, 1);
        }
        return this;
      },

      removeListener(event, listener) {
        return this.off(event, listener);
      },

      emit,

      kill(signal = "SIGTERM") {
        if (this._id === null) return false;
        this.killed = true;
        try {
          return ops.op_howth_spawn_kill(this._id, signal);
        } catch (e) {
          return false;
        }
      },

      ref() {
        return this;
      },

      unref() {
        return this;
      },
    };

    // Spawn the process asynchronously
    (async () => {
      try {
        const result = await ops.op_howth_spawn_async(command, args, options);

        if (result.error) {
          const err = new Error(result.error);
          err.code = "ENOENT";
          err.syscall = "spawn " + command;
          err.path = command;
          childReady(); // Signal that spawn failed
          process.nextTick(() => emit("error", err));
          return;
        }

        // Set the child ID (used by streams via closure)
        childId = result.id;
        child._id = result.id;
        child.pid = result.pid;

        // Signal that child is ready (streams can start reading)
        childReady();

        // Emit spawn event
        process.nextTick(() => emit("spawn"));

        // Wait for the process to exit
        const exitResult = await ops.op_howth_spawn_wait(result.id);

        child.exitCode = exitResult.code;
        child.signalCode = exitResult.signal || null;

        emit("close", exitResult.code, exitResult.signal || null);
        emit("exit", exitResult.code, exitResult.signal || null);
      } catch (e) {
        emit("error", e);
        emit("close", 1, null);
        emit("exit", 1, null);
      }
    })();

    return child;
  }

  // The child_process module
  const childProcessModule = {
    exec,
    execSync,
    execFile,
    execFileSync,
    spawn,
    spawnSync,
    // fork - spawn a new Node.js process with IPC support
    fork(modulePath, args = [], options = {}) {
      // Handle overloaded signatures
      if (Array.isArray(args)) {
        // fork(modulePath, args, options)
      } else if (typeof args === 'object' && args !== null) {
        // fork(modulePath, options)
        options = args;
        args = [];
      }

      // Get the howth executable path
      const execPath = options.execPath || process.execPath || 'howth';

      // Build command arguments
      // Format: howth run --native <module> -- <args>
      const execArgs = options.execArgv || process.execArgv || [];
      const runArgs = args.length > 0
        ? ['run', '--native', modulePath, '--', ...args]
        : ['run', '--native', modulePath];
      const allArgs = [...execArgs, ...runArgs];

      // Create IPC files for bidirectional communication
      const ipcId = `${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
      const tmpDir = process.env.TMPDIR || '/tmp';
      const parentToChildPath = `${tmpDir}/howth-ipc-p2c-${ipcId}.jsonl`;
      const childToParentPath = `${tmpDir}/howth-ipc-c2p-${ipcId}.jsonl`;

      // Initialize IPC files
      const fs = globalThis.__howth_modules["fs"];
      fs.writeFileSync(parentToChildPath, '');
      fs.writeFileSync(childToParentPath, '');

      // Set up environment with IPC paths
      const env = {
        ...process.env,
        ...options.env,
        FORK_CHILD_PROCESS_ID: String(Date.now()),
        HOWTH_IPC_P2C: parentToChildPath,
        HOWTH_IPC_C2P: childToParentPath,
      };

      // Spawn the child process
      const spawnOptions = {
        cwd: options.cwd || process.cwd(),
        env,
        stdio: options.stdio || ['pipe', 'pipe', 'pipe'],
        detached: options.detached || false,
      };

      const child = spawn(execPath, allArgs, spawnOptions);

      // IPC state
      let connected = true;
      let lastReadPosition = 0;
      let pollInterval = null;
      const messageListeners = [];

      // Poll for messages from child
      const pollMessages = () => {
        if (!connected) return;
        try {
          const content = fs.readFileSync(childToParentPath, 'utf8');
          if (content.length > lastReadPosition) {
            const newContent = content.slice(lastReadPosition);
            lastReadPosition = content.length;

            // Parse JSON lines
            const lines = newContent.split('\n').filter(l => l.trim());
            for (const line of lines) {
              try {
                const msg = JSON.parse(line);
                // Emit 'message' event to our listeners only
                for (const listener of messageListeners) {
                  listener(msg);
                }
              } catch (e) {
                // Ignore parse errors
              }
            }
          }
        } catch (e) {
          // File might not exist yet
        }
      };

      // Start polling
      pollInterval = setInterval(pollMessages, 50);

      // Override on() to capture 'message' listeners (don't pass to origOn for 'message')
      const origOn = child.on.bind(child);
      child.on = (event, listener) => {
        if (event === 'message') {
          messageListeners.push(listener);
          return child; // Return child for chaining
        }
        return origOn(event, listener);
      };

      // Add fork-specific IPC methods
      child.connected = true;

      child.send = (message, sendHandle, options, callback) => {
        // Handle overloaded signatures
        if (typeof sendHandle === 'function') {
          callback = sendHandle;
          sendHandle = undefined;
        } else if (typeof options === 'function') {
          callback = options;
          options = undefined;
        }

        if (!connected) {
          if (callback) callback(new Error('Channel closed'));
          return false;
        }

        try {
          fs.appendFileSync(parentToChildPath, JSON.stringify(message) + '\n');
          if (callback) callback(null);
          return true;
        } catch (e) {
          if (callback) callback(e);
          return false;
        }
      };

      child.disconnect = () => {
        if (!connected) return;
        connected = false;
        child.connected = false;
        if (pollInterval) {
          clearInterval(pollInterval);
          pollInterval = null;
        }
        // Clean up IPC files
        try {
          fs.unlinkSync(parentToChildPath);
          fs.unlinkSync(childToParentPath);
        } catch (e) {
          // Ignore cleanup errors
        }
        child.emit && child.emit('disconnect');
      };

      // Clean up on exit
      const origClose = child.emit;
      child.emit = (event, ...eventArgs) => {
        if (event === 'exit' || event === 'close') {
          child.disconnect();
        }
        return origClose.call(child, event, ...eventArgs);
      };

      return child;
    },
  };

  // Register the child_process module
  globalThis.__howth_modules["node:child_process"] = childProcessModule;
  globalThis.__howth_modules["child_process"] = childProcessModule;

  // ============================================================================
  // os module
  // ============================================================================

  /**
   * Get the operating system's CPU architecture.
   */
  function arch() {
    return process.arch;
  }

  /**
   * Get operating system constants.
   */
  const osConstants = {
    UV_UDP_REUSEADDR: 4,
    dlopen: {},
    errno: {
      E2BIG: 7,
      EACCES: 13,
      EADDRINUSE: 48,
      EADDRNOTAVAIL: 49,
      EAFNOSUPPORT: 47,
      EAGAIN: 35,
      EALREADY: 37,
      EBADF: 9,
      EBADMSG: 94,
      EBUSY: 16,
      ECANCELED: 89,
      ECHILD: 10,
      ECONNABORTED: 53,
      ECONNREFUSED: 61,
      ECONNRESET: 54,
      EDEADLK: 11,
      EDESTADDRREQ: 39,
      EDOM: 33,
      EDQUOT: 69,
      EEXIST: 17,
      EFAULT: 14,
      EFBIG: 27,
      EHOSTUNREACH: 65,
      EIDRM: 90,
      EILSEQ: 92,
      EINPROGRESS: 36,
      EINTR: 4,
      EINVAL: 22,
      EIO: 5,
      EISCONN: 56,
      EISDIR: 21,
      ELOOP: 62,
      EMFILE: 24,
      EMLINK: 31,
      EMSGSIZE: 40,
      EMULTIHOP: 95,
      ENAMETOOLONG: 63,
      ENETDOWN: 50,
      ENETRESET: 52,
      ENETUNREACH: 51,
      ENFILE: 23,
      ENOBUFS: 55,
      ENODATA: 96,
      ENODEV: 19,
      ENOENT: 2,
      ENOEXEC: 8,
      ENOLCK: 77,
      ENOLINK: 97,
      ENOMEM: 12,
      ENOMSG: 91,
      ENOPROTOOPT: 42,
      ENOSPC: 28,
      ENOSR: 98,
      ENOSTR: 99,
      ENOSYS: 78,
      ENOTCONN: 57,
      ENOTDIR: 20,
      ENOTEMPTY: 66,
      ENOTSOCK: 38,
      ENOTSUP: 45,
      ENOTTY: 25,
      ENXIO: 6,
      EOPNOTSUPP: 102,
      EOVERFLOW: 84,
      EPERM: 1,
      EPIPE: 32,
      EPROTO: 100,
      EPROTONOSUPPORT: 43,
      EPROTOTYPE: 41,
      ERANGE: 34,
      EROFS: 30,
      ESPIPE: 29,
      ESRCH: 3,
      ESTALE: 70,
      ETIME: 101,
      ETIMEDOUT: 60,
      ETXTBSY: 26,
      EWOULDBLOCK: 35,
      EXDEV: 18,
    },
    signals: {
      SIGHUP: 1,
      SIGINT: 2,
      SIGQUIT: 3,
      SIGILL: 4,
      SIGTRAP: 5,
      SIGABRT: 6,
      SIGIOT: 6,
      SIGBUS: 10,
      SIGFPE: 8,
      SIGKILL: 9,
      SIGUSR1: 30,
      SIGSEGV: 11,
      SIGUSR2: 31,
      SIGPIPE: 13,
      SIGALRM: 14,
      SIGTERM: 15,
      SIGCHLD: 20,
      SIGCONT: 19,
      SIGSTOP: 17,
      SIGTSTP: 18,
      SIGTTIN: 21,
      SIGTTOU: 22,
      SIGURG: 16,
      SIGXCPU: 24,
      SIGXFSZ: 25,
      SIGVTALRM: 26,
      SIGPROF: 27,
      SIGWINCH: 28,
      SIGIO: 23,
      SIGINFO: 29,
      SIGSYS: 12,
    },
    priority: {
      PRIORITY_LOW: 19,
      PRIORITY_BELOW_NORMAL: 10,
      PRIORITY_NORMAL: 0,
      PRIORITY_ABOVE_NORMAL: -7,
      PRIORITY_HIGH: -14,
      PRIORITY_HIGHEST: -20,
    },
  };

  /**
   * Get CPU information.
   */
  function cpus() {
    // Return a basic CPU info array
    const numCpus = (typeof navigator !== 'undefined' && navigator.hardwareConcurrency) || 4;
    const cpuInfo = [];
    for (let i = 0; i < numCpus; i++) {
      cpuInfo.push({
        model: "Unknown CPU",
        speed: 2400, // MHz (placeholder)
        times: {
          user: 0,
          nice: 0,
          sys: 0,
          idle: 0,
          irq: 0,
        },
      });
    }
    return cpuInfo;
  }

  /**
   * Get the system endianness.
   */
  function endianness() {
    const buffer = new ArrayBuffer(2);
    new DataView(buffer).setInt16(0, 256, true);
    return new Int16Array(buffer)[0] === 256 ? "LE" : "BE";
  }

  /**
   * Get the amount of free system memory in bytes.
   */
  function freemem() {
    // Placeholder - would need native op
    return 4 * 1024 * 1024 * 1024; // 4GB placeholder
  }

  /**
   * Get the home directory of the current user.
   */
  function homedir() {
    return process.env.HOME || process.env.USERPROFILE || "/";
  }

  /**
   * Get the hostname.
   */
  function hostname() {
    return process.env.HOSTNAME || "localhost";
  }

  /**
   * Get system load averages.
   */
  function loadavg() {
    // Placeholder - would need native op
    return [0.0, 0.0, 0.0];
  }

  /**
   * Get network interfaces.
   */
  function networkInterfaces() {
    // Placeholder - would need native op
    return {};
  }

  /**
   * Get the operating system platform.
   */
  function platform() {
    return process.platform || "unknown";
  }

  /**
   * Get the operating system release.
   */
  function release() {
    // Placeholder - would need native op
    return "0.0.0";
  }

  /**
   * Get the operating system temporary directory.
   */
  function tmpdir() {
    return (
      process.env.TMPDIR ||
      process.env.TMP ||
      process.env.TEMP ||
      (process.platform === "win32" ? "C:\\Windows\\Temp" : "/tmp")
    );
  }

  /**
   * Get the total amount of system memory in bytes.
   */
  function totalmem() {
    // Placeholder - would need native op
    return 8 * 1024 * 1024 * 1024; // 8GB placeholder
  }

  /**
   * Get the operating system type.
   */
  function type() {
    const p = process.platform;
    if (p === "darwin") return "Darwin";
    if (p === "win32") return "Windows_NT";
    if (p === "linux") return "Linux";
    if (p === "freebsd") return "FreeBSD";
    return "Unknown";
  }

  /**
   * Get system uptime in seconds.
   */
  function uptime() {
    // Placeholder - would need native op
    return 0;
  }

  /**
   * Get user info.
   */
  function userInfo(options = {}) {
    return {
      uid: -1,
      gid: -1,
      username: process.env.USER || process.env.USERNAME || "unknown",
      homedir: homedir(),
      shell: process.env.SHELL || null,
    };
  }

  /**
   * Get OS version.
   */
  function version() {
    // Placeholder - would need native op
    return "";
  }

  /**
   * Get machine type.
   */
  function machine() {
    // Map to common machine names
    const a = arch();
    if (a === "x64") return "x86_64";
    if (a === "arm64") return "arm64";
    if (a === "ia32") return "i686";
    return a;
  }

  // End-of-line character for the OS
  const EOL = process.platform === "win32" ? "\r\n" : "\n";

  // Dev null path
  const devNull = process.platform === "win32" ? "\\\\.\\nul" : "/dev/null";

  const osModule = {
    arch,
    constants: osConstants,
    cpus,
    devNull,
    endianness,
    EOL,
    freemem,
    getPriority: () => 0,
    homedir,
    hostname,
    loadavg,
    machine,
    networkInterfaces,
    platform,
    release,
    setPriority: () => {},
    tmpdir,
    totalmem,
    type,
    uptime,
    userInfo,
    version,
  };

  // Register the os module
  globalThis.__howth_modules["node:os"] = osModule;
  globalThis.__howth_modules["os"] = osModule;

  // ============================================================================
  // querystring module
  // ============================================================================

  /**
   * Parse a query string into an object.
   */
  function qsParse(str, sep = "&", eq = "=", options = {}) {
    const obj = {};
    if (typeof str !== "string" || str.length === 0) {
      return obj;
    }

    const maxKeys = options.maxKeys !== undefined ? options.maxKeys : 1000;
    let count = 0;

    const pairs = str.split(sep);
    for (const pair of pairs) {
      if (maxKeys > 0 && count >= maxKeys) break;

      const idx = pair.indexOf(eq);
      let key, value;
      if (idx >= 0) {
        key = decodeURIComponent(pair.slice(0, idx).replace(/\+/g, " "));
        value = decodeURIComponent(pair.slice(idx + 1).replace(/\+/g, " "));
      } else {
        key = decodeURIComponent(pair.replace(/\+/g, " "));
        value = "";
      }

      if (obj.hasOwnProperty(key)) {
        if (Array.isArray(obj[key])) {
          obj[key].push(value);
        } else {
          obj[key] = [obj[key], value];
        }
      } else {
        obj[key] = value;
      }
      count++;
    }

    return obj;
  }

  /**
   * Stringify an object into a query string.
   */
  function qsStringify(obj, sep = "&", eq = "=", options = {}) {
    if (obj === null || typeof obj !== "object") {
      return "";
    }

    const encode = options.encodeURIComponent || encodeURIComponent;
    const pairs = [];

    for (const key of Object.keys(obj)) {
      const value = obj[key];
      const encodedKey = encode(String(key));

      if (Array.isArray(value)) {
        for (const v of value) {
          pairs.push(`${encodedKey}${eq}${encode(String(v))}`);
        }
      } else {
        pairs.push(`${encodedKey}${eq}${encode(String(value))}`);
      }
    }

    return pairs.join(sep);
  }

  /**
   * Escape a string for use in a query string.
   */
  function qsEscape(str) {
    return encodeURIComponent(str);
  }

  /**
   * Unescape a query string component.
   */
  function qsUnescape(str) {
    return decodeURIComponent(str.replace(/\+/g, " "));
  }

  const querystringModule = {
    parse: qsParse,
    stringify: qsStringify,
    escape: qsEscape,
    unescape: qsUnescape,
    encode: qsStringify,
    decode: qsParse,
  };

  // Register the querystring module
  globalThis.__howth_modules["node:querystring"] = querystringModule;
  globalThis.__howth_modules["querystring"] = querystringModule;

  // ============================================================================
  // timers module
  // ============================================================================

  // Timeout class that wraps timer IDs with ref/unref methods
  class Timeout {
    constructor(id, isInterval = false) {
      this._id = id;
      this._isInterval = isInterval;
      this._refed = true;
    }
    ref() {
      this._refed = true;
      return this;
    }
    unref() {
      this._refed = false;
      return this;
    }
    refresh() {
      // Stub - can't easily refresh in standard JS
      return this;
    }
    hasRef() {
      return this._refed;
    }
    [Symbol.toPrimitive]() {
      return this._id;
    }
  }

  // Store original timers
  const _originalSetTimeout = globalThis.setTimeout;
  const _originalClearTimeout = globalThis.clearTimeout;
  const _originalSetInterval = globalThis.setInterval;
  const _originalClearInterval = globalThis.clearInterval;

  // Wrap setTimeout to return Timeout object
  globalThis.setTimeout = function setTimeout(callback, delay, ...args) {
    const id = _originalSetTimeout(callback, delay, ...args);
    return new Timeout(id, false);
  };

  globalThis.clearTimeout = function clearTimeout(timeout) {
    const id = timeout instanceof Timeout ? timeout._id : timeout;
    return _originalClearTimeout(id);
  };

  globalThis.setInterval = function setInterval(callback, delay, ...args) {
    const id = _originalSetInterval(callback, delay, ...args);
    return new Timeout(id, true);
  };

  globalThis.clearInterval = function clearInterval(timeout) {
    const id = timeout instanceof Timeout ? timeout._id : timeout;
    return _originalClearInterval(id);
  };

  // setImmediate using setTimeout(0)
  globalThis.setImmediate = function setImmediate(callback, ...args) {
    return globalThis.setTimeout(callback, 0, ...args);
  };

  globalThis.clearImmediate = globalThis.clearTimeout;

  const timersModule = {
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
    setInterval: globalThis.setInterval,
    clearInterval: globalThis.clearInterval,
    setImmediate: globalThis.setImmediate,
    clearImmediate: globalThis.clearImmediate,
    Timeout,
  };

  // timers/promises
  const timersPromises = {
    setTimeout: (delay, value, options = {}) => {
      return new Promise((resolve, reject) => {
        if (options.signal?.aborted) {
          reject(new DOMException("Aborted", "AbortError"));
          return;
        }
        const id = setTimeout(() => resolve(value), delay);
        if (options.signal) {
          options.signal.addEventListener("abort", () => {
            clearTimeout(id);
            reject(new DOMException("Aborted", "AbortError"));
          });
        }
      });
    },
    setInterval: async function* (delay, value, options = {}) {
      if (options.signal?.aborted) {
        throw new DOMException("Aborted", "AbortError");
      }
      while (true) {
        yield await timersPromises.setTimeout(delay, value, options);
      }
    },
    setImmediate: (value, options = {}) => {
      return timersPromises.setTimeout(0, value, options);
    },
  };

  timersModule.promises = timersPromises;

  // Register the timers module
  globalThis.__howth_modules["node:timers"] = timersModule;
  globalThis.__howth_modules["timers"] = timersModule;
  globalThis.__howth_modules["node:timers/promises"] = timersPromises;
  globalThis.__howth_modules["timers/promises"] = timersPromises;

  // ============================================================================
  // Child-side IPC setup for forked processes
  // ============================================================================
  (function setupChildIPC() {
    const p2cPath = ops.op_howth_env_get('HOWTH_IPC_P2C');
    const c2pPath = ops.op_howth_env_get('HOWTH_IPC_C2P');

    if (!p2cPath || !c2pPath) {
      // Not a forked child, nothing to set up
      return;
    }

    let connected = true;
    let lastReadPosition = 0;
    let pollInterval = null;

    // Set up process.send for child-to-parent communication
    globalThis.process.send = (message, sendHandle, options, callback) => {
      // Handle overloaded signatures
      if (typeof sendHandle === 'function') {
        callback = sendHandle;
        sendHandle = undefined;
      } else if (typeof options === 'function') {
        callback = options;
        options = undefined;
      }

      if (!connected) {
        const error = new Error('Channel closed');
        if (callback) {
          callback(error);
          return false;
        }
        throw error;
      }

      try {
        // Use ops to write directly
        const line = JSON.stringify(message) + '\n';
        // Read current content and append
        let content = '';
        try {
          content = ops.op_howth_read_file(c2pPath);
        } catch (e) {
          // File might not exist yet
        }
        ops.op_howth_write_file(c2pPath, content + line);
        if (callback) callback(null);
        return true;
      } catch (e) {
        if (callback) callback(e);
        return false;
      }
    };

    globalThis.process.connected = true;

    globalThis.process.disconnect = () => {
      if (!connected) return;
      connected = false;
      globalThis.process.connected = false;
      if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
      }
      globalThis.process.emit('disconnect');
    };

    // Poll for messages from parent
    const pollMessages = () => {
      if (!connected) return;
      try {
        const content = ops.op_howth_read_file(p2cPath);
        if (content.length > lastReadPosition) {
          const newContent = content.slice(lastReadPosition);
          lastReadPosition = content.length;

          // Parse JSON lines
          const lines = newContent.split('\n').filter(l => l.trim());
          for (const line of lines) {
            try {
              const msg = JSON.parse(line);
              // Emit 'message' event on process
              globalThis.process.emit('message', msg);
            } catch (e) {
              // Ignore parse errors
            }
          }
        }
      } catch (e) {
        // File might not exist yet or read error
      }
    };

    // Start polling after a short delay to let the process initialize
    setTimeout(() => {
      pollInterval = setInterval(pollMessages, 50);
    }, 10);
  })();

  // ============================================================================
  // string_decoder module
  // ============================================================================

  /**
   * StringDecoder for decoding Buffer objects into strings.
   */
  class StringDecoder {
    #encoding;
    #lastNeed = 0;
    #lastTotal = 0;
    #lastChar;

    constructor(encoding = "utf8") {
      this.#encoding = encoding.toLowerCase().replace("-", "");
      if (this.#encoding === "utf8" || this.#encoding === "utf-8") {
        this.#encoding = "utf8";
        this.#lastChar = Buffer.allocUnsafe(4);
      } else if (this.#encoding === "utf16le" || this.#encoding === "ucs2") {
        this.#encoding = "utf16le";
        this.#lastChar = Buffer.allocUnsafe(4);
      } else if (this.#encoding === "base64") {
        this.#lastChar = Buffer.allocUnsafe(3);
      } else {
        this.#lastChar = Buffer.allocUnsafe(0);
      }
    }

    get encoding() {
      return this.#encoding;
    }

    write(buffer) {
      if (buffer.length === 0) return "";

      // For simple encodings, just convert directly
      if (
        this.#encoding === "ascii" ||
        this.#encoding === "latin1" ||
        this.#encoding === "binary" ||
        this.#encoding === "hex"
      ) {
        return buffer.toString(this.#encoding);
      }

      // UTF-8 decoding with multi-byte character handling
      if (this.#encoding === "utf8") {
        return this.#utf8Write(buffer);
      }

      // UTF-16LE decoding
      if (this.#encoding === "utf16le") {
        return this.#utf16leWrite(buffer);
      }

      // Base64 decoding
      if (this.#encoding === "base64") {
        return this.#base64Write(buffer);
      }

      return buffer.toString(this.#encoding);
    }

    #utf8Write(buffer) {
      // Handle any leftover bytes from previous write
      if (this.#lastNeed > 0) {
        const needed = Math.min(buffer.length, this.#lastNeed);
        buffer.copy(this.#lastChar, this.#lastTotal - this.#lastNeed, 0, needed);
        if (needed < this.#lastNeed) {
          this.#lastNeed -= needed;
          return "";
        }
        const result = this.#lastChar.toString("utf8", 0, this.#lastTotal);
        this.#lastNeed = 0;
        if (needed === buffer.length) {
          return result;
        }
        buffer = buffer.slice(needed);
        return result + this.#utf8Write(buffer);
      }

      // Check for incomplete multi-byte sequence at end
      const len = buffer.length;
      if (len === 0) return "";

      // Check last few bytes for incomplete sequences
      let incomplete = 0;
      const lastByte = buffer[len - 1];

      if ((lastByte & 0x80) === 0) {
        // ASCII - complete
        incomplete = 0;
      } else if ((lastByte & 0xc0) === 0x80) {
        // Continuation byte - check backwards
        let pos = len - 1;
        while (pos >= 0 && (buffer[pos] & 0xc0) === 0x80) {
          pos--;
        }
        if (pos >= 0) {
          const startByte = buffer[pos];
          let expectedLen = 1;
          if ((startByte & 0xf8) === 0xf0) expectedLen = 4;
          else if ((startByte & 0xf0) === 0xe0) expectedLen = 3;
          else if ((startByte & 0xe0) === 0xc0) expectedLen = 2;

          const actualLen = len - pos;
          if (actualLen < expectedLen) {
            incomplete = actualLen;
            this.#lastTotal = expectedLen;
            this.#lastNeed = expectedLen - actualLen;
            buffer.copy(this.#lastChar, 0, pos);
            return buffer.toString("utf8", 0, pos);
          }
        }
      } else if ((lastByte & 0xe0) === 0xc0) {
        // 2-byte start
        incomplete = 1;
        this.#lastTotal = 2;
        this.#lastNeed = 1;
        this.#lastChar[0] = lastByte;
        return buffer.toString("utf8", 0, len - 1);
      } else if ((lastByte & 0xf0) === 0xe0) {
        // 3-byte start
        incomplete = 1;
        this.#lastTotal = 3;
        this.#lastNeed = 2;
        this.#lastChar[0] = lastByte;
        return buffer.toString("utf8", 0, len - 1);
      } else if ((lastByte & 0xf8) === 0xf0) {
        // 4-byte start
        incomplete = 1;
        this.#lastTotal = 4;
        this.#lastNeed = 3;
        this.#lastChar[0] = lastByte;
        return buffer.toString("utf8", 0, len - 1);
      }

      return buffer.toString("utf8");
    }

    #utf16leWrite(buffer) {
      let result = "";

      if (this.#lastNeed > 0) {
        if (buffer.length >= this.#lastNeed) {
          buffer.copy(this.#lastChar, 2 - this.#lastNeed, 0, this.#lastNeed);
          result = this.#lastChar.toString("utf16le", 0, 2);
          buffer = buffer.slice(this.#lastNeed);
          this.#lastNeed = 0;
        } else {
          buffer.copy(this.#lastChar, 2 - this.#lastNeed, 0, buffer.length);
          this.#lastNeed -= buffer.length;
          return "";
        }
      }

      const len = buffer.length - (buffer.length % 2);
      result += buffer.toString("utf16le", 0, len);

      if (buffer.length % 2 === 1) {
        this.#lastChar[0] = buffer[buffer.length - 1];
        this.#lastNeed = 1;
        this.#lastTotal = 2;
      }

      return result;
    }

    #base64Write(buffer) {
      let result = "";

      if (this.#lastNeed > 0) {
        const needed = Math.min(buffer.length, this.#lastNeed);
        buffer.copy(this.#lastChar, 3 - this.#lastNeed, 0, needed);
        if (needed < this.#lastNeed) {
          this.#lastNeed -= needed;
          return "";
        }
        result = this.#lastChar.toString("base64", 0, 3);
        buffer = buffer.slice(needed);
        this.#lastNeed = 0;
      }

      const len = buffer.length - (buffer.length % 3);
      result += buffer.toString("base64", 0, len);

      const remaining = buffer.length % 3;
      if (remaining > 0) {
        buffer.copy(this.#lastChar, 0, len);
        this.#lastNeed = 3 - remaining;
        this.#lastTotal = 3;
      }

      return result;
    }

    end(buffer) {
      let result = "";
      if (buffer && buffer.length > 0) {
        result = this.write(buffer);
      }
      if (this.#lastNeed > 0) {
        // Output remaining incomplete character as replacement
        if (this.#encoding === "utf8") {
          result += "\ufffd";
        } else if (this.#encoding === "utf16le") {
          result += this.#lastChar.toString("utf16le", 0, 2 - this.#lastNeed);
        } else if (this.#encoding === "base64") {
          result += this.#lastChar.toString("base64", 0, 3 - this.#lastNeed);
        }
        this.#lastNeed = 0;
      }
      return result;
    }

    text(buffer, offset) {
      if (offset) {
        buffer = buffer.slice(offset);
      }
      return this.write(buffer);
    }
  }

  const stringDecoderModule = {
    StringDecoder,
  };

  // Register the string_decoder module
  globalThis.__howth_modules["node:string_decoder"] = stringDecoderModule;
  globalThis.__howth_modules["string_decoder"] = stringDecoderModule;

  // ============================================================================
  // url module (Node.js style, wrapping WHATWG URL)
  // ============================================================================

  /**
   * Parse a URL string into an object (legacy Node.js API).
   */
  function urlParse(urlString, parseQueryString = false, slashesDenoteHost = false) {
    const result = {
      protocol: null,
      slashes: null,
      auth: null,
      host: null,
      port: null,
      hostname: null,
      hash: null,
      search: null,
      query: null,
      pathname: null,
      path: null,
      href: urlString,
    };

    try {
      // Handle protocol-relative URLs
      let url;
      if (urlString.startsWith("//") && slashesDenoteHost) {
        url = new URL("http:" + urlString);
        result.protocol = null;
        result.slashes = true;
      } else {
        url = new URL(urlString);
        result.protocol = url.protocol;
        result.slashes = url.protocol.endsWith(":");
      }

      result.hostname = url.hostname || null;
      result.port = url.port || null;
      result.host = url.host || null;
      result.pathname = url.pathname || null;
      result.search = url.search || null;
      result.hash = url.hash || null;

      if (url.username || url.password) {
        result.auth = url.username + (url.password ? ":" + url.password : "");
      }

      if (parseQueryString && url.search) {
        result.query = querystringModule.parse(url.search.slice(1));
      } else {
        result.query = url.search ? url.search.slice(1) : null;
      }

      result.path = (result.pathname || "") + (result.search || "");
      result.href = url.href;
    } catch (e) {
      // If URL parsing fails (e.g., for relative URLs like "/"), do basic parsing
      result.href = urlString;

      // Handle relative paths (common case like "/" or "/path?query")
      if (urlString.startsWith("/")) {
        // Extract query string and hash
        let pathname = urlString;
        let search = null;
        let hash = null;

        const hashIndex = pathname.indexOf("#");
        if (hashIndex !== -1) {
          hash = pathname.slice(hashIndex);
          pathname = pathname.slice(0, hashIndex);
        }

        const queryIndex = pathname.indexOf("?");
        if (queryIndex !== -1) {
          search = pathname.slice(queryIndex);
          pathname = pathname.slice(0, queryIndex);
        }

        result.pathname = pathname;
        result.search = search;
        result.hash = hash;
        result.path = (pathname || "") + (search || "");

        if (parseQueryString && search) {
          result.query = querystringModule.parse(search.slice(1));
        } else {
          result.query = search ? search.slice(1) : null;
        }
      }
    }

    return result;
  }

  /**
   * Format a URL object into a string (legacy Node.js API).
   */
  function urlFormat(urlObject) {
    if (typeof urlObject === "string") {
      return urlObject;
    }

    let result = "";

    if (urlObject.protocol) {
      result += urlObject.protocol;
      if (!urlObject.protocol.endsWith(":")) {
        result += ":";
      }
    }

    if (urlObject.slashes || (urlObject.protocol && /^https?:?$/.test(urlObject.protocol))) {
      result += "//";
    }

    if (urlObject.auth) {
      result += urlObject.auth + "@";
    }

    if (urlObject.hostname) {
      result += urlObject.hostname;
    } else if (urlObject.host) {
      result += urlObject.host.split(":")[0];
    }

    if (urlObject.port) {
      result += ":" + urlObject.port;
    }

    if (urlObject.pathname) {
      result += urlObject.pathname;
    }

    if (urlObject.search) {
      result += urlObject.search.startsWith("?") ? urlObject.search : "?" + urlObject.search;
    } else if (urlObject.query) {
      const query =
        typeof urlObject.query === "string"
          ? urlObject.query
          : querystringModule.stringify(urlObject.query);
      if (query) {
        result += "?" + query;
      }
    }

    if (urlObject.hash) {
      result += urlObject.hash.startsWith("#") ? urlObject.hash : "#" + urlObject.hash;
    }

    return result;
  }

  /**
   * Resolve a target URL relative to a base URL.
   */
  function urlResolve(from, to) {
    return new URL(to, from).href;
  }

  /**
   * Convert a file path to a file:// URL.
   * Relative paths are resolved against the current working directory.
   */
  function pathToFileURL(filepath) {
    // Resolve relative paths against cwd (like Node.js does)
    if (!posixPath.isAbsolute(filepath)) {
      filepath = posixPath.resolve(process.cwd(), filepath);
    }

    let url = "file://";
    if (process.platform === "win32") {
      filepath = filepath.replace(/\\/g, "/");
      if (filepath.startsWith("//")) {
        // UNC path
        url += filepath;
      } else {
        url += "/" + filepath;
      }
    } else {
      url += filepath;
    }
    return new URL(url);
  }

  /**
   * Convert a file:// URL to a file path.
   */
  function fileURLToPath(url) {
    if (typeof url === "string") {
      url = new URL(url);
    }
    if (url.protocol !== "file:") {
      throw new TypeError('The URL must be of scheme file');
    }
    let pathname = decodeURIComponent(url.pathname);
    if (process.platform === "win32") {
      if (url.hostname) {
        // UNC path
        return "\\\\" + url.hostname + pathname.replace(/\//g, "\\");
      }
      // Remove leading slash for Windows paths
      if (/^\/[a-zA-Z]:/.test(pathname)) {
        pathname = pathname.slice(1);
      }
      return pathname.replace(/\//g, "\\");
    }
    return pathname;
  }

  const urlModule = {
    parse: urlParse,
    format: urlFormat,
    resolve: urlResolve,
    URL: globalThis.URL,
    URLSearchParams: globalThis.URLSearchParams,
    pathToFileURL,
    fileURLToPath,
    // Deprecated but still used
    Url: function Url() {
      return urlParse.apply(this, arguments);
    },
  };

  // Register the url module
  globalThis.__howth_modules["node:url"] = urlModule;
  globalThis.__howth_modules["url"] = urlModule;

  // ============================================================================
  // punycode module (deprecated but still used)
  // ============================================================================

  /**
   * Punycode encoding/decoding for internationalized domain names.
   * This is a simplified implementation.
   */
  const punycode = {
    version: "2.1.0",

    /**
     * Convert a Punycode string to Unicode.
     */
    decode: function (input) {
      // Basic implementation - for full support would need complete algorithm
      const basic = [];
      const nonBasic = [];

      let i = input.lastIndexOf("-");
      if (i >= 0) {
        for (let j = 0; j < i; j++) {
          basic.push(input.charCodeAt(j));
        }
      }

      // For now, just handle basic ASCII
      return String.fromCodePoint(...basic);
    },

    /**
     * Convert a Unicode string to Punycode.
     */
    encode: function (input) {
      const output = [];
      const inputLength = input.length;

      // Handle basic code points
      for (let i = 0; i < inputLength; i++) {
        const codePoint = input.charCodeAt(i);
        if (codePoint < 0x80) {
          output.push(String.fromCharCode(codePoint));
        }
      }

      const basicLength = output.length;
      if (basicLength > 0) {
        output.push("-");
      }

      // For simplicity, just return basic ASCII for now
      return output.join("");
    },

    /**
     * Convert a Unicode domain name to ASCII (Punycode).
     */
    toASCII: function (domain) {
      return domain
        .split(".")
        .map((label) => {
          if (/[^\x00-\x7F]/.test(label)) {
            return "xn--" + punycode.encode(label);
          }
          return label;
        })
        .join(".");
    },

    /**
     * Convert an ASCII (Punycode) domain name to Unicode.
     */
    toUnicode: function (domain) {
      return domain
        .split(".")
        .map((label) => {
          if (label.startsWith("xn--")) {
            return punycode.decode(label.slice(4));
          }
          return label;
        })
        .join(".");
    },

    ucs2: {
      decode: function (string) {
        const output = [];
        for (let i = 0; i < string.length; i++) {
          const value = string.charCodeAt(i);
          if (value >= 0xd800 && value <= 0xdbff && i + 1 < string.length) {
            const extra = string.charCodeAt(i + 1);
            if ((extra & 0xfc00) === 0xdc00) {
              output.push(((value & 0x3ff) << 10) + (extra & 0x3ff) + 0x10000);
              i++;
              continue;
            }
          }
          output.push(value);
        }
        return output;
      },
      encode: function (array) {
        return String.fromCodePoint(...array);
      },
    },
  };

  // Register the punycode module
  globalThis.__howth_modules["node:punycode"] = punycode;
  globalThis.__howth_modules["punycode"] = punycode;

  // ============================================================================
  // console module (export the global console)
  // ============================================================================

  const consoleModule = globalThis.console;

  // Register the console module
  globalThis.__howth_modules["node:console"] = consoleModule;
  globalThis.__howth_modules["console"] = consoleModule;

  // ============================================================================
  // constants module (deprecated, use fs.constants or os.constants)
  // ============================================================================

  const constantsModule = {
    ...fsConstants,
    ...osConstants.signals,
    ...osConstants.errno,
  };

  // Register the constants module
  globalThis.__howth_modules["node:constants"] = constantsModule;
  globalThis.__howth_modules["constants"] = constantsModule;

  // ============================================================================
  // perf_hooks module
  // ============================================================================

  /**
   * PerformanceObserver for observing performance entries.
   */
  class PerformanceObserver {
    #callback;
    #entryTypes = [];

    constructor(callback) {
      this.#callback = callback;
    }

    observe(options) {
      this.#entryTypes = options.entryTypes || [];
      // In a full implementation, this would register with the performance timeline
    }

    disconnect() {
      this.#entryTypes = [];
    }

    takeRecords() {
      return [];
    }
  }

  /**
   * PerformanceEntry base class.
   */
  class PerformanceEntry {
    constructor(name, entryType, startTime, duration) {
      this.name = name;
      this.entryType = entryType;
      this.startTime = startTime;
      this.duration = duration;
    }

    toJSON() {
      return {
        name: this.name,
        entryType: this.entryType,
        startTime: this.startTime,
        duration: this.duration,
      };
    }
  }

  /**
   * PerformanceMark for user timing marks.
   */
  class PerformanceMark extends PerformanceEntry {
    constructor(name, startTime) {
      super(name, "mark", startTime, 0);
    }
  }

  /**
   * PerformanceMeasure for user timing measures.
   */
  class PerformanceMeasure extends PerformanceEntry {
    constructor(name, startTime, duration) {
      super(name, "measure", startTime, duration);
    }
  }

  const perfMarks = new Map();
  const perfMeasures = [];

  const perfHooksPerformance = {
    now: () => globalThis.performance.now(),
    timeOrigin: Date.now() - globalThis.performance.now(),

    mark(name, options = {}) {
      const startTime = options.startTime ?? this.now();
      const mark = new PerformanceMark(name, startTime);
      perfMarks.set(name, mark);
      return mark;
    },

    measure(name, startMarkOrOptions, endMark) {
      let startTime = 0;
      let endTime = this.now();

      if (typeof startMarkOrOptions === "string") {
        const startMarkEntry = perfMarks.get(startMarkOrOptions);
        if (startMarkEntry) startTime = startMarkEntry.startTime;
        if (endMark) {
          const endMarkEntry = perfMarks.get(endMark);
          if (endMarkEntry) endTime = endMarkEntry.startTime;
        }
      } else if (startMarkOrOptions) {
        if (startMarkOrOptions.start !== undefined) {
          if (typeof startMarkOrOptions.start === "string") {
            const m = perfMarks.get(startMarkOrOptions.start);
            startTime = m ? m.startTime : 0;
          } else {
            startTime = startMarkOrOptions.start;
          }
        }
        if (startMarkOrOptions.end !== undefined) {
          if (typeof startMarkOrOptions.end === "string") {
            const m = perfMarks.get(startMarkOrOptions.end);
            endTime = m ? m.startTime : this.now();
          } else {
            endTime = startMarkOrOptions.end;
          }
        }
        if (startMarkOrOptions.duration !== undefined) {
          endTime = startTime + startMarkOrOptions.duration;
        }
      }

      const measure = new PerformanceMeasure(name, startTime, endTime - startTime);
      perfMeasures.push(measure);
      return measure;
    },

    clearMarks(name) {
      if (name) {
        perfMarks.delete(name);
      } else {
        perfMarks.clear();
      }
    },

    clearMeasures(name) {
      if (name) {
        const idx = perfMeasures.findIndex((m) => m.name === name);
        if (idx !== -1) perfMeasures.splice(idx, 1);
      } else {
        perfMeasures.length = 0;
      }
    },

    getEntries() {
      return [...perfMarks.values(), ...perfMeasures];
    },

    getEntriesByName(name, type) {
      return this.getEntries().filter(
        (e) => e.name === name && (!type || e.entryType === type)
      );
    },

    getEntriesByType(type) {
      return this.getEntries().filter((e) => e.entryType === type);
    },

    toJSON() {
      return {
        timeOrigin: this.timeOrigin,
      };
    },
  };

  const perfHooksModule = {
    performance: perfHooksPerformance,
    PerformanceObserver,
    PerformanceEntry,
    monitorEventLoopDelay: () => ({
      enable: () => {},
      disable: () => {},
      reset: () => {},
      percentile: () => 0,
      percentiles: new Map(),
      min: 0,
      max: 0,
      mean: 0,
      stddev: 0,
      exceeds: 0,
    }),
    createHistogram: () => ({
      record: () => {},
      reset: () => {},
      percentile: () => 0,
      percentiles: new Map(),
      min: 0,
      max: 0,
      mean: 0,
      stddev: 0,
      exceeds: 0,
    }),
  };

  // Register the perf_hooks module
  globalThis.__howth_modules["node:perf_hooks"] = perfHooksModule;
  globalThis.__howth_modules["perf_hooks"] = perfHooksModule;

  // ============================================================================
  // tty module
  // ============================================================================

  /**
   * Check if a file descriptor refers to a TTY.
   */
  function isatty(fd) {
    // In a native runtime, we'd check the actual fd
    // For now, assume stdin/stdout/stderr might be TTYs
    return fd >= 0 && fd <= 2;
  }

  /**
   * ReadStream class for TTY input.
   */
  class ReadStream extends EventEmitter {
    constructor(fd) {
      super();
      this.fd = fd;
      this.isTTY = isatty(fd);
      this.isRaw = false;
    }

    setRawMode(mode) {
      this.isRaw = mode;
      return this;
    }
  }

  /**
   * WriteStream class for TTY output.
   */
  class WriteStream extends EventEmitter {
    constructor(fd) {
      super();
      this.fd = fd;
      this.isTTY = isatty(fd);
      this.columns = 80;
      this.rows = 24;
    }

    clearLine(dir, callback) {
      if (callback) callback();
    }

    clearScreenDown(callback) {
      if (callback) callback();
    }

    cursorTo(x, y, callback) {
      if (callback) callback();
    }

    moveCursor(dx, dy, callback) {
      if (callback) callback();
    }

    getColorDepth() {
      return 8;
    }

    hasColors(count = 16) {
      return count <= 256;
    }

    getWindowSize() {
      return [this.columns, this.rows];
    }
  }

  const ttyModule = {
    isatty,
    ReadStream,
    WriteStream,
  };

  // Register the tty module
  globalThis.__howth_modules["node:tty"] = ttyModule;
  globalThis.__howth_modules["tty"] = ttyModule;

  // ============================================================================
  // v8 module
  // ============================================================================

  const v8Module = {
    getHeapStatistics: () => ({
      total_heap_size: 0,
      total_heap_size_executable: 0,
      total_physical_size: 0,
      total_available_size: 0,
      used_heap_size: 0,
      heap_size_limit: 0,
      malloced_memory: 0,
      peak_malloced_memory: 0,
      does_zap_garbage: 0,
      number_of_native_contexts: 0,
      number_of_detached_contexts: 0,
    }),
    getHeapSpaceStatistics: () => [],
    getHeapCodeStatistics: () => ({
      code_and_metadata_size: 0,
      bytecode_and_metadata_size: 0,
      external_script_source_size: 0,
    }),
    setFlagsFromString: () => {},
    serialize: (value) => {
      // Simple serialization using JSON
      return Buffer.from(JSON.stringify(value));
    },
    deserialize: (buffer) => {
      return JSON.parse(buffer.toString());
    },
    cachedDataVersionTag: () => 0,
    writeHeapSnapshot: () => "",
    takeCoverage: () => {},
    stopCoverage: () => {},
  };

  // Register the v8 module
  globalThis.__howth_modules["node:v8"] = v8Module;
  globalThis.__howth_modules["v8"] = v8Module;

  // ============================================================================
  // domain module (deprecated but still used)
  // ============================================================================

  /**
   * Domain class for error handling.
   */
  class Domain extends EventEmitter {
    constructor() {
      super();
      this.members = [];
    }

    add(emitter) {
      this.members.push(emitter);
      emitter.domain = this;
    }

    remove(emitter) {
      const idx = this.members.indexOf(emitter);
      if (idx !== -1) {
        this.members.splice(idx, 1);
        emitter.domain = null;
      }
    }

    bind(callback) {
      return (...args) => {
        try {
          return callback(...args);
        } catch (err) {
          this.emit("error", err);
        }
      };
    }

    intercept(callback) {
      return (err, ...args) => {
        if (err) {
          this.emit("error", err);
        } else {
          try {
            return callback(...args);
          } catch (e) {
            this.emit("error", e);
          }
        }
      };
    }

    run(fn) {
      try {
        return fn();
      } catch (err) {
        this.emit("error", err);
      }
    }

    dispose() {
      this.members = [];
      this.removeAllListeners();
    }

    enter() {}
    exit() {}
  }

  const domainModule = {
    create: () => new Domain(),
    Domain,
  };

  // Register the domain module
  globalThis.__howth_modules["node:domain"] = domainModule;
  globalThis.__howth_modules["domain"] = domainModule;

  // ============================================================================
  // async_hooks module (basic stub)
  // ============================================================================

  let asyncIdCounter = 1;

  class AsyncResource {
    constructor(type, options = {}) {
      this.type = type;
      this.asyncId = asyncIdCounter++;
      this.triggerAsyncId = options.triggerAsyncId ?? 0;
    }

    runInAsyncScope(fn, thisArg, ...args) {
      return fn.apply(thisArg, args);
    }

    emitDestroy() {
      return this;
    }

    asyncId() {
      return this.asyncId;
    }

    triggerAsyncId() {
      return this.triggerAsyncId;
    }

    bind(fn, thisArg) {
      return fn.bind(thisArg ?? this);
    }

    static bind(fn, type, thisArg) {
      return fn.bind(thisArg);
    }
  }

  const asyncHooksModule = {
    createHook: (callbacks) => ({
      enable: () => {},
      disable: () => {},
    }),
    executionAsyncId: () => 0,
    executionAsyncResource: () => ({}),
    triggerAsyncId: () => 0,
    AsyncResource,
    AsyncLocalStorage: class AsyncLocalStorage {
      #store;

      disable() {
        this.#store = undefined;
      }

      getStore() {
        return this.#store;
      }

      run(store, callback, ...args) {
        const prev = this.#store;
        this.#store = store;
        try {
          return callback(...args);
        } finally {
          this.#store = prev;
        }
      }

      exit(callback, ...args) {
        const prev = this.#store;
        this.#store = undefined;
        try {
          return callback(...args);
        } finally {
          this.#store = prev;
        }
      }

      enterWith(store) {
        this.#store = store;
      }
    },
  };

  // Register the async_hooks module
  globalThis.__howth_modules["node:async_hooks"] = asyncHooksModule;
  globalThis.__howth_modules["async_hooks"] = asyncHooksModule;

  // ============================================================================
  // zlib module
  // ============================================================================

  const zlibModule = {
    // Compression levels
    Z_NO_COMPRESSION: 0,
    Z_BEST_SPEED: 1,
    Z_BEST_COMPRESSION: 9,
    Z_DEFAULT_COMPRESSION: -1,

    // Strategy
    Z_FILTERED: 1,
    Z_HUFFMAN_ONLY: 2,
    Z_RLE: 3,
    Z_FIXED: 4,
    Z_DEFAULT_STRATEGY: 0,

    // Flush modes
    Z_NO_FLUSH: 0,
    Z_PARTIAL_FLUSH: 1,
    Z_SYNC_FLUSH: 2,
    Z_FULL_FLUSH: 3,
    Z_FINISH: 4,
    Z_BLOCK: 5,
    Z_TREES: 6,

    // Return codes
    Z_OK: 0,
    Z_STREAM_END: 1,
    Z_NEED_DICT: 2,
    Z_ERRNO: -1,
    Z_STREAM_ERROR: -2,
    Z_DATA_ERROR: -3,
    Z_MEM_ERROR: -4,
    Z_BUF_ERROR: -5,
    Z_VERSION_ERROR: -6,

    // Sync compression functions
    gzipSync(buffer, options = {}) {
      const level = options.level !== undefined ? options.level : -1;
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_gzip(input, level);
      return Buffer.from(result);
    },

    gunzipSync(buffer, options = {}) {
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_gunzip(input);
      return Buffer.from(result);
    },

    deflateSync(buffer, options = {}) {
      const level = options.level !== undefined ? options.level : -1;
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_deflate(input, level);
      return Buffer.from(result);
    },

    inflateSync(buffer, options = {}) {
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_inflate(input);
      return Buffer.from(result);
    },

    deflateRawSync(buffer, options = {}) {
      const level = options.level !== undefined ? options.level : -1;
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_deflate_raw(input, level);
      return Buffer.from(result);
    },

    inflateRawSync(buffer, options = {}) {
      const input = buffer instanceof Buffer ? [...buffer] : [...Buffer.from(buffer)];
      const result = ops.op_howth_zlib_inflate_raw(input);
      return Buffer.from(result);
    },

    // Async compression functions (callback style)
    gzip(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.gzipSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    gunzip(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.gunzipSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    deflate(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.deflateSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    inflate(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.inflateSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    deflateRaw(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.deflateRawSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    inflateRaw(buffer, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      try {
        const result = zlibModule.inflateRawSync(buffer, options);
        process.nextTick(() => callback(null, result));
      } catch (e) {
        process.nextTick(() => callback(e));
      }
    },

    // Stream-based compression (Transform streams)
    createGzip(options = {}) {
      return zlibModule._createCompressor('gzip', options);
    },

    createGunzip(options = {}) {
      return zlibModule._createDecompressor('gunzip', options);
    },

    createDeflate(options = {}) {
      return zlibModule._createCompressor('deflate', options);
    },

    createInflate(options = {}) {
      return zlibModule._createDecompressor('inflate', options);
    },

    createDeflateRaw(options = {}) {
      return zlibModule._createCompressor('deflateRaw', options);
    },

    createInflateRaw(options = {}) {
      return zlibModule._createDecompressor('inflateRaw', options);
    },

    // Helper to create a compressor transform stream
    _createCompressor(type, options) {
      const chunks = [];
      const compressFunc = zlibModule[type + 'Sync'];

      const stream = {
        _chunks: chunks,
        writable: true,
        readable: true,
        write(chunk, encoding, callback) {
          if (typeof encoding === 'function') {
            callback = encoding;
            encoding = undefined;
          }
          const buf = chunk instanceof Buffer ? chunk : Buffer.from(chunk, encoding);
          chunks.push(buf);
          if (callback) process.nextTick(callback);
          return true;
        },
        end(chunk, encoding, callback) {
          if (typeof chunk === 'function') {
            callback = chunk;
            chunk = undefined;
          } else if (typeof encoding === 'function') {
            callback = encoding;
            encoding = undefined;
          }
          if (chunk) this.write(chunk, encoding);

          const input = Buffer.concat(chunks);
          try {
            const result = compressFunc(input, options);
            this.emit('data', result);
            this.emit('end');
            if (callback) callback();
          } catch (e) {
            this.emit('error', e);
            if (callback) callback(e);
          }
        },
        on: function(event, fn) {
          if (!this._events) this._events = {};
          if (!this._events[event]) this._events[event] = [];
          this._events[event].push(fn);
          return this;
        },
        emit: function(event, ...args) {
          if (!this._events || !this._events[event]) return false;
          for (const fn of this._events[event]) fn(...args);
          return true;
        },
        pipe(dest) {
          this.on('data', (chunk) => dest.write(chunk));
          this.on('end', () => dest.end());
          return dest;
        },
      };

      return stream;
    },

    // Helper to create a decompressor transform stream
    _createDecompressor(type, options) {
      const chunks = [];
      const decompressFunc = zlibModule[type + 'Sync'];

      const stream = {
        _chunks: chunks,
        writable: true,
        readable: true,
        write(chunk, encoding, callback) {
          if (typeof encoding === 'function') {
            callback = encoding;
            encoding = undefined;
          }
          const buf = chunk instanceof Buffer ? chunk : Buffer.from(chunk, encoding);
          chunks.push(buf);
          if (callback) process.nextTick(callback);
          return true;
        },
        end(chunk, encoding, callback) {
          if (typeof chunk === 'function') {
            callback = chunk;
            chunk = undefined;
          } else if (typeof encoding === 'function') {
            callback = encoding;
            encoding = undefined;
          }
          if (chunk) this.write(chunk, encoding);

          const input = Buffer.concat(chunks);
          try {
            const result = decompressFunc(input, options);
            this.emit('data', result);
            this.emit('end');
            if (callback) callback();
          } catch (e) {
            this.emit('error', e);
            if (callback) callback(e);
          }
        },
        on: function(event, fn) {
          if (!this._events) this._events = {};
          if (!this._events[event]) this._events[event] = [];
          this._events[event].push(fn);
          return this;
        },
        emit: function(event, ...args) {
          if (!this._events || !this._events[event]) return false;
          for (const fn of this._events[event]) fn(...args);
          return true;
        },
        pipe(dest) {
          this.on('data', (chunk) => dest.write(chunk));
          this.on('end', () => dest.end());
          return dest;
        },
      };

      return stream;
    },

    // Convenience classes
    Gzip: class Gzip {
      constructor(options) { return zlibModule.createGzip(options); }
    },
    Gunzip: class Gunzip {
      constructor(options) { return zlibModule.createGunzip(options); }
    },
    Deflate: class Deflate {
      constructor(options) { return zlibModule.createDeflate(options); }
    },
    Inflate: class Inflate {
      constructor(options) { return zlibModule.createInflate(options); }
    },
    DeflateRaw: class DeflateRaw {
      constructor(options) { return zlibModule.createDeflateRaw(options); }
    },
    InflateRaw: class InflateRaw {
      constructor(options) { return zlibModule.createInflateRaw(options); }
    },

    // Constants object
    constants: {
      Z_NO_COMPRESSION: 0,
      Z_BEST_SPEED: 1,
      Z_BEST_COMPRESSION: 9,
      Z_DEFAULT_COMPRESSION: -1,
      Z_FILTERED: 1,
      Z_HUFFMAN_ONLY: 2,
      Z_RLE: 3,
      Z_FIXED: 4,
      Z_DEFAULT_STRATEGY: 0,
      Z_NO_FLUSH: 0,
      Z_PARTIAL_FLUSH: 1,
      Z_SYNC_FLUSH: 2,
      Z_FULL_FLUSH: 3,
      Z_FINISH: 4,
      Z_BLOCK: 5,
      Z_TREES: 6,
      Z_OK: 0,
      Z_STREAM_END: 1,
      Z_NEED_DICT: 2,
      Z_ERRNO: -1,
      Z_STREAM_ERROR: -2,
      Z_DATA_ERROR: -3,
      Z_MEM_ERROR: -4,
      Z_BUF_ERROR: -5,
      Z_VERSION_ERROR: -6,
    },
  };

  // Register the zlib module
  globalThis.__howth_modules["node:zlib"] = zlibModule;
  globalThis.__howth_modules["zlib"] = zlibModule;

  // ============================================================================
  // worker_threads module
  // ============================================================================

  /**
   * MessageChannel for structured message passing
   */
  class MessageChannel {
    constructor() {
      this.port1 = new MessagePort();
      this.port2 = new MessagePort();
      // Connect the ports
      this.port1._otherPort = this.port2;
      this.port2._otherPort = this.port1;
    }
  }

  /**
   * MessagePort for communication between threads
   */
  class MessagePort {
    #listeners = new Map();
    #otherPort = null;
    #started = false;
    #queue = [];

    set _otherPort(port) {
      this.#otherPort = port;
    }

    postMessage(message, transferList) {
      if (this.#otherPort) {
        const data = JSON.parse(JSON.stringify(message)); // Clone
        if (this.#otherPort.#started) {
          this.#otherPort._deliver(data);
        } else {
          this.#otherPort.#queue.push(data);
        }
      }
    }

    _deliver(data) {
      const listeners = this.#listeners.get('message') || [];
      for (const fn of listeners) {
        fn({ data });
      }
    }

    on(event, listener) {
      if (!this.#listeners.has(event)) {
        this.#listeners.set(event, []);
      }
      this.#listeners.get(event).push(listener);
      return this;
    }

    once(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener(...args);
      };
      return this.on(event, wrapper);
    }

    off(event, listener) {
      const list = this.#listeners.get(event);
      if (list) {
        const idx = list.indexOf(listener);
        if (idx !== -1) list.splice(idx, 1);
      }
      return this;
    }

    addListener(event, listener) { return this.on(event, listener); }
    removeListener(event, listener) { return this.off(event, listener); }

    start() {
      this.#started = true;
      // Deliver queued messages
      while (this.#queue.length > 0) {
        this._deliver(this.#queue.shift());
      }
    }

    close() {
      this.#listeners.clear();
      this.#otherPort = null;
    }

    ref() { return this; }
    unref() { return this; }
  }

  /**
   * Worker class for spawning worker threads
   */
  class Worker {
    #listeners = new Map();
    #workerId = null;
    #pollInterval = null;
    #terminated = false;

    constructor(filename, options = {}) {
      const resolvedPath = posixPath.resolve(filename);
      const workerData = options.workerData ? JSON.stringify(options.workerData) : '{}';

      // Create the worker
      const result = ops.op_howth_worker_create(resolvedPath, workerData);
      if (result.error) {
        throw new Error(result.error);
      }

      this.#workerId = result.id;
      this.threadId = result.id;

      // Start polling for messages
      this.#startPolling();
    }

    #startPolling() {
      this.#pollInterval = setInterval(() => {
        if (this.#terminated) {
          clearInterval(this.#pollInterval);
          return;
        }

        // Check for messages from worker
        const message = ops.op_howth_worker_recv_message(this.#workerId);
        if (message) {
          try {
            const data = JSON.parse(message);
            this.#emit('message', data);
          } catch (e) {
            this.#emit('message', message);
          }
        }

        // Check if worker is still running
        if (!ops.op_howth_worker_is_running(this.#workerId)) {
          this.#terminated = true;
          clearInterval(this.#pollInterval);
          this.#emit('exit', 0);
        }
      }, 10);
    }

    #emit(event, ...args) {
      const listeners = this.#listeners.get(event) || [];
      for (const fn of listeners) {
        fn(...args);
      }
    }

    postMessage(message, transferList) {
      if (this.#terminated) return;
      const data = JSON.stringify(message);
      ops.op_howth_worker_post_message(this.#workerId, data);
    }

    terminate() {
      if (this.#terminated) return Promise.resolve(0);
      this.#terminated = true;
      if (this.#pollInterval) {
        clearInterval(this.#pollInterval);
      }
      ops.op_howth_worker_terminate(this.#workerId);
      return Promise.resolve(0);
    }

    on(event, listener) {
      if (!this.#listeners.has(event)) {
        this.#listeners.set(event, []);
      }
      this.#listeners.get(event).push(listener);
      return this;
    }

    once(event, listener) {
      const wrapper = (...args) => {
        this.off(event, wrapper);
        listener(...args);
      };
      return this.on(event, wrapper);
    }

    off(event, listener) {
      const list = this.#listeners.get(event);
      if (list) {
        const idx = list.indexOf(listener);
        if (idx !== -1) list.splice(idx, 1);
      }
      return this;
    }

    addListener(event, listener) { return this.on(event, listener); }
    removeListener(event, listener) { return this.off(event, listener); }

    ref() { return this; }
    unref() { return this; }
  }

  /**
   * Parent port for communication with main thread (only available in workers)
   */
  let parentPort = null;
  let workerData = null;

  // If this is a worker thread, set up parentPort
  if (!ops.op_howth_worker_is_main_thread()) {
    const parentPortListeners = new Map();
    let parentPollInterval = null;

    parentPort = {
      postMessage(message) {
        const data = JSON.stringify(message);
        ops.op_howth_worker_parent_post(data);
      },

      on(event, listener) {
        if (!parentPortListeners.has(event)) {
          parentPortListeners.set(event, []);
        }
        parentPortListeners.get(event).push(listener);

        // Start polling when first listener is added
        if (!parentPollInterval) {
          parentPollInterval = setInterval(() => {
            const message = ops.op_howth_worker_parent_recv();
            if (message) {
              try {
                const data = JSON.parse(message);
                const listeners = parentPortListeners.get('message') || [];
                for (const fn of listeners) {
                  fn(data);
                }
              } catch (e) {
                const listeners = parentPortListeners.get('message') || [];
                for (const fn of listeners) {
                  fn(message);
                }
              }
            }
          }, 10);
        }

        return parentPort;
      },

      once(event, listener) {
        const wrapper = (...args) => {
          parentPort.off(event, wrapper);
          listener(...args);
        };
        return parentPort.on(event, wrapper);
      },

      off(event, listener) {
        const list = parentPortListeners.get(event);
        if (list) {
          const idx = list.indexOf(listener);
          if (idx !== -1) list.splice(idx, 1);
        }
        return parentPort;
      },

      addListener(event, listener) { return parentPort.on(event, listener); },
      removeListener(event, listener) { return parentPort.off(event, listener); },

      close() {
        if (parentPollInterval) {
          clearInterval(parentPollInterval);
          parentPollInterval = null;
        }
        parentPortListeners.clear();
      },

      ref() { return parentPort; },
      unref() { return parentPort; },
    };
  }

  const workerThreadsModule = {
    isMainThread: ops.op_howth_worker_is_main_thread(),
    parentPort,
    workerData,
    threadId: ops.op_howth_worker_thread_id(),
    Worker,
    MessageChannel,
    MessagePort,

    // Additional exports for compatibility
    resourceLimits: {},
    SHARE_ENV: Symbol('SHARE_ENV'),
    setEnvironmentData(key, value) {
      // Store in process.env for simplicity
      if (value === undefined) {
        delete process.env[`__WORKER_ENV_${key}`];
      } else {
        process.env[`__WORKER_ENV_${key}`] = String(value);
      }
    },
    getEnvironmentData(key) {
      return process.env[`__WORKER_ENV_${key}`];
    },
    markAsUntransferable(obj) {
      // No-op for now
    },
    moveMessagePortToContext(port, context) {
      // No-op for now
      return port;
    },
    receiveMessageOnPort(port) {
      // Synchronous receive - not fully supported
      return undefined;
    },
  };

  // Register the worker_threads module
  globalThis.__howth_modules["node:worker_threads"] = workerThreadsModule;
  globalThis.__howth_modules["worker_threads"] = workerThreadsModule;

  // ============================================================================
  // http module
  // ============================================================================

  /**
   * HTTP Agent for connection pooling.
   */
  class Agent {
    constructor(options = {}) {
      this.keepAlive = options.keepAlive || false;
      this.keepAliveMsecs = options.keepAliveMsecs || 1000;
      this.maxSockets = options.maxSockets || Infinity;
      this.maxFreeSockets = options.maxFreeSockets || 256;
      this.timeout = options.timeout;
      this.options = options;
      this.requests = {};
      this.sockets = {};
      this.freeSockets = {};
    }

    createConnection(options, callback) {
      // Would create actual socket connection
      if (callback) callback();
    }

    destroy() {
      this.requests = {};
      this.sockets = {};
      this.freeSockets = {};
    }
  }

  const globalAgent = new Agent({ keepAlive: true });

  /**
   * HTTP IncomingMessage - represents a request (server) or response (client).
   */
  class IncomingMessage extends Readable {
    constructor(socket) {
      super();
      this.socket = socket || null;
      this.connection = this.socket;
      this.httpVersion = "1.1";
      this.httpVersionMajor = 1;
      this.httpVersionMinor = 1;
      this.complete = false;
      this.headers = {};
      this.rawHeaders = [];
      this.trailers = {};
      this.rawTrailers = [];
      this.method = null;
      this.url = null;
      this.statusCode = null;
      this.statusMessage = null;
      this.aborted = false;
      this.readable = true;
    }

    setTimeout(msecs, callback) {
      if (callback) this.once("timeout", callback);
      return this;
    }

    destroy(error) {
      this.aborted = true;
      this.readable = false;
      this.complete = true;
      this.emit("close");
      if (error) this.emit("error", error);
      return this;
    }

    _read() {
      // No-op, data is pushed from the accept loop
    }
  }

  /**
   * HTTP ServerResponse - response to a server request.
   */
  class ServerResponse extends Writable {
    constructor(req) {
      super();
      this.req = req;
      this.statusCode = 200;
      this.statusMessage = "OK";
      this.headersSent = false;
      this._headerSent = false;
      this.sendDate = true;
      this.finished = false;
      this.writableEnded = false;
      this._headers = {};
      this._body = "";
      // OutgoingMessage compatibility properties
      this._header = null;
      this._hasBody = true;
      this._trailer = "";
      this._contentLength = null;
      this._closed = false;
      this._removedTE = false;
      this._removedContLen = false;
      this._removedConnection = false;
      this.usesChunkedEncodingByDefault = true;
      this.chunkedEncoding = false;
      this.shouldKeepAlive = true;
      this._keepAliveTimeout = 0;
      this._defaultKeepAlive = true;
      this.strictContentLength = false;
      this.writable = true;
      this.destroyed = false;
      this.outputData = [];
      this.outputSize = 0;
      this._onPendingData = function() {};
      this._socket = null;
      this.useChunkedEncodingByDefault = true;
    }

    setHeader(name, value) {
      this._headers[name.toLowerCase()] = value;
      return this;
    }

    getHeader(name) {
      return this._headers[name.toLowerCase()];
    }

    getHeaders() {
      return { ...this._headers };
    }

    getHeaderNames() {
      return Object.keys(this._headers);
    }

    hasHeader(name) {
      return name.toLowerCase() in this._headers;
    }

    removeHeader(name) {
      delete this._headers[name.toLowerCase()];
    }

    writeHead(statusCode, statusMessage, headers) {
      if (typeof statusMessage === "object") {
        headers = statusMessage;
        statusMessage = undefined;
      }
      this.statusCode = statusCode;
      if (statusMessage) this.statusMessage = statusMessage;
      if (headers) {
        if (Array.isArray(headers)) {
          // writeHead can accept an array of [key, value] pairs
          for (let i = 0; i < headers.length; i += 2) {
            this.setHeader(headers[i], headers[i + 1]);
          }
        } else {
          for (const [key, value] of Object.entries(headers)) {
            this.setHeader(key, value);
          }
        }
      }
      this.headersSent = true;
      return this;
    }

    write(chunk, encoding, callback) {
      if (typeof encoding === "function") {
        callback = encoding;
        encoding = undefined;
      }
      if (chunk) {
        if (Buffer.isBuffer(chunk)) {
          this._body += chunk.toString(encoding || "utf8");
        } else {
          this._body += String(chunk);
        }
      }
      if (callback) callback();
      return true;
    }

    _write(chunk, encoding, callback) {
      this.write(chunk, encoding, callback);
    }

    end(data, encoding, callback) {
      if (typeof data === "function") {
        callback = data;
        data = null;
      }
      if (typeof encoding === "function") {
        callback = encoding;
        encoding = undefined;
      }
      if (data) this.write(data, encoding);
      this.headersSent = true;
      this.finished = true;
      this.writableEnded = true;
      this.emit("finish");
      if (callback) callback();
      return this;
    }

    flushHeaders() {
      this.headersSent = true;
    }

    writeContinue() {}
    writeProcessing() {}

    cork() {}
    uncork() {}

    addTrailers(headers) {
      // Trailers not supported in our HTTP impl, but don't crash
    }

    setTimeout(msecs, callback) {
      if (callback) this.once("timeout", callback);
      return this;
    }

    // Internal method called before writing data to ensure headers are sent
    _implicitHeader() {
      if (!this.headersSent) {
        this.headersSent = true;
      }
    }

    // Get socket/connection (used by some frameworks)
    get socket() {
      return this._socket || null;
    }

    get connection() {
      return this.socket;
    }

    get writableFinished() {
      return this.finished;
    }

    get writableLength() {
      return 0;
    }

    get writableHighWaterMark() {
      return 16384;
    }

    get writableCorked() {
      return 0;
    }

    get writableNeedDrain() {
      return false;
    }

    // Internal methods used by Node.js internals and frameworks like Next.js
    _send(data, encoding, callback, byteLength) {
      if (data) {
        this.write(data, encoding, callback);
      }
      return true;
    }

    _writeRaw(data, encoding, callback, size) {
      if (data) {
        this.write(data, encoding, callback);
      }
      return true;
    }

    _flushOutput(conn) {
      const data = this.outputData;
      for (let i = 0; i < data.length; i++) {
        this.write(data[i].data, data[i].encoding, data[i].callback);
      }
      this.outputData = [];
      this.outputSize = 0;
    }

    assignSocket(socket) {
      this._socket = socket;
      socket._httpMessage = this;
    }

    detachSocket(socket) {
      this._socket = null;
      if (socket) socket._httpMessage = null;
    }
  }

  /**
   * HTTP ClientRequest - request from client to server.
   */
  class ClientRequest extends Writable {
    constructor(options, callback) {
      super();

      if (typeof options === "string") {
        options = urlParse(options);
      }

      this.method = (options.method || "GET").toUpperCase();
      this.path = options.path || options.pathname || "/";
      // Use hostname (without port) for host
      this.hostname = options.hostname || (options.host ? options.host.split(':')[0] : "localhost");
      this.host = options.host || options.hostname || "localhost";
      this.port = options.port ? parseInt(options.port, 10) : (options.protocol === "https:" ? 443 : 80);
      this.protocol = options.protocol || "http:";
      this.headers = {};
      this.timeout = options.timeout;
      this.agent = options.agent !== undefined ? options.agent : globalAgent;
      this.reusedSocket = false;
      this.maxHeadersCount = 2000;
      this._body = [];
      this._ended = false;
      this.aborted = false;
      this.socket = null;

      // Set default headers
      if (options.headers) {
        for (const [key, value] of Object.entries(options.headers)) {
          this.setHeader(key, value);
        }
      }

      if (options.auth) {
        this.setHeader("Authorization", "Basic " + btoa(options.auth));
      }

      if (callback) {
        this.once("response", callback);
      }
    }

    setHeader(name, value) {
      this.headers[name.toLowerCase()] = value;
      return this;
    }

    getHeader(name) {
      return this.headers[name.toLowerCase()];
    }

    removeHeader(name) {
      delete this.headers[name.toLowerCase()];
    }

    setNoDelay(noDelay) {
      return this;
    }

    setSocketKeepAlive(enable, initialDelay) {
      return this;
    }

    setTimeout(timeout, callback) {
      this.timeout = timeout;
      if (callback) this.once("timeout", callback);
      return this;
    }

    _write(chunk, encoding, callback) {
      this._body.push(chunk);
      if (callback) callback();
    }

    abort() {
      this.aborted = true;
      this.emit("abort");
    }

    end(data, encoding, callback) {
      if (typeof data === "function") {
        callback = data;
        data = null;
      }
      if (data) this._body.push(data);
      this._ended = true;

      // Perform the actual fetch
      this._doFetch().then(() => {
        if (callback) callback();
      }).catch((err) => {
        this.emit("error", err);
      });

      return this;
    }

    async _doFetch() {
      // Use hostname (without port) to avoid double-port issues
      const portSuffix = (this.port !== 80 && this.port !== 443) ? ":" + this.port : "";
      const url = `${this.protocol}//${this.hostname}${portSuffix}${this.path}`;

      const fetchOptions = {
        method: this.method,
        headers: this.headers,
      };

      if (this._body.length > 0 && this.method !== "GET" && this.method !== "HEAD") {
        fetchOptions.body = Buffer.concat(this._body.map(b =>
          typeof b === "string" ? Buffer.from(b) : b
        ));
      }

      try {
        const response = await fetch(url, fetchOptions);

        // Create IncomingMessage from response
        const incoming = new IncomingMessage(null);
        incoming.statusCode = response.status;
        incoming.statusMessage = response.statusText;
        incoming.httpVersion = "1.1";

        // Copy headers
        response.headers.forEach((value, key) => {
          incoming.headers[key.toLowerCase()] = value;
          incoming.rawHeaders.push(key, value);
        });

        // Get body
        const body = await response.arrayBuffer();
        const bodyBuffer = Buffer.from(body);

        // Emit response first, then push body data asynchronously
        // This allows listeners to attach before data arrives
        this.emit("response", incoming);

        // Push body data on next tick so event listeners have time to attach
        setTimeout(() => {
          if (bodyBuffer.length > 0) {
            incoming.push(bodyBuffer);
          }
          incoming.push(null);
          incoming.complete = true;
        }, 0);
      } catch (err) {
        this.emit("error", err);
      }
    }
  }

  /**
   * HTTP Server class.
   */
  class Server extends EventEmitter {
    constructor(options, requestListener) {
      super();

      if (typeof options === "function") {
        requestListener = options;
        options = {};
      }

      this.timeout = options?.timeout || 0;
      this.keepAliveTimeout = options?.keepAliveTimeout || 5000;
      this.maxHeadersCount = options?.maxHeadersCount || 2000;
      this.headersTimeout = options?.headersTimeout || 60000;
      this.requestTimeout = options?.requestTimeout || 0;
      this.listening = false;
      this._connections = 0;
      this._serverId = null;

      if (requestListener) {
        this.on("request", requestListener);
      }
    }

    listen(port, hostname, backlog, callback) {
      // Handle different argument patterns
      if (typeof port === "object") {
        const options = port;
        port = options.port;
        hostname = options.host || options.hostname;
        callback = hostname;
      }
      if (typeof hostname === "function") {
        callback = hostname;
        hostname = undefined;
      }
      if (typeof backlog === "function") {
        callback = backlog;
        backlog = undefined;
      }

      hostname = hostname || "0.0.0.0";
      port = port || 0;
      this._hostname = hostname;
      this._port = port;

      const self = this;

      // Start the native HTTP server
      (async () => {
        try {
          const result = await ops.op_howth_http_listen(port, hostname);
          self._serverId = result.id;
          self._port = result.port;
          self._address = result.address;
          self.listening = true;
          self.emit("listening");
          if (callback) callback();

          // Start accepting connections
          self._acceptLoop();
        } catch (err) {
          self.emit("error", err);
        }
      })();

      return this;
    }

    async _acceptLoop() {
      while (this.listening && this._serverId !== null) {
        try {
          const request = await ops.op_howth_http_accept(this._serverId);
          if (!request) continue;

          this._connections++;

          // Create a mock socket for req.socket / req.connection
          const mockSocket = new EventEmitter();
          mockSocket.remoteAddress = request.remoteAddress || "127.0.0.1";
          mockSocket.remotePort = request.remotePort || 0;
          mockSocket.remoteFamily = "IPv4";
          mockSocket.localAddress = this._address || "127.0.0.1";
          mockSocket.localPort = this._port || 0;
          mockSocket.encrypted = false;
          mockSocket.writable = true;
          mockSocket.readable = true;
          mockSocket.destroyed = false;
          mockSocket.connecting = false;
          mockSocket.bufferSize = 0;
          mockSocket.bytesRead = 0;
          mockSocket.bytesWritten = 0;
          mockSocket.address = () => ({ address: mockSocket.localAddress, family: mockSocket.remoteFamily, port: mockSocket.localPort });
          mockSocket.setTimeout = (ms, cb) => { if (cb) mockSocket.once("timeout", cb); return mockSocket; };
          mockSocket.setNoDelay = () => mockSocket;
          mockSocket.setKeepAlive = () => mockSocket;
          mockSocket.ref = () => mockSocket;
          mockSocket.unref = () => mockSocket;
          mockSocket.destroy = () => { mockSocket.destroyed = true; };
          mockSocket.end = () => {};
          mockSocket.write = () => true;

          // Create IncomingMessage and ServerResponse
          const req = new IncomingMessage(mockSocket);
          req.method = request.method;
          req.url = request.url;
          req.headers = request.headers || {};
          req.httpVersion = "1.1";
          req.httpVersionMajor = 1;
          req.httpVersionMinor = 1;
          req._requestId = request.id;
          req.connection = mockSocket;

          // Populate rawHeaders from headers (alternating key/value pairs)
          const rawHeaders = [];
          if (request.headers) {
            for (const [key, value] of Object.entries(request.headers)) {
              rawHeaders.push(key, value);
            }
          }
          req.rawHeaders = rawHeaders;

          // Push body data if present
          if (request.body) {
            req.push(Buffer.from(request.body));
          }
          req.push(null); // End the stream

          const res = new ServerResponse(req);
          res._requestId = request.id;
          res._socket = mockSocket;

          // Override end to send the response
          const originalEnd = res.end.bind(res);
          res.end = (data, encoding, callback) => {
            if (typeof data === "function") {
              callback = data;
              data = undefined;
            }
            if (typeof encoding === "function") {
              callback = encoding;
              encoding = undefined;
            }

            if (data) {
              res.write(data, encoding);
            }

            // Send the response via native op
            const headers = {};
            if (res._headers) {
              for (const [key, value] of Object.entries(res._headers)) {
                headers[key] = Array.isArray(value) ? value.join(", ") : String(value);
              }
            }

            ops.op_howth_http_respond(request.id, {
              status: res.statusCode,
              headers: headers,
              body: res._body || "",
            }).then(() => {
              res.finished = true;
              res.writableEnded = true;
              res.emit("finish");
              if (callback) callback();
              this._connections--;
            }).catch((err) => {
              res.emit("error", err);
              this._connections--;
            });
          };

          // Emit the request event with error protection
          try {
            this.emit("request", req, res);
          } catch (handlerErr) {
            // Log the error for debugging
            console.error("[howth] Request handler error:", handlerErr.message);
            console.error("[howth] Stack:", handlerErr.stack);
            // If the handler throws, send a 500 response to avoid connection leak
            if (!res.finished && !res.writableEnded) {
              try {
                res.statusCode = 500;
                res.end("Internal Server Error");
              } catch (e) {
                // Last resort: clean up the connection directly
                ops.op_howth_http_respond(request.id, {
                  status: 500,
                  headers: {},
                  body: "Internal Server Error",
                }).catch(() => {});
                this._connections--;
              }
            }
          }
        } catch (err) {
          if (this.listening) {
            this.emit("error", err);
          }
          break;
        }
      }
    }

    close(callback) {
      this.listening = false;
      const self = this;

      if (this._serverId !== null) {
        ops.op_howth_http_close(this._serverId).then(() => {
          self._serverId = null;
          self.emit("close");
          if (callback) callback();
        }).catch((err) => {
          self.emit("error", err);
          if (callback) callback(err);
        });
      } else {
        if (callback) this.once("close", callback);
        setTimeout(() => this.emit("close"), 0);
      }

      return this;
    }

    address() {
      return this.listening ? { port: this._port, family: "IPv4", address: this._address || this._hostname } : null;
    }

    getConnections(callback) {
      if (callback) callback(null, this._connections);
    }

    setTimeout(msecs, callback) {
      this.timeout = msecs;
      if (callback) this.on("timeout", callback);
      return this;
    }

    ref() { return this; }
    unref() { return this; }
  }

  /**
   * Create an HTTP request.
   */
  function httpRequest(options, callback) {
    return new ClientRequest(options, callback);
  }

  /**
   * HTTP GET request (convenience method).
   */
  function httpGet(options, callback) {
    const req = httpRequest(options, callback);
    req.end();
    return req;
  }

  /**
   * Create an HTTP server.
   */
  function createServer(options, requestListener) {
    return new Server(options, requestListener);
  }

  // Status codes
  const STATUS_CODES = {
    100: "Continue",
    101: "Switching Protocols",
    102: "Processing",
    200: "OK",
    201: "Created",
    202: "Accepted",
    203: "Non-Authoritative Information",
    204: "No Content",
    205: "Reset Content",
    206: "Partial Content",
    207: "Multi-Status",
    208: "Already Reported",
    226: "IM Used",
    300: "Multiple Choices",
    301: "Moved Permanently",
    302: "Found",
    303: "See Other",
    304: "Not Modified",
    305: "Use Proxy",
    307: "Temporary Redirect",
    308: "Permanent Redirect",
    400: "Bad Request",
    401: "Unauthorized",
    402: "Payment Required",
    403: "Forbidden",
    404: "Not Found",
    405: "Method Not Allowed",
    406: "Not Acceptable",
    407: "Proxy Authentication Required",
    408: "Request Timeout",
    409: "Conflict",
    410: "Gone",
    411: "Length Required",
    412: "Precondition Failed",
    413: "Payload Too Large",
    414: "URI Too Long",
    415: "Unsupported Media Type",
    416: "Range Not Satisfiable",
    417: "Expectation Failed",
    418: "I'm a Teapot",
    421: "Misdirected Request",
    422: "Unprocessable Entity",
    423: "Locked",
    424: "Failed Dependency",
    425: "Too Early",
    426: "Upgrade Required",
    428: "Precondition Required",
    429: "Too Many Requests",
    431: "Request Header Fields Too Large",
    451: "Unavailable For Legal Reasons",
    500: "Internal Server Error",
    501: "Not Implemented",
    502: "Bad Gateway",
    503: "Service Unavailable",
    504: "Gateway Timeout",
    505: "HTTP Version Not Supported",
    506: "Variant Also Negotiates",
    507: "Insufficient Storage",
    508: "Loop Detected",
    509: "Bandwidth Limit Exceeded",
    510: "Not Extended",
    511: "Network Authentication Required",
  };

  // HTTP methods
  const METHODS = [
    "ACL", "BIND", "CHECKOUT", "CONNECT", "COPY", "DELETE", "GET", "HEAD",
    "LINK", "LOCK", "M-SEARCH", "MERGE", "MKACTIVITY", "MKCALENDAR", "MKCOL",
    "MOVE", "NOTIFY", "OPTIONS", "PATCH", "POST", "PROPFIND", "PROPPATCH",
    "PURGE", "PUT", "REBIND", "REPORT", "SEARCH", "SOURCE", "SUBSCRIBE",
    "TRACE", "UNBIND", "UNLINK", "UNLOCK", "UNSUBSCRIBE",
  ];

  const httpModule = {
    Agent,
    ClientRequest,
    IncomingMessage,
    Server,
    ServerResponse,
    createServer,
    get: httpGet,
    request: httpRequest,
    globalAgent,
    maxHeaderSize: 16384,
    METHODS,
    STATUS_CODES,
    // Validate header name/value
    validateHeaderName: (name) => {
      if (typeof name !== "string" || name.length === 0) {
        throw new TypeError("Header name must be a non-empty string");
      }
    },
    validateHeaderValue: (name, value) => {
      if (value === undefined) {
        throw new TypeError(`Invalid value "${value}" for header "${name}"`);
      }
    },
  };

  // Register the http module
  globalThis.__howth_modules["node:http"] = httpModule;
  globalThis.__howth_modules["http"] = httpModule;

  // ============================================================================
  // https module (wraps http with TLS)
  // ============================================================================

  const httpsAgent = new Agent({ keepAlive: true });

  /**
   * Create an HTTPS request.
   */
  function httpsRequest(options, callback) {
    if (typeof options === "string") {
      options = urlParse(options);
    }
    options = { ...options, protocol: "https:" };
    if (!options.port) options.port = 443;
    return new ClientRequest(options, callback);
  }

  /**
   * HTTPS GET request.
   */
  function httpsGet(options, callback) {
    const req = httpsRequest(options, callback);
    req.end();
    return req;
  }

  /**
   * Create an HTTPS server (stub - needs TLS support).
   */
  function createHttpsServer(options, requestListener) {
    // For now, just create a regular server
    // Full HTTPS would need TLS/SSL support
    return createServer(options, requestListener);
  }

  const httpsModule = {
    Agent,
    Server,
    createServer: createHttpsServer,
    get: httpsGet,
    request: httpsRequest,
    globalAgent: httpsAgent,
  };

  // Register the https module
  globalThis.__howth_modules["node:https"] = httpsModule;
  globalThis.__howth_modules["https"] = httpsModule;

  // ============================================================================
  // net module (basic stub for TCP)
  // ============================================================================

  /**
   * TCP Socket class.
   */
  class Socket extends Duplex {
    constructor(options = {}) {
      super(options);
      this.connecting = false;
      this.destroyed = false;
      this.pending = true;
      this.readyState = "closed";
      this.bufferSize = 0;
      this.bytesRead = 0;
      this.bytesWritten = 0;
      this.localAddress = undefined;
      this.localPort = undefined;
      this.localFamily = undefined;
      this.remoteAddress = undefined;
      this.remotePort = undefined;
      this.remoteFamily = undefined;
      this.timeout = 0;
    }

    connect(options, connectListener) {
      if (typeof options === "number") {
        options = { port: options };
      }
      this.connecting = true;
      this.readyState = "opening";

      if (connectListener) {
        this.once("connect", connectListener);
      }

      // Would need native TCP ops to actually connect
      setTimeout(() => {
        this.connecting = false;
        this.pending = false;
        this.readyState = "open";
        this.emit("connect");
        this.emit("ready");
      }, 0);

      return this;
    }

    setTimeout(timeout, callback) {
      this.timeout = timeout;
      if (callback) this.once("timeout", callback);
      return this;
    }

    setNoDelay(noDelay) {
      return this;
    }

    setKeepAlive(enable, initialDelay) {
      return this;
    }

    address() {
      return {
        port: this.localPort,
        family: this.localFamily,
        address: this.localAddress,
      };
    }

    ref() { return this; }
    unref() { return this; }

    destroy(error) {
      if (this.destroyed) return this;
      this.destroyed = true;
      this.readyState = "closed";
      this.emit("close", !!error);
      return this;
    }

    end(data, encoding, callback) {
      super.end(data, encoding, callback);
      return this;
    }
  }

  /**
   * TCP Server class.
   */
  class NetServer extends EventEmitter {
    constructor(options, connectionListener) {
      super();

      if (typeof options === "function") {
        connectionListener = options;
        options = {};
      }

      this.listening = false;
      this.maxConnections = undefined;
      this._connections = 0;

      if (connectionListener) {
        this.on("connection", connectionListener);
      }
    }

    listen(port, host, backlog, callback) {
      if (typeof host === "function") {
        callback = host;
        host = undefined;
      }
      if (typeof backlog === "function") {
        callback = backlog;
        backlog = undefined;
      }

      const self = this;
      setTimeout(() => {
        self.listening = true;
        self.emit("listening");
        if (callback) callback();
      }, 0);

      return this;
    }

    close(callback) {
      this.listening = false;
      if (callback) this.once("close", callback);
      setTimeout(() => this.emit("close"), 0);
      return this;
    }

    address() {
      return this.listening ? { port: this._port, family: "IPv4", address: this._host } : null;
    }

    getConnections(callback) {
      if (callback) callback(null, this._connections);
    }

    ref() { return this; }
    unref() { return this; }
  }

  /**
   * Create a TCP connection.
   */
  function netConnect(options, connectListener) {
    const socket = new Socket();
    return socket.connect(options, connectListener);
  }

  /**
   * Create a TCP server.
   */
  function netCreateServer(options, connectionListener) {
    return new NetServer(options, connectionListener);
  }

  /**
   * Check if input is an IP address.
   */
  function isIP(input) {
    if (isIPv4(input)) return 4;
    if (isIPv6(input)) return 6;
    return 0;
  }

  function isIPv4(input) {
    return /^(\d{1,3}\.){3}\d{1,3}$/.test(input) &&
      input.split(".").every(n => parseInt(n) <= 255);
  }

  function isIPv6(input) {
    return /^([0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}$/.test(input) ||
      /^::ffff:\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(input);
  }

  const netModule = {
    Socket,
    Server: NetServer,
    connect: netConnect,
    createConnection: netConnect,
    createServer: netCreateServer,
    isIP,
    isIPv4,
    isIPv6,
  };

  // Register the net module
  globalThis.__howth_modules["node:net"] = netModule;
  globalThis.__howth_modules["net"] = netModule;

  // ============================================================================
  // dns module (stub)
  // ============================================================================

  const dnsPromises = {
    lookup(hostname, options) {
      return Promise.resolve({ address: '127.0.0.1', family: 4 });
    },
    resolve(hostname, rrtype) {
      return Promise.resolve(['127.0.0.1']);
    },
    resolve4(hostname) {
      return Promise.resolve(['127.0.0.1']);
    },
    resolve6(hostname) {
      return Promise.resolve(['::1']);
    },
    resolveCname(hostname) {
      return Promise.resolve([]);
    },
    resolveMx(hostname) {
      return Promise.resolve([]);
    },
    resolveTxt(hostname) {
      return Promise.resolve([]);
    },
    resolveNs(hostname) {
      return Promise.resolve([]);
    },
    resolveSrv(hostname) {
      return Promise.resolve([]);
    },
    resolvePtr(hostname) {
      return Promise.resolve([]);
    },
    resolveSoa(hostname) {
      return Promise.resolve({});
    },
    resolveNaptr(hostname) {
      return Promise.resolve([]);
    },
    resolveCaa(hostname) {
      return Promise.resolve([]);
    },
    reverse(ip) {
      return Promise.resolve([]);
    },
    setServers(servers) {},
    getServers() { return ['8.8.8.8']; },
    Resolver: class Resolver {
      resolve(hostname, callback) {
        callback(null, ['127.0.0.1']);
      }
    },
  };

  const dnsModule = {
    lookup(hostname, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      process.nextTick(() => callback(null, '127.0.0.1', 4));
    },
    resolve(hostname, rrtype, callback) {
      if (typeof rrtype === 'function') {
        callback = rrtype;
        rrtype = 'A';
      }
      process.nextTick(() => callback(null, ['127.0.0.1']));
    },
    resolve4(hostname, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      process.nextTick(() => callback(null, ['127.0.0.1']));
    },
    resolve6(hostname, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      process.nextTick(() => callback(null, ['::1']));
    },
    setServers(servers) {},
    getServers() { return ['8.8.8.8']; },
    promises: dnsPromises,
    Resolver: dnsPromises.Resolver,
    // Constants
    ADDRCONFIG: 1024,
    V4MAPPED: 2048,
    ALL: 256,
  };

  globalThis.__howth_modules["node:dns"] = dnsModule;
  globalThis.__howth_modules["dns"] = dnsModule;
  globalThis.__howth_modules["node:dns/promises"] = dnsPromises;
  globalThis.__howth_modules["dns/promises"] = dnsPromises;

  // ============================================================================
  // diagnostics_channel module (stub)
  // ============================================================================

  class DiagnosticsChannel {
    constructor(name) {
      this.name = name;
      this._subscribers = [];
    }
    subscribe(onMessage) {
      this._subscribers.push(onMessage);
    }
    unsubscribe(onMessage) {
      const idx = this._subscribers.indexOf(onMessage);
      if (idx !== -1) this._subscribers.splice(idx, 1);
      return idx !== -1;
    }
    publish(message) {
      for (const subscriber of this._subscribers) {
        try { subscriber(message, this.name); } catch {}
      }
    }
    get hasSubscribers() {
      return this._subscribers.length > 0;
    }
  }

  const channels = new Map();

  const diagnosticsChannelModule = {
    channel(name) {
      if (!channels.has(name)) {
        channels.set(name, new DiagnosticsChannel(name));
      }
      return channels.get(name);
    },
    hasSubscribers(name) {
      const ch = channels.get(name);
      return ch ? ch.hasSubscribers : false;
    },
    subscribe(name, onMessage) {
      this.channel(name).subscribe(onMessage);
    },
    unsubscribe(name, onMessage) {
      const ch = channels.get(name);
      return ch ? ch.unsubscribe(onMessage) : false;
    },
    Channel: DiagnosticsChannel,
  };

  globalThis.__howth_modules["node:diagnostics_channel"] = diagnosticsChannelModule;
  globalThis.__howth_modules["diagnostics_channel"] = diagnosticsChannelModule;

  // inspector module - Node.js debugging/profiling interface
  class InspectorSession {
    constructor() {
      this._connected = false;
    }
    connect() { this._connected = true; }
    connectToMainThread() { this._connected = true; }
    disconnect() { this._connected = false; }
    post(method, params, callback) {
      if (typeof params === 'function') {
        callback = params;
        params = {};
      }
      if (callback) {
        setImmediate(() => callback(new Error('Inspector not available')));
      }
    }
    on(event, listener) { return this; }
    once(event, listener) { return this; }
    off(event, listener) { return this; }
    addListener(event, listener) { return this; }
    removeListener(event, listener) { return this; }
  }

  const inspectorModule = {
    Session: InspectorSession,
    open(port, host, wait) { /* no-op */ },
    close() { /* no-op */ },
    url() { return undefined; },
    waitForDebugger() { /* no-op */ },
    console: {
      log: globalThis.console.log,
      error: globalThis.console.error,
      warn: globalThis.console.warn,
      info: globalThis.console.info,
      debug: globalThis.console.debug,
      dir: globalThis.console.dir,
      time: globalThis.console.time,
      timeEnd: globalThis.console.timeEnd,
      trace: globalThis.console.trace,
      assert: globalThis.console.assert,
      clear: globalThis.console.clear,
      count: globalThis.console.count,
      countReset: globalThis.console.countReset,
      group: globalThis.console.group,
      groupCollapsed: globalThis.console.groupCollapsed,
      groupEnd: globalThis.console.groupEnd,
      table: globalThis.console.table,
      profile() {},
      profileEnd() {},
      timeLog: globalThis.console.timeLog,
      timeStamp() {},
    },
  };

  globalThis.__howth_modules["node:inspector"] = inspectorModule;
  globalThis.__howth_modules["inspector"] = inspectorModule;

  // vm module - Virtual machine / code execution
  class Script {
    constructor(code, options = {}) {
      this.code = code;
      this.filename = options.filename || 'evalmachine.<anonymous>';
      this.lineOffset = options.lineOffset || 0;
      this.columnOffset = options.columnOffset || 0;
      this.cachedData = options.cachedData;
      this.produceCachedData = options.produceCachedData || false;
      this.importModuleDynamically = options.importModuleDynamically;
    }

    runInContext(contextifiedObject, options = {}) {
      return this.runInThisContext(options);
    }

    runInNewContext(contextObject = {}, options = {}) {
      const keys = Object.keys(contextObject);
      const values = keys.map(k => contextObject[k]);
      const fn = new Function(...keys, this.code);
      return fn.apply(null, values);
    }

    runInThisContext(options = {}) {
      return eval(this.code);
    }

    createCachedData() {
      return Buffer.alloc(0);
    }
  }

  function createContext(contextObject = {}, options = {}) {
    // In real Node.js, this creates a V8 context
    // We just return the object as-is for now
    contextObject[Symbol.for('vm.contextified')] = true;
    return contextObject;
  }

  function isContext(object) {
    return object && object[Symbol.for('vm.contextified')] === true;
  }

  function runInContext(code, contextifiedObject, options = {}) {
    const script = new Script(code, options);
    return script.runInContext(contextifiedObject, options);
  }

  function runInNewContext(code, contextObject = {}, options = {}) {
    const script = new Script(code, options);
    return script.runInNewContext(contextObject, options);
  }

  function runInThisContext(code, options = {}) {
    const script = new Script(code, options);
    return script.runInThisContext(options);
  }

  function compileFunction(code, params = [], options = {}) {
    const fn = new Function(...params, code);
    return fn;
  }

  const vmModule = {
    Script,
    createContext,
    isContext,
    runInContext,
    runInNewContext,
    runInThisContext,
    compileFunction,
  };

  globalThis.__howth_modules["node:vm"] = vmModule;
  globalThis.__howth_modules["vm"] = vmModule;

  // tls module - TLS/SSL functionality (stub)
  const tlsModule = (() => {
    class TLSSocket extends Duplex {
      constructor(socket, options = {}) {
        super(options);
        this._socket = socket;
        this.authorized = true;
        this.encrypted = true;
        this.alpnProtocol = null;
        this.servername = options.servername || null;
      }
      getPeerCertificate(detailed) { return {}; }
      getProtocol() { return 'TLSv1.3'; }
      getCipher() { return { name: 'TLS_AES_256_GCM_SHA384', standardName: 'TLS_AES_256_GCM_SHA384', version: 'TLSv1.3' }; }
      renegotiate(options, callback) { if (callback) callback(null); }
      setMaxSendFragment(size) { return true; }
      setServername(name) { this.servername = name; }
      address() { return this._socket ? this._socket.address() : {}; }
    }

    class TLSServer extends Stream {
      constructor(options, listener) {
        super();
        this._connections = 0;
        this.maxConnections = undefined;
        if (typeof options === 'function') {
          listener = options;
          options = {};
        }
        if (listener) this.on('secureConnection', listener);
      }
      listen(...args) {
        process.nextTick(() => this.emit('listening'));
        return this;
      }
      close(callback) {
        if (callback) process.nextTick(callback);
        return this;
      }
      address() { return { port: 0, family: 'IPv4', address: '0.0.0.0' }; }
      getTicketKeys() { return Buffer.alloc(48); }
      setTicketKeys(keys) {}
      setSecureContext(options) {}
    }

    return {
      TLSSocket,
      Server: TLSServer,
      createServer(options, listener) {
        return new TLSServer(options, listener);
      },
      connect(...args) {
        const socket = new TLSSocket(null);
        process.nextTick(() => socket.emit('connect'));
        process.nextTick(() => socket.emit('secureConnect'));
        return socket;
      },
      createSecureContext(options) { return {}; },
      getCiphers() { return ['TLS_AES_256_GCM_SHA384', 'TLS_AES_128_GCM_SHA256']; },
      DEFAULT_CIPHERS: 'TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256',
      DEFAULT_ECDH_CURVE: 'auto',
      DEFAULT_MIN_VERSION: 'TLSv1.2',
      DEFAULT_MAX_VERSION: 'TLSv1.3',
      rootCertificates: [],
    };
  })();

  globalThis.__howth_modules["node:tls"] = tlsModule;
  globalThis.__howth_modules["tls"] = tlsModule;
  return globalThis.__howth_modules["node:events"];
  }); // end heavy lazy block

  // Set up unhandled promise rejection handler using Deno core API
  core.setUnhandledPromiseRejectionHandler((promise, reason) => {
    // Check if process has handlers for 'unhandledRejection'
    const listeners = processListeners.get('unhandledRejection');
    if (listeners && listeners.length > 0) {
      for (const listener of [...listeners]) {
        listener(reason, promise);
      }
      return true; // handled
    }
    // Log and swallow the error to prevent crash (Node.js behavior: warn but don't crash by default)
    if (reason instanceof Error) {
      console.error('[howth] UnhandledPromiseRejection:', reason.stack || reason.message);
    } else {
      console.error('[howth] UnhandledPromiseRejection:', reason);
    }
    return true; // prevent crash
  });

  // Set up exception callback for better error reporting
  core.setReportExceptionCallback((err) => {
    const listeners = processListeners.get('uncaughtException');
    if (listeners && listeners.length > 0) {
      for (const listener of [...listeners]) {
        listener(err);
      }
      return;
    }
    if (err instanceof Error) {
      console.error('[howth] Uncaught exception:', err.stack || err.message);
    } else {
      console.error('[howth] Uncaught exception:', err);
    }
  });

  // node:test module — lightweight test runner for howth's native V8 executor.
  // Implements test(), describe(), it(), before(), after(), beforeEach(), afterEach().
  // Results are collected in globalThis.__howth_test_results after __howth_run_tests().
  (function() {
    const suiteStack = []; // stack of { name, before, after, beforeEach, afterEach, children }
    let rootSuite = { name: '<root>', before: [], after: [], beforeEach: [], afterEach: [], children: [] };
    let currentSuite = rootSuite;

    function pushSuite(name) {
      const suite = { name, before: [], after: [], beforeEach: [], afterEach: [], children: [] };
      currentSuite.children.push({ type: 'suite', suite });
      suiteStack.push(currentSuite);
      currentSuite = suite;
      return suite;
    }

    function popSuite() {
      currentSuite = suiteStack.pop() || rootSuite;
    }

    // test(name, [options], fn)
    function test(name, optionsOrFn, maybeFn) {
      let fn, options;
      if (typeof optionsOrFn === 'function') {
        fn = optionsOrFn;
        options = {};
      } else {
        options = optionsOrFn || {};
        fn = maybeFn;
      }
      const skip = options.skip === true || (typeof options.skip === 'string');
      const todo = options.todo === true || (typeof options.todo === 'string');
      currentSuite.children.push({ type: 'test', name, fn, skip: skip || todo });
    }

    // describe(name, [options], fn) — synchronous suite registration
    function describe(name, optionsOrFn, maybeFn) {
      let fn, options;
      if (typeof optionsOrFn === 'function') {
        fn = optionsOrFn;
        options = {};
      } else {
        options = optionsOrFn || {};
        fn = maybeFn;
      }
      const skip = options.skip === true;
      if (skip) {
        currentSuite.children.push({ type: 'suite', suite: { name, before: [], after: [], beforeEach: [], afterEach: [], children: [], skip: true } });
        return;
      }
      pushSuite(name);
      if (fn) fn();
      popSuite();
    }

    // it() is an alias for test()
    const it = test;

    function before(fn) { currentSuite.before.push(fn); }
    function after(fn) { currentSuite.after.push(fn); }
    function beforeEach(fn) { currentSuite.beforeEach.push(fn); }
    function afterEach(fn) { currentSuite.afterEach.push(fn); }

    // Execute all registered tests and return results
    async function __howth_run_tests() {
      const results = [];
      const startTime = performance.now();

      async function runSuite(suite, parentBeforeEach, parentAfterEach, prefix) {
        if (suite.skip) {
          // Mark all children as skipped
          function skipAll(s, pfx) {
            for (const child of s.children) {
              if (child.type === 'test') {
                results.push({ name: pfx ? pfx + ' > ' + child.name : child.name, status: 'skip', duration_ms: 0, error: null });
              } else if (child.type === 'suite') {
                skipAll(child.suite, pfx ? pfx + ' > ' + child.suite.name : child.suite.name);
              }
            }
          }
          skipAll(suite, prefix);
          return;
        }

        const allBeforeEach = parentBeforeEach.concat(suite.beforeEach);
        const allAfterEach = suite.afterEach.concat(parentAfterEach);

        // Run suite-level before hooks
        for (const hook of suite.before) {
          try { await hook(); } catch (e) {
            // If before hook fails, skip all tests in this suite
            for (const child of suite.children) {
              if (child.type === 'test') {
                const fullName = prefix ? prefix + ' > ' + child.name : child.name;
                results.push({ name: fullName, status: 'fail', duration_ms: 0, error: 'before hook failed: ' + (e.message || String(e)) });
              }
            }
            return;
          }
        }

        for (const child of suite.children) {
          if (child.type === 'test') {
            const fullName = prefix ? prefix + ' > ' + child.name : child.name;
            if (child.skip) {
              results.push({ name: fullName, status: 'skip', duration_ms: 0, error: null });
              continue;
            }

            const t0 = performance.now();
            let status = 'pass';
            let error = null;

            try {
              for (const hook of allBeforeEach) await hook();
              if (child.fn) {
                const result = child.fn();
                if (result && typeof result.then === 'function') await result;
              }
            } catch (e) {
              status = 'fail';
              error = e && (e.stack || e.message || String(e));
            }

            try {
              for (const hook of allAfterEach) await hook();
            } catch (e) {
              if (status === 'pass') {
                status = 'fail';
                error = 'afterEach hook failed: ' + (e.message || String(e));
              }
            }

            const duration_ms = performance.now() - t0;
            results.push({ name: fullName, status, duration_ms, error });
          } else if (child.type === 'suite') {
            const childPrefix = prefix ? prefix + ' > ' + child.suite.name : child.suite.name;
            await runSuite(child.suite, allBeforeEach, allAfterEach, childPrefix);
          }
        }

        // Run suite-level after hooks
        for (const hook of suite.after) {
          try { await hook(); } catch (e) { /* after hook errors are swallowed */ }
        }
      }

      await runSuite(rootSuite, [], [], '');

      const totalDuration = performance.now() - startTime;
      let passed = 0, failed = 0, skipped = 0;
      for (const r of results) {
        if (r.status === 'pass') passed++;
        else if (r.status === 'fail') failed++;
        else skipped++;
      }

      const report = {
        ok: failed === 0,
        total: results.length,
        passed,
        failed,
        skipped,
        duration_ms: totalDuration,
        tests: results,
      };

      globalThis.__howth_test_results = report;

      // Reset for next run
      rootSuite = { name: '<root>', before: [], after: [], beforeEach: [], afterEach: [], children: [] };
      currentSuite = rootSuite;
      suiteStack.length = 0;

      return report;
    }

    // mock stub — minimal implementation
    const mock = {
      fn(impl) {
        const mockFn = function(...args) {
          mockFn.mock.calls.push({ arguments: args });
          mockFn.mock.callCount++;
          if (impl) return impl.apply(this, args);
        };
        mockFn.mock = { calls: [], callCount: 0 };
        mockFn.mock.resetCalls = () => { mockFn.mock.calls = []; mockFn.mock.callCount = 0; };
        return mockFn;
      },
      reset() {},
      restoreAll() {},
    };

    const testModule = test;
    // Attach named exports as properties
    testModule.test = test;
    testModule.describe = describe;
    testModule.it = it;
    testModule.before = before;
    testModule.after = after;
    testModule.beforeEach = beforeEach;
    testModule.afterEach = afterEach;
    testModule.mock = mock;

    globalThis.__howth_modules["node:test"] = testModule;
    globalThis.__howth_modules["test"] = testModule;
    globalThis.__howth_run_tests = __howth_run_tests;
  })();

  // Mark bootstrap as complete
  globalThis.__howth_ready = true;

})(globalThis);
