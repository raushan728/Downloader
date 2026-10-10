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
    let home = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .arg("https://youtube.com/watch?v=123")
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid URL detected"));
}

#[test]
fn test_config_show() {
    let temp_dir = tempdir().unwrap();
    let home = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("Current Configuration:"));
}

#[test]
fn test_config_set_valid() {
    let temp_dir = tempdir().unwrap();
    let home = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .args(&["config", "set", "port", "8080"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Successfully updated port to 8080",
        ));

    // Verify it was actually saved and applied
    let mut cmd2 = Command::cargo_bin("dwdr").unwrap();
    cmd2.env("HOME", home)
        .env("USERPROFILE", home)
        .args(&["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));
}

#[test]
fn test_config_set_invalid_port() {
    let temp_dir = tempdir().unwrap();
    let home = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("dwdr").unwrap();
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .args(&["config", "set", "port", "invalid"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ERROR: Invalid port value"));
}
