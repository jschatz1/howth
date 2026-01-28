// Simple reactive counter app
let count = 0;

function render() {
  document.getElementById('counter').innerHTML = `
    <p>Count: <strong>${count}</strong></p>
    <button onclick="increment()">Increment</button>
    <button onclick="decrement()">Decrement</button>
  `;
}

window.increment = () => { count++; render(); };
window.decrement = () => { count--; render(); };

// Fetch time from API
async function fetchTime() {
  try {
    const res = await fetch('/api/time');
    const data = await res.json();
    document.getElementById('time').innerHTML = `
      <p>Server time: ${data.time}</p>
    `;
  } catch (e) {
    console.error('Failed to fetch time:', e);
  }
}

// Initialize
render();
fetchTime();
setInterval(fetchTime, 5000);

console.log('App loaded! Try editing public/app.js or public/style.css');
