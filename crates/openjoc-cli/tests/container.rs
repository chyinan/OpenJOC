use openjoc_container::{
    DEFAULT_MAX_EAC3_BYTES, InputMediaError, InputMediaKind, load_eac3, open_seekable_iso_bmff,
};
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

fn external_fixture() -> Option<(PathBuf, Vec<u8>)> {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("mp4box").arg("-version").output().is_err()
    {
        eprintln!("skipping external container test: ffmpeg and MP4Box are required");
        return None;
    }
    let root = unique_root("openjoc-container");
    fs::create_dir_all(&root).expect("test directory");
    let source_raw = root.join("source.ec3");
    let raw_path = root.join("input.ec3");
    let container_path = root.join("input.mp4");
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
    let copy = root.join("stream-copy.ec3");
    let copied = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&container_path)
        .args(["-map", "0:0", "-c:a", "copy", "-f", "eac3"])
        .arg(&copy)
        .status()
        .expect("run FFmpeg stream copy");
    assert!(copied.success(), "FFmpeg failed to copy E-AC-3");
    let raw = fs::read(&copy).expect("stream-copy fixture");
    fs::write(&raw_path, &raw).expect("raw fixture");
    Some((root, raw))
}

#[test]
fn container_demux_is_byte_equivalent_and_detected_as_iso_bmff() {
    let Some((root, raw)) = external_fixture() else {
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
    let Some((root, _raw)) = external_fixture() else {
        return;
    };
    let container = root.join("input.mp4");
    let inspect = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["inspect", container.to_str().expect("container path")])
        .output()
        .expect("run inspect");
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let stdout = String::from_utf8(inspect.stdout).expect("inspect UTF-8");
    assert!(stdout.contains("input: ISO BMFF (stream-copied E-AC-3)"));
    assert!(stdout.contains("frames: 3"));

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
        .output();
    let Ok(result) = result else {
        fs::remove_dir_all(root).expect("remove fixture");
        return;
    };
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
        .output();
    let Ok(result) = result else {
        fs::remove_dir_all(root).expect("remove fixture");
        return;
    };
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
    let Some((root, raw)) = external_fixture() else {
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
