/**
 * Real-Time Game Example
 *
 * Demonstrates a simple game loop with worker threads handling
 * physics simulation while main thread handles rendering.
 */

const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

// Shared game state layout (Float32):
// [0]: player X position
// [1]: player Y position
// [2]: player velocity X
// [3]: player velocity Y
// [4]: score
// [5]: game running flag (1 = running, 0 = stopped)
// [6-13]: enemy positions (4 enemies, x/y each)

const STATE_SIZE = 14;
const PLAYER_X = 0, PLAYER_Y = 1, PLAYER_VX = 2, PLAYER_VY = 3;
const SCORE = 4, RUNNING = 5;
const ENEMIES_START = 6;

if (!isMainThread) {
  // Physics worker: update positions based on velocity
  const { gameState } = workerData;
  const state = new Float32Array(gameState);

  const GRAVITY = 0.5;
  const FRICTION = 0.98;
  const GROUND = 180;
  const TICK_MS = 16; // ~60 FPS

  function physicsTick() {
    if (Atomics.load(new Int32Array(gameState), RUNNING) === 0) {
      process.exit(0);
    }

    // Apply gravity to player
    state[PLAYER_VY] += GRAVITY;

    // Apply friction
    state[PLAYER_VX] *= FRICTION;

    // Update player position
    state[PLAYER_X] += state[PLAYER_VX];
    state[PLAYER_Y] += state[PLAYER_VY];

    // Ground collision
    if (state[PLAYER_Y] > GROUND) {
      state[PLAYER_Y] = GROUND;
      state[PLAYER_VY] = 0;
    }

    // Walls
    if (state[PLAYER_X] < 0) state[PLAYER_X] = 0;
    if (state[PLAYER_X] > 300) state[PLAYER_X] = 300;

    // Move enemies
    for (let i = 0; i < 4; i++) {
      const ex = ENEMIES_START + i * 2;
      state[ex] += (Math.random() - 0.5) * 4;
      state[ex + 1] += (Math.random() - 0.5) * 4;

      // Keep enemies in bounds
      state[ex] = Math.max(0, Math.min(300, state[ex]));
      state[ex + 1] = Math.max(0, Math.min(200, state[ex + 1]));

      // Collision detection with player
      const dx = state[PLAYER_X] - state[ex];
      const dy = state[PLAYER_Y] - state[ex + 1];
      const dist = Math.sqrt(dx * dx + dy * dy);

      if (dist < 20) {
        // "Collect" enemy - respawn and add score
        state[ex] = Math.random() * 300;
        state[ex + 1] = Math.random() * 150;
        state[SCORE] += 10;
      }
    }

    setTimeout(physicsTick, TICK_MS);
  }

  physicsTick();
}

// Main thread: rendering and input
async function main() {
  console.log('=== Real-Time Game Example ===\n');
  console.log('A simple game demonstrating physics in a worker thread.');
  console.log('The player (P) collects enemies (E) for points.\n');

  // Initialize shared game state
  const gameState = new SharedArrayBuffer(STATE_SIZE * 4);
  const state = new Float32Array(gameState);
  const stateInt = new Int32Array(gameState);

  // Initialize player at center
  state[PLAYER_X] = 150;
  state[PLAYER_Y] = 100;
  state[PLAYER_VX] = 0;
  state[PLAYER_VY] = 0;
  state[SCORE] = 0;
  stateInt[RUNNING] = 1;

  // Initialize enemies at random positions
  for (let i = 0; i < 4; i++) {
    state[ENEMIES_START + i * 2] = Math.random() * 300;
    state[ENEMIES_START + i * 2 + 1] = Math.random() * 150;
  }

  // Start physics worker
  const physicsWorker = new Worker(__filename, {
    workerData: { gameState }
  });

  // Simple ASCII renderer
  function render() {
    const width = 40;
    const height = 12;
    const scaleX = 300 / width;
    const scaleY = 200 / height;

    // Create empty grid
    const grid = Array(height).fill(null).map(() => Array(width).fill(' '));

    // Draw player
    const px = Math.floor(state[PLAYER_X] / scaleX);
    const py = Math.floor(state[PLAYER_Y] / scaleY);
    if (px >= 0 && px < width && py >= 0 && py < height) {
      grid[py][px] = 'P';
    }

    // Draw enemies
    for (let i = 0; i < 4; i++) {
      const ex = Math.floor(state[ENEMIES_START + i * 2] / scaleX);
      const ey = Math.floor(state[ENEMIES_START + i * 2 + 1] / scaleY);
      if (ex >= 0 && ex < width && ey >= 0 && ey < height) {
        grid[ey][ex] = 'E';
      }
    }

    // Draw border and grid
    console.clear();
    console.log('┌' + '─'.repeat(width) + '┐');
    for (const row of grid) {
      console.log('│' + row.join('') + '│');
    }
    console.log('└' + '─'.repeat(width) + '┘');
    console.log(`Score: ${Math.floor(state[SCORE])}  |  Position: (${state[PLAYER_X].toFixed(0)}, ${state[PLAYER_Y].toFixed(0)})`);
    console.log('\nPhysics running in worker thread!');
  }

  // Simulate some player input (random jumps and movement)
  function simulateInput() {
    if (Math.random() < 0.1 && state[PLAYER_Y] >= 180) {
      // Jump
      state[PLAYER_VY] = -15;
    }
    if (Math.random() < 0.3) {
      // Move left/right
      state[PLAYER_VX] += (Math.random() - 0.5) * 10;
    }
  }

  // Game loop
  let frames = 0;
  const maxFrames = 60; // Run for ~1 second

  const gameLoop = setInterval(() => {
    simulateInput();
    render();
    frames++;

    if (frames >= maxFrames) {
      clearInterval(gameLoop);
      stateInt[RUNNING] = 0;

      console.log('\n\n=== Game Over ===');
      console.log(`Final Score: ${Math.floor(state[SCORE])}`);
      console.log('\n✓ Physics worker ran independently from render loop');

      // Give worker time to exit
      setTimeout(() => process.exit(0), 100);
    }
  }, 50);
}

main().catch(console.error);
