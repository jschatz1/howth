/// Integration test: try bundling the rolldown benchmark apps/1000 fixture.
/// This is not a CI test - it requires /tmp/rolldown-benchmarks to be cloned.
use std::path::Path;
use std::time::Instant;

#[test]
fn test_bundle_rolldown_bench_1000() {
    let app_dir = Path::new("/tmp/rolldown-benchmarks/apps/1000");
    if !app_dir.exists() {
        eprintln!("Skipping: /tmp/rolldown-benchmarks not cloned");
        return;
    }

    let entry = app_dir.join("src/index.jsx");
    let workspace_root = Path::new("/tmp/rolldown-benchmarks");

    let bundler = fastnode_core::bundler::Bundler::with_cwd(app_dir);

    let options = fastnode_core::bundler::BundleOptions {
        minify: true,
        sourcemap: true,
        scope_hoist: true,
        ..Default::default()
    };

    // Warmup run
    let _ = bundler.bundle(&entry, workspace_root, &options);

    // Measured runs
    let mut times = Vec::new();
    let runs = 3;
    let mut last_result = None;

    for i in 0..runs {
        let start = Instant::now();
        let result = bundler.bundle(&entry, workspace_root, &options);
        let elapsed = start.elapsed();
        times.push(elapsed.as_secs_f64() * 1000.0);

        match result {
            Ok(r) => {
                println!(
                    "Run {}: {:.2}ms ({} modules, {:.2} KB JS)",
                    i + 1,
                    elapsed.as_secs_f64() * 1000.0,
                    r.modules.len(),
                    r.code.len() as f64 / 1024.0
                );
                last_result = Some(r);
            }
            Err(e) => panic!("Bundle failed: {}", e),
        }
    }

    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    println!("\nMedian: {:.2}ms (over {} runs)", median, runs);

    // Write output for size comparison
    if let Some(r) = last_result {
        let out_dir = app_dir.join("dist-howth");
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::write(out_dir.join("main.js"), &r.code).unwrap();
        if let Some(ref map) = r.map {
            std::fs::write(out_dir.join("main.js.map"), map).unwrap();
        }
        if let Some(ref css) = r.css {
            std::fs::write(out_dir.join(&css.name), &css.code).unwrap();
        }
        println!("Output written to {}", out_dir.display());
        println!("  JS: {:.2} MB", r.code.len() as f64 / (1024.0 * 1024.0));
        if let Some(ref map) = r.map {
            println!(
                "  Sourcemap: {:.2} MB",
                map.len() as f64 / (1024.0 * 1024.0)
            );
        }
        assert!(r.modules.len() > 100);
    }
}
