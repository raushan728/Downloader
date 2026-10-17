use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn test_cli_invalid_url() {
    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.arg("invalid_url")
        .assert()
        .success()
        .stdout(predicate::str::contains("ERROR: Invalid URL provided"));
}

#[test]
fn test_cli_valid_url() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .arg("https://youtube.com/watch?v=123")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"));
}

#[test]
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
fn test_config_set_invalid_parallel() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "parallel", "invalid"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ERROR: Invalid parallel downloads value",
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
fn test_config_reset_command_with_confirmation() {
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
        .args(["config", "reset"])
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration reset to defaults"));

    let mut cmd_verify = Command::cargo_bin("dwdr").unwrap();
    cmd_verify
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1080p"));
}

#[test]
fn test_config_reset_cancelled() {
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
        .args(["config", "reset"])
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "WARNING: Configuration reset cancelled",
        ));

    let mut cmd_verify = Command::cargo_bin("dwdr").unwrap();
    cmd_verify
        .env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("720p"));
}

#[test]
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
fn test_config_set_invalid_update_days() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .args(["config", "set", "update-days", "9999"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ERROR: Invalid update-days value"));
}

#[test]
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
fn test_interactive_shell_url_extraction_failure_recovery() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("https://youtube.com/watch?v=123\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"))
        .stdout(predicate::str::contains(
            "ERROR: Failed to extract video info",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
fn test_interactive_shell_uppercase_url() {
    let temp_dir = tempdir().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("DWDR_CONFIG_PATH", config_file.to_str().unwrap())
        .write_stdin("HTTP://youtube.com/watch?v=123\nexit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"))
        .stdout(predicate::str::contains(
            "ERROR: Failed to extract video info",
        ))
        .stdout(predicate::str::contains("Thanks for using DWDR!"));
}

#[test]
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
