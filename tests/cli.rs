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
