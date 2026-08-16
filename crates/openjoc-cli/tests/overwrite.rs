use std::{fs, process::Command, time::SystemTime};

fn unique_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "openjoc-render-overwrite-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

#[test]
fn render_joc_help_documents_overwrite() {
    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["render-joc", "--help"])
        .output()
        .expect("render-joc help");
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 help");
    assert!(stdout.contains("--overwrite"));
    assert!(stdout.contains("Non-interactive renders refuse existing outputs"));
}

#[test]
fn noninteractive_render_preflight_refuses_all_existing_outputs_before_loading_input() {
    let root = unique_root("preflight");
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("missing.ec3");
    let output = root.join("render.wav");
    let report = root.join("render-performance.json");
    fs::write(&output, b"previous wav").expect("old WAV");
    fs::write(&report, b"previous report").expect("old report");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "render-joc",
            input.to_str().expect("input path"),
            "--layout",
            "7.1.4",
            "--output",
            output.to_str().expect("output path"),
            "--performance-report",
            report.to_str().expect("report path"),
        ])
        .output()
        .expect("render-joc preflight");
    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 stderr");
    assert!(!stderr.contains("Overwrite?"));
    assert!(stderr.contains(output.to_str().expect("output path")));
    assert!(stderr.contains(report.to_str().expect("report path")));
    assert!(!stderr.contains("input file does not exist"));
    assert_eq!(fs::read(&output).expect("old WAV remains"), b"previous wav");
    assert_eq!(
        fs::read(&report).expect("old report remains"),
        b"previous report"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn overwrite_does_not_allow_input_output_aliasing_or_truncate_input() {
    let root = unique_root("alias");
    fs::create_dir_all(&root).expect("test directory");
    let input_output = root.join("same.ec3");
    fs::write(&input_output, b"input must remain intact").expect("input");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "render-joc",
            input_output.to_str().expect("input path"),
            "--layout",
            "7.1.4",
            "--output",
            input_output.to_str().expect("output path"),
            "--overwrite",
        ])
        .output()
        .expect("render-joc alias check");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("aliases output path"));
    assert_eq!(
        fs::read(&input_output).expect("input remains"),
        b"input must remain intact"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
