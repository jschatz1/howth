export default function handler(req, res) {
  res.status(200).json({
    message: 'Hello from Next.js API!',
    runtime: 'howth',
    timestamp: new Date().toISOString(),
  });
}
