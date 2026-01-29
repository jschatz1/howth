var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf, __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: !0 });
}, __copyProps = (to, from, except, desc) => {
  if (from && typeof from == "object" || typeof from == "function")
    for (let key of __getOwnPropNames(from))
      !__hasOwnProp.call(to, key) && key !== except && __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: !0 }) : target,
  mod
)), __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: !0 }), mod);

// <stdin>
var stdin_exports = {};
__export(stdin_exports, {
  assets: () => assets_manifest_default,
  assetsBuildDirectory: () => assetsBuildDirectory,
  entry: () => entry,
  future: () => future,
  mode: () => mode,
  publicPath: () => publicPath,
  routes: () => routes
});
module.exports = __toCommonJS(stdin_exports);

// node_modules/.pnpm/@remix-run+dev@2.17.4_@remix-run+react@2.17.4_react-dom@18.3.1_react@18.3.1__react@18.3_6fe10aa011872bfa63a1fb3171833573/node_modules/@remix-run/dev/dist/config/defaults/entry.server.node.tsx
var entry_server_node_exports = {};
__export(entry_server_node_exports, {
  default: () => handleRequest
});
var import_node_stream = require("node:stream"), import_node = require("@remix-run/node"), import_react = require("@remix-run/react"), isbotModule = __toESM(require("isbot")), import_server = require("react-dom/server"), import_jsx_runtime = require("react/jsx-runtime"), ABORT_DELAY = 5e3;
function handleRequest(request, responseStatusCode, responseHeaders, remixContext, loadContext) {
  return isBotRequest(request.headers.get("user-agent")) || remixContext.isSpaMode ? handleBotRequest(
    request,
    responseStatusCode,
    responseHeaders,
    remixContext
  ) : handleBrowserRequest(
    request,
    responseStatusCode,
    responseHeaders,
    remixContext
  );
}
function isBotRequest(userAgent) {
  return userAgent ? "isbot" in isbotModule && typeof isbotModule.isbot == "function" ? isbotModule.isbot(userAgent) : "default" in isbotModule && typeof isbotModule.default == "function" ? isbotModule.default(userAgent) : !1 : !1;
}
function handleBotRequest(request, responseStatusCode, responseHeaders, remixContext) {
  return new Promise((resolve, reject) => {
    let shellRendered = !1, { pipe, abort } = (0, import_server.renderToPipeableStream)(
      /* @__PURE__ */ (0, import_jsx_runtime.jsx)(
        import_react.RemixServer,
        {
          context: remixContext,
          url: request.url,
          abortDelay: ABORT_DELAY
        }
      ),
      {
        onAllReady() {
          shellRendered = !0;
          let body = new import_node_stream.PassThrough(), stream = (0, import_node.createReadableStreamFromReadable)(body);
          responseHeaders.set("Content-Type", "text/html"), resolve(
            new Response(stream, {
              headers: responseHeaders,
              status: responseStatusCode
            })
          ), pipe(body);
        },
        onShellError(error) {
          reject(error);
        },
        onError(error) {
          responseStatusCode = 500, shellRendered && console.error(error);
        }
      }
    );
    setTimeout(abort, ABORT_DELAY);
  });
}
function handleBrowserRequest(request, responseStatusCode, responseHeaders, remixContext) {
  return new Promise((resolve, reject) => {
    let shellRendered = !1, { pipe, abort } = (0, import_server.renderToPipeableStream)(
      /* @__PURE__ */ (0, import_jsx_runtime.jsx)(
        import_react.RemixServer,
        {
          context: remixContext,
          url: request.url,
          abortDelay: ABORT_DELAY
        }
      ),
      {
        onShellReady() {
          shellRendered = !0;
          let body = new import_node_stream.PassThrough(), stream = (0, import_node.createReadableStreamFromReadable)(body);
          responseHeaders.set("Content-Type", "text/html"), resolve(
            new Response(stream, {
              headers: responseHeaders,
              status: responseStatusCode
            })
          ), pipe(body);
        },
        onShellError(error) {
          reject(error);
        },
        onError(error) {
          responseStatusCode = 500, shellRendered && console.error(error);
        }
      }
    );
    setTimeout(abort, ABORT_DELAY);
  });
}

// app/root.jsx
var root_exports = {};
__export(root_exports, {
  default: () => App
});
var import_react2 = require("@remix-run/react"), import_jsx_runtime2 = require("react/jsx-runtime"), styles = `
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, sans-serif; background: #f5f5f5; color: #333; }
  .container { max-width: 640px; margin: 2rem auto; padding: 0 1rem; }
  .card { background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1rem; }
  h1 { font-size: 2rem; margin-bottom: 0.5rem; }
  h2 { font-size: 1.2rem; margin-bottom: 1rem; }
  .subtitle { color: #666; margin-bottom: 2rem; }
  .badge { background: #4f46e5; color: #fff; border-radius: 12px; padding: 2px 8px; font-size: 0.8rem; }
  nav { margin-top: 2rem; }
  nav a { color: #4f46e5; text-decoration: none; margin-right: 1rem; }
  nav a:hover { text-decoration: underline; }
  ul { list-style: none; }
  li { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  li:last-child { border-bottom: none; }
  li.done { color: #999; text-decoration: line-through; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  td:first-child { font-weight: 600; width: 40%; }
  input[type="text"] { padding: 0.5rem; border: 1px solid #ddd; border-radius: 4px; width: 70%; margin-right: 0.5rem; }
  button { padding: 0.5rem 1rem; background: #4f46e5; color: #fff; border: none; border-radius: 4px; cursor: pointer; }
  button:hover { background: #4338ca; }
  .todo-form { display: flex; margin-bottom: 1rem; }
`;
function App() {
  return /* @__PURE__ */ (0, import_jsx_runtime2.jsxs)("html", { lang: "en", children: [
    /* @__PURE__ */ (0, import_jsx_runtime2.jsxs)("head", { children: [
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)("meta", { charSet: "utf-8" }),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)("meta", { name: "viewport", content: "width=device-width, initial-scale=1" }),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Meta, {}),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Links, {}),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)("style", { dangerouslySetInnerHTML: { __html: styles } })
    ] }),
    /* @__PURE__ */ (0, import_jsx_runtime2.jsxs)("body", { children: [
      /* @__PURE__ */ (0, import_jsx_runtime2.jsxs)("div", { className: "container", children: [
        /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Outlet, {}),
        /* @__PURE__ */ (0, import_jsx_runtime2.jsxs)("nav", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Link, { to: "/", children: "Home" }),
          /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Link, { to: "/about", children: "About" })
        ] })
      ] }),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.ScrollRestoration, {}),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.Scripts, {}),
      /* @__PURE__ */ (0, import_jsx_runtime2.jsx)(import_react2.LiveReload, {})
    ] })
  ] });
}

// app/routes/_index.jsx
var index_exports = {};
__export(index_exports, {
  action: () => action,
  default: () => Index,
  loader: () => loader
});
var import_node2 = require("@remix-run/node"), import_react3 = require("@remix-run/react"), import_jsx_runtime3 = require("react/jsx-runtime"), todos = [
  { id: 1, title: "Try Remix on Howth", done: !0 },
  { id: 2, title: "Build something cool", done: !1 },
  { id: 3, title: "Read the docs", done: !1 }
], nextId = 4;
async function loader() {
  return (0, import_node2.json)({ todos });
}
async function action({ request }) {
  let formData = await request.formData(), intent = formData.get("intent");
  if (intent === "add") {
    let title = formData.get("title");
    title && todos.push({ id: nextId++, title, done: !1 });
  } else if (intent === "toggle") {
    let id = parseInt(formData.get("id"), 10), todo = todos.find((t) => t.id === id);
    todo && (todo.done = !todo.done);
  } else if (intent === "delete") {
    let id = parseInt(formData.get("id"), 10), idx = todos.findIndex((t) => t.id === id);
    idx !== -1 && todos.splice(idx, 1);
  }
  return (0, import_node2.json)({ todos });
}
function Index() {
  let { todos: todos2 } = (0, import_react3.useLoaderData)();
  return /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)(import_jsx_runtime3.Fragment, { children: [
    /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("h1", { children: "Remix on Howth" }),
    /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("p", { className: "subtitle", children: "A full-stack React framework running on the Howth runtime" }),
    /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)("div", { className: "card", children: [
      /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)("h2", { children: [
        "Todo List ",
        /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("span", { className: "badge", children: todos2.length })
      ] }),
      /* @__PURE__ */ (0, import_jsx_runtime3.jsx)(import_react3.Form, { method: "post", children: /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)("div", { className: "todo-form", children: [
        /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "hidden", name: "intent", value: "add" }),
        /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "text", name: "title", placeholder: "Add a new todo..." }),
        /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("button", { type: "submit", children: "Add" })
      ] }) }),
      /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("ul", { children: todos2.map((todo) => /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)("li", { className: todo.done ? "done" : "", children: [
        /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)(import_react3.Form, { method: "post", style: { display: "inline" }, children: [
          /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "hidden", name: "intent", value: "toggle" }),
          /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "hidden", name: "id", value: todo.id }),
          /* @__PURE__ */ (0, import_jsx_runtime3.jsx)(
            "button",
            {
              type: "submit",
              style: {
                background: "none",
                color: "#4f46e5",
                padding: "0",
                cursor: "pointer",
                textDecoration: todo.done ? "line-through" : "none"
              },
              children: todo.title
            }
          )
        ] }),
        todo.done ? " \u2713" : "",
        /* @__PURE__ */ (0, import_jsx_runtime3.jsxs)(
          import_react3.Form,
          {
            method: "post",
            style: { display: "inline", marginLeft: "0.5rem" },
            children: [
              /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "hidden", name: "intent", value: "delete" }),
              /* @__PURE__ */ (0, import_jsx_runtime3.jsx)("input", { type: "hidden", name: "id", value: todo.id }),
              /* @__PURE__ */ (0, import_jsx_runtime3.jsx)(
                "button",
                {
                  type: "submit",
                  style: {
                    background: "none",
                    color: "#999",
                    padding: "0",
                    fontSize: "0.8rem"
                  },
                  children: "x"
                }
              )
            ]
          }
        )
      ] }, todo.id)) })
    ] })
  ] });
}

// app/routes/about.jsx
var about_exports = {};
__export(about_exports, {
  default: () => About,
  loader: () => loader2
});
var import_node3 = require("@remix-run/node"), import_react4 = require("@remix-run/react"), import_jsx_runtime4 = require("react/jsx-runtime");
async function loader2() {
  return (0, import_node3.json)({
    runtime: process.versions ? `Howth (V8 ${process.versions.v8 || "unknown"})` : "Howth",
    nodeVersion: process.version || "unknown",
    platform: process.platform || "unknown",
    arch: process.arch || "unknown",
    uptime: Math.floor(process.uptime()) + "s"
  });
}
function About() {
  let data = (0, import_react4.useLoaderData)();
  return /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)(import_jsx_runtime4.Fragment, { children: [
    /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("h1", { children: "About" }),
    /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("p", { className: "subtitle", children: "Runtime information" }),
    /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("div", { className: "card", children: [
      /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("h2", { children: "Environment" }),
      /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("table", { children: /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tbody", { children: [
        /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tr", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: "Runtime" }),
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: data.runtime })
        ] }),
        /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tr", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: "Node Version" }),
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: data.nodeVersion })
        ] }),
        /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tr", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: "Platform" }),
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: data.platform })
        ] }),
        /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tr", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: "Architecture" }),
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: data.arch })
        ] }),
        /* @__PURE__ */ (0, import_jsx_runtime4.jsxs)("tr", { children: [
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: "Uptime" }),
          /* @__PURE__ */ (0, import_jsx_runtime4.jsx)("td", { children: data.uptime })
        ] })
      ] }) })
    ] })
  ] });
}

// server-assets-manifest:@remix-run/dev/assets-manifest
var assets_manifest_default = { entry: { module: "/build/entry.client-CVX4BRSG.js", imports: ["/build/_shared/chunk-KLB75NCB.js"] }, routes: { root: { id: "root", parentId: void 0, path: "", index: void 0, caseSensitive: void 0, module: "/build/root-46BEUQGY.js", imports: void 0, hasAction: !1, hasLoader: !1, hasClientAction: !1, hasClientLoader: !1, hasErrorBoundary: !1 }, "routes/_index": { id: "routes/_index", parentId: "root", path: void 0, index: !0, caseSensitive: void 0, module: "/build/routes/_index-M3C3MJOM.js", imports: ["/build/_shared/chunk-VG5YMS3V.js"], hasAction: !0, hasLoader: !0, hasClientAction: !1, hasClientLoader: !1, hasErrorBoundary: !1 }, "routes/about": { id: "routes/about", parentId: "root", path: "about", index: void 0, caseSensitive: void 0, module: "/build/routes/about-W2G5RJBX.js", imports: ["/build/_shared/chunk-VG5YMS3V.js"], hasAction: !1, hasLoader: !0, hasClientAction: !1, hasClientLoader: !1, hasErrorBoundary: !1 } }, version: "b01cb4aa", hmr: void 0, url: "/build/manifest-B01CB4AA.js" };

// server-entry-module:@remix-run/dev/server-build
var mode = "production", assetsBuildDirectory = "public/build", future = { v3_fetcherPersist: !1, v3_relativeSplatPath: !1, v3_throwAbortReason: !1, v3_routeConfig: !1, v3_singleFetch: !1, v3_lazyRouteDiscovery: !1, unstable_optimizeDeps: !1 }, publicPath = "/build/", entry = { module: entry_server_node_exports }, routes = {
  root: {
    id: "root",
    parentId: void 0,
    path: "",
    index: void 0,
    caseSensitive: void 0,
    module: root_exports
  },
  "routes/_index": {
    id: "routes/_index",
    parentId: "root",
    path: void 0,
    index: !0,
    caseSensitive: void 0,
    module: index_exports
  },
  "routes/about": {
    id: "routes/about",
    parentId: "root",
    path: "about",
    index: void 0,
    caseSensitive: void 0,
    module: about_exports
  }
};
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  assets,
  assetsBuildDirectory,
  entry,
  future,
  mode,
  publicPath,
  routes
});
