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
    assert!(!stdout.contains("\x1b["));
    assert!(!stdout.contains("o---O"));
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
    assert!(stderr.starts_with("openjoc: usage:"));
    assert!(!stderr.contains("\x1b["));
    assert!(!stderr.contains("o---O"));
}

#[test]
fn actual_subcommand_error_output_is_not_polluted_by_a_banner() {
    let result = openjoc().arg("inspect").output().expect("run openjoc");
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).expect("UTF-8 stderr");
    assert!(stderr.starts_with("openjoc: usage:"));
    assert!(!stderr.contains("Open the objects"));
    assert!(!stderr.contains("\x1b["));
}
