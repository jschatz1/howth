<script>
	let { data } = $props();
</script>

<svelte:head>
	<title>About - SvelteKit on Howth</title>
</svelte:head>

<h1>About</h1>

<div class="card">
	<table>
		<tbody>
			<tr><td>Runtime</td><td>{data.runtime}</td></tr>
			<tr><td>Node Version</td><td>{data.nodeVersion}</td></tr>
			<tr><td>Platform</td><td>{data.platform}</td></tr>
			<tr><td>Architecture</td><td>{data.arch}</td></tr>
		</tbody>
	</table>
</div>

<style>
	h1 { font-size: 2rem; margin-bottom: 2rem; }
	.card { background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1rem; }
	table { width: 100%; border-collapse: collapse; }
	td { padding: 0.5rem 0; border-bottom: 1px solid #eee; }
	td:first-child { font-weight: 600; width: 40%; }
</style>
