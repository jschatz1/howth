import { json } from "@remix-run/node";
import { useLoaderData } from "@remix-run/react";

export async function loader() {
  return json({
    runtime: process.versions
      ? `Howth (V8 ${process.versions.v8 || "unknown"})`
      : "Howth",
    nodeVersion: process.version || "unknown",
    platform: process.platform || "unknown",
    arch: process.arch || "unknown",
    uptime: Math.floor(process.uptime()) + "s",
  });
}

export default function About() {
  const data = useLoaderData();

  return (
    <>
      <h1>About</h1>
      <p className="subtitle">Runtime information</p>

      <div className="card">
        <h2>Environment</h2>
        <table>
          <tbody>
            <tr>
              <td>Runtime</td>
              <td>{data.runtime}</td>
            </tr>
            <tr>
              <td>Node Version</td>
              <td>{data.nodeVersion}</td>
            </tr>
            <tr>
              <td>Platform</td>
              <td>{data.platform}</td>
            </tr>
            <tr>
              <td>Architecture</td>
              <td>{data.arch}</td>
            </tr>
            <tr>
              <td>Uptime</td>
              <td>{data.uptime}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </>
  );
}
