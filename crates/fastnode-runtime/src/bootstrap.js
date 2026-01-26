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

  // Mark bootstrap as complete
  globalThis.__howth_ready = true;

})(globalThis);
