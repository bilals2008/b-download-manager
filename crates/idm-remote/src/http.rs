//! The HTTP the remote speaks.
//!
//! One request per connection: it is read, answered and closed. A phone on a
//! home network needs nothing more, and a parser that holds only that is
//! small enough to be sure about.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The largest request head that is read. A real request is a few hundred
/// bytes; the limit is there so a broken client cannot grow the buffer.
const MAX_HEAD: usize = 16 * 1024;

/// The largest body that is read. A request is one small JSON object.
const MAX_BODY: usize = 64 * 1024;

/// How long a client has to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// What can go wrong while reading a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    Malformed,
    TooLarge,
    Timeout,
    Disconnected,
}

impl ReadError {
    /// The status the client is answered with.
    pub fn status(self) -> u16 {
        match self {
            ReadError::Malformed => 400,
            ReadError::TooLarge => 413,
            ReadError::Timeout => 408,
            ReadError::Disconnected => 400,
        }
    }
}

/// One request, after the head and the body have been read in full.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    /// The target as it arrived, query string included.
    pub target: String,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// A header by name, in any case. The stored names are lower case.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The path of the target, without the query string.
    pub fn path(&self) -> &str {
        self.target
            .split_once('?')
            .map(|(path, _)| path)
            .unwrap_or(&self.target)
    }

    /// A value of the query string.
    pub fn query(&self, name: &str) -> Option<String> {
        let (_, query) = self.target.split_once('?')?;
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == name {
                Some(percent_decode(value))
            } else {
                None
            }
        })
    }
}

/// A value of the query string, with the escapes undone.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read one request. The caller closes the connection afterwards.
pub async fn read(stream: &mut TcpStream) -> Result<Request, ReadError> {
    let mut buffer = Vec::with_capacity(2048);
    let head_end = loop {
        if let Some(end) = find_head_end(&buffer) {
            break end;
        }
        if buffer.len() > MAX_HEAD {
            return Err(ReadError::TooLarge);
        }
        let mut chunk = [0u8; 4096];
        let read = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| ReadError::Timeout)?
            .map_err(|_| ReadError::Disconnected)?;
        if read == 0 {
            return Err(ReadError::Disconnected);
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let (method, target, headers, length) = {
        let head = std::str::from_utf8(&buffer[..head_end]).map_err(|_| ReadError::Malformed)?;
        let mut lines = head.split("\r\n");
        let request_line = lines.next().ok_or(ReadError::Malformed)?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().ok_or(ReadError::Malformed)?;
        let target = parts.next().ok_or(ReadError::Malformed)?;
        let version = parts.next().ok_or(ReadError::Malformed)?;
        if !version.starts_with("HTTP/") || method.is_empty() || target.is_empty() {
            return Err(ReadError::Malformed);
        }

        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let (name, value) = line.split_once(':').ok_or(ReadError::Malformed)?;
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }

        let length = headers
            .iter()
            .find(|(name, _)| name == "content-length")
            .map(|(_, value)| value.parse::<usize>().map_err(|_| ReadError::Malformed))
            .transpose()?
            .unwrap_or(0);
        if length > MAX_BODY {
            return Err(ReadError::TooLarge);
        }

        (method.to_string(), target.to_string(), headers, length)
    };

    // The first read may already hold part of the body; only what is missing
    // is read from the socket.
    while buffer.len() < head_end + 4 + length {
        let mut chunk = [0u8; 4096];
        let read = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| ReadError::Timeout)?
            .map_err(|_| ReadError::Disconnected)?;
        if read == 0 {
            return Err(ReadError::Malformed);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Ok(Request {
        method: method.to_ascii_uppercase(),
        target,
        headers,
        body: buffer[head_end + 4..head_end + 4 + length].to_vec(),
    })
}

/// Answer one request and close the connection.
pub async fn write(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n",
        reason(status),
        body.len()
    );
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");

    stream.write_all(head.as_bytes()).await?;
    if !body.is_empty() {
        stream.write_all(body).await?;
    }
    stream.flush().await
}

/// The reason a status is written with.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

/// The byte offset just before the blank line that ends the head.
fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_end_is_found() {
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nrest"), Some(14));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn every_error_maps_to_a_status() {
        assert_eq!(ReadError::Malformed.status(), 400);
        assert_eq!(ReadError::TooLarge.status(), 413);
        assert_eq!(ReadError::Timeout.status(), 408);
        assert_eq!(ReadError::Disconnected.status(), 400);
    }

    #[test]
    fn the_query_is_read() {
        let request = Request {
            method: "GET".to_string(),
            target: "/api/downloads?token=abc%20def&x=1".to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        };
        assert_eq!(request.path(), "/api/downloads");
        assert_eq!(request.query("token").as_deref(), Some("abc def"));
        assert_eq!(request.query("missing"), None);
    }
}
