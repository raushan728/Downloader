# Practical Usage & Examples Guide

This guide provides practical walkthroughs of common tasks and workflows in DWDR.

---

## Scenario 1: One-Shot Direct Video Download

If you want to quickly download a video using your default quality setting (default: `1080p`), pass the URL directly to `dwdr`:

```bash
dwdr "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```

### Terminal Output
```text
Valid URL detected. Resolving engine paths...
INFO: Fetching media details from the server...
Successfully fetched data for: Rick Astley - Never Gonna Give You Up (Official Music Video)
Extracted 28 formats.
INFO: Exact quality (1080p) found! Auto-starting download...
/ [download]  45.2% of 42.10MiB at 4.25MiB/s ETA 00:05
SUCCESS: Download completed successfully! File saved to: /home/user/Downloads/DWDR
```

---

## Scenario 2: Interactive REPL Shell Session

Run `dwdr` without arguments to launch the interactive shell. This mode is ideal for downloading multiple videos or selecting specific streams manually:

```bash
dwdr
```

### Step-by-Step Shell Walkthrough

```text
INFO: Welcome to DWDR Interactive Shell. Type a URL to download, or 'exit' to quit.
dwdr > https://www.youtube.com/watch?v=dQw4w9WgXcQ
Valid URL detected. Resolving engine paths...
INFO: Fetching media details from the server...
Successfully fetched data for: Rick Astley - Never Gonna Give You Up (Official Music Video)
Extracted 28 formats.
WARNING: Quality (720p) not found. Falling back to manual selection...
? Select a format to download (Use arrow keys):
> [VIDEO] | 1080p      | mp4   | 42.1 MB
  [VIDEO] | 480p       | mp4   | 18.5 MB
  [VIDEO] | 360p       | mp4   | 11.2 MB
  [AUDIO] | Audio      | m4a   | 3.4 MB
  [AUDIO] | Audio      | opus  | 2.8 MB
```

Use the **Up/Down arrow keys** to navigate and press **Enter** to confirm your format selection.

---

## Scenario 3: Audio Extraction

To download only the audio track from a video:
1. Start the shell:
   ```bash
   dwdr
   ```
2. Paste the URL.
3. When the format picker appears, navigate to the `[AUDIO]` formats:
   ```text
   > [AUDIO] | Audio      | m4a   | 3.4 MB
   ```
4. Press **Enter**. DWDR will download the dedicated audio stream without downloading unnecessary video data.

---

## Scenario 4: Changing Download Path with Native File Picker

You can select a new download directory using your operating system's native graphical folder chooser (`rfd`):

### Option A: From the Terminal
```bash
dwdr config
```

### Option B: From Inside the Interactive Shell
```text
dwdr > config
```

### Menu Interaction
```text
? Configuration Menu (Use arrow keys):
> 1. Download Path (Current: /home/user/Downloads/DWDR)
  2. Default Quality (Current: 1080p)
  3. Max Parallel Downloads (Current: 3)
  4. Save & Exit
  5. Cancel
```

1. Select `1. Download Path` and press **Enter**.
2. Your desktop's native folder dialog will appear. Browse to your preferred directory (e.g. `/home/user/Videos`) and click **Select Folder**.
3. Select `4. Save & Exit` to persist the setting.

---

## Scenario 5: Programmatic Configuration via CLI

To update settings directly from terminal scripts without interactive prompts:

```bash
# Set preferred quality to 720p
dwdr config set quality 720p

# Set parallel download fragments to 5
dwdr config set parallel 5

# Set custom download path
dwdr config set path /media/external/downloads

# Verify changes
dwdr config show
```

### Output
```text
Current Configuration:
download_path: /media/external/downloads
update_check_days: 0
max_parallel_downloads: 5
default_quality: 720p
```

---

## Scenario 6: Refreshing Engine Dependencies

If a platform updates its video streaming algorithms and downloads begin failing, refresh your isolated `yt-dlp` and `ffmpeg` engines:

```bash
dwdr update
```

### Output
```text
INFO: Forcing update for core engines...
INFO: Core engine missing. Downloading latest yt-dlp... This will only happen once.
SUCCESS: Engine downloaded successfully!
SUCCESS: All core engines are updated to the latest versions!
```

---

## Scenario 7: Resetting Configuration to Factory Defaults

To discard all custom settings and restore default paths and quality:

```bash
dwdr config reset
```

### Output
```text
SUCCESS: Configuration reset to defaults.
```
