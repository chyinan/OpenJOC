use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn cli_writes_verified_rust_source() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let archive = std::env::var_os("OPENJOC_ETSI_TABLE_ARCHIVE").map_or_else(
        || manifest.join("../../references/etsi/ts_103420v010201p0.zip"),
        PathBuf::from,
    );
    match fs::metadata(&archive) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "skipping official companion CLI test: set OPENJOC_ETSI_TABLE_ARCHIVE ({})",
                archive.display()
            );
            return;
        }
        Err(error) => panic!("failed to inspect {}: {error}", archive.display()),
    }
    let output = std::env::temp_dir().join(format!(
        "openjoc-etsi-tables-{}-{}.rs",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));

    let status = Command::new(env!("CARGO_BIN_EXE_import-etsi-tables"))
        .arg(&archive)
        .arg(&output)
        .status()
        .expect("importer process should start");

    assert!(status.success());
    let generated = fs::read_to_string(&output).expect("generated Rust should be readable");
    assert!(generated.contains("pub const PROT64: [f32; 640]"));
    fs::remove_file(output).expect("temporary generated source should be removable");
}
