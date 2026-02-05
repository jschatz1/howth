import React from "react";
import Counter from "./Counter";

export default function App() {
  return (
    <div style={{ fontFamily: "system-ui", padding: "2rem", maxWidth: "600px", margin: "0 auto" }}>
      <h1>Howth React Example</h1>
      <p>This is a minimal React app running on Howth's dev server.</p>

      <Counter />

      <div style={{ marginTop: "2rem", padding: "1rem", background: "#f5f5f5", borderRadius: "8px" }}>
        <h3>Try editing this file!</h3>
        <p>Changes will hot-reload instantly with state preserved.</p>
      </div>
    </div>
  );
}
