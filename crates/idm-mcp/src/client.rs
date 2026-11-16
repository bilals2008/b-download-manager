//! The client that talks to the running application.
//!
//! The application is the only place that knows how a download works, so this
//! crate never starts an engine of its own: it asks the running app over the
//! app's local API and reports what the app says. The app is also the one that
//! serialises the commands, so the server sends one command at a time and
//! never holds two connections open at once.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The address of the application's local API. The bridge is bound to the
/// loopback interface on purpose: nothing outside this machine may reach it.
pub const ADDRESS: &str = "127.0.0.1:27500";

/// The header every request carries, so the app can tell the assistant's
/// server from the browser extension.
pub const CLIENT_HEADER: &str = "x-bdm-client";

/// The value of that header for this client.
pub const CLIENT_VALUE: &str = "mcp-server";

/// How long the server waits for the app to answer.
const TIMEOUT: Duration = Duration::from_secs(5);

/// A sample of a download's byte count, used to work out a speed.
type Sample = (u64, Instant);

/// What can go wrong while talking to the application.
#[derive(Debug, Clone)]
pub enum AppError {
    /// Nothing is listening: the application is not running.
    NotRunning,
    /// The application answered and said no.
    Refused(String),
    /// The answer was not what this server expects.
    Malformed(String),
}

impl AppError {
    /// The sentence the assistant reads.
    pub fn message(&self) -> String {
        match self {
            AppError::NotRunning => {
                "the download manager is not running. Start it and try again.".to_string()
            }
            AppError::Refused(reason) => {
                format!("the download manager refused the request: {reason}")
            }
            AppError::Malformed(reason) => format!(
                "the download manager answered with something this server cannot read: {reason}"
            ),
        }
    }
}

/// The operations the tools need from the running application.
pub trait App {
    /// The downloads and their state.
    fn downloads(&mut self) -> Result<Value, AppError>;
    /// Start a download.
    fn add_download(
        &mut self,
        url: &str,
        folder: Option<&str>,
        connections: Option<u32>,
    ) -> Result<Value, AppError>;
    /// Pause a download.
    fn pause(&mut self, id: u64) -> Result<Value, AppError>;
    /// Start a paused download again.
    fn resume(&mut self, id: u64) -> Result<Value, AppError>;
    /// Take a download out of the list, with or without its file.
    fn remove(&mut self, id: u64, delete_file: bool) -> Result<Value, AppError>;
    /// The queue: what waits, what runs and how many may run at once.
    fn queue(&mut self) -> Result<Value, AppError>;
}

/// The client that speaks to the application's local API over HTTP.
pub struct HttpClient {
    address: String,
    /// The last byte count of every download, so a speed can be worked out.
    samples: HashMap<u64, Sample>,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    /// A client that talks to the application on its own address.
    pub fn new() -> Self {
        Self::with_address(ADDRESS)
    }

    /// A client that talks to another address, for the tests.
    pub fn with_address(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            samples: HashMap::new(),
        }
    }

    /// The address this client talks to.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Ask the application for something and hand back what it said.
    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, AppError> {
        let address = self.address.clone();
        let socket = resolve(&address)?;
        let mut stream =
            TcpStream::connect_timeout(&socket, TIMEOUT).map_err(|_| AppError::NotRunning)?;
        let _ = stream.set_read_timeout(Some(TIMEOUT));
        let _ = stream.set_write_timeout(Some(TIMEOUT));

        let payload = body
            .map(|value| serde_json::to_vec(value).unwrap_or_default())
            .unwrap_or_default();
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{CLIENT_HEADER}: {CLIENT_VALUE}\r\nAccept: application/json\r\n"
        );
        if body.is_some() {
            head.push_str("Content-Type: application/json\r\n");
            head.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        head.push_str("Connection: close\r\n\r\n");

        stream
            .write_all(head.as_bytes())
            .map_err(|error| AppError::Malformed(error.to_string()))?;
        stream
            .write_all(&payload)
            .map_err(|error| AppError::Malformed(error.to_string()))?;
        stream
            .flush()
            .map_err(|error| AppError::Malformed(error.to_string()))?;

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|error| AppError::Malformed(error.to_string()))?;
        read_answer(&raw)
    }

    /// Add the speed of every download, worked out from the bytes it received
    /// since the last list.
    ///
    /// The app publishes the byte count, not the speed, and the window works
    /// the speed out the same way: from two counts and the time between them.
    fn add_speeds(&mut self, data: &mut Value) {
        let now = Instant::now();
        let Some(downloads) = data.get_mut("downloads").and_then(Value::as_array_mut) else {
            return;
        };
        for download in downloads.iter_mut() {
            let Some(id) = download.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let received = download
                .get("downloaded")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let speed = self.samples.get(&id).and_then(|(before, at)| {
                let seconds = now.duration_since(*at).as_secs_f64();
                (seconds > 0.0).then(|| received.saturating_sub(*before) as f64 / seconds)
            });
            if let Some(speed) = speed {
                if let Some(object) = download.as_object_mut() {
                    object.insert("speedBps".to_string(), json!(speed.round() as u64));
                }
            }
            self.samples.insert(id, (received, now));
        }
    }
}

impl App for HttpClient {
    fn downloads(&mut self) -> Result<Value, AppError> {
        let mut data = self.request("GET", "/v1/mcp/downloads", None)?;
        self.add_speeds(&mut data);
        Ok(data)
    }

    fn add_download(
        &mut self,
        url: &str,
        folder: Option<&str>,
        connections: Option<u32>,
    ) -> Result<Value, AppError> {
        let mut body = json!({ "url": url });
        if let Some(folder) = folder {
            body["folder"] = Value::String(folder.to_string());
        }
        if let Some(connections) = connections {
            body["connections"] = json!(connections);
        }
        self.request("POST", "/v1/mcp/downloads", Some(&body))
    }

    fn pause(&mut self, id: u64) -> Result<Value, AppError> {
        self.request(
            "POST",
            &format!("/v1/mcp/downloads/{id}/pause"),
            Some(&json!({})),
        )
    }

    fn resume(&mut self, id: u64) -> Result<Value, AppError> {
        self.request(
            "POST",
            &format!("/v1/mcp/downloads/{id}/resume"),
            Some(&json!({})),
        )
    }

    fn remove(&mut self, id: u64, delete_file: bool) -> Result<Value, AppError> {
        self.request(
            "POST",
            &format!("/v1/mcp/downloads/{id}/remove"),
            Some(&json!({ "deleteFile": delete_file })),
        )
    }

    fn queue(&mut self) -> Result<Value, AppError> {
        self.request("GET", "/v1/mcp/queue", None)
    }
}

/// The socket of the address, which has to be on this machine.
fn resolve(address: &str) -> Result<SocketAddr, AppError> {
    let mut addresses = address
        .to_socket_addrs()
        .map_err(|error| AppError::Malformed(format!("{address} is not an address: {error}")))?;
    let socket = addresses
        .next()
        .ok_or_else(|| AppError::Malformed(format!("{address} names no host")))?;
    if !socket.ip().is_loopback() {
        return Err(AppError::Malformed(
            "this server only talks to the loopback interface".to_string(),
        ));
    }
    Ok(socket)
}

/// Read the answer of the application: the envelope the bridge wraps every
/// answer in.
fn read_answer(raw: &[u8]) -> Result<Value, AppError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| AppError::Malformed("the answer was not text".to_string()))?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| AppError::Malformed("the answer had no body".to_string()))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(body.len());
    let body = &body[..length.min(body.len())];

    let answer: Value = serde_json::from_str(body)
        .map_err(|error| AppError::Malformed(format!("the answer was not JSON: {error}")))?;
    match answer.get("ok") {
        Some(Value::Bool(true)) => Ok(answer.get("data").cloned().unwrap_or(Value::Null)),
        Some(Value::Bool(false)) => Err(AppError::Refused(
            answer
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("no reason was given")
                .to_string(),
        )),
        _ => Err(AppError::Malformed(format!(
            "the answer was not the shape this server expects (status {status})"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// How many bytes the body of a request promises, read out of its head.
    fn content_length(head: &str) -> usize {
        head.lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Read one whole request: the head, and then the body it announced.
    ///
    /// Answering before the body is read makes the kernel reset the connection
    /// on Windows, because closing a socket with unread bytes throws away the
    /// answer too. The client then reports a transport failure instead of the
    /// refusal it was sent, which is what made this suite flake under load.
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        let mut whole = None;
        loop {
            let read = stream.read(&mut chunk).expect("a request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
            if whole.is_none() {
                if let Some(end) = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                {
                    let head = String::from_utf8_lossy(&request[..end + 4]).to_string();
                    whole = Some(end + 4 + content_length(&head));
                }
            }
            if whole.is_some_and(|whole| request.len() >= whole) {
                break;
            }
        }
        String::from_utf8_lossy(&request).to_string()
    }

    /// A tiny HTTP server that answers one request with a fixed body and
    /// hands the request back to the test.
    fn serve_once(body: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("an address").to_string();
        let body = body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let request = read_request(&mut stream);
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(answer.as_bytes()).expect("an answer");
            stream.flush().expect("a flush");
            request
        });
        (address, handle)
    }

    /// A tiny HTTP server that answers two requests, one after the other.
    fn serve_twice(bodies: [&str; 2]) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("an address").to_string();
        let bodies = bodies.map(str::to_string);
        let handle = std::thread::spawn(move || {
            let mut requests = String::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().expect("a connection");
                requests.push_str(&read_request(&mut stream));
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(answer.as_bytes()).expect("an answer");
                stream.flush().expect("a flush");
            }
            requests
        });
        (address, handle)
    }

    /// A port nothing listens on.
    fn closed_port() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("an address").to_string();
        drop(listener);
        address
    }

    #[test]
    fn the_client_asks_the_app_for_the_downloads() {
        let (address, server) = serve_once(r#"{"ok":true,"data":{"downloads":[],"active":0}}"#);
        let mut client = HttpClient::with_address(address);
        let data = client.downloads().expect("the downloads");
        assert_eq!(data["downloads"].as_array().map(Vec::len), Some(0));

        let request = server.join().expect("the request");
        assert!(request.starts_with("GET /v1/mcp/downloads HTTP/1.1"));
        assert!(request.contains(&format!("{CLIENT_HEADER}: {CLIENT_VALUE}")));
        assert!(request.contains("Host: 127.0.0.1:"));
    }

    #[test]
    fn the_client_says_the_app_is_not_running() {
        let mut client = HttpClient::with_address(closed_port());
        let error = client.downloads().expect_err("no app");
        assert!(matches!(error, AppError::NotRunning));
        assert!(error
            .message()
            .contains("the download manager is not running"));
    }

    #[test]
    fn a_refusal_is_carried_to_the_reader() {
        let (address, _server) =
            serve_once(r#"{"ok":false,"error":{"code":"not_found","message":"unknown download"}}"#);
        let mut client = HttpClient::with_address(address);
        let error = client.pause(7).expect_err("a refusal");
        assert!(error.message().contains("unknown download"));
    }

    #[test]
    fn a_speed_is_worked_out_from_two_counts() {
        let (address, server) = serve_twice([
            r#"{"ok":true,"data":{"downloads":[{"id":1,"downloaded":100}],"active":1}}"#,
            r#"{"ok":true,"data":{"downloads":[{"id":1,"downloaded":400}],"active":1}}"#,
        ]);
        let mut client = HttpClient::with_address(address);
        let first = client.downloads().expect("the downloads");
        assert!(first["downloads"][0].get("speedBps").is_none());

        let second = client.downloads().expect("the downloads");
        let speed = second["downloads"][0]["speedBps"]
            .as_u64()
            .expect("a speed");
        assert!(speed > 0, "the speed was {speed}");
        server.join().expect("the requests");
    }
}
