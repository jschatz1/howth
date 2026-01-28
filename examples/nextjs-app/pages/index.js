export default function Home() {
  return (
    <div style={{ padding: '2rem', fontFamily: 'system-ui, sans-serif' }}>
      <h1>Welcome to Next.js on Howth!</h1>
      <p>This is a Next.js application running on the Howth runtime.</p>
      <ul>
        <li><a href="/about">About Page</a></li>
        <li><a href="/api/hello">API Route</a></li>
      </ul>
    </div>
  );
}
