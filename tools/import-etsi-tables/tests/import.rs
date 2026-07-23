use import_etsi_tables::{ImportError, TABLES_C_SHA256, ZIP_SHA256, import_archive};
use std::fs;
use std::path::PathBuf;

fn official_archive() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/etsi/ts_103420v010201p0.zip");
    fs::read(path).expect("official ETSI companion archive is required for this test")
}

#[test]
fn imports_the_verified_official_companion_tables() {
    let imported = import_archive(&official_archive()).expect("official archive must import");

    assert_eq!(imported.zip_sha256, ZIP_SHA256);
    assert_eq!(imported.source_sha256, TABLES_C_SHA256);
    assert_eq!(imported.coarse_generic.len(), 95);
    assert_eq!(imported.fine_generic.len(), 191);
    assert_eq!(imported.coarse_coeff_sparse.len(), 95);
    assert_eq!(imported.fine_coeff_sparse.len(), 191);
    assert_eq!(imported.pos_index_5ch_sparse.len(), 4);
    assert_eq!(imported.pos_index_7ch_sparse.len(), 6);
    assert_eq!(imported.prototype_64.len(), 640);
}

#[test]
fn rejects_an_archive_with_any_changed_byte() {
    let mut archive = official_archive();
    archive[20] ^= 1;

    assert!(matches!(
        import_archive(&archive),
        Err(ImportError::ArchiveHashMismatch { .. })
    ));
}

#[test]
fn generated_rust_records_normative_provenance_and_all_tables() {
    let imported = import_archive(&official_archive()).expect("official archive must import");
    let generated = imported.to_rust_source();

    assert!(generated.contains("source: ts_103420_tables.c"));
    assert!(generated.contains(TABLES_C_SHA256));
    for name in [
        "JOC_HUFF_CODE_COARSE_GENERIC",
        "JOC_HUFF_CODE_FINE_GENERIC",
        "JOC_HUFF_CODE_COARSE_COEFF_SPARSE",
        "JOC_HUFF_CODE_FINE_COEFF_SPARSE",
        "JOC_HUFF_CODE_5CH_POS_INDEX_SPARSE",
        "JOC_HUFF_CODE_7CH_POS_INDEX_SPARSE",
        "PROT64",
    ] {
        assert!(generated.contains(name), "missing generated table {name}");
    }
}
