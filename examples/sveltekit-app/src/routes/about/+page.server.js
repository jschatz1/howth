export function load() {
	return {
		runtime: process.versions ? `Howth (V8 ${process.versions.v8 || 'unknown'})` : 'Howth',
		nodeVersion: process.version || 'unknown',
		platform: process.platform || 'unknown',
		arch: process.arch || 'unknown',
	};
}
