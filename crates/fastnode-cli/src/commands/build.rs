use fastnode_core::Config;
use miette::Result;
use tracing::info;

pub fn run(config: &Config) -> Result<()> {
    info!(cwd = %config.cwd.display(), "BUILD command invoked");
    println!("BUILD not implemented yet (cwd: {})", config.cwd.display());
    Ok(())
}
