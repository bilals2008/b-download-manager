//! The listener a phone on the same network talks to.
//!
//! This is a second door next to the loopback bridge, and it is guarded the
//! same way: it is bound to one local address, it refuses a request that did
//! not come from the local network, it refuses a request whose `Host` header
//! names somewhere else, and it is off until the reader turns it on.
//!
//! The difference is the door itself. The bridge is reached by the browser
//! extension on this machine; the remote is reached by a phone on the home
//! network, so it asks for a one-time code first and keeps a token per
//! device afterwards. The token is stored as a hash, the device can be
//! revoked, and the listener can be stopped without a window.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::backend::RemoteBackend;
use crate::devices::DeviceStore;
use crate::http::{self, Request};
use crate::net::{host_header_is_local, is_local_address, split_host};
use crate::page::PAGE;
use crate::pairing::{new_pairing_code, PairingCode};

/// The port the remote listens on when the reader has not chosen one. It is
/// next to the bridge's port and has nothing to do with it.
pub const DEFAULT_PORT: u16 = 27600;

/// What the listener needs to know to start.
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// Whether the reader turned the remote on at all.
    pub enabled: bool,
    /// The address it is bound to. `None` picks the address of this machine
    /// on its own network, never every address.
    pub interface: Option<IpAddr>,
    pub port: u16,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interface: None,
            port: DEFAULT_PORT,
        }
    }
}

/// Why the remote did not start.
#[derive(Debug)]
pub enum RemoteError {
    /// The reader has not turned it on.
    Disabled,
    /// The port could not be taken.
    Bind(std::io::Error),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteError::Disabled => write!(f, "the remote is off"),
            RemoteError::Bind(error) => write!(f, "the remote could not be bound: {error}"),
        }
    }
}

impl std::error::Error for RemoteError {}

/// The running remote.
#[derive(Clone)]
pub struct RemoteServer {
    /// The address it is listening on, with the port the system gave it.
    local_addr: SocketAddr,
    store: Arc<DeviceStore>,
    backend: Arc<RwLock<Option<Arc<dyn RemoteBackend>>>>,
    pairing: Arc<Mutex<Option<PairingCode>>>,
    stop: watch::Sender<bool>,
}

impl RemoteServer {
    /// Start the listener.
    ///
    /// It refuses to start when the reader has not turned it on, and it binds
    /// to one address: the one that was chosen, or the address of this
    /// machine on its own network.
    pub async fn start(
        config: RemoteConfig,
        store: Arc<DeviceStore>,
    ) -> Result<RemoteServer, RemoteError> {
        if !config.enabled {
            return Err(RemoteError::Disabled);
        }

        let address = SocketAddr::new(
            config.interface.unwrap_or_else(default_address),
            config.port,
        );
        let listener = TcpListener::bind(address)
            .await
            .map_err(RemoteError::Bind)?;
        let local_addr = listener.local_addr().map_err(RemoteError::Bind)?;

        let (stop, mut stopped) = watch::channel(false);
        let server = RemoteServer {
            local_addr,
            store,
            backend: Arc::new(RwLock::new(None)),
            pairing: Arc::new(Mutex::new(Some(new_pairing_code()))),
            stop,
        };

        let running = server.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((stream, peer)) = accepted else {
                            continue;
                        };
                        let serving = running.clone();
                        tokio::spawn(async move {
                            let _ = serve(serving, stream, peer).await;
                        });
                    }
                }
            }
        });

        Ok(server)
    }

    /// Close the door to new requests.
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }

    /// The address the phone reaches the remote on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The port the remote listens on.
    pub fn port(&self) -> u16 {
        self.local_addr.port()
    }

    /// Tell the remote where the downloads are. The application owns them, so
    /// it answers the questions the remote asks.
    pub fn set_backend(&self, backend: Arc<dyn RemoteBackend>) {
        let mut slot = self
            .backend
            .write()
            .unwrap_or_else(|poison| poison.into_inner());
        *slot = Some(backend);
    }

    /// The code the phone is paired with, while it is still good.
    pub fn pairing_code(&self) -> Option<PairingCode> {
        self.pairing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// A new code, for the next phone.
    pub fn rotate_pairing_code(&self) -> PairingCode {
        let code = new_pairing_code();
        let mut slot = self
            .pairing
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *slot = Some(code.clone());
        code
    }
}

/// The address of this machine on its own network.
///
/// A UDP socket that is connected but never written to only picks the route
/// the machine would take, which is the address the phone can reach.
fn default_address() -> IpAddr {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("192.0.2.1:9").is_ok() {
            if let Ok(address) = socket.local_addr() {
                if !address.ip().is_unspecified() {
                    return address.ip();
                }
            }
        }
    }
    IpAddr::from([127, 0, 0, 1])
}

/// Read one request, answer it, and close the connection.
async fn serve(
    server: RemoteServer,
    mut stream: TcpStream,
    peer: SocketAddr,
) -> std::io::Result<()> {
    let request = match http::read(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            return http::write(
                &mut stream,
                error.status(),
                "application/json",
                &json_bytes(json!({ "error": "the request could not be read" })),
                &[],
            )
            .await;
        }
    };

    // A request that did not come from the local network is not a phone on
    // the sofa, whatever it says.
    if !is_local_address(&peer) {
        return http::write(
            &mut stream,
            403,
            "application/json",
            &json_bytes(json!({ "error": "this is not on the local network" })),
            &[],
        )
        .await;
    }

    // The `Host` header has to name this machine. A name can be pointed
    // somewhere else, and that is a rebinding attempt.
    let host = request.header("host").unwrap_or_default();
    if !host_is_local(&server, host) {
        return http::write(
            &mut stream,
            403,
            "application/json",
            &json_bytes(json!({ "error": "this is not this machine" })),
            &[],
        )
        .await;
    }

    let (status, content_type, body) = route(&server, &request).await;
    http::write(&mut stream, status, content_type, &body, &[]).await
}

/// The routes of the remote.
async fn route(server: &RemoteServer, request: &Request) -> (u16, &'static str, Vec<u8>) {
    let path = request.path();

    // The page itself carries nothing: it is the shell the phone pairs
    // through, so it is served to anyone on the network. A token that is
    // offered and is wrong is refused all the same.
    if request.method == "GET" && path == "/" {
        if let Some(token) = token_of(request) {
            if !server.store.check(&token) {
                return json_reply(401, json!({ "error": "this device is not paired" }));
            }
        }
        return (200, "text/html; charset=utf-8", PAGE.as_bytes().to_vec());
    }

    // The pairing exchange is the one route that does not need a token: it
    // has the one-time code instead.
    if request.method == "POST" && path == "/api/pair" {
        return pair(server, request);
    }

    // Every other route carries data or changes something, so it checks the
    // token.
    if !authorized(server, request) {
        return json_reply(401, json!({ "error": "this device is not paired" }));
    }

    match (request.method.as_str(), path) {
        ("GET", "/api/downloads") => downloads(server),
        ("POST", "/api/downloads") => add(server, request),
        ("POST", path) => match control_route(path) {
            Some((id, action)) => control(server, id, action),
            None => json_reply(404, json!({ "error": "there is no such route" })),
        },
        ("GET", _) => json_reply(404, json!({ "error": "there is no such route" })),
        _ => json_reply(405, json!({ "error": "this method is not used here" })),
    }
}

/// The body of a pairing request.
#[derive(Debug, Deserialize)]
struct PairRequest {
    #[serde(default)]
    code: String,
    #[serde(default)]
    name: String,
}

/// The body of an add request.
#[derive(Debug, Deserialize)]
struct AddRequest {
    #[serde(default)]
    url: String,
    #[serde(default)]
    folder: Option<String>,
}

/// Exchange the one-time code for a token.
fn pair(server: &RemoteServer, request: &Request) -> (u16, &'static str, Vec<u8>) {
    let body: PairRequest = match serde_json::from_slice(&request.body) {
        Ok(body) => body,
        Err(_) => return json_reply(400, json!({ "error": "the body could not be read" })),
    };

    let mut slot = server
        .pairing
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(code) = slot.as_ref() else {
        return json_reply(
            401,
            json!({ "error": "there is no pairing code, ask the app for one" }),
        );
    };
    if !code.accepts(&body.code) {
        return json_reply(401, json!({ "error": "the pairing code is not valid" }));
    }
    // The code is good for one phone only.
    *slot = None;
    drop(slot);

    let (device, token) = server.store.add(&body.name);
    json_reply(
        200,
        json!({
            "token": token,
            "deviceId": device.id,
            "name": device.name,
        }),
    )
}

/// The list of downloads.
fn downloads(server: &RemoteServer) -> (u16, &'static str, Vec<u8>) {
    let Some(backend) = backend_of(server) else {
        return json_reply(
            503,
            json!({ "error": "the app has not connected the remote yet" }),
        );
    };
    let downloads = backend.downloads();
    json_reply(200, json!({ "downloads": downloads }))
}

/// Add a link.
fn add(server: &RemoteServer, request: &Request) -> (u16, &'static str, Vec<u8>) {
    let body: AddRequest = match serde_json::from_slice(&request.body) {
        Ok(body) => body,
        Err(_) => return json_reply(400, json!({ "error": "the body could not be read" })),
    };
    if body.url.trim().is_empty() {
        return json_reply(400, json!({ "error": "there is no link" }));
    }
    let Some(backend) = backend_of(server) else {
        return json_reply(
            503,
            json!({ "error": "the app has not connected the remote yet" }),
        );
    };
    let folder = body
        .folder
        .map(|folder| folder.trim().to_string())
        .filter(|folder| !folder.is_empty());

    match backend.add(body.url.trim(), folder.as_deref()) {
        Ok(id) => json_reply(200, json!({ "id": id })),
        Err(message) => json_reply(409, json!({ "error": message })),
    }
}

/// Pause, resume or remove one download.
fn control(server: &RemoteServer, id: u64, action: &str) -> (u16, &'static str, Vec<u8>) {
    let Some(backend) = backend_of(server) else {
        return json_reply(
            503,
            json!({ "error": "the app has not connected the remote yet" }),
        );
    };
    let result = match action {
        "pause" => backend.pause(id),
        "resume" => backend.resume(id),
        "remove" => backend.remove(id),
        _ => return json_reply(404, json!({ "error": "there is no such action" })),
    };
    match result {
        Ok(()) => json_reply(200, json!({ "id": id, "action": action })),
        Err(message) => json_reply(409, json!({ "error": message })),
    }
}

/// The id and the action of an `/api/downloads/<id>/<action>` route.
fn control_route(path: &str) -> Option<(u64, &str)> {
    let rest = path.strip_prefix("/api/downloads/")?;
    let (id, action) = rest.split_once('/')?;
    if action.is_empty() || action.contains('/') {
        return None;
    }
    Some((id.parse().ok()?, action))
}

/// Whether the `Host` header names this machine.
///
/// A local address is enough, and so is the address the reader bound the
/// remote to, which is not a private one on a machine that is on the
/// internet itself.
fn host_is_local(server: &RemoteServer, host: &str) -> bool {
    let port = server.local_addr.port();
    if host_header_is_local(host, port) {
        return true;
    }
    let (name, given) = split_host(host.trim());
    if let Some(given) = given {
        if given != port {
            return false;
        }
    }
    let name = name.trim_matches(|c| c == '[' || c == ']');
    match name.parse::<IpAddr>() {
        Ok(ip) => ip == server.local_addr.ip(),
        Err(_) => false,
    }
}

/// Whether the request carries a token that belongs to a device.
fn authorized(server: &RemoteServer, request: &Request) -> bool {
    match token_of(request) {
        Some(token) => server.store.check(&token),
        None => false,
    }
}

/// The token of a request, from the header or from the query string.
fn token_of(request: &Request) -> Option<String> {
    if let Some(header) = request.header("authorization") {
        let (scheme, token) = header.split_once(' ')?;
        if scheme.eq_ignore_ascii_case("bearer") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    request.query("token")
}

/// The application behind the remote.
fn backend_of(server: &RemoteServer) -> Option<Arc<dyn RemoteBackend>> {
    server
        .backend
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// An answer as bytes.
fn json_bytes(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap_or_default()
}

/// An answer that is JSON.
fn json_reply(status: u16, value: Value) -> (u16, &'static str, Vec<u8>) {
    (status, "application/json", json_bytes(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::RemoteDownload;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A backend that remembers what it was asked.
    #[derive(Default)]
    struct Fake {
        added: Mutex<Vec<(String, Option<String>)>>,
    }

    impl RemoteBackend for Fake {
        fn downloads(&self) -> Vec<RemoteDownload> {
            vec![RemoteDownload {
                id: 7,
                name: "file.zip".to_string(),
                size: 100,
                downloaded: 25,
                speed: 10,
                state: "running".to_string(),
            }]
        }

        fn add(&self, url: &str, folder: Option<&str>) -> Result<String, String> {
            self.added
                .lock()
                .unwrap()
                .push((url.to_string(), folder.map(str::to_string)));
            Ok("8".to_string())
        }

        fn pause(&self, _id: u64) -> Result<(), String> {
            Ok(())
        }

        fn resume(&self, _id: u64) -> Result<(), String> {
            Ok(())
        }

        fn remove(&self, _id: u64) -> Result<(), String> {
            Ok(())
        }
    }

    /// A running remote on the loopback, with a fake application behind it.
    async fn running() -> (RemoteServer, Arc<Fake>) {
        let server = RemoteServer::start(
            RemoteConfig {
                enabled: true,
                interface: Some(IpAddr::from([127, 0, 0, 1])),
                port: 0,
            },
            Arc::new(DeviceStore::new()),
        )
        .await
        .expect("the remote starts");
        let fake = Arc::new(Fake::default());
        server.set_backend(fake.clone());
        (server, fake)
    }

    /// Ask the remote something and read the answer.
    async fn ask(server: &RemoteServer, request: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(server.local_addr())
            .await
            .expect("the remote answers");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).await.unwrap();
        let text = String::from_utf8_lossy(&answer).to_string();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    /// A request with the right host header.
    fn request(server: &RemoteServer, method: &str, path: &str, extra: &str, body: &str) -> String {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            server.port(),
            body.len()
        )
    }

    /// The token of a paired phone.
    async fn pair(server: &RemoteServer) -> String {
        let code = server.pairing_code().expect("there is a code");
        let (_, body) = ask(
            server,
            &request(
                server,
                "POST",
                "/api/pair",
                "Content-Type: application/json\r\n",
                &format!("{{\"code\":\"{}\",\"name\":\"phone\"}}", code.code),
            ),
        )
        .await;
        let value: Value = serde_json::from_str(&body).expect("the answer is JSON");
        value["token"]
            .as_str()
            .expect("there is a token")
            .to_string()
    }

    #[tokio::test]
    async fn a_request_without_a_token_is_refused() {
        let (server, _fake) = running().await;
        let (status, _) = ask(&server, &request(&server, "GET", "/api/downloads", "", "")).await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn a_request_with_a_token_answers_the_list() {
        let (server, _fake) = running().await;
        let token = pair(&server).await;
        let (status, body) = ask(
            &server,
            &request(
                &server,
                "GET",
                "/api/downloads",
                &format!("Authorization: Bearer {token}\r\n"),
                "",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert!(body.contains("file.zip"));
        assert!(body.contains("\"downloaded\":25"));
    }

    #[tokio::test]
    async fn an_add_reaches_the_application() {
        let (server, fake) = running().await;
        let token = pair(&server).await;
        let (status, body) = ask(
            &server,
            &request(
                &server,
                "POST",
                "/api/downloads",
                &format!("Authorization: Bearer {token}\r\nContent-Type: application/json\r\n"),
                "{\"url\":\"https://example.org/file.zip\",\"folder\":\"Downloads\"}",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert!(body.contains("\"id\":\"8\""));
        let added = fake.added.lock().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, "https://example.org/file.zip");
        assert_eq!(added[0].1.as_deref(), Some("Downloads"));
    }

    #[tokio::test]
    async fn a_foreign_host_header_is_refused() {
        let (server, _fake) = running().await;
        let request = format!(
            "GET /api/downloads HTTP/1.1\r\nHost: evil.example.com:{}\r\nConnection: close\r\n\r\n",
            server.port()
        );
        let (status, _) = ask(&server, &request).await;
        assert_eq!(status, 403);
    }

    #[tokio::test]
    async fn the_page_says_the_connection_is_plain_http() {
        let (server, _fake) = running().await;
        let (status, body) = ask(&server, &request(&server, "GET", "/", "", "")).await;
        assert_eq!(status, 200);
        assert!(body.contains("HTTP"));
    }

    #[tokio::test]
    async fn a_code_is_good_for_one_phone_only() {
        let (server, _fake) = running().await;
        let code = server.pairing_code().expect("there is a code");
        let body = format!("{{\"code\":\"{}\",\"name\":\"phone\"}}", code.code);
        let (status, _) = ask(
            &server,
            &request(
                &server,
                "POST",
                "/api/pair",
                "Content-Type: application/json\r\n",
                &body,
            ),
        )
        .await;
        assert_eq!(status, 200);
        let (status, _) = ask(
            &server,
            &request(
                &server,
                "POST",
                "/api/pair",
                "Content-Type: application/json\r\n",
                &body,
            ),
        )
        .await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn a_wrong_code_is_refused() {
        let (server, _fake) = running().await;
        let (status, _) = ask(
            &server,
            &request(
                &server,
                "POST",
                "/api/pair",
                "Content-Type: application/json\r\n",
                "{\"code\":\"00000000\",\"name\":\"phone\"}",
            ),
        )
        .await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn a_revoked_device_is_refused_at_once() {
        let (server, _fake) = running().await;
        let token = pair(&server).await;
        let device = server.store.list().remove(0);
        assert!(server.store.revoke(&device.id));
        let (status, _) = ask(
            &server,
            &request(
                &server,
                "GET",
                "/api/downloads",
                &format!("Authorization: Bearer {token}\r\n"),
                "",
            ),
        )
        .await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn the_remote_is_off_until_the_reader_turns_it_on() {
        let result =
            RemoteServer::start(RemoteConfig::default(), Arc::new(DeviceStore::new())).await;
        assert!(matches!(result, Err(RemoteError::Disabled)));
    }

    #[tokio::test]
    async fn stopping_closes_the_door() {
        let (server, _fake) = running().await;
        server.stop();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut refused = false;
        while std::time::Instant::now() < deadline {
            if TcpStream::connect(server.local_addr()).await.is_err() {
                refused = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(refused, "the door is still open after it was stopped");
    }
}
