use openjoc_container::{
    DEFAULT_MAX_EAC3_BYTES, InputMediaError, InputMediaKind, load_eac3, open_seekable_iso_bmff,
};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static UNIQUE_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sequence = UNIQUE_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

const FIXED_RAW_NAME: &str = "sine-1000hz-48k-3frames.ec3";
const FIXED_CONTAINER_NAME: &str = "sine-1000hz-48k-3frames.mp4";
const FIXED_RAW_SHA256: &str = "426fb024a3949bf86380b1db655a635948a8ae8c8d291bbdd4d35426564bcd72";
const FIXED_CONTAINER_SHA256: &str =
    "8b0af5d18e9edd140b5afb4cae56988e31d6bd033361cbb62f2f64f71cfb3c83";

#[derive(Clone, Copy, Debug)]
struct ContainerTestMode {
    required: bool,
    fixed: bool,
}

fn mode_flag(name: &str, default: &str, off: &str, on: &str) -> bool {
    let value = match env::var(name) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => default.to_owned(),
        Err(error) => panic!("invalid {name}: {error}"),
    };
    match value.as_str() {
        value if value == off => false,
        value if value == on => true,
        _ => panic!("invalid {name}={value:?}; expected {off:?} or {on:?}"),
    }
}

fn container_test_mode() -> ContainerTestMode {
    ContainerTestMode {
        required: mode_flag(
            "OPENJOC_CONTAINER_TEST_MODE",
            "optional",
            "optional",
            "required",
        ),
        fixed: mode_flag(
            "OPENJOC_CONTAINER_FIXTURE_MODE",
            "dynamic",
            "dynamic",
            "fixed",
        ),
    }
}

fn tool_version(tool: &str) -> Result<String, String> {
    let output = Command::new(tool)
        .arg("-version")
        .output()
        .map_err(|error| format!("{tool} -version could not run: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{tool} -version failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let first_line = version.lines().find(|line| !line.trim().is_empty());
    let Some(first_line) = first_line else {
        return Err(format!("{tool} -version returned no version text"));
    };
    if matches!(tool, "ffmpeg" | "ffprobe") && !first_line.starts_with(&format!("{tool} version "))
    {
        return Err(format!(
            "{tool} -version returned invalid version text: {first_line}"
        ));
    }
    Ok(first_line.to_owned())
}

fn tools_available(mode: ContainerTestMode, tools: &[&str]) -> bool {
    for tool in tools {
        match tool_version(tool) {
            Ok(version) => eprintln!("container tool: {version}"),
            Err(error) => {
                assert!(
                    !mode.required,
                    "required container dependency unavailable: {error}"
                );
                eprintln!("skipping optional external container test: {error}");
                return false;
            }
        }
    }
    true
}

fn fixed_fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/container")
}

fn read_fixed_bytes(directory: &Path, name: &str, expected_hash: &str) -> Result<Vec<u8>, String> {
    let path = directory.join(name);
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "cannot read fixed container fixture {}: {error}",
            path.display()
        )
    })?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != expected_hash {
        return Err(format!(
            "fixed container fixture SHA-256 mismatch for {}: expected {expected_hash}, got {hash}",
            path.display()
        ));
    }
    Ok(bytes)
}

fn read_fixed_fixture(directory: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let raw = read_fixed_bytes(directory, FIXED_RAW_NAME, FIXED_RAW_SHA256)?;
    let container = read_fixed_bytes(directory, FIXED_CONTAINER_NAME, FIXED_CONTAINER_SHA256)?;
    Ok((raw, container))
}

fn external_fixture(case: &str) -> Option<(PathBuf, Vec<u8>)> {
    let mode = container_test_mode();
    // Validate pinned bytes independently of any demux output or external tool.
    // Missing/changed fixed fixtures are errors even in optional mode.
    let fixed = mode.fixed.then(|| {
        read_fixed_fixture(&fixed_fixture_directory()).expect("valid fixed container fixtures")
    });
    let tools: &[&str] = if mode.fixed {
        &["ffmpeg", "ffprobe"]
    } else {
        &["ffmpeg", "ffprobe", "mp4box"]
    };
    if !tools_available(mode, tools) {
        return None;
    }
    eprintln!("container case executed: {case}; mode={mode:?}");
    let root = unique_root("openjoc-container");
    fs::create_dir_all(&root).expect("test directory");
    let source_raw = root.join("source.ec3");
    let raw_path = root.join("input.ec3");
    let container_path = root.join("input.mp4");
    if let Some((raw, container)) = &fixed {
        fs::write(&raw_path, raw).expect("copy frozen raw fixture");
        fs::write(&container_path, container).expect("copy frozen container fixture");
    } else {
        let encoded = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000:duration=0.096",
                "-c:a",
                "eac3",
                "-b:a",
                "640k",
                "-f",
                "eac3",
            ])
            .arg(&source_raw)
            .status()
            .expect("run FFmpeg encoder");
        assert!(encoded.success(), "FFmpeg failed to create E-AC-3 source");
        let result = Command::new("mp4box")
            .args(["-add"])
            .arg(&source_raw)
            .args(["-new"])
            .arg(&container_path)
            .output()
            .expect("run MP4Box");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let copy = root.join("stream-copy.ec3");
    let copied = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&container_path)
        .args(["-map", "0:0", "-c:a", "copy", "-f", "eac3"])
        .arg(&copy)
        .status()
        .expect("run FFmpeg stream copy");
    assert!(copied.success(), "FFmpeg failed to copy E-AC-3");
    let copied_raw = fs::read(&copy).expect("stream-copy fixture");
    let raw = if let Some((raw, _container)) = fixed {
        assert_eq!(
            copied_raw, raw,
            "stream-copy differs from frozen raw fixture"
        );
        raw
    } else {
        // Preserve the local dynamic MP4Box comparison path.
        fs::write(&raw_path, &copied_raw).expect("raw fixture");
        copied_raw
    };
    Some((root, raw))
}

#[test]
fn container_demux_is_byte_equivalent_and_detected_as_iso_bmff() {
    let Some((root, raw)) =
        external_fixture("container_demux_is_byte_equivalent_and_detected_as_iso_bmff")
    else {
        return;
    };
    let container = root.join("input.mp4");
    let loaded = load_eac3(&container).expect("demux E-AC-3");
    assert_eq!(loaded.kind, InputMediaKind::IsoBmff);
    assert_eq!(loaded.bytes, raw);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn inspect_and_decode_use_iso_bmff_container_path() {
    let Some((root, _raw)) = external_fixture("inspect_and_decode_use_iso_bmff_container_path")
    else {
        return;
    };
    let container = root.join("input.mp4");
    let inspect = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "inspect",
            container.to_str().expect("container path"),
            "--json",
        ])
        .output()
        .expect("run inspect");
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let stdout = String::from_utf8(inspect.stdout).expect("inspect UTF-8");
    // Preserve the exact three-frame and container-routing contract using
    // the current inspect schema instead of the retired human-summary labels.
    let inspection: serde_json::Value = serde_json::from_str(&stdout).expect("inspect JSON");
    assert_eq!(inspection["container"]["kind"], "iso_bmff");
    assert_eq!(inspection["eac3"]["frame_count"], 3);
    assert_eq!(inspection["eac3"]["access_unit_count"], 3);
    assert_eq!(inspection["container"]["sample_count"], 3);

    let output = root.join("output");
    let decode_result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            container.to_str().expect("container path"),
            "--internal-base",
            "-o",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run decode");
    assert!(!decode_result.status.success());
    let stderr = String::from_utf8_lossy(&decode_result.stderr);
    assert!(stderr.contains("JOC"), "unexpected decode error: {stderr}");
    assert!(!stderr.contains("invalid E-AC-3 syncword"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn malformed_iso_bmff_reports_container_error_not_syncword_error() {
    let root = unique_root("openjoc-malformed-container");
    fs::create_dir_all(&root).expect("test directory");
    let path = root.join("broken.mp4");
    fs::write(&path, [0, 0, 0, 24, b'f', b't', b'y', b'p']).expect("broken container");
    let error = load_eac3(&path).expect_err("malformed container should fail");
    assert!(matches!(
        error,
        InputMediaError::Io { .. }
            | InputMediaError::MissingAudioTrack
            | InputMediaError::MultipleAudioTracks { .. }
            | InputMediaError::NoMatchingAudioTrack { .. }
            | InputMediaError::ProbeFailed { .. }
            | InputMediaError::MalformedProbeRow { .. }
            | InputMediaError::DemuxFailed { .. }
            | InputMediaError::DemuxOutputTooLarge { .. }
            | InputMediaError::MalformedPacketProbeRow { .. }
            | InputMediaError::EmptyDemuxOutput
    ));
    assert!(!error.to_string().contains("syncword"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn container_without_eac3_track_reports_no_matching_audio_track() {
    let mode = container_test_mode();
    if !tools_available(mode, &["ffmpeg", "ffprobe"]) {
        return;
    }
    eprintln!(
        "container case executed: container_without_eac3_track_reports_no_matching_audio_track; mode={mode:?}"
    );
    let root = unique_root("openjoc-no-eac3");
    fs::create_dir_all(&root).expect("test directory");
    let path = root.join("aac.mp4");
    let result = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.05",
            "-c:a",
            "aac",
        ])
        .arg(&path)
        .output()
        .expect("run FFmpeg AAC encoder");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let error = load_eac3(&path).expect_err("AAC-only container should fail");
    assert!(matches!(
        error,
        InputMediaError::NoMatchingAudioTrack { .. }
    ));
    assert!(!error.to_string().contains("syncword"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn container_with_multiple_audio_tracks_is_rejected_structurally() {
    let mode = container_test_mode();
    if !tools_available(mode, &["ffmpeg", "ffprobe"]) {
        return;
    }
    eprintln!(
        "container case executed: container_with_multiple_audio_tracks_is_rejected_structurally; mode={mode:?}"
    );
    let root = unique_root("openjoc-multiple-audio");
    fs::create_dir_all(&root).expect("test directory");
    let path = root.join("multiple.mp4");
    let result = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.05",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=0.05",
            "-map",
            "0:a",
            "-map",
            "1:a",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&path)
        .output()
        .expect("run FFmpeg AAC encoder");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let error = load_eac3(&path).expect_err("multiple tracks should fail");
    assert!(matches!(
        error,
        InputMediaError::MultipleAudioTracks { count: 2 }
    ));
    assert!(!error.to_string().contains("syncword"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn raw_eac3_remains_raw_input_kind() {
    let Some((root, raw)) = external_fixture("raw_eac3_remains_raw_input_kind") else {
        return;
    };
    let path = root.join("input.ec3");
    let loaded = load_eac3(Path::new(&path)).expect("raw input");
    assert_eq!(loaded.kind, InputMediaKind::RawEac3);
    assert_eq!(loaded.bytes, raw);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn seekable_frozen_isobmff_samples_match_stream_copy() {
    let Some(fixture_directory) = env::var_os("OPENJOC_PRIVATE_J1_FIXTURE_DIR").map(PathBuf::from)
    else {
        eprintln!("skipping frozen seekable ISO BMFF test: set OPENJOC_PRIVATE_J1_FIXTURE_DIR");
        return;
    };
    let container = fixture_directory.join("J1R6_FR_997_R0_DDP_Atmos_ec3.mp4");
    let raw_path = fixture_directory.join("J1R6_FR_997_R0.ec3");
    if !container.is_file()
        || !raw_path.is_file()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping frozen seekable ISO BMFF test: private fixture or ffprobe absent");
        return;
    }
    let expected = fs::read(raw_path).expect("frozen raw stream");
    let mut reader =
        open_seekable_iso_bmff(&container, Path::new("ffprobe"), DEFAULT_MAX_EAC3_BYTES)
            .expect("open seekable ISO BMFF");
    let mut delivered = Vec::new();
    reader
        .read_to_end(&mut delivered)
        .expect("read packet stream");
    assert_eq!(delivered, expected);
    let stats = reader.stats();
    assert_eq!(stats.samples_delivered, stats.sample_count);
    assert_eq!(stats.max_samples_simultaneously_retained, 1);
    assert_eq!(stats.derived_sample_index_entries, 0);
    assert_eq!(stats.cursor_state_entries, 1);
}

#[test]
fn user_supplied_dee_fixture_uses_container_boundary_when_enabled() {
    let Some(path) = env::var_os("OPENJOC_DEE_FIXTURE").map(PathBuf::from) else {
        eprintln!("skipping legal DEE fixture lane: set OPENJOC_DEE_FIXTURE");
        return;
    };
    if !path.is_file() {
        eprintln!("skipping legal DEE fixture lane: fixture path is absent");
        return;
    }
    let loaded = load_eac3(&path).expect("DEE container demux");
    assert_eq!(loaded.kind, InputMediaKind::IsoBmff);
    let frames = openjoc_eac3::index_syncframes(&loaded.bytes).expect("demuxed frames");
    assert_eq!(frames.len(), 7_773);

    let root = unique_root("openjoc-dee-lane");
    fs::create_dir_all(&root).expect("test directory");
    let inspect = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["inspect", path.to_str().expect("fixture path")])
        .output()
        .expect("run inspect");
    assert!(inspect.status.success());
    let inspect_stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(inspect_stdout.contains("frames: 7773"));
    assert!(inspect_stdout.contains("profile: ETSI_STRICT"));
    assert!(inspect_stdout.contains("profile: OBSERVED_VENDOR_COMPAT"));
    assert!(inspect_stdout.contains("result: failed"));

    let decode = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            path.to_str().expect("fixture path"),
            "--internal-base",
            "-o",
            root.to_str().expect("output path"),
        ])
        .output()
        .expect("run internal-base decode");
    let stderr = String::from_utf8_lossy(&decode.stderr);
    assert!(!stderr.contains("invalid E-AC-3 syncword"));
    fs::remove_dir_all(root).expect("remove fixture");
}

fn container_case_subprocess(
    case: &str,
    tool_path: &Path,
    test_mode: Option<&str>,
    fixture_mode: Option<&str>,
) -> std::process::Output {
    // Child-only configuration avoids racing other tests' process environment.
    let mut command = Command::new(env::current_exe().expect("container test executable"));
    command
        .args(["--exact", case, "--nocapture"])
        .env("PATH", tool_path)
        .env_remove("OPENJOC_CONTAINER_TEST_MODE")
        .env_remove("OPENJOC_CONTAINER_FIXTURE_MODE");
    if let Some(mode) = test_mode {
        command.env("OPENJOC_CONTAINER_TEST_MODE", mode);
    }
    if let Some(mode) = fixture_mode {
        command.env("OPENJOC_CONTAINER_FIXTURE_MODE", mode);
    }
    command.output().expect("run isolated container test")
}

fn subprocess_log(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn container_modes_fail_closed_without_changing_process_environment() {
    let empty_path = unique_root("openjoc-container-empty-path");
    fs::create_dir_all(&empty_path).expect("empty tool directory");
    for case in [
        "container_demux_is_byte_equivalent_and_detected_as_iso_bmff",
        "inspect_and_decode_use_iso_bmff_container_path",
        "raw_eac3_remains_raw_input_kind",
        "container_without_eac3_track_reports_no_matching_audio_track",
        "container_with_multiple_audio_tracks_is_rejected_structurally",
    ] {
        let optional = container_case_subprocess(case, &empty_path, None, None);
        let log = subprocess_log(&optional);
        assert!(optional.status.success(), "{case}: {log}");
        assert!(
            log.contains("skipping optional external container test"),
            "{log}"
        );
        assert!(!log.contains("container case executed:"), "{log}");

        let required =
            container_case_subprocess(case, &empty_path, Some("required"), Some("fixed"));
        let log = subprocess_log(&required);
        assert!(!required.status.success(), "{case}: {log}");
        assert!(
            log.contains("required container dependency unavailable: ffmpeg"),
            "{log}"
        );
        assert!(!log.contains("skipping"), "{log}");
    }
    for (test_mode, fixture_mode, error) in [
        ("typo", "fixed", "invalid OPENJOC_CONTAINER_TEST_MODE"),
        ("required", "typo", "invalid OPENJOC_CONTAINER_FIXTURE_MODE"),
    ] {
        let output = container_case_subprocess(
            "raw_eac3_remains_raw_input_kind",
            &empty_path,
            Some(test_mode),
            Some(fixture_mode),
        );
        let log = subprocess_log(&output);
        assert!(!output.status.success(), "{log}");
        assert!(log.contains(error), "{log}");
        assert!(!log.contains("skipping"), "{log}");
    }
    fs::remove_dir_all(empty_path).expect("remove empty tool directory");
}

#[test]
fn fixed_container_fixture_hashes_reject_missing_or_changed_bytes() {
    let (raw, container) = read_fixed_fixture(&fixed_fixture_directory()).expect("frozen fixtures");
    let root = unique_root("openjoc-container-fixture-integrity");
    fs::create_dir_all(&root).expect("test fixture directory");
    let error = read_fixed_fixture(&root).expect_err("missing fixed fixture must fail");
    assert!(
        error.contains("cannot read fixed container fixture"),
        "{error}"
    );

    fs::write(root.join(FIXED_RAW_NAME), &raw).expect("copy frozen raw");
    fs::write(root.join(FIXED_CONTAINER_NAME), &container).expect("copy frozen MP4");
    read_fixed_fixture(&root).expect("unchanged fixture copies");
    let mut changed_raw = raw.clone();
    changed_raw[0] ^= 1;
    fs::write(root.join(FIXED_RAW_NAME), &changed_raw).expect("change fixture copy");
    let error = read_fixed_fixture(&root).expect_err("changed raw must fail");
    assert!(error.contains("SHA-256 mismatch"), "{error}");
    assert!(error.contains(FIXED_RAW_NAME), "{error}");

    fs::write(root.join(FIXED_RAW_NAME), &raw).expect("restore frozen raw copy");
    let mut changed_container = container;
    changed_container[0] ^= 1;
    fs::write(root.join(FIXED_CONTAINER_NAME), &changed_container).expect("change MP4 copy");
    let error = read_fixed_fixture(&root).expect_err("changed MP4 must fail");
    assert!(error.contains("SHA-256 mismatch"), "{error}");
    assert!(error.contains(FIXED_CONTAINER_NAME), "{error}");
    fs::remove_file(root.join(FIXED_CONTAINER_NAME)).expect("remove MP4 copy");
    let error = read_fixed_fixture(&root).expect_err("missing MP4 must fail");
    assert!(
        error.contains("cannot read fixed container fixture"),
        "{error}"
    );
    assert!(error.contains(FIXED_CONTAINER_NAME), "{error}");
    fs::remove_dir_all(root).expect("remove fixture copies");
}

#[cfg(unix)]
#[test]
fn required_container_tools_need_successful_version_probes() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_root("openjoc-container-failed-version");
    fs::create_dir_all(&root).expect("fake tool directory");
    let write_tool = |name: &str, script: &str| {
        let path = root.join(name);
        fs::write(&path, script).expect("write version-probe-only test tool");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("test executable");
    };
    let check_failure = |message: &str| {
        let output = container_case_subprocess(
            "raw_eac3_remains_raw_input_kind",
            &root,
            Some("required"),
            Some("fixed"),
        );
        let log = subprocess_log(&output);
        assert!(!output.status.success(), "{log}");
        assert!(log.contains(message), "{log}");
        assert!(!log.contains("skipping"), "{log}");
        assert!(!log.contains("container case executed:"), "{log}");
    };
    write_tool(
        "ffmpeg",
        "#!/bin/sh\necho 'ffmpeg version test-probe-only'\n",
    );
    check_failure("ffprobe -version could not run");
    write_tool("ffprobe", "#!/bin/sh\nexit 7\n");
    check_failure("ffprobe -version failed");
    write_tool("ffmpeg", "#!/bin/sh\nexit 7\n");
    check_failure("ffmpeg -version failed");
    write_tool("ffmpeg", "#!/bin/sh\nexit 0\n");
    check_failure("ffmpeg -version returned no version text");
    write_tool("ffmpeg", "#!/bin/sh\necho 'unrecognized version'\n");
    check_failure("ffmpeg -version returned invalid version text");
    fs::remove_dir_all(root).expect("remove fake tools");
}
