//! The bridge the browser extension talks to.
//!
//! IDM hands a link to its browser extension the other way round: the
//! extension asks the application, over a native messaging host, to take
//! over a download. The clone does the same thing with a small server on the
//! loopback interface, which is what a browser extension can reach without
//! any installation on the system.
//!
//! The server does nothing itself: it reads the address of the link out of
//! the request and hands it to the window, which asks the user the way the
//! add dialog does, with the folder of the options window.

use tauri::{AppHandle, Emitter};

use idm_core::log;

/// The event the window listens to.
const DOWNLOAD_EVENT: &str = "bridge://download";

/// The port the extension talks to. It is on the loopback interface, so it
/// is only reachable from this machine.
pub const PORT: u16 = 27500;

/// Start the server. It runs until the application ends.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(listener) => listener,
            Err(error) => {
                log::write("warn", &format!("the browser bridge did not start: {error}"));
                return;
            }
        };
        log::write(
            "info",
            &format!("the browser bridge waits on 127.0.0.1:{PORT}"),
        );

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = serve(app, stream).await;
            });
        }
    });
}

/// Read one request and answer it.
async fn serve(app: AppHandle, mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
    use tokio::io::AsyncReadExt;

    let mut buffer = vec![0u8; 8192];
    let read = stream.read(&mut buffer).await?;
    let head = String::from_utf8_lossy(&buffer[..read]).to_string();

    let origin = header(&head, "origin");

    match parse_request(&head) {
        Err(status) => {
            let _ = reply(&mut stream, status, "no", origin.as_deref()).await;
        }
        Ok(request) => {
            if request.ping {
                let _ = reply(&mut stream, 200, "ok", origin.as_deref()).await;
                return Ok(());
            }
            let payload = serde_json::json!({
                "url": request.url,
                "name": request.name,
                "referer": request.referer,
            });
            log::write("info", &format!("the browser asked for {}", request.url));
            // The dialog the link opens lives in the main window, which may
            // be in the tray. Bring it out before asking the user.
            crate::show_window(&app);
            let _ = app.emit(DOWNLOAD_EVENT, payload);
            let _ = reply(
                &mut stream,
                200,
                "the download waits in the add dialog",
                origin.as_deref(),
            )
            .await;
        }
    }
    Ok(())
}

/// What the extension asked for.
struct Request {
    url: String,
    name: String,
    referer: String,
    /// The popup of the extension only asks whether the application is there.
    ping: bool,
}

/// Read the request head. The number is the status to answer with when the
/// request cannot be used.
fn parse_request(head: &str) -> Result<Request, u16> {
    let Some(line) = head.lines().next() else {
        return Err(400);
    };
    let mut parts = line.split_whitespace();
    let _method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    // A page in the browser must not be able to start a download by itself:
    // only the extension, which has no page origin, is let in.
    if let Some(origin) = header(head, "origin") {
        if !origin.starts_with("chrome-extension://")
            && !origin.starts_with("moz-extension://")
        {
            return Err(403);
        }
    }

    let query = target.split_once('?').map(|(_, query)| query).unwrap_or("");

    // The popup of the extension asks whether the application is there.
    if target == "/ping" || target.starts_with("/ping?") {
        return Ok(Request {
            url: String::new(),
            name: String::new(),
            referer: String::new(),
            ping: true,
        });
    }

    let url = percent_decode(&value_of(query, "url"));
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")) {
        return Err(400);
    }

    Ok(Request {
        url,
        name: percent_decode(&value_of(query, "name")),
        referer: percent_decode(&value_of(query, "referer")),
        ping: false,
    })
}

/// Answer one request.
async fn reply(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    text: &str,
    origin: Option<&str>,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        text.len()
    );
    if let Some(origin) = origin {
        head.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(text.as_bytes()).await?;
    stream.flush().await
}

/// The value of one header, in lower case.
fn header(head: &str, name: &str) -> Option<String> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim().to_string())
}

/// The value of one field of the query string.
fn value_of(query: &str, name: &str) -> String {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
}

/// Decode the percent escapes of a query value.
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
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_value_is_read_out() {
        let query = "url=https%3A%2F%2Fexample.org%2Fa.bin&name=a.bin";
        assert_eq!(value_of(query, "url"), "https%3A%2F%2Fexample.org%2Fa.bin");
        assert_eq!(percent_decode(&value_of(query, "url")), "https://example.org/a.bin");
        assert_eq!(value_of(query, "missing"), "");
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn a_header_is_read_out() {
        let head = "GET / HTTP/1.1\r\nOrigin: chrome-extension://abc\r\n\r\n";
        assert_eq!(
            header(head, "origin").as_deref(),
            Some("chrome-extension://abc")
        );
        assert_eq!(header(head, "referer"), None);
    }

    #[test]
    fn a_link_from_the_extension_is_read() {
        let head = "GET /download?url=https%3A%2F%2Fexample.org%2Fa.bin&name=a.bin HTTP/1.1\r\nOrigin: chrome-extension://abc\r\n\r\n";
        let request = parse_request(head).expect("the request is good");
        assert_eq!(request.url, "https://example.org/a.bin");
        assert_eq!(request.name, "a.bin");
        assert!(!request.ping);
    }

    #[test]
    fn a_page_of_the_browser_is_not_let_in() {
        let head = "GET /download?url=https%3A%2F%2Fexample.org%2Fa.bin HTTP/1.1\r\nOrigin: https://example.org\r\n\r\n";
        assert_eq!(parse_request(head).err(), Some(403));
    }

    #[test]
    fn a_request_without_an_address_is_refused() {
        let head = "GET /download?url=not-an-address HTTP/1.1\r\n\r\n";
        assert_eq!(parse_request(head).err(), Some(400));
    }

    #[test]
    fn the_popup_may_ask_whether_the_application_is_there() {
        let head = "GET /ping HTTP/1.1\r\nOrigin: chrome-extension://abc\r\n\r\n";
        let request = parse_request(head).expect("the ping is good");
        assert!(request.ping);
    }
}
