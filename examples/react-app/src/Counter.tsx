import React, { useState } from "react";

export default function Counter() {
  const [count, setCount] = useState(0);

  return (
    <div style={{ padding: "1rem", border: "1px solid #ddd", borderRadius: "8px", marginTop: "1rem" }}>
      <h2>Counter: {count}</h2>
      <div style={{ display: "flex", gap: "0.5rem" }}>
        <button onClick={() => setCount(c => c - 1)}>-</button>
        <button onClick={() => setCount(c => c + 1)}>+</button>
        <button onClick={() => setCount(0)}>Reset</button>
      </div>
      <p style={{ fontSize: "0.875rem", color: "#666", marginTop: "0.5rem" }}>
        State is preserved during HMR!
      </p>
    </div>
  );
}
