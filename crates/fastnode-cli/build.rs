// Build script for fastnode-cli.
// On macOS, export napi_* symbols so dlopen'd .node files can find them.

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "macos" {
        // -export_dynamic makes all global symbols visible to dlopen'd shared
        // libraries.  Native .node addons resolve napi_* symbols at load time
        // via dlsym, so they must be in the dynamic symbol table.
        println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-arg=-Wl,--export-dynamic");
    }
}
