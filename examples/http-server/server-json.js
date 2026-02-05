const http = require("http");

// Simple in-memory data store
const todos = [
  { id: 1, title: "Learn Howth", completed: true },
  { id: 2, title: "Build something awesome", completed: false },
];

let nextId = 3;

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);

  // CORS headers
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, POST, DELETE");
  res.setHeader("Content-Type", "application/json");

  if (req.method === "GET" && url.pathname === "/api/todos") {
    res.writeHead(200);
    res.end(JSON.stringify(todos));
  } else if (req.method === "POST" && url.pathname === "/api/todos") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const { title } = JSON.parse(body);
      const todo = { id: nextId++, title, completed: false };
      todos.push(todo);
      res.writeHead(201);
      res.end(JSON.stringify(todo));
    });
  } else if (req.method === "DELETE" && url.pathname.startsWith("/api/todos/")) {
    const id = parseInt(url.pathname.split("/").pop());
    const index = todos.findIndex((t) => t.id === id);
    if (index !== -1) {
      todos.splice(index, 1);
      res.writeHead(204);
      res.end();
    } else {
      res.writeHead(404);
      res.end(JSON.stringify({ error: "Not found" }));
    }
  } else {
    res.writeHead(404);
    res.end(JSON.stringify({ error: "Not found" }));
  }
});

const PORT = process.env.PORT || 3000;
server.listen(PORT, () => {
  console.log(`JSON API server running at http://localhost:${PORT}`);
  console.log("Endpoints:");
  console.log("  GET    /api/todos     - List all todos");
  console.log("  POST   /api/todos     - Create a todo");
  console.log("  DELETE /api/todos/:id - Delete a todo");
});
