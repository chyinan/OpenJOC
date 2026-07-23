use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn cli_writes_verified_rust_source() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let archive = manifest.join("../../references/etsi/ts_103420v010201p0.zip");
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
