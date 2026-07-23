// pattern: Functional Core

//! Verified importer for the normative ETSI TS 103 420 companion attachment.

use sha2::{Digest, Sha256};
use std::fmt::{self, Write as _};
use std::io::{Cursor, Read};

/// Required SHA-256 of `ts_103420v010201p0.zip`.
pub const ZIP_SHA256: &str = "a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced";
/// Required SHA-256 of the extracted `ts_103420_tables.c`.
pub const TABLES_C_SHA256: &str =
    "4db8ae83e3c2e9269e88365be92a1a3ed6a9e6ee3851afac8ca03902723b1fcd";

const SOURCE_NAME: &str = "ts_103420_tables.c";

/// Fully parsed normative table data.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedTables {
    pub zip_sha256: String,
    pub source_sha256: String,
    pub coarse_generic: Vec<[i16; 2]>,
    pub fine_generic: Vec<[i16; 2]>,
    pub coarse_coeff_sparse: Vec<[i16; 2]>,
    pub fine_coeff_sparse: Vec<[i16; 2]>,
    pub pos_index_5ch_sparse: Vec<[i16; 2]>,
    pub pos_index_7ch_sparse: Vec<[i16; 2]>,
    pub prototype_64: Vec<f32>,
}

/// Failures at the attachment trust boundary or while parsing its declarations.
#[derive(Debug)]
pub enum ImportError {
    ArchiveHashMismatch {
        expected: &'static str,
        actual: String,
    },
    SourceHashMismatch {
        expected: &'static str,
        actual: String,
    },
    Archive(String),
    UnexpectedMember {
        expected: &'static str,
        actual: String,
    },
    InvalidUtf8,
    MissingDeclaration(&'static str),
    InvalidValue {
        declaration: &'static str,
        value: String,
    },
    WrongCount {
        declaration: &'static str,
        expected: usize,
        actual: usize,
    },
    Generation,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArchiveHashMismatch { expected, actual } => {
                write!(
                    f,
                    "companion archive hash mismatch: expected {expected}, got {actual}"
                )
            }
            Self::SourceHashMismatch { expected, actual } => {
                write!(
                    f,
                    "table source hash mismatch: expected {expected}, got {actual}"
                )
            }
            Self::Archive(message) => write!(f, "invalid companion archive: {message}"),
            Self::UnexpectedMember { expected, actual } => {
                write!(f, "unexpected archive member {actual}; expected {expected}")
            }
            Self::InvalidUtf8 => f.write_str("table source is not valid UTF-8"),
            Self::MissingDeclaration(name) => write!(f, "missing table declaration {name}"),
            Self::InvalidValue { declaration, value } => {
                write!(f, "invalid value {value:?} in {declaration}")
            }
            Self::WrongCount {
                declaration,
                expected,
                actual,
            } => write!(
                f,
                "wrong element count for {declaration}: expected {expected}, got {actual}"
            ),
            Self::Generation => f.write_str("failed to generate Rust table source"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Verifies and imports the official TS 103 420 V1.2.1 companion archive.
///
/// # Errors
///
/// Returns [`ImportError`] if either normative hash differs, the ZIP structure
/// is unexpected, or any required declaration is malformed or has a wrong size.
pub fn import_archive(archive_bytes: &[u8]) -> Result<ImportedTables, ImportError> {
    let archive_hash = sha256(archive_bytes);
    if archive_hash != ZIP_SHA256 {
        return Err(ImportError::ArchiveHashMismatch {
            expected: ZIP_SHA256,
            actual: archive_hash,
        });
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))
        .map_err(|error| ImportError::Archive(error.to_string()))?;
    if archive.len() != 1 {
        return Err(ImportError::Archive(format!(
            "expected exactly one member, got {}",
            archive.len()
        )));
    }
    let mut member = archive
        .by_index(0)
        .map_err(|error| ImportError::Archive(error.to_string()))?;
    if member.name() != SOURCE_NAME {
        return Err(ImportError::UnexpectedMember {
            expected: SOURCE_NAME,
            actual: member.name().to_owned(),
        });
    }
    let mut source_bytes = Vec::with_capacity(usize::try_from(member.size()).unwrap_or(0));
    member
        .read_to_end(&mut source_bytes)
        .map_err(|error| ImportError::Archive(error.to_string()))?;
    let source_hash = sha256(&source_bytes);
    if source_hash != TABLES_C_SHA256 {
        return Err(ImportError::SourceHashMismatch {
            expected: TABLES_C_SHA256,
            actual: source_hash,
        });
    }
    let source = std::str::from_utf8(&source_bytes).map_err(|_| ImportError::InvalidUtf8)?;

    Ok(ImportedTables {
        zip_sha256: archive_hash,
        source_sha256: source_hash,
        coarse_generic: parse_nodes(source, "joc_huff_code_coarse_generic", 95)?,
        fine_generic: parse_nodes(source, "joc_huff_code_fine_generic", 191)?,
        coarse_coeff_sparse: parse_nodes(source, "joc_huff_code_coarse_coeff_sparse", 95)?,
        fine_coeff_sparse: parse_nodes(source, "joc_huff_code_fine_coeff_sparse", 191)?,
        pos_index_5ch_sparse: parse_nodes(source, "joc_huff_code_5ch_pos_index_sparse", 4)?,
        pos_index_7ch_sparse: parse_nodes(source, "joc_huff_code_7ch_pos_index_sparse", 6)?,
        prototype_64: parse_floats(source, "prot64", 640)?,
    })
}

impl ImportedTables {
    /// Emits local Rust constants with source hash provenance.
    #[must_use]
    pub fn to_rust_source(&self) -> String {
        let mut output = format!(
            "// Generated from ETSI TS 103 420 companion data.\n// source: {SOURCE_NAME}\n// sha256: {}\n\n",
            self.source_sha256
        );
        for (name, nodes) in [
            ("JOC_HUFF_CODE_COARSE_GENERIC", &self.coarse_generic),
            ("JOC_HUFF_CODE_FINE_GENERIC", &self.fine_generic),
            (
                "JOC_HUFF_CODE_COARSE_COEFF_SPARSE",
                &self.coarse_coeff_sparse,
            ),
            ("JOC_HUFF_CODE_FINE_COEFF_SPARSE", &self.fine_coeff_sparse),
            (
                "JOC_HUFF_CODE_5CH_POS_INDEX_SPARSE",
                &self.pos_index_5ch_sparse,
            ),
            (
                "JOC_HUFF_CODE_7CH_POS_INDEX_SPARSE",
                &self.pos_index_7ch_sparse,
            ),
        ] {
            let _ = writeln!(
                output,
                "pub const {name}: [[i16; 2]; {}] = {nodes:?};",
                nodes.len()
            );
        }
        let _ = writeln!(
            output,
            "#[allow(clippy::unreadable_literal)]\npub const PROT64: [f32; {}] = {:?};",
            self.prototype_64.len(),
            self.prototype_64
        );
        output
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn declaration_body<'a>(source: &'a str, name: &'static str) -> Result<&'a str, ImportError> {
    let name_offset = source
        .find(name)
        .ok_or(ImportError::MissingDeclaration(name))?;
    let after_name = &source[name_offset + name.len()..];
    let open = after_name
        .find('{')
        .ok_or(ImportError::MissingDeclaration(name))?;
    let after_open = &after_name[open + 1..];
    let close = after_open
        .find("};")
        .ok_or(ImportError::MissingDeclaration(name))?;
    Ok(&after_open[..close])
}

fn parse_nodes(
    source: &str,
    name: &'static str,
    expected: usize,
) -> Result<Vec<[i16; 2]>, ImportError> {
    let body = declaration_body(source, name)?;
    let mut nodes = Vec::with_capacity(expected);
    for pair in body.split('{').skip(1) {
        let contents = pair
            .split_once('}')
            .ok_or_else(|| ImportError::InvalidValue {
                declaration: name,
                value: pair.to_owned(),
            })?
            .0;
        let mut values = contents.split(',').map(str::trim);
        let left = parse_i16(values.next(), name)?;
        let right = parse_i16(values.next(), name)?;
        if values.next().is_some() {
            return Err(ImportError::InvalidValue {
                declaration: name,
                value: contents.to_owned(),
            });
        }
        nodes.push([left, right]);
    }
    validate_count(name, expected, nodes.len())?;
    Ok(nodes)
}

fn parse_i16(value: Option<&str>, name: &'static str) -> Result<i16, ImportError> {
    let value = value.ok_or_else(|| ImportError::InvalidValue {
        declaration: name,
        value: String::new(),
    })?;
    value.parse().map_err(|_| ImportError::InvalidValue {
        declaration: name,
        value: value.to_owned(),
    })
}

fn parse_floats(
    source: &str,
    name: &'static str,
    expected: usize,
) -> Result<Vec<f32>, ImportError> {
    let body = declaration_body(source, name)?;
    let values = body
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .strip_suffix('f')
                .unwrap_or(value)
                .parse()
                .map_err(|_| ImportError::InvalidValue {
                    declaration: name,
                    value: value.to_owned(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_count(name, expected, values.len())?;
    Ok(values)
}

fn validate_count(name: &'static str, expected: usize, actual: usize) -> Result<(), ImportError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ImportError::WrongCount {
            declaration: name,
            expected,
            actual,
        })
    }
}
