//! The MCP server: JSON-RPC 2.0 over stdio.
//!
//! The assistant starts this program and owns its life: it speaks the Model
//! Context Protocol on standard input and standard output, one JSON message
//! per line. Every tool it offers goes to the running application over the
//! app's local API, so there is one place that knows how a download works.
//!
//! The protocol is small enough to hold in the head: `initialize` says who
//! the server is, `tools/list` says what it can do and `tools/call` does one
//! of those things. A tool that fails answers with a sentence and
//! `isError: true`, never with a crash.

pub mod client;
pub mod tools;

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use client::{App, AppError};
use tools::{Call, ParseError};

/// The protocol version the server answers with when the client asks for one
/// this server does not know.
pub const DEFAULT_PROTOCOL: &str = "2024-11-05";

/// The protocol versions this server speaks.
const PROTOCOLS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

/// The name the server answers with.
pub const SERVER_NAME: &str = "idm-mcp";

/// The version of the server.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A JSON-RPC error, with the code the protocol names.
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("there is no method called {method}"),
        }
    }
}

/// The server, and the application it talks to.
pub struct Server<'a> {
    app: &'a mut dyn App,
}

impl<'a> Server<'a> {
    /// A server that talks to this application.
    pub fn new(app: &'a mut dyn App) -> Self {
        Self { app }
    }

    /// Handle one message. The answer is `None` for a notification, which is
    /// a message the client does not expect an answer to.
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("the message was not JSON: {error}") },
                }))
            }
        };
        let Some(object) = request.as_object() else {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32600, "message": "the message was not a request" },
            }));
        };
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        // A message without an id is a notification: it is read, and nothing
        // is answered.
        let id = object.get("id").cloned()?;

        match self.handle_request(method, &params) {
            Ok(result) => Some(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
            Err(error) => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": error.code, "message": error.message },
            })),
        }
    }

    /// Do what one request asks for.
    fn handle_request(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools::schemas()),
            "tools/call" => self.call_tool(params),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    /// Who the server is, and which protocol it speaks.
    fn initialize(&self, params: &Value) -> Value {
        let asked = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let version = if PROTOCOLS.contains(&asked) {
            asked
        } else {
            DEFAULT_PROTOCOL
        };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        })
    }

    /// Run one tool and answer with what it said.
    fn call_tool(&mut self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("a tool name is needed"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        let call = match tools::parse(name, &arguments) {
            Ok(call) => call,
            // A tool that does not exist is a mistake in the request itself.
            Err(ParseError::UnknownTool(name)) => {
                return Err(RpcError::invalid_params(format!(
                    "there is no tool called {name}"
                )))
            }
            // Arguments that do not fit are a mistake in the call, and the
            // assistant is told which one.
            Err(ParseError::BadArguments(message)) => return Ok(failed(&message)),
        };

        match self.run(&call) {
            Ok(data) => Ok(content(&tools::render(&call, &data))),
            Err(error) => Ok(failed(&error.message())),
        }
    }

    /// Ask the application to do what the call means.
    fn run(&mut self, call: &Call) -> Result<Value, AppError> {
        match call {
            Call::ListDownloads => self.app.downloads(),
            Call::AddDownload {
                url,
                folder,
                connections,
            } => self.app.add_download(url, folder.as_deref(), *connections),
            Call::PauseDownload { id } => self.app.pause(*id),
            Call::ResumeDownload { id } => self.app.resume(*id),
            Call::RemoveDownload { id, delete_file } => self.app.remove(*id, *delete_file),
            Call::QueueStatus | Call::CheckQueue => self.app.queue(),
        }
    }
}

/// The answer of a tool that worked.
fn content(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
    })
}

/// The answer of a tool that failed: a sentence, never a crash.
fn failed(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": true,
    })
}

/// Read the messages of a client and answer them, one line each.
///
/// The client owns the life of the server: it ends the stream when it is done.
pub fn serve(
    reader: impl BufRead,
    mut writer: impl Write,
    app: &mut dyn App,
) -> std::io::Result<()> {
    let mut server = Server::new(app);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.handle_line(&line) {
            writeln!(writer, "{answer}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// An application that answers with fixed answers.
    struct FakeApp {
        /// When set, every call fails with this reason.
        fail: Option<AppError>,
    }

    impl FakeApp {
        fn working() -> Self {
            Self { fail: None }
        }

        fn broken() -> Self {
            Self {
                fail: Some(AppError::NotRunning),
            }
        }
    }

    impl App for FakeApp {
        fn downloads(&mut self) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({
                "downloads": [{
                    "id": 1,
                    "dest": "C:\\Downloads\\holiday.mp4",
                    "total": 1000,
                    "downloaded": 400,
                    "status": "running",
                }],
                "active": 1,
            }))
        }

        fn add_download(
            &mut self,
            url: &str,
            folder: Option<&str>,
            connections: Option<u32>,
        ) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({
                "id": 7,
                "url": url,
                "folder": folder,
                "connections": connections,
                "name": "holiday.mp4",
                "dest": "C:\\Downloads\\holiday.mp4",
            }))
        }

        fn pause(&mut self, id: u64) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({ "id": id, "status": "paused" }))
        }

        fn resume(&mut self, id: u64) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({ "id": id, "status": "running" }))
        }

        fn remove(&mut self, id: u64, delete_file: bool) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({ "id": id, "deleted": delete_file }))
        }

        fn queue(&mut self) -> Result<Value, AppError> {
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            Ok(json!({
                "order": [4],
                "running": [3],
                "maxSimultaneous": 3,
                "active": true,
            }))
        }
    }

    /// Send the messages of a client and read the answers.
    fn exchange(app: &mut dyn App, lines: &[&str]) -> Vec<Value> {
        let input = format!("{}\n", lines.join("\n"));
        let mut output = Vec::new();
        serve(Cursor::new(input.into_bytes()), &mut output, app).expect("the exchange");
        String::from_utf8(output)
            .expect("text")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a JSON answer"))
            .collect()
    }

    #[test]
    fn the_server_says_who_it_is() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            ],
        );
        let answer = &answers[0];
        assert_eq!(answer["jsonrpc"], "2.0");
        assert_eq!(answer["id"], 1);
        assert_eq!(answer["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(answer["result"]["serverInfo"]["name"], "idm-mcp");
        assert!(answer["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn an_unknown_protocol_version_gets_one_this_server_speaks() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
            ],
        );
        assert_eq!(answers[0]["result"]["protocolVersion"], DEFAULT_PROTOCOL);
    }

    #[test]
    fn the_tool_list_names_every_tool_with_a_schema() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#],
        );
        let tools = answers[0]["result"]["tools"].as_array().expect("the tools");
        assert_eq!(tools.len(), 7);
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn a_tool_call_answers_with_text() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_downloads","arguments":{}}}"#,
            ],
        );
        assert_eq!(answers.len(), 3);
        let call = &answers[2];
        assert_eq!(call["id"], 3);
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"]["content"][0]["type"], "text");
        assert!(call["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("holiday.mp4"));
    }

    #[test]
    fn a_notification_is_not_answered() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#],
        );
        assert!(answers.is_empty());
    }

    #[test]
    fn a_tool_that_fails_answers_with_a_sentence() {
        let mut app = FakeApp::broken();
        let answers = exchange(
            &mut app,
            &[
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_downloads","arguments":{}}}"#,
            ],
        );
        let result = &answers[0]["result"];
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().expect("text");
        assert!(
            text.contains("the download manager is not running"),
            "{text}"
        );
    }

    #[test]
    fn arguments_that_do_not_fit_are_refused_in_words() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_download","arguments":{}}}"#,
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add_download","arguments":{"url":"https://example.org/f","connections":99}}}"#,
            ],
        );
        for answer in &answers {
            assert_eq!(answer["result"]["isError"], true);
            assert!(answer["result"]["content"][0]["text"].is_string());
        }
        assert!(answers[0]["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("url"));
        assert!(answers[1]["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("between 1 and 32"));
    }

    #[test]
    fn an_unknown_tool_is_a_protocol_error() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"make_coffee"}}"#],
        );
        assert_eq!(answers[0]["error"]["code"], -32602);
    }

    #[test]
    fn an_unknown_method_is_a_protocol_error() {
        let mut app = FakeApp::working();
        let answers = exchange(&mut app, &[r#"{"jsonrpc":"2.0","id":9,"method":"dance"}"#]);
        assert_eq!(answers[0]["error"]["code"], -32601);
    }

    #[test]
    fn a_broken_message_does_not_stop_the_server() {
        let mut app = FakeApp::working();
        let answers = exchange(
            &mut app,
            &[
                "not json",
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            ],
        );
        assert_eq!(answers[0]["error"]["code"], -32700);
        assert!(answers[1]["result"]["tools"].is_array());
    }
}
