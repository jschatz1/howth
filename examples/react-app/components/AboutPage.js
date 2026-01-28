const React = require('react');
const e = React.createElement;

function AboutPage(props) {
  const { runtime, nodeVersion, platform, arch } = props;

  return e(React.Fragment, null,
    e('h1', null, 'About'),
    e('div', { className: 'card' },
      e('table', null,
        e('tbody', null,
          e('tr', null, e('td', null, 'Runtime'), e('td', null, runtime)),
          e('tr', null, e('td', null, 'Node Version'), e('td', null, nodeVersion)),
          e('tr', null, e('td', null, 'Platform'), e('td', null, platform)),
          e('tr', null, e('td', null, 'Architecture'), e('td', null, arch))
        )
      )
    )
  );
}

module.exports = AboutPage;
