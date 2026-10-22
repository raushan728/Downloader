#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub struct MockYtDlpBuilder {
    json_output: Option<String>,
    exit_code: i32,
    stderr_output: Option<String>,
    progress_lines: Vec<String>,
}

impl Default for MockYtDlpBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockYtDlpBuilder {
    pub fn new() -> Self {
        Self {
            json_output: None,
            exit_code: 0,
            stderr_output: None,
            progress_lines: Vec::new(),
        }
    }

    pub fn with_json(mut self, json: impl Into<String>) -> Self {
        self.json_output = Some(json.into());
        self
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    pub fn with_stderr(mut self, err: impl Into<String>) -> Self {
        self.stderr_output = Some(err.into());
        self
    }

    pub fn with_progress_lines(mut self, lines: Vec<String>) -> Self {
        self.progress_lines = lines;
        self
    }

    pub fn build(self, dir: &Path) -> PathBuf {
        let has_both = self.json_output.is_some() && !self.progress_lines.is_empty();

        #[cfg(unix)]
        {
            let script_path = dir.join("yt-dlp");
            let mut script = String::from("#!/bin/sh\n");

            if has_both {
                script.push_str("case \"$*\" in\n");
                script.push_str("  *-J*|*--dump-json*)\n");
                if let Some(json) = &self.json_output {
                    script.push_str(&format!("    echo '{}'\n", json.replace('\'', "'\\''")));
                }
                script.push_str("    exit 0\n    ;;\n");
                script.push_str("  *--newline*|*--progress-template*|*--print*)\n");
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("    echo '{}' >&2\n", err.replace('\'', "'\\''")));
                }
                for line in &self.progress_lines {
                    script.push_str(&format!("    echo '{}'\n", line.replace('\'', "'\\''")));
                }
                script.push_str(&format!("    exit {}\n    ;;\n", self.exit_code));
                script.push_str("  *)\n");
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("    echo '{}' >&2\n", err.replace('\'', "'\\''")));
                }
                for line in &self.progress_lines {
                    script.push_str(&format!("    echo '{}'\n", line.replace('\'', "'\\''")));
                }
                if let Some(json) = &self.json_output {
                    script.push_str(&format!("    echo '{}'\n", json.replace('\'', "'\\''")));
                }
                script.push_str(&format!("    exit {}\n    ;;\nesac\n", self.exit_code));
            } else {
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("echo '{}' >&2\n", err.replace('\'', "'\\''")));
                }

                for line in &self.progress_lines {
                    script.push_str(&format!("echo '{}'\n", line.replace('\'', "'\\''")));
                }

                if let Some(json) = &self.json_output {
                    script.push_str(&format!("echo '{}'\n", json.replace('\'', "'\\''")));
                }

                script.push_str(&format!("exit {}\n", self.exit_code));
            }

            std::fs::write(&script_path, script).expect("Failed to write mock yt-dlp");
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
                .expect("Failed to set mock yt-dlp permissions");

            script_path
        }
        #[cfg(windows)]
        {
            let script_path = dir.join("yt-dlp.bat");
            let mut script = String::from("@echo off\r\n");

            if has_both {
                script.push_str("echo %* | findstr /C:\"-J\" >nul\r\n");
                script.push_str("if %ERRORLEVEL% equ 0 (\r\n");
                if let Some(json) = &self.json_output {
                    script.push_str(&format!("    echo {}\r\n", escape_batch_line(json)));
                }
                script.push_str("    exit /b 0\r\n)\r\n");
                script.push_str("echo %* | findstr /C:\"--dump-json\" >nul\r\n");
                script.push_str("if %ERRORLEVEL% equ 0 (\r\n");
                if let Some(json) = &self.json_output {
                    script.push_str(&format!("    echo {}\r\n", escape_batch_line(json)));
                }
                script.push_str("    exit /b 0\r\n)\r\n");
                script.push_str("echo %* | findstr /C:\"--progress-template\" /C:\"--newline\" /C:\"--print\" >nul\r\n");
                script.push_str("if %ERRORLEVEL% equ 0 (\r\n");
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("    echo {} >&2\r\n", escape_batch_line(err)));
                }
                for line in &self.progress_lines {
                    script.push_str(&format!("    echo {}\r\n", escape_batch_line(line)));
                }
                script.push_str(&format!("    exit /b {}\r\n)\r\n", self.exit_code));
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("echo {} >&2\r\n", escape_batch_line(err)));
                }
                for line in &self.progress_lines {
                    script.push_str(&format!("echo {}\r\n", escape_batch_line(line)));
                }
                if let Some(json) = &self.json_output {
                    script.push_str(&format!("echo {}\r\n", escape_batch_line(json)));
                }
                script.push_str(&format!("exit /b {}\r\n", self.exit_code));
            } else {
                if let Some(err) = &self.stderr_output {
                    script.push_str(&format!("echo {} >&2\r\n", escape_batch_line(err)));
                }

                for line in &self.progress_lines {
                    script.push_str(&format!("echo {}\r\n", escape_batch_line(line)));
                }

                if let Some(json) = &self.json_output {
                    script.push_str(&format!("echo {}\r\n", escape_batch_line(json)));
                }

                script.push_str(&format!("exit /b {}\r\n", self.exit_code));
            }

            std::fs::write(&script_path, script).expect("Failed to write mock yt-dlp.bat");
            script_path
        }
    }
}

pub fn escape_batch_line(s: &str) -> String {
    s.replace('^', "^^")
        .replace('%', "%%")
        .replace('&', "^&")
        .replace('<', "^<")
        .replace('>', "^>")
        .replace('|', "^|")
}

pub fn sample_single_format_json(title: &str) -> String {
    format!(
        r#"{{"title":"{}","duration":120.0,"formats":[{{"format_id":"22","ext":"mp4","resolution":"720p","filesize":15000000.0,"vcodec":"avc1","acodec":"mp4a"}}]}}"#,
        title
    )
}

pub fn sample_multi_format_json(title: &str) -> String {
    format!(
        r#"{{"title":"{}","duration":300.0,"formats":[{{"format_id":"137","ext":"mp4","resolution":"1080p","filesize":50000000.0,"vcodec":"avc1","acodec":"none"}},{{"format_id":"140","ext":"m4a","resolution":"Audio","filesize":5000000.0,"vcodec":"none","acodec":"mp4a"}}]}}"#,
        title
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_batch_line() {
        assert_eq!(
            escape_batch_line("[download]  50.0% of ~ 10.00MiB at 2.00MiB/s"),
            "[download]  50.0%% of ~ 10.00MiB at 2.00MiB/s"
        );
        assert_eq!(
            escape_batch_line("foo & bar < baz > qux | pipe ^ caret % percent"),
            "foo ^& bar ^< baz ^> qux ^| pipe ^^ caret %% percent"
        );
    }
}
