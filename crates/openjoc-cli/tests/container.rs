use openjoc_container::{InputMediaError, InputMediaKind, load_eac3};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
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
    assert!(stderr.contains("JOC metadata") || stderr.contains("missing"));
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
    assert!(matches!(error, InputMediaError::ProbeFailed { .. }));
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
    assert!(inspect_stdout.contains(
        "JOC profile candidate found but validation failed in examined carriers: failed to decode carried EMDF: invalid JOC-profile EMDF payload configuration"
    ));

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
