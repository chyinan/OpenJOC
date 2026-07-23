// pattern: Imperative Shell

use import_etsi_tables::import_archive;
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = generate_tables() {
        panic!("failed to generate verified ETSI QMF table: {error}");
    }
}

fn generate_tables() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest dir")?);
    let archive = manifest.join("../../references/etsi/ts_103420v010201p0.zip");
    println!("cargo:rerun-if-changed={}", archive.display());
    let imported = import_archive(&fs::read(archive)?)?;
    let output =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("missing output dir")?).join("etsi_tables.rs");
    fs::write(output, imported.to_rust_source())?;
    Ok(())
}
