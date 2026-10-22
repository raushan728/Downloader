mod common;

use assert_cmd::Command;
use common::{
    MockFfmpegBuilder, MockResponse, MockServer, MockYtDlpBuilder, sample_multi_format_json,
    sample_single_format_json,
};
use predicates::prelude::*;
use serial_test::serial;
use tempfile::tempdir;

#[test]
fn test_cli_invalid_url() {
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.arg("invalid_url")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("ERROR: Invalid URL provided"));
}

#[test]
#[serial]
fn test_cli_valid_url() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");
    let mock_ytdlp = MockYtDlpBuilder::new()
        .with_exit_code(1)
        .with_stderr("ERROR: Video unavailable")
        .build(temp_dir.path());
    let mock_ffmpeg = MockFfmpegBuilder::new().build(temp_dir.path());

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .env("DWDR_YTDLP_PATH", mock_ytdlp.to_str().unwrap())
        .env("DWDR_FFMPEG_PATH", mock_ffmpeg.to_str().unwrap())
        .arg("https://youtube.com/watch?v=123")
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("Valid URL detected"))
        .stderr(predicate::str::contains(
            "ERROR: Failed to extract video info",
        ));
}

#[test]
#[serial]
fn test_config_show() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("Current Configuration:"));
}

#[test]
#[serial]
fn test_config_set_valid() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "quality", "720p"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated quality to 720p",
        ));

    // Verify it was actually saved and applied
    let mut cmd2 = Command::cargo_bin("dwdr").unwrap();
    cmd2.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("720p"));
}

#[test]
#[serial]
fn test_config_set_invalid_parallel() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    // Non-numeric string
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "invalid"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Invalid parallel downloads value",
        ));

    // Lower bound violation: 0
    let mut cmd_zero = Command::cargo_bin("dwdr").unwrap();
    cmd_zero
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Invalid parallel downloads value. Must be between 1 and 16.",
        ));

    // Upper bound violation: 17
    let mut cmd_high = Command::cargo_bin("dwdr").unwrap();
    cmd_high
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "17"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Invalid parallel downloads value. Must be between 1 and 16.",
        ));

    // Valid lower boundary: 1
    let mut cmd_valid_low = Command::cargo_bin("dwdr").unwrap();
    cmd_valid_low
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated parallel to 1",
        ));

    // Valid upper boundary: 16
    let mut cmd_valid_high = Command::cargo_bin("dwdr").unwrap();
    cmd_valid_high
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "16"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated parallel to 16",
        ));
}

#[test]
fn test_cli_self_update_help() {
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("self-update"));
}

#[test]
#[serial]
fn test_cli_help_does_not_create_config_file() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .arg("--help")
        .assert()
        .success();

    assert!(!config_file.exists());
}

#[test]
#[serial]
fn test_config_reset_command_in_non_terminal_requires_force() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "quality", "720p"])
        .assert()
        .success();

    let mut cmd_reset = Command::cargo_bin("dwdr").unwrap();
    cmd_reset
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("")
        .args(["config", "reset"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use 'dwdr config reset --force'"));

    let mut cmd_verify = Command::cargo_bin("dwdr").unwrap();
    cmd_verify
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("720p"));
}

#[test]
#[serial]
fn test_config_reset_force() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "quality", "720p"])
        .assert()
        .success();

    let mut cmd_reset = Command::cargo_bin("dwdr").unwrap();
    cmd_reset
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "reset", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration reset to defaults"));
}

#[test]
#[serial]
fn test_config_set_invalid_quality() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "quality", "arbitrary_invalid"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ERROR: Invalid quality value"));
}

#[test]
#[serial]
fn test_config_set_invalid_path() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "path", "/nonexistent/test/path/dwdr"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Download path does not exist",
        ));
}

#[test]
#[serial]
fn test_config_set_obsolete_update_days() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "update-days", "14"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ERROR: Unknown config key."));
}

#[test]
#[serial]
fn test_interactive_config_non_tty_error() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .arg("config")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Interactive configuration requires an interactive terminal",
        ));
}

#[test]
#[serial]
fn test_cross_process_active_download_block() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");
    let lock_file = temp_dir.path().join("active_downloads.lock");

    // Write the current test runner's PID to simulate an active download in another process
    std::fs::write(&lock_file, std::process::id().to_string()).unwrap();

    // 1. Without --force, config set must be blocked
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Cannot change settings while downloads are active. Use --force to override.",
        ));

    // 2. With --force, config set succeeds
    let mut cmd_force = Command::cargo_bin("dwdr").unwrap();
    cmd_force
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "5", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated parallel to 5",
        ));
}

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dwdr"));
}

#[test]
fn test_cli_help_shows_all_subcommands() {
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("self-update"))
        .stdout(predicate::str::contains("INTERACTIVE SHELL COMMANDS"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("queue"));
}

#[test]
#[serial]
fn test_interactive_shell_help_and_exit() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("help\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Welcome to DWDR Interactive Shell",
        ))
        .stdout(predicate::str::contains("Available Commands:"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("queue"))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_status_list_queue_commands() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("status\nlist\nqueue\nactive\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Active Downloads"))
        .stdout(predicate::str::contains("Download Queue"))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_unrecognized_command_guidance() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("some_random_cmd\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Unrecognized command: \"some_random_cmd\"",
        ))
        .stdout(predicate::str::contains(
            "Type a valid media URL, or 'help' for available commands",
        ));
}

#[test]
#[serial]
fn test_interactive_shell_empty_lines_and_eof() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("\n   \n\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Welcome to DWDR Interactive Shell",
        ));
}

#[test]
#[serial]
fn test_interactive_shell_cancel_and_clear() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("cancel\ncls\nclear\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Cancelled all active downloads and cleared queue",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_case_insensitivity() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("HELP\nSTATUS\nLIST\nQUEUE\nEXIT\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available Commands:"))
        .stdout(predicate::str::contains("Active Downloads"))
        .stdout(predicate::str::contains("Download Queue"))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_url_extraction_failure_recovery() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");
    let mock_ytdlp = MockYtDlpBuilder::new()
        .with_exit_code(1)
        .with_stderr("ERROR: Video unavailable")
        .build(temp_dir.path());
    let mock_ffmpeg = MockFfmpegBuilder::new().build(temp_dir.path());

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .env("DWDR_YTDLP_PATH", mock_ytdlp.to_str().unwrap())
        .env("DWDR_FFMPEG_PATH", mock_ffmpeg.to_str().unwrap())
        .write_stdin("https://youtube.com/watch?v=123\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"))
        .stderr(predicate::str::contains(
            "ERROR: Failed to extract video info",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_uppercase_url() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");
    let mock_ytdlp = MockYtDlpBuilder::new()
        .with_exit_code(1)
        .with_stderr("ERROR: Video unavailable")
        .build(temp_dir.path());
    let mock_ffmpeg = MockFfmpegBuilder::new().build(temp_dir.path());

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .env("DWDR_YTDLP_PATH", mock_ytdlp.to_str().unwrap())
        .env("DWDR_FFMPEG_PATH", mock_ffmpeg.to_str().unwrap())
        .write_stdin("HTTP://youtube.com/watch?v=123\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"))
        .stderr(predicate::str::contains(
            "ERROR: Failed to extract video info",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_config_path_pointing_to_directory_resolves_config_file() {
    let temp_dir = tempdir().unwrap();
    let dir_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", dir_path.to_str().unwrap())
        .args(["config", "set", "quality", "720p"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated quality to 720p",
        ));

    assert!(dir_path.join("config.toml").is_file());

    let mut cmd_show = Command::cargo_bin("dwdr").unwrap();
    cmd_show
        .env("DWDR_CONFIG_PATH", dir_path.to_str().unwrap())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("720p"));
}

#[test]
#[serial]
fn test_interactive_shell_config_in_non_tty() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("config\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Interactive configuration requires an interactive terminal (TTY)",
        ))
        .stdout(predicate::str::contains(
            "Tip: In scripts or headless environments, use 'dwdr config set <key> <value>' instead.",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
#[serial]
fn test_interactive_shell_queue_live_command() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("queue live\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Download Queue"))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
fn test_mock_server_offline_responses() {
    let server = MockServer::start();
    server.mock_path("/api/version", MockResponse::ok(b"1.0.0"));
    server.mock_path(
        "/error/403",
        MockResponse::status(403, "Forbidden", b"Access Denied"),
    );
    server.mock_path(
        "/error/429",
        MockResponse::status(429, "Too Many Requests", b"Rate limited"),
    );

    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();

    let resp200 = client.get(server.url("/api/version")).send().unwrap();
    assert_eq!(resp200.status().as_u16(), 200);
    assert_eq!(resp200.text().unwrap(), "1.0.0");

    let resp403 = client.get(server.url("/error/403")).send().unwrap();
    assert_eq!(resp403.status().as_u16(), 403);
    assert_eq!(resp403.text().unwrap(), "Access Denied");

    let resp429 = client.get(server.url("/error/429")).send().unwrap();
    assert_eq!(resp429.status().as_u16(), 429);
    assert_eq!(resp429.text().unwrap(), "Rate limited");

    let resp404 = client.get(server.url("/unmocked")).send().unwrap();
    assert_eq!(resp404.status().as_u16(), 404);
}

#[test]
fn test_mock_ytdlp_and_ffmpeg_execution() {
    let temp_dir = tempdir().unwrap();

    // 1. Mock yt-dlp returning multi-format JSON with progress lines
    let sample_json = sample_multi_format_json("Hermetic Mock Video");
    let ytdlp_path = MockYtDlpBuilder::new()
        .with_json(sample_json)
        .with_progress_lines(vec![
            "[download]  50.0% of ~ 10.00MiB at 2.00MiB/s ETA 00:05".to_string(),
        ])
        .build(temp_dir.path());

    let ytdlp_output = std::process::Command::new(&ytdlp_path)
        .output()
        .expect("Failed to execute mock yt-dlp");
    assert!(ytdlp_output.status.success());
    let stdout = String::from_utf8_lossy(&ytdlp_output.stdout);
    assert!(stdout.contains("Hermetic Mock Video"));
    assert!(stdout.contains("1080p"));
    assert!(stdout.contains("[download]  50.0%"));

    // 2. Mock yt-dlp single-format JSON fixture and error simulation
    let single_json = sample_single_format_json("Single Format Video");
    assert!(single_json.contains("Single Format Video"));

    let err_dir = temp_dir.path().join("err_sub");
    std::fs::create_dir_all(&err_dir).unwrap();
    let error_ytdlp_path = MockYtDlpBuilder::new()
        .with_stderr("ERROR: Video unavailable")
        .with_exit_code(1)
        .build(&err_dir);
    let err_output = std::process::Command::new(&error_ytdlp_path)
        .output()
        .expect("Failed to execute error mock yt-dlp");
    assert!(!err_output.status.success());
    assert_eq!(err_output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&err_output.stderr).contains("ERROR: Video unavailable"));

    // 3. Mock ffmpeg handling -version probe
    let ffmpeg_path = MockFfmpegBuilder::new().build(temp_dir.path());
    let ffmpeg_output = std::process::Command::new(&ffmpeg_path)
        .arg("-version")
        .output()
        .expect("Failed to execute mock ffmpeg");
    assert!(
        ffmpeg_output.status.success(),
        "Mock ffmpeg failed: status={:?}, stdout={}, stderr={}",
        ffmpeg_output.status,
        String::from_utf8_lossy(&ffmpeg_output.stdout),
        String::from_utf8_lossy(&ffmpeg_output.stderr)
    );
    let stdout_ff = String::from_utf8_lossy(&ffmpeg_output.stdout);
    assert!(stdout_ff.contains("ffmpeg version"));
    assert!(stdout_ff.contains("dwdr-mock"));

    // 4. Mock ffmpeg error simulation
    let ff_err_dir = temp_dir.path().join("ff_err");
    std::fs::create_dir_all(&ff_err_dir).unwrap();
    let err_ffmpeg_path = MockFfmpegBuilder::new()
        .with_stderr("Conversion failed")
        .with_exit_code(2)
        .build(&ff_err_dir);
    let ff_err_output = std::process::Command::new(&err_ffmpeg_path)
        .output()
        .expect("Failed to execute error mock ffmpeg");
    assert!(!ff_err_output.status.success());
    assert_eq!(ff_err_output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&ff_err_output.stderr).contains("Conversion failed"));
}
