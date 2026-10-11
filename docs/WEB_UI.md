# Web UI & REST API Guide

DWDR includes an embedded, zero-dependency HTTP server that hosts a modern browser interface and REST API.

---

## Launching the Web UI

To start the Web UI server:

```bash
dwdr --web
```

Or configure DWDR to launch the Web UI by default:

```bash
dwdr config set default_ui browser
dwdr
```

The server binds to `0.0.0.0:<port>` (default: `5500`) and automatically attempts to open your default browser to `http://localhost:<port>`.

---

## Web Interface Features

- **Wide Canvas Layout**: Generous `max-w-4xl` layout accommodating extensive format lists with a `60vh` scrollable viewer.
- **Audio & Video Classification**: Badges distinguish audio-only streams (`[AUDIO]`) from full video formats (`[VIDEO]`).
- **Clean Format Representation**: Resolution, container extension, and file size are neatly aligned, with missing size metadata displayed as a muted `N/A`.
- **Background Downloads**: Downloads execute in background worker threads without freezing the browser interface.
- **In-Browser Settings Modal**: Click the gear icon in the top right corner to inspect and update the download path or preferred default quality without opening a terminal.

---

## REST API Reference

The embedded server exposes a JSON REST API for frontend actions and automation.

### `GET /`
Serves the embedded HTML application.

### `GET /app.js`
Serves the embedded vanilla JavaScript client.

---

### `POST /api/extract`
Inspects media metadata and returns available audio/video streams.

#### Request Body
```json
{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID"
}
```

#### Success Response (200 OK)
```json
{
  "status": "success",
  "data": {
    "title": "Example Media Title",
    "duration": 214.5,
    "formats": [
      {
        "format_id": "137",
        "ext": "mp4",
        "resolution": "1080p",
        "filesize": 45120300,
        "vcodec": "avc1",
        "acodec": "none"
      }
    ]
  }
}
```

#### Error Response (200 OK / 400 Bad Request)
```json
{
  "status": "error",
  "message": "Invalid URL provided."
}
```

---

### `POST /api/download`
Spawns an asynchronous background download job for the specified format.

#### Request Body
```json
{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "format_id": "137"
}
```

#### Success Response (200 OK)
```json
{
  "status": "success",
  "task_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479"
}
```

---

### `GET /api/status?task_id=<UUID>`
Queries the current execution state of a background download job.

#### Success Response (200 OK)
```json
{
  "status": "success",
  "progress": "Starting" | "Completed" | "Error: <message>"
}
```

---

### `GET /api/config`
Retrieves the active DWDR configuration.

#### Success Response (200 OK)
```json
{
  "status": "success",
  "data": {
    "default_ui": "cli",
    "download_path": "/home/user/Downloads/DWDR",
    "port": 5500,
    "update_check_days": 0,
    "max_parallel_downloads": 3,
    "default_quality": "1080p"
  }
}
```

---

### `POST /api/config`
Updates runtime and persistent configuration settings.

#### Request Body
```json
{
  "download_path": "/custom/download/directory",
  "default_quality": "720p"
}
```

#### Success Response (200 OK)
```json
{
  "status": "success"
}
```
