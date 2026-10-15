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
fn test_config_reset_command() {
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
