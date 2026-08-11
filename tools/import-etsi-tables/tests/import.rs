use import_etsi_tables::{ImportError, TABLES_C_SHA256, ZIP_SHA256, import_archive};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

fn official_archive() -> Option<Vec<u8>> {
    let path = env::var_os("OPENJOC_ETSI_TABLE_ARCHIVE").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../references/etsi/ts_103420v010201p0.zip")
        },
        PathBuf::from,
    );
    match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "skipping official companion test: set OPENJOC_ETSI_TABLE_ARCHIVE ({})",
                path.display()
            );
            None
        }
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

#[test]
fn imports_the_verified_official_companion_tables() {
    let Some(archive) = official_archive() else {
        return;
    };
    let imported = import_archive(&archive).expect("official archive must import");

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
    let Some(mut archive) = official_archive() else {
        return;
    };
    archive[20] ^= 1;

    assert!(matches!(
        import_archive(&archive),
        Err(ImportError::ArchiveHashMismatch { .. })
    ));
}

#[test]
fn generated_rust_records_normative_provenance_and_all_tables() {
    let Some(archive) = official_archive() else {
        return;
    };
    let imported = import_archive(&archive).expect("official archive must import");
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

#[test]
fn committed_runtime_tables_match_the_verified_generator() {
    let Some(archive) = official_archive() else {
        return;
    };
    let imported = import_archive(&archive).expect("official archive must import");
    let expected = imported.to_rust_source();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    for path in [
        "crates/openjoc-joc/src/generated_etsi_tables.rs",
        "crates/openjoc-qmf/src/generated_etsi_tables.rs",
    ] {
        let actual = fs::read_to_string(workspace.join(path)).expect("committed runtime tables");
        assert_eq!(actual, expected, "stale generated runtime table: {path}");
    }
}

#[test]
fn committed_runtime_tables_match_each_other_and_record_normative_provenance() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let joc_path = workspace.join("crates/openjoc-joc/src/generated_etsi_tables.rs");
    let qmf_path = workspace.join("crates/openjoc-qmf/src/generated_etsi_tables.rs");
    if !joc_path.is_file() || !qmf_path.is_file() {
        eprintln!("skipping workspace runtime-table comparison outside the OpenJOC source tree");
        return;
    }
    let joc = fs::read_to_string(joc_path).expect("committed JOC tables");
    let qmf = fs::read_to_string(qmf_path).expect("committed QMF tables");

    assert_eq!(joc, qmf);
    assert!(joc.contains("source: ts_103420_tables.c"));
    assert!(joc.contains(TABLES_C_SHA256));
}
