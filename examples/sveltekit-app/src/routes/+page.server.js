const todos = [
	{ id: 1, title: 'Try SvelteKit on Howth', done: true },
	{ id: 2, title: 'Build something cool', done: false },
	{ id: 3, title: 'Read the docs', done: false },
];

export function load() {
	return { todos };
}
