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
    exit(code = 0) {
      // Emit exit event before exiting
      this.exitCode = code;
      this.emit("exit", code);
      ops.op_howth_exit(code);
    },
    exitCode: 0,
    argv: ops.op_howth_args(),
    platform: Deno.build?.os || "unknown",
    version: "v20.0.0", // Fake Node.js version for compatibility
    versions: {
      node: "20.0.0",
      v8: "11.0.0",
      howth: "0.1.0",
    },
    pid: 1,
    ppid: 0,
    arch: "x64",
    title: "howth",
    hrtime: {
      bigint() {
        return BigInt(ops.op_howth_hrtime());
      },
    },
    nextTick(callback, ...args) {
      queueMicrotask(() => callback(...args));
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

  // URL implementation (basic)
  globalThis.URL = class URL {
    constructor(url, base) {
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

      const match = fullUrl.match(/^([a-z]+):\/\/([^\/:?#]+)(?::(\d+))?(\/[^?#]*)?(\?[^#]*)?(#.*)?$/i);
      if (!match) {
        throw new TypeError("Invalid URL: " + url);
      }

      this.protocol = match[1].toLowerCase() + ":";
      this.hostname = match[2];
      this.port = match[3] || "";
      this.pathname = match[4] || "/";
      this.search = match[5] || "";
      this.hash = match[6] || "";
      this.host = this.port ? this.hostname + ":" + this.port : this.hostname;
      this.origin = this.protocol + "//" + this.host;
      this.href = this.origin + this.pathname + this.search + this.hash;
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

  // crypto implementation
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

    toJSON() {
      return { type: "Buffer", data: Array.from(this) };
    }
  };

  // performance API
  const performanceStart = Date.now();
  globalThis.performance = {
    now() {
      return Date.now() - performanceStart;
    },
    timeOrigin: performanceStart,
    mark(name) {
      // Minimal implementation
      return { name, startTime: this.now() };
    },
    measure(name, startMark, endMark) {
      return { name, duration: 0 };
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
  globalThis.__howth_modules["node:path"] = pathModule;
  globalThis.__howth_modules["path"] = pathModule;
  // ============================================
  // node:fs module
  // ============================================

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
      if (typeof data === "string") {
        ops.op_howth_write_file(String(path), data);
      } else if (data instanceof Uint8Array || Buffer.isBuffer(data)) {
        const base64 = Buffer.from(data).toString("base64");
        ops.op_howth_fs_write_bytes(String(path), base64);
      } else {
        ops.op_howth_write_file(String(path), String(data));
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
      const recursive = options?.recursive || false;
      ops.op_howth_fs_mkdir(String(path), recursive);
    },

    rmdirSync(path, options) {
      const recursive = options?.recursive || false;
      ops.op_howth_fs_rmdir(String(path), recursive);
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
      ops.op_howth_fs_unlink(String(path));
    },

    renameSync(oldPath, newPath) {
      ops.op_howth_fs_rename(String(oldPath), String(newPath));
    },

    copyFileSync(src, dest, mode) {
      ops.op_howth_fs_copy(String(src), String(dest));
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
        throw e;
      }
    },

    realpathSync(path, options) {
      return ops.op_howth_fs_realpath(String(path));
    },

    chmodSync(path, mode) {
      ops.op_howth_fs_chmod(String(path), mode);
    },

    accessSync(path, mode) {
      // mode: F_OK=0, R_OK=4, W_OK=2, X_OK=1
      const m = mode === undefined ? 0 : mode;
      ops.op_howth_fs_access(String(path), m);
    },

    // File descriptor-based operations (simplified implementation)
    // We use a simple fd counter and map to simulate file descriptors
    openSync(path, flags, mode) {
      // Simplified: just verify file exists/can be created and return a pseudo-fd
      const strPath = String(path);
      const flagStr = typeof flags === 'string' ? flags : '';

      // Check if file exists for read operations
      if (flagStr === 'r' || flags === 0) {
        if (!ops.op_howth_fs_exists(strPath)) {
          const err = new Error(`ENOENT: no such file or directory, open '${strPath}'`);
          err.code = 'ENOENT';
          err.syscall = 'open';
          err.path = strPath;
          throw err;
        }
      }

      // For write operations, create the file if it doesn't exist
      if (flagStr === 'w' || flagStr === 'w+' || (flags & 64)) { // O_CREAT
        if (!ops.op_howth_fs_exists(strPath)) {
          ops.op_howth_write_file(strPath, '');
        }
      }

      // Return a pseudo file descriptor (we track paths by fd)
      if (!globalThis.__howth_fd_map) {
        globalThis.__howth_fd_map = new Map();
        globalThis.__howth_fd_counter = 3; // Start after stdin/stdout/stderr
      }
      const fd = globalThis.__howth_fd_counter++;
      globalThis.__howth_fd_map.set(fd, { path: strPath, flags, mode });
      return fd;
    },

    closeSync(fd) {
      if (globalThis.__howth_fd_map) {
        globalThis.__howth_fd_map.delete(fd);
      }
    },

    fstatSync(fd, options) {
      if (!globalThis.__howth_fd_map || !globalThis.__howth_fd_map.has(fd)) {
        const err = new Error(`EBADF: bad file descriptor, fstat`);
        err.code = 'EBADF';
        err.syscall = 'fstat';
        throw err;
      }
      const info = globalThis.__howth_fd_map.get(fd);
      return fsSync.statSync(info.path, options);
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
  };

  // Register the fs module
  globalThis.__howth_modules["node:fs"] = fsModule;
  globalThis.__howth_modules["fs"] = fsModule;
  globalThis.__howth_modules["node:fs/promises"] = fsPromises;
  globalThis.__howth_modules["fs/promises"] = fsPromises;

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
      let current = from;
      while (current !== "/") {
        const nodeModules = posixPath.join(current, "node_modules");
        paths.push(nodeModules);
        const parent = posixPath.dirname(current);
        if (parent === current) break;
        current = parent;
      }
      paths.push("/node_modules");
      return paths;
    }

    static _resolveFilename(request, parent) {
      // Handle built-in modules
      if (request.startsWith("node:") || globalThis.__howth_modules[request]) {
        return request;
      }

      // Handle relative and absolute paths
      if (request.startsWith("./") || request.startsWith("../") || request.startsWith("/")) {
        const basePath = request.startsWith("/")
          ? request
          : posixPath.resolve(parent ? parent.dirname : ops.op_howth_cwd(), request);
        return Module._resolveAsFile(basePath) || Module._resolveAsDirectory(basePath);
      }

      // Handle bare specifiers (node_modules)
      const paths = parent ? parent.paths : Module._nodeModulePaths(ops.op_howth_cwd());
      for (const modulesPath of paths) {
        const modulePath = posixPath.join(modulesPath, request);
        const resolved = Module._resolveAsFile(modulePath) || Module._resolveAsDirectory(modulePath);
        if (resolved) return resolved;
      }

      throw new Error(`Cannot find module '${request}'`);
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
      const content = ops.op_howth_read_file(filename);

      // The Node.js module wrapper
      const wrapper = [
        "(function (exports, require, module, __filename, __dirname) { ",
        "\n});"
      ];

      const wrappedCode = wrapper[0] + content + wrapper[1];

      // Create a require function for this module
      const self = this;
      function require(id) {
        return Module._load(id, self);
      }
      require.resolve = (id) => Module._resolveFilename(id, self);
      require.cache = Object.fromEntries(moduleCache);
      require.main = globalThis.__howth_main_module;

      // Execute the wrapped code
      try {
        const compiledWrapper = (0, eval)(wrappedCode);
        compiledWrapper.call(
          this.exports,
          this.exports,
          require,
          this,
          this.filename,
          this.dirname
        );
      } catch (e) {
        // Remove from cache on error
        moduleCache.delete(filename);
        throw e;
      }
    }
  }

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

  // Global require (uses main module path or cwd as parent)
  function globalRequire(id) {
    // Use main module path if set, otherwise fall back to cwd
    const parentPath = globalThis.__howth_main_module_path ||
                       posixPath.join(ops.op_howth_cwd(), "__entrypoint__");
    const parentModule = new Module(parentPath, null);
    return Module._load(id, parentModule);
  }
  globalRequire.resolve = (id) => {
    const parentPath = globalThis.__howth_main_module_path ||
                       posixPath.join(ops.op_howth_cwd(), "__entrypoint__");
    const parentModule = new Module(parentPath, null);
    return Module._resolveFilename(id, parentModule);
  };
  globalRequire.cache = {};
  Object.defineProperty(globalRequire, "cache", {
    get() { return Object.fromEntries(moduleCache); }
  });

  // Export require globally
  globalThis.require = globalRequire;
  globalThis.module = { exports: {} };
  globalThis.exports = globalThis.module.exports;

  // createRequire for ESM interop
  const moduleModule = {
    createRequire,
    Module,
    _cache: moduleCache,
    _resolveFilename: Module._resolveFilename.bind(Module),
    builtinModules: Object.keys(globalThis.__howth_modules),
  };

  globalThis.__howth_modules["node:module"] = moduleModule;
  globalThis.__howth_modules["module"] = moduleModule;

  // ============================================
  // node:assert module
  // ============================================

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

  // ============================================================================
  // events module (EventEmitter)
  // ============================================================================

  class EventEmitter {
    #listeners = new Map();

    on(event, listener) {
      if (!this.#listeners.has(event)) {
        this.#listeners.set(event, []);
      }
      this.#listeners.get(event).push(listener);
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
      const listeners = this.#listeners.get(event);
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
      if (event !== undefined) {
        this.#listeners.delete(event);
      } else {
        this.#listeners.clear();
      }
      return this;
    }

    emit(event, ...args) {
      const listeners = this.#listeners.get(event);
      if (!listeners || listeners.length === 0) return false;
      for (const listener of [...listeners]) {
        listener.apply(this, args);
      }
      return true;
    }

    listeners(event) {
      const list = this.#listeners.get(event);
      if (!list) return [];
      return list.map((l) => l.listener || l);
    }

    listenerCount(event) {
      const list = this.#listeners.get(event);
      return list ? list.length : 0;
    }

    eventNames() {
      return [...this.#listeners.keys()];
    }

    prependListener(event, listener) {
      if (!this.#listeners.has(event)) {
        this.#listeners.set(event, []);
      }
      this.#listeners.get(event).unshift(listener);
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
      return this.#listeners.get(event) || [];
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
    if (superCtor.prototype === undefined) throw new TypeError("The super constructor must have a prototype");
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

    // Create a simple event-emitter-like ChildProcess
    const listeners = new Map();
    const child = {
      pid: 0,
      stdin: null,
      stdout: null,
      stderr: null,
      killed: false,
      exitCode: null,
      signalCode: null,

      on(event, listener) {
        if (!listeners.has(event)) {
          listeners.set(event, []);
        }
        listeners.get(event).push(listener);
        return this;
      },

      once(event, listener) {
        const onceListener = (...args) => {
          this.off(event, onceListener);
          listener(...args);
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

      emit(event, ...args) {
        const eventListeners = listeners.get(event);
        if (eventListeners) {
          for (const listener of eventListeners) {
            listener(...args);
          }
        }
      },

      kill(signal) {
        this.killed = true;
        return true;
      },
    };

    // Run the command asynchronously
    process.nextTick(() => {
      try {
        const result = ops.op_howth_spawn_sync(command, args, options);

        child.exitCode = result.status;

        if (result.error) {
          const err = new Error(result.error);
          child.emit("error", err);
        }

        child.emit("close", result.status, null);
        child.emit("exit", result.status, null);
      } catch (e) {
        child.emit("error", e);
        child.emit("close", 1, null);
        child.emit("exit", 1, null);
      }
    });

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
    // fork is not implemented (requires worker threads / IPC)
    fork: () => {
      throw new Error("fork() is not implemented in howth native runtime");
    },
  };

  // Register the child_process module
  globalThis.__howth_modules["node:child_process"] = childProcessModule;
  globalThis.__howth_modules["child_process"] = childProcessModule;

  // Mark bootstrap as complete
  globalThis.__howth_ready = true;

})(globalThis);
