<script>
	let { children } = $props();
</script>

<svelte:head>
	<title>SvelteKit on Howth</title>
</svelte:head>

<div class="container">
	{@render children()}
	<nav>
		<a href="/">Home</a>
		<a href="/about">About</a>
	</nav>
</div>

<style>
	:global(*) { margin: 0; padding: 0; box-sizing: border-box; }
	:global(body) { font-family: system-ui, -apple-system, sans-serif; background: #f5f5f5; color: #333; }
	.container { max-width: 640px; margin: 2rem auto; padding: 0 1rem; }
	nav { margin-top: 2rem; }
	nav a { color: #4f46e5; text-decoration: none; margin-right: 1rem; }
	nav a:hover { text-decoration: underline; }
</style>
