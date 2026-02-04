import React, { useState } from 'react';

// Test relative imports
import { Counter } from './components/Counter';
import { useToggle } from './hooks/useToggle';

// Test JSON import
import data from './data.json';

export function App() {
  const [count, setCount] = useState(0);
  const [isVisible, toggle] = useToggle(true);

  return (
    <div className="app">
      <h1>Vite Compat Test</h1>
      <p>Data from JSON: {data.message}</p>
      <Counter count={count} onIncrement={() => setCount(c => c + 1)} />
      <button onClick={toggle}>
        {isVisible ? 'Hide' : 'Show'}
      </button>
      {isVisible && <p>Visible content</p>}
    </div>
  );
}
