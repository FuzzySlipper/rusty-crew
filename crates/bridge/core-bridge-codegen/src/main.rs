use anyhow::Result;

fn main() -> Result<()> {
    let manifest = include_str!("../../core-bridge-api/bridge-manifest.toml");
    let operation_count = manifest.matches("[[operation]]").count();
    println!("core bridge codegen scaffold: found {operation_count} manifest operations");
    Ok(())
}
