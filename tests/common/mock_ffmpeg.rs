#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use super::mock_ytdlp::escape_batch_line;

pub struct MockFfmpegBuilder {
    exit_code: i32,
    stderr_output: Option<String>,
}

impl Default for MockFfmpegBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFfmpegBuilder {
    pub fn new() -> Self {
        Self {
            exit_code: 0,
            stderr_output: None,
        }
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    pub fn with_stderr(mut self, err: impl Into<String>) -> Self {
        self.stderr_output = Some(err.into());
        self
    }

    pub fn build(self, dir: &Path) -> PathBuf {
        #[cfg(unix)]
        {
            let script_path = dir.join("ffmpeg");
            let mut script = String::from("#!/bin/sh\n");

            // Handle -version probe
            script.push_str("for arg in \"$@\"; do\n");
            script.push_str("  if [ \"$arg\" = \"-version\" ]; then\n");
            script.push_str("    echo \"ffmpeg version 6.1.1-dwdr-mock Copyright (c) 2000-2023 the FFmpeg developers\"\n");
            script.push_str("    exit 0\n");
            script.push_str("  fi\n");
            script.push_str("done\n");

            if let Some(err) = &self.stderr_output {
                script.push_str(&format!("echo '{}' >&2\n", err.replace('\'', "'\\''")));
            }

            script.push_str(&format!("exit {}\n", self.exit_code));

            std::fs::write(&script_path, script).expect("Failed to write mock ffmpeg");
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
                .expect("Failed to set mock ffmpeg permissions");

            script_path
        }
        #[cfg(windows)]
        {
            let script_path = dir.join("ffmpeg.bat");
            let mut script = String::from("@echo off\r\n");

            script.push_str("setlocal\r\n");
            script.push_str(":arg_loop\r\n");
            script.push_str("if \"%~1\"==\"\" goto no_version\r\n");
            script.push_str("if \"%~1\"==\"-version\" goto do_version\r\n");
            script.push_str("shift\r\n");
            script.push_str("goto arg_loop\r\n");
            script.push_str(":do_version\r\n");
            script.push_str("echo ffmpeg version 6.1.1-dwdr-mock Copyright (c) 2000-2023 the FFmpeg developers\r\n");
            script.push_str("exit /b 0\r\n");
            script.push_str(":no_version\r\n");

            if let Some(err) = &self.stderr_output {
                script.push_str(&format!("echo {} >&2\r\n", escape_batch_line(err)));
            }

            script.push_str(&format!("exit /b {}\r\n", self.exit_code));

            std::fs::write(&script_path, script).expect("Failed to write mock ffmpeg.bat");
            script_path
        }
    }
}
