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
    hrtime: {
      bigint() {
        return BigInt(ops.op_howth_hrtime());
      },
    },
    nextTick(callback, ...args) {
      queueMicrotask(() => callback(...args));
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

      const match = fullUrl.match(/^([a-z]+):\/\/([^/:]+)(?::(\d+))?(\/[^?#]*)?(\?[^#]*)?(#.*)?$/i);
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

  // Mark bootstrap as complete
  globalThis.__howth_ready = true;

})(globalThis);
