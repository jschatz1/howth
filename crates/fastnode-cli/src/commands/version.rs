use fastnode_core::version::version_string;
use miette::Result;

pub fn run() -> Result<()> {
    println!("{}", version_string());
    Ok(())
}
