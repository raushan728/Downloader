use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct MockResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl MockResponse {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            status_text: "OK".to_string(),
            headers: vec![(
                "Content-Type".to_string(),
                "application/octet-stream".to_string(),
            )],
            body: body.into(),
        }
    }

    pub fn status(status: u16, status_text: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            status_text: status_text.to_string(),
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: body.into(),
        }
    }
}

pub struct MockServer {
    addr: std::net::SocketAddr,
    shutdown: Arc<AtomicBool>,
    responses: Arc<Mutex<Vec<(String, MockResponse)>>>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind mock server");
        let addr = listener.local_addr().expect("Failed to get local addr");
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let responses = Arc::new(Mutex::new(Vec::new()));
        let responses_clone = Arc::clone(&responses);

        listener
            .set_nonblocking(true)
            .expect("Failed to set nonblocking");

        let server_thread = thread::spawn(move || {
            while !shutdown_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        handle_client(&mut stream, &responses_clone);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            shutdown,
            responses,
            server_thread: Some(server_thread),
        }
    }

    pub fn url(&self, path: &str) -> String {
        let clean_path = path.trim_start_matches('/');
        format!("http://{}/{}", self.addr, clean_path)
    }

    pub fn mock_path(&self, path: &str, response: MockResponse) {
        let clean_path = format!("/{}", path.trim_start_matches('/'));
        self.responses.lock().unwrap().push((clean_path, response));
    }
}

fn handle_client(stream: &mut TcpStream, responses: &Arc<Mutex<Vec<(String, MockResponse)>>>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }

    loop {
        let mut header_line = String::new();
        match reader.read_line(&mut header_line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = header_line.trim();
                if trimmed.is_empty() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let path = if parts.len() >= 2 { parts[1] } else { "/" };

    let resp = {
        let guard = responses.lock().unwrap();
        guard.iter().find(|(p, _)| p == path).map(|(_, r)| {
            (
                r.status,
                r.status_text.clone(),
                r.headers.clone(),
                r.body.clone(),
            )
        })
    };

    if let Some((status, status_text, headers, body)) = resp {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            status,
            status_text,
            body.len()
        );
        for (k, v) in headers {
            response.push_str(&format!("{}: {}\r\n", k, v));
        }
        response.push_str("\r\n");
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(&body);
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
    } else {
        let not_found = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(not_found);
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.server_thread.take() {
            let _ = handle.join();
        }
    }
}
