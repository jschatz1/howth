const React = require('react');
const e = React.createElement;

const styles = `
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, sans-serif; background: #f5f5f5; color: #333; }
  .container { max-width: 640px; margin: 2rem auto; padding: 0 1rem; }
  h1 { font-size: 2rem; margin-bottom: 0.5rem; }
  .subtitle { color: #666; margin-bottom: 2rem; }
  .card { background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1rem; }
  .card h2 { font-size: 1.2rem; margin-bottom: 1rem; }
  ul { list-style: none; }
  li { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  li:last-child { border-bottom: none; }
  li.done { color: #999; text-decoration: line-through; }
  .badge { background: #4f46e5; color: #fff; border-radius: 12px; padding: 2px 8px; font-size: 0.8rem; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
  td:first-child { font-weight: 600; width: 40%; }
  nav { margin-top: 2rem; }
  nav a { color: #4f46e5; text-decoration: none; margin-right: 1rem; }
  nav a:hover { text-decoration: underline; }
`;

function Layout(props) {
  return e('html', { lang: 'en' },
    e('head', null,
      e('meta', { charSet: 'UTF-8' }),
      e('meta', { name: 'viewport', content: 'width=device-width, initial-scale=1.0' }),
      e('title', null, 'React SSR on Howth'),
      e('style', { dangerouslySetInnerHTML: { __html: styles } })
    ),
    e('body', null,
      e('div', { className: 'container' },
        props.children,
        e('nav', null,
          e('a', { href: '/' }, 'Home'),
          e('a', { href: '/about' }, 'About'),
          e('a', { href: '/api/todos' }, 'API: Todos'),
          e('a', { href: '/api/health' }, 'API: Health')
        )
      )
    )
  );
}

module.exports = Layout;
