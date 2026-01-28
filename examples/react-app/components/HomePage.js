const React = require('react');
const e = React.createElement;

function HomePage(props) {
  const { todos } = props;

  return e(React.Fragment, null,
    e('h1', null, 'React SSR on Howth'),
    e('p', { className: 'subtitle' }, 'A todo app with server-side rendered React'),
    e('div', { className: 'card' },
      e('h2', null,
        'Todos ',
        e('span', { className: 'badge' }, String(todos.length))
      ),
      e('ul', null,
        todos.map(t =>
          e('li', { key: t.id, className: t.done ? 'done' : '' },
            t.title, t.done ? ' \u2713' : ''
          )
        )
      )
    )
  );
}

module.exports = HomePage;
