const http = require("http");

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);

  if (url.pathname === "/") {
    res.writeHead(200, { "Content-Type": "text/html" });
    res.end(`
      <html>
        <head><title>Howth HTTP Server</title></head>
        <body>
          <h1>Hello from Howth!</h1>
          <p>Try these routes:</p>
          <ul>
            <li><a href="/api/hello">/api/hello</a> - JSON greeting</li>
            <li><a href="/api/time">/api/time</a> - Current time</li>
          </ul>
        </body>
      </html>
    `);
  } else if (url.pathname === "/api/hello") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ message: "Hello from Howth!" }));
  } else if (url.pathname === "/api/time") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ time: new Date().toISOString() }));
  } else {
    res.writeHead(404, { "Content-Type": "text/plain" });
    res.end("Not Found");
  }
});

const PORT = process.env.PORT || 3000;
server.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}`);
});
