use deno_core::{JsRuntimeForSnapshot, RuntimeOptions};

fn main() {
    let mut runtime = JsRuntimeForSnapshot::new(RuntimeOptions {
        ..Default::default()
    });

    let bootstrap = include_str!("src/bootstrap.js");
    runtime
        .execute_script("<howth:bootstrap>", bootstrap.to_string())
        .expect("Failed to execute bootstrap.js for snapshot");

    let snapshot = runtime.snapshot();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("SNAPSHOT.bin");
    std::fs::write(&path, &*snapshot).unwrap();

    println!("cargo:rerun-if-changed=src/bootstrap.js");
    println!("cargo:rerun-if-changed=build.rs");
}
