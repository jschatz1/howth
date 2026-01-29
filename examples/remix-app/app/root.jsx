import {
  Links,
  LiveReload,
  Meta,
  Outlet,
  Scripts,
  ScrollRestoration,
  Link,
} from "@remix-run/react";

const styles = `
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, sans-serif; background: #f5f5f5; color: #333; }
  .container { max-width: 640px; margin: 2rem auto; padding: 0 1rem; }
  .card { background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1rem; }
  h1 { font-size: 2rem; margin-bottom: 0.5rem; }
  h2 { font-size: 1.2rem; margin-bottom: 1rem; }
  .subtitle { color: #666; margin-bottom: 2rem; }
  .badge { background: #4f46e5; color: #fff; border-radius: 12px; padding: 2px 8px; font-size: 0.8rem; }
  nav { margin-top: 2rem; }
  nav a { color: #4f46e5; text-decoration: none; margin-right: 1rem; }
  nav a:hover { text-decoration: underline; }
  ul { list-style: none; }
  li { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  li:last-child { border-bottom: none; }
  li.done { color: #999; text-decoration: line-through; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  td:first-child { font-weight: 600; width: 40%; }
  input[type="text"] { padding: 0.5rem; border: 1px solid #ddd; border-radius: 4px; width: 70%; margin-right: 0.5rem; }
  button { padding: 0.5rem 1rem; background: #4f46e5; color: #fff; border: none; border-radius: 4px; cursor: pointer; }
  button:hover { background: #4338ca; }
  .todo-form { display: flex; margin-bottom: 1rem; }
`;

export default function App() {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <Meta />
        <Links />
        <style dangerouslySetInnerHTML={{ __html: styles }} />
      </head>
      <body>
        <div className="container">
          <Outlet />
          <nav>
            <Link to="/">Home</Link>
            <Link to="/about">About</Link>
          </nav>
        </div>
        <ScrollRestoration />
        <Scripts />
        <LiveReload />
      </body>
    </html>
  );
}
