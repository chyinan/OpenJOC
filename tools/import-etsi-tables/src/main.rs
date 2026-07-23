// pattern: Imperative Shell

use import_etsi_tables::import_archive;
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("failed to import ETSI tables: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let archive = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: import-etsi-tables <ts_103420v010201p0.zip> <output.rs>")?,
    );
    let output = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: import-etsi-tables <ts_103420v010201p0.zip> <output.rs>")?,
    );
    if arguments.next().is_some() {
        return Err("usage: import-etsi-tables <ts_103420v010201p0.zip> <output.rs>".into());
    }

    let archive_bytes = fs::read(&archive)?;
    let imported = import_archive(&archive_bytes)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, imported.to_rust_source())?;
    Ok(())
}
