use std::process::Command;

fn openjoc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_openjoc"))
}

#[test]
fn redirected_root_help_is_plain_and_lists_real_commands() {
    let result = openjoc().arg("--help").output().expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 stdout");
    assert!(stdout.contains(&format!("OpenJOC {}", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("openjoc inspect <FILE>"));
    assert!(stdout.contains("openjoc decode <FILE>"));
    assert!(stdout.contains("openjoc decode-payload"));
    assert!(stdout.contains("metadata-only scene"));
    assert!(stdout.contains("ReconstructionBasis rows are not authored-object PCM"));
    assert!(stdout.contains("seekable ordinary ISO BMFF"));
    assert!(stdout.contains("never auto-downgraded"));
    assert!(stdout.contains("openjoc --version"));
    assert!(!stdout.contains("\x1b["));
    assert!(!stdout.contains("o---O"));
}

#[test]
fn version_is_a_script_safe_stdout_only_contract() {
    let result = openjoc().arg("--version").output().expect("run openjoc");
    assert!(result.status.success());
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.9.2");
    assert_eq!(
        String::from_utf8(result.stdout).expect("UTF-8 version"),
        format!("OpenJOC {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(result.stderr.is_empty());
}

#[test]
fn redirected_help_honors_all_banner_and_color_controls() {
    for (variable, value) in [
        ("NO_COLOR", "1"),
        ("OPENJOC_NO_BANNER", "1"),
        ("TERM", "dumb"),
    ] {
        let result = openjoc()
            .arg("--help")
            .env(variable, value)
            .output()
            .expect("run openjoc");
        assert!(result.status.success());
        assert!(!result.stdout.contains(&0x1b));
        assert!(!String::from_utf8_lossy(&result.stdout).contains("o---O"));
    }

    let result = openjoc()
        .args(["--no-banner", "--help"])
        .output()
        .expect("run openjoc");
    assert!(result.status.success());
    assert!(!result.stdout.contains(&0x1b));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("o---O"));
}

#[test]
fn root_without_arguments_remains_script_safe_when_redirected() {
    let result = openjoc().output().expect("run openjoc");
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 stderr");
    assert!(stderr.starts_with("openjoc[usage]: usage:"));
    assert!(!stderr.contains("\x1b["));
    assert!(!stderr.contains("o---O"));
}

#[test]
fn actual_subcommand_error_output_is_not_polluted_by_a_banner() {
    let result = openjoc().arg("inspect").output().expect("run openjoc");
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 stderr");
    assert!(stderr.starts_with("openjoc[usage]: usage:"));
    assert!(!stderr.contains("Open the objects"));
    assert!(!stderr.contains("\x1b["));
}

#[test]
fn every_public_command_has_successful_scoped_help() {
    for command in [
        "inspect",
        "decode",
        "decode-payload",
        "diagnose-tools",
        "census",
        "diagnose-oamd",
    ] {
        let result = openjoc()
            .args([command, "--help"])
            .output()
            .expect("run subcommand help");
        assert!(
            result.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stdout = String::from_utf8(result.stdout).expect("UTF-8 help");
        assert!(stdout.starts_with(&format!("usage: openjoc {command}")));
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn decode_help_freezes_semantic_profile_and_streaming_boundaries() {
    let result = openjoc()
        .args(["decode", "--help"])
        .output()
        .expect("run decode help");
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 help");
    assert!(stdout.contains("metadata-only scene"));
    assert!(stdout.contains("not authored-object PCM"));
    assert!(stdout.contains("seekable ordinary ISO BMFF"));
    assert!(stdout.contains("never downgraded"));
    assert!(!stdout.contains("object stems"));
}

#[test]
fn render_help_explains_calibration_and_offline_level_workflows() {
    let result = openjoc()
        .args(["render-joc", "--help"])
        .output()
        .expect("run render-joc help");
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 help");
    assert!(stdout.contains("--dialnorm default uses calibrated default behavior"));
    assert!(stdout.contains("recommended for normal playback/decoding"));
    assert!(stdout.contains("--dialnorm analog uses unity dialnorm gain"));
    assert!(stdout.contains("advanced compatibility/diagnostic policy"));
    assert!(stdout.contains("--normalize-peak TARGET_DBFS normalizes the final rendered file"));
    assert!(stdout.contains("not DRC, dialnorm, a limiter, compressor, LUFS, or true-peak"));
    assert!(stdout.contains("Do not choose analog merely because it is louder"));
}

#[test]
fn unsupported_streaming_combination_has_stable_category_and_nonzero_exit() {
    let result = openjoc()
        .args(["decode", "missing.ec3", "--streaming", "-o", "out"])
        .output()
        .expect("run invalid streaming combination");
    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 stderr");
    assert!(stderr.starts_with("openjoc[invalid-argument]:"));
    assert!(stderr.contains("requires --internal-base"));
}
