use colored::Colorize;
use tiny_http::{Header, Response, Server};

pub fn start_server(port: u16) {
    let html_content = include_str!("static/index.html");

    let server_address = format!("0.0.0.0:{}", port);
    let server = match Server::http(&server_address) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "{}",
                format!("\nERROR: Failed to bind server to port {}: {}\n", port, e)
                    .red()
                    .bold()
            );
            return;
        }
    };

    println!(
        "{}",
        format!("INFO: Web UI server running at http://localhost:{}", port).cyan()
    );

    let url = format!("http://localhost:{}", port);
    if let Err(e) = open::that(&url) {
        println!(
            "{}",
            format!("WARNING: Failed to open browser automatically: {}", e).yellow()
        );
    }

    for request in server.incoming_requests() {
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
    }
}
