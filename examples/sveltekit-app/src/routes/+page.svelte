<script>
	let { data } = $props();
</script>

<svelte:head>
	<title>SvelteKit on Howth</title>
</svelte:head>

<h1>SvelteKit on Howth</h1>
<p class="subtitle">A todo app running on the Howth runtime</p>

<div class="card">
	<h2>Todos <span class="badge">{data.todos.length}</span></h2>
	<ul>
		{#each data.todos as todo (todo.id)}
			<li class:done={todo.done}>
				{todo.title} {todo.done ? '\u2713' : ''}
			</li>
		{/each}
	</ul>
</div>

<style>
	h1 { font-size: 2rem; margin-bottom: 0.5rem; }
	.subtitle { color: #666; margin-bottom: 2rem; }
	.card { background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1rem; }
	.card h2 { font-size: 1.2rem; margin-bottom: 1rem; }
	.badge { background: #4f46e5; color: #fff; border-radius: 12px; padding: 2px 8px; font-size: 0.8rem; }
	ul { list-style: none; }
	li { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
	li:last-child { border-bottom: none; }
	.done { color: #999; text-decoration: line-through; }
</style>
