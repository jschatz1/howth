/**
 * WebSocket Chat Server Example
 *
 * A simple WebSocket implementation over HTTP:
 * - WebSocket handshake
 * - Frame encoding/decoding
 * - Multi-client broadcast
 * - Ping/pong keepalive
 *
 * Run: howth run --native examples/websocket-chat/chat.js
 * Connect with: wscat -c ws://localhost:3000 (or browser console)
 */

const http = require('http');
const crypto = require('crypto');

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

const PORT = process.env.PORT || 3000;
const WEBSOCKET_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

// Connected clients
const clients = new Map();
let clientId = 0;

// WebSocket frame opcodes
const OPCODES = {
  CONTINUATION: 0x0,
  TEXT: 0x1,
  BINARY: 0x2,
  CLOSE: 0x8,
  PING: 0x9,
  PONG: 0xA,
};

// Parse WebSocket frame
function parseFrame(buffer) {
  if (buffer.length < 2) return null;

  const firstByte = buffer[0];
  const secondByte = buffer[1];

  const fin = (firstByte & 0x80) !== 0;
  const opcode = firstByte & 0x0F;
  const masked = (secondByte & 0x80) !== 0;
  let payloadLength = secondByte & 0x7F;

  let offset = 2;

  if (payloadLength === 126) {
    if (buffer.length < 4) return null;
    payloadLength = buffer.readUInt16BE(2);
    offset = 4;
  } else if (payloadLength === 127) {
    if (buffer.length < 10) return null;
    payloadLength = Number(buffer.readBigUInt64BE(2));
    offset = 10;
  }

  let maskingKey = null;
  if (masked) {
    if (buffer.length < offset + 4) return null;
    maskingKey = buffer.slice(offset, offset + 4);
    offset += 4;
  }

  if (buffer.length < offset + payloadLength) return null;

  let payload = buffer.slice(offset, offset + payloadLength);

  // Unmask payload
  if (masked && maskingKey) {
    for (let i = 0; i < payload.length; i++) {
      payload[i] ^= maskingKey[i % 4];
    }
  }

  return {
    fin,
    opcode,
    payload,
    totalLength: offset + payloadLength,
  };
}

// Create WebSocket frame
function createFrame(opcode, payload) {
  const data = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  let header;

  if (data.length <= 125) {
    header = Buffer.alloc(2);
    header[0] = 0x80 | opcode; // FIN + opcode
    header[1] = data.length;
  } else if (data.length <= 65535) {
    header = Buffer.alloc(4);
    header[0] = 0x80 | opcode;
    header[1] = 126;
    header.writeUInt16BE(data.length, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x80 | opcode;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(data.length), 2);
  }

  return Buffer.concat([header, data]);
}

// Send message to client
function sendMessage(socket, message) {
  const frame = createFrame(OPCODES.TEXT, message);
  socket.write(frame);
}

// Broadcast to all clients
function broadcast(message, excludeId = null) {
  const frame = createFrame(OPCODES.TEXT, message);
  for (const [id, client] of clients) {
    if (id !== excludeId && !client.socket.destroyed) {
      client.socket.write(frame);
    }
  }
}

// Handle WebSocket upgrade
function handleUpgrade(req, socket) {
  const key = req.headers['sec-websocket-key'];
  if (!key) {
    socket.end('HTTP/1.1 400 Bad Request\r\n\r\n');
    return;
  }

  // Calculate accept key
  const acceptKey = crypto
    .createHash('sha1')
    .update(key + WEBSOCKET_GUID)
    .digest('base64');

  // Send upgrade response
  const response = [
    'HTTP/1.1 101 Switching Protocols',
    'Upgrade: websocket',
    'Connection: Upgrade',
    `Sec-WebSocket-Accept: ${acceptKey}`,
    '',
    '',
  ].join('\r\n');

  socket.write(response);

  // Register client
  const id = ++clientId;
  const client = {
    id,
    socket,
    buffer: Buffer.alloc(0),
    username: `User${id}`,
  };
  clients.set(id, client);

  console.log(`${c.green}+${c.reset} Client ${id} connected (${clients.size} total)`);

  // Send welcome message
  sendMessage(socket, JSON.stringify({
    type: 'system',
    message: `Welcome! You are ${client.username}. ${clients.size} user(s) online.`,
  }));

  // Notify others
  broadcast(JSON.stringify({
    type: 'system',
    message: `${client.username} joined the chat.`,
  }), id);

  // Handle incoming data
  socket.on('data', (data) => {
    client.buffer = Buffer.concat([client.buffer, data]);

    while (true) {
      const frame = parseFrame(client.buffer);
      if (!frame) break;

      client.buffer = client.buffer.slice(frame.totalLength);

      switch (frame.opcode) {
        case OPCODES.TEXT:
          handleMessage(client, frame.payload.toString());
          break;
        case OPCODES.CLOSE:
          socket.end(createFrame(OPCODES.CLOSE, ''));
          break;
        case OPCODES.PING:
          socket.write(createFrame(OPCODES.PONG, frame.payload));
          break;
      }
    }
  });

  // Handle disconnect
  socket.on('close', () => {
    clients.delete(id);
    console.log(`${c.red}-${c.reset} Client ${id} disconnected (${clients.size} total)`);

    broadcast(JSON.stringify({
      type: 'system',
      message: `${client.username} left the chat.`,
    }));
  });

  socket.on('error', () => {
    clients.delete(id);
  });
}

// Handle chat message
function handleMessage(client, text) {
  try {
    const data = JSON.parse(text);

    switch (data.type) {
      case 'chat':
        console.log(`${c.blue}[${client.username}]${c.reset} ${data.message}`);
        broadcast(JSON.stringify({
          type: 'chat',
          user: client.username,
          message: data.message,
          timestamp: new Date().toISOString(),
        }));
        break;

      case 'rename':
        const oldName = client.username;
        client.username = data.name.slice(0, 20);
        console.log(`${c.yellow}*${c.reset} ${oldName} renamed to ${client.username}`);
        broadcast(JSON.stringify({
          type: 'system',
          message: `${oldName} is now known as ${client.username}`,
        }));
        break;

      case 'list':
        const users = [...clients.values()].map(c => c.username);
        sendMessage(client.socket, JSON.stringify({
          type: 'users',
          users,
        }));
        break;
    }
  } catch (e) {
    // Plain text message
    console.log(`${c.blue}[${client.username}]${c.reset} ${text}`);
    broadcast(JSON.stringify({
      type: 'chat',
      user: client.username,
      message: text,
      timestamp: new Date().toISOString(),
    }));
  }
}

// Create HTTP server
const server = http.createServer((req, res) => {
  // Serve a simple HTML client
  if (req.url === '/' || req.url === '/index.html') {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!DOCTYPE html>
<html>
<head>
  <title>WebSocket Chat</title>
  <style>
    body { font-family: sans-serif; max-width: 600px; margin: 2rem auto; padding: 1rem; }
    #messages { height: 300px; overflow-y: auto; border: 1px solid #ccc; padding: 1rem; margin-bottom: 1rem; }
    .system { color: #888; font-style: italic; }
    .user { color: #667eea; font-weight: bold; }
    input { width: 80%; padding: 0.5rem; }
    button { padding: 0.5rem 1rem; }
  </style>
</head>
<body>
  <h1>WebSocket Chat</h1>
  <div id="messages"></div>
  <input type="text" id="input" placeholder="Type a message..." onkeypress="if(event.key==='Enter')send()">
  <button onclick="send()">Send</button>
  <script>
    const ws = new WebSocket('ws://' + location.host);
    const messages = document.getElementById('messages');
    const input = document.getElementById('input');

    ws.onmessage = (e) => {
      const data = JSON.parse(e.data);
      const div = document.createElement('div');
      if (data.type === 'system') {
        div.className = 'system';
        div.textContent = data.message;
      } else if (data.type === 'chat') {
        div.innerHTML = '<span class="user">' + data.user + ':</span> ' + data.message;
      } else if (data.type === 'users') {
        div.className = 'system';
        div.textContent = 'Online: ' + data.users.join(', ');
      }
      messages.appendChild(div);
      messages.scrollTop = messages.scrollHeight;
    };

    function send() {
      const text = input.value.trim();
      if (!text) return;
      if (text.startsWith('/name ')) {
        ws.send(JSON.stringify({ type: 'rename', name: text.slice(6) }));
      } else if (text === '/list') {
        ws.send(JSON.stringify({ type: 'list' }));
      } else {
        ws.send(JSON.stringify({ type: 'chat', message: text }));
      }
      input.value = '';
    }
  </script>
</body>
</html>`);
    return;
  }

  res.writeHead(404);
  res.end('Not Found');
});

// Handle upgrade requests
server.on('upgrade', (req, socket, head) => {
  if (req.headers.upgrade?.toLowerCase() === 'websocket') {
    handleUpgrade(req, socket);
  } else {
    socket.end('HTTP/1.1 400 Bad Request\r\n\r\n');
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`\n${c.bold}${c.cyan}WebSocket Chat Server${c.reset}`);
  console.log(`${c.dim}${'─'.repeat(40)}${c.reset}`);
  console.log(`  ${c.green}➜${c.reset}  Web:  ${c.cyan}http://localhost:${PORT}${c.reset}`);
  console.log(`  ${c.green}➜${c.reset}  WS:   ${c.cyan}ws://localhost:${PORT}${c.reset}`);
  console.log(`\n${c.dim}Commands in chat:${c.reset}`);
  console.log(`  /name <new-name>  - Change your name`);
  console.log(`  /list             - List online users`);
  console.log(`\n${c.dim}Press Ctrl+C to stop${c.reset}\n`);
});
