import React from 'react';

interface CounterProps {
  count: number;
  onIncrement: () => void;
}

export function Counter({ count, onIncrement }: CounterProps) {
  return (
    <div className="counter">
      <span>Count: {count}</span>
      <button onClick={onIncrement}>+</button>
    </div>
  );
}
