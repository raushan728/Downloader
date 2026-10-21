use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

use dwdr::cli::shell::reset_terminal_state;
use dwdr::engine::downloader::{
    build_ytdlp_download_cmd_ext, clean_interrupted_media_files, clean_partial_files,
    derive_output_stem, find_existing_media_file, resolve_unique_output_template,
    sanitize_media_title,
};

#[test]
#[ignore]
fn test_real_media_title_sanitization_and_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let title = "Real Media: Test / Special * Video?";
    let sanitized = sanitize_media_title(title);
    assert_eq!(sanitized, "Real Media Test  Special  Video");

    let video_file = temp_dir.path().join(format!("{}.mp4", sanitized));
    std::fs::write(&video_file, b"fake video bytes").unwrap();

    let detected = find_existing_media_file(temp_dir.path(), title, false);
    assert!(detected.is_some());
    assert_eq!(detected.unwrap(), video_file);

    // Verify prefix matching does NOT cause false duplicate positive
    let different_title = format!("{} Extended Edition", title);
    let false_match = find_existing_media_file(temp_dir.path(), &different_title, false);
    assert!(false_match.is_none());
}

#[test]
#[ignore]
fn test_real_media_audio_vs_video_extension_isolation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let title = "Podcast Episode 42";

    let audio_file = temp_dir.path().join(format!("{}.m4a", title));
    std::fs::write(&audio_file, b"audio bytes").unwrap();

    // When searching for an existing video, the audio file must NOT be detected as a duplicate video
    assert!(find_existing_media_file(temp_dir.path(), title, false).is_none());

    // When searching for an existing audio download, the audio file MUST be detected
    let detected_audio = find_existing_media_file(temp_dir.path(), title, true);
    assert!(detected_audio.is_some());
    assert_eq!(detected_audio.unwrap(), audio_file);
}

#[test]
#[ignore]
fn test_real_media_unique_numbering_sequential_and_gap_filling() {
    let temp_dir = tempfile::tempdir().unwrap();
    let title = "Conference Keynote";

    // When no file exists, template should be (1)
    let (tmpl, idx) = resolve_unique_output_template(temp_dir.path(), title, false);
    assert_eq!(idx, 1);
    assert_eq!(tmpl, "%(title)s (1).%(ext)s");

    // Create the base file and copy 1
    let base_file = temp_dir.path().join(format!("{}.mp4", title));
    let copy1_file = temp_dir.path().join(format!("{} (1).mp4", title));
    std::fs::write(&base_file, b"base").unwrap();
    std::fs::write(&copy1_file, b"copy 1").unwrap();

    // Now next should be (2)
    let (tmpl2, idx2) = resolve_unique_output_template(temp_dir.path(), title, false);
    assert_eq!(idx2, 2);
    assert_eq!(tmpl2, "%(title)s (2).%(ext)s");

    // If (2) exists as an audio file (.m4a), video resolution also checks all video formats
    let copy2_file = temp_dir.path().join(format!("{} (2).mkv", title));
    std::fs::write(&copy2_file, b"copy 2").unwrap();

    let (tmpl3, idx3) = resolve_unique_output_template(temp_dir.path(), title, false);
    assert_eq!(idx3, 3);
    assert_eq!(tmpl3, "%(title)s (3).%(ext)s");
}

#[test]
#[ignore]
fn test_real_media_interrupted_cleanup_preserves_original_and_unrelated() {
    let temp_dir = tempfile::tempdir().unwrap();
    let title = "Documentary Film";

    // Original completed file that user already had on disk
    let original_file = temp_dir.path().join(format!("{}.mp4", title));
    std::fs::write(&original_file, b"original content").unwrap();

    // Unrelated file with similar starting prefix
    let similar_prefix_file = temp_dir.path().join(format!("{} Series Ep 2.mp4", title));
    std::fs::write(&similar_prefix_file, b"similar prefix content").unwrap();

    // The user was downloading a new copy "(1)"
    let copy1_file = temp_dir.path().join(format!("{} (1).mp4", title));
    std::fs::write(&copy1_file, b"in-progress copy 1").unwrap();

    let target_stem = derive_output_stem(title, Some("%(title)s (1).%(ext)s"));
    assert_eq!(target_stem, "Documentary Film (1)");

    // Simulate interruption on the copy 1 download
    let removed =
        clean_interrupted_media_files(temp_dir.path().to_str().unwrap(), title, Some(&target_stem));

    assert_eq!(removed, 1);
    assert!(!copy1_file.exists()); // Interrupted copy 1 MUST be deleted
    assert!(original_file.exists()); // Original pre-existing file MUST be preserved
    assert!(similar_prefix_file.exists()); // File with similar prefix MUST be preserved
}

#[test]
#[ignore]
fn test_real_clean_partial_files_removes_part_and_temp_preserves_completed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let title = "Lecture 1";

    // Completed file
    let completed_file = temp_dir.path().join("Lecture 1.mp4");
    std::fs::write(&completed_file, b"completed video").unwrap();

    // Temporary artifacts created during download
    let part_file = temp_dir.path().join("Lecture 1.mp4.part");
    std::fs::write(&part_file, b"partial data").unwrap();

    let ytdl_file = temp_dir.path().join("Lecture 1.ytdl");
    std::fs::write(&ytdl_file, b"ytdl data").unwrap();

    let unmerged_chunk = temp_dir.path().join("Lecture 1.f137");
    std::fs::write(&unmerged_chunk, b"unmerged video stream").unwrap();

    let removed = clean_partial_files(temp_dir.path().to_str().unwrap(), Some(title));
    assert!(removed >= 3);
    assert!(completed_file.exists());
    assert!(!part_file.exists());
    assert!(!ytdl_file.exists());
    assert!(!unmerged_chunk.exists());
}

#[test]
#[ignore]
fn test_real_cmd_builder_invariants_and_safeguards() {
    let yt = Path::new("yt-dlp");
    let ff = Path::new("ffmpeg");

    // Standard video download command
    let cmd = build_ytdlp_download_cmd_ext(
        yt,
        ff,
        "https://example.com/watch?v=abc",
        "best",
        "/tmp/downloads",
        None,
        false,
        None,
    );

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();

    assert!(args.contains(&"--no-playlist".to_string()));

    assert!(args.contains(&"--socket-timeout".to_string()));
    assert!(args.contains(&"30".to_string()));
    assert!(args.contains(&"--windows-filenames".to_string()));
    assert!(args.contains(&"--trim-filenames".to_string()));
    assert!(args.contains(&"bestvideo+bestaudio/best".to_string()));
    assert!(args.contains(&"%(title)s.%(ext)s".to_string()));
    assert!(!args.contains(&"--force-overwrites".to_string()));

    let cmd_overwrite = build_ytdlp_download_cmd_ext(
        yt,
        ff,
        "https://example.com/watch?v=abc",
        "audio:140",
        "/tmp/downloads",
        Some("%(title)s (1).%(ext)s"),
        true,
        None,
    );

    let args_overwrite: Vec<String> = cmd_overwrite
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();

    assert!(args_overwrite.contains(&"--force-overwrites".to_string()));
    assert!(args_overwrite.contains(&"Merger:-y".to_string()));
    assert!(args_overwrite.contains(&"%(title)s (1).%(ext)s".to_string()));
    assert!(args_overwrite.contains(&"140".to_string()));
}

#[test]
#[ignore]
fn test_real_cli_flag_parsing_help_and_version() {
    // Test that dwdr binary runs --version cleanly
    let mut cmd_version = Command::cargo_bin("dwdr").unwrap();
    cmd_version
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dwdr"));

    // Test that dwdr binary runs --help and shows --force option
    let mut cmd_help = Command::cargo_bin("dwdr").unwrap();
    cmd_help
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--force"))
        .stdout(predicate::str::contains(
            "Quick download with Smart Fallback",
        ));
}

#[test]
#[ignore]
fn test_real_terminal_state_reset_safe_execution() {
    // Calling reset_terminal_state must succeed without panicking even in headless/test environments
    reset_terminal_state();
}
