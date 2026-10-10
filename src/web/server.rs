use colored::Colorize;
use std::io::Read;
use tiny_http::{Header, Response, Server, StatusCode};

pub fn start_server(config: &crate::config::manager::DwdrConfig) {
    let html_content = include_str!("static/index.html");
    let js_content = include_str!("static/app.js");

    let server_address = format!("0.0.0.0:{}", config.port);
    let server = match Server::http(&server_address) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "{}",
                format!(
                    "\nERROR: Failed to bind server to port {}: {}\n",
                    config.port, e
                )
                .red()
                .bold()
            );
            return;
        }
    };

    println!(
        "{}",
        format!(
            "INFO: Web UI server running at http://localhost:{}",
            config.port
        )
        .cyan()
    );

    let url = format!("http://localhost:{}", config.port);
    if let Err(e) = open::that(&url) {
        println!(
            "{}",
            format!("WARNING: Failed to open browser automatically: {}", e).yellow()
        );
    }

    for mut request in server.incoming_requests() {
        if request.url() == "/" {
            let response = Response::from_string(html_content);
            let header =
                Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf8"[..]).unwrap();
            let response = response.with_header(header);

            if let Err(e) = request.respond(response) {
                println!(
                    "{}",
                    format!("WARNING: Failed to respond to request: {}", e).yellow()
                );
            }
        } else if request.url() == "/app.js" {
            let response = Response::from_string(js_content);
            let header = Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/javascript; charset=utf8"[..],
            )
            .unwrap();
            let response = response.with_header(header);

            if let Err(e) = request.respond(response) {
                println!(
                    "{}",
                    format!("WARNING: Failed to respond to request: {}", e).yellow()
                );
            }
        } else if request.url().starts_with("/api/extract") && request.method().as_str() == "POST" {
            let mut body = String::new();
            if let Err(e) = request.as_reader().read_to_string(&mut body) {
                println!(
                    "{}",
                    format!("WARNING: Failed to read request body: {}", e).yellow()
                );
                let _ =
                    request.respond(Response::from_string("").with_status_code(StatusCode(400)));
                continue;
            }

            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json) => {
                    let extracted_url = json.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if !crate::utils::validator::is_valid_url(extracted_url) {
                        let dummy_resp = serde_json::json!({
                            "status": "error",
                            "message": "Invalid URL provided."
                        });
                        let response_body = dummy_resp.to_string();
                        let response = Response::from_string(response_body);
                        let header =
                            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                                .unwrap();
                        let _ = request.respond(response.with_header(header));
                        continue;
                    }

                    if let Err(e) = crate::engine::updater::ensure_dependencies() {
                        let dummy_resp = serde_json::json!({
                            "status": "error",
                            "message": e
                        });
                        let response_body = dummy_resp.to_string();
                        let response = Response::from_string(response_body);
                        let header =
                            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                                .unwrap();
                        let _ = request.respond(response.with_header(header));
                        continue;
                    }

                    let yt_dlp_path = crate::engine::paths::get_ytdlp_path();
                    match crate::engine::downloader::fetch_video_info(extracted_url, &yt_dlp_path) {
                        Ok(info) => {
                            let dummy_resp = serde_json::json!({
                                "status": "success",
                                "data": info
                            });
                            let response_body = dummy_resp.to_string();
                            let response = Response::from_string(response_body);
                            let header =
                                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                                    .unwrap();
                            let _ = request.respond(response.with_header(header));
                        }
                        Err(e) => {
                            let dummy_resp = serde_json::json!({
                                "status": "error",
                                "message": e
                            });
                            let response_body = dummy_resp.to_string();
                            let response = Response::from_string(response_body);
                            let header =
                                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                                    .unwrap();
                            let _ = request.respond(response.with_header(header));
                        }
                    }
                }
                Err(e) => {
                    println!("{}", format!("WARNING: Invalid JSON body: {}", e).yellow());
                    let _ = request
                        .respond(Response::from_string("").with_status_code(StatusCode(400)));
                }
            }
        } else {
            let _ = request.respond(Response::empty(StatusCode(404)));
        }
    }
}
