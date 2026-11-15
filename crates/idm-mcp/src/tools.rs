//! The tools the assistant sees: their names, their schemas, the arguments
//! they accept and the words their answers are made of.
//!
//! The names are the ones a reader would use, and the answers are sentences
//! rather than a dump of numbers, so an assistant can pass them on.

use serde_json::{json, Value};

/// The tools, as the assistant reads them.
pub fn list() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_downloads",
            description: "List the downloads and what they are doing now: the name, the size, how far along they are and how fast they are going.",
            schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        },
        Tool {
            name: "add_download",
            description: "Start a download from a link. The file lands in the app's download folder unless a folder is given.",
            schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The link to download."
                    },
                    "folder": {
                        "type": "string",
                        "description": "Where to save the file. The app's own folder when this is left out."
                    },
                    "connections": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 32,
                        "description": "How many connections to use for this download."
                    }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
        },
        Tool {
            name: "pause_download",
            description: "Pause a download. The bytes it has already received are kept.",
            schema: id_schema(),
        },
        Tool {
            name: "resume_download",
            description: "Start a paused download again.",
            schema: id_schema(),
        },
        Tool {
            name: "remove_download",
            description: "Take a download out of the list. The file on disk is kept, unless delete_file is true, which deletes it.",
            schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "The number of the download."
                    },
                    "delete_file": {
                        "type": "boolean",
                        "default": false,
                        "description": "Delete the file on disk as well. Leave this out to keep the file."
                    }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
        },
        Tool {
            name: "queue_status",
            description: "Show the queue: how many downloads wait, how many run and how many may run at the same time.",
            schema: empty_schema(),
        },
        Tool {
            name: "check_queue",
            description: "Say whether the queue is running and what it is doing right now.",
            schema: empty_schema(),
        },
    ]
}

/// One tool as the assistant reads it.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// The JSON schema of the arguments.
    pub schema: Value,
}

/// The tool list, in the shape `tools/list` answers with.
pub fn schemas() -> Value {
    json!({
        "tools": list()
            .iter()
            .map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.schema,
            }))
            .collect::<Vec<Value>>(),
    })
}

/// The schema of a tool that takes nothing.
fn empty_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false,
    })
}

/// The schema of a tool that takes the number of a download.
fn id_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "integer",
                "minimum": 1,
                "description": "The number of the download."
            }
        },
        "required": ["id"],
        "additionalProperties": false,
    })
}

/// Why a call was refused before it reached the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// There is no tool with that name.
    UnknownTool(String),
    /// The arguments do not fit the tool.
    BadArguments(String),
}

/// A tool call with its arguments checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    ListDownloads,
    AddDownload {
        url: String,
        folder: Option<String>,
        connections: Option<u32>,
    },
    PauseDownload {
        id: u64,
    },
    ResumeDownload {
        id: u64,
    },
    RemoveDownload {
        id: u64,
        /// Whether the file on disk goes too. It never does without being
        /// asked for: this is the one explicit call that deletes a file.
        delete_file: bool,
    },
    QueueStatus,
    CheckQueue,
}

/// Check the arguments of one call.
pub fn parse(name: &str, arguments: &Value) -> Result<Call, ParseError> {
    let bad = |message: String| ParseError::BadArguments(message);
    match name {
        "list_downloads" => Ok(Call::ListDownloads),
        "queue_status" => Ok(Call::QueueStatus),
        "check_queue" => Ok(Call::CheckQueue),
        "add_download" => {
            let url = required_text(arguments, "url").map_err(bad)?;
            if !is_link(&url) {
                return Err(bad(
                    "`url` must be a link that starts with http://, https:// or ftp://."
                        .to_string(),
                ));
            }
            let folder = optional_text(arguments, "folder").map_err(bad)?;
            let connections = optional_connections(arguments).map_err(bad)?;
            Ok(Call::AddDownload {
                url,
                folder,
                connections,
            })
        }
        "pause_download" => Ok(Call::PauseDownload {
            id: required_id(arguments).map_err(bad)?,
        }),
        "resume_download" => Ok(Call::ResumeDownload {
            id: required_id(arguments).map_err(bad)?,
        }),
        "remove_download" => Ok(Call::RemoveDownload {
            id: required_id(arguments).map_err(bad)?,
            delete_file: optional_flag(arguments, "delete_file").map_err(bad)?,
        }),
        other => Err(ParseError::UnknownTool(other.to_string())),
    }
}

/// The answer of a tool, in the words a reader would use.
pub fn render(call: &Call, data: &Value) -> String {
    match call {
        Call::ListDownloads => list_text(data),
        Call::AddDownload { .. } => added_text(data),
        Call::PauseDownload { id } => state_text(*id, data, "paused"),
        Call::ResumeDownload { id } => state_text(*id, data, "running"),
        Call::RemoveDownload { id, .. } => removed_text(*id, data),
        Call::QueueStatus => queue_text(data),
        Call::CheckQueue => check_text(data),
    }
}

/// The downloads, one per line.
fn list_text(data: &Value) -> String {
    let downloads = data
        .get("downloads")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if downloads.is_empty() {
        return "There are no downloads.".to_string();
    }
    let mut out = String::new();
    for download in &downloads {
        let id = number(download, "id");
        let name = file_name(download.get("dest").and_then(Value::as_str).unwrap_or(""));
        let total = number(download, "total");
        let done = number(download, "downloaded");
        let status = download
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let progress = if total > 0 {
            format!(
                "{} of {} ({}%)",
                bytes(done),
                bytes(total),
                done * 100 / total
            )
        } else {
            format!("{} so far", bytes(done))
        };
        let speed = match download.get("speedBps").and_then(Value::as_u64) {
            Some(speed) => format!(" — {}/s", bytes(speed)),
            None => String::new(),
        };
        out.push_str(&format!("{id}. {name} — {progress} — {status}{speed}\n"));
    }
    out.push_str(&format!(
        "{} download{} in total.",
        downloads.len(),
        if downloads.len() == 1 { "" } else { "s" }
    ));
    out
}

/// What a new download was called and where it went.
fn added_text(data: &Value) -> String {
    let id = number(data, "id");
    let name = data
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("the download")
        .to_string();
    let mut out = format!("Added {name} as download {id}.");
    if let Some(dest) = data.get("dest").and_then(Value::as_str) {
        if !dest.is_empty() {
            out.push_str(&format!(" It will be saved in {dest}."));
        }
    }
    out
}

/// What happened to a download that was paused or started again.
fn state_text(id: u64, data: &Value, fallback: &str) -> String {
    let status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    format!("Download {id} is {status}.")
}

/// What happened to a download that left the list.
fn removed_text(id: u64, data: &Value) -> String {
    let deleted = data
        .get("deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if deleted {
        format!("Download {id} is gone and its file was deleted.")
    } else {
        format!("Download {id} is gone. Its file is still on disk.")
    }
}

/// The queue, its limit and what waits in it.
fn queue_text(data: &Value) -> String {
    let waiting = ids(data, "order");
    let running = ids(data, "running");
    let max = number(data, "maxSimultaneous");
    let mut out = format!(
        "{} waiting, {} running, and up to {max} at a time.",
        waiting.len(),
        running.len()
    );
    if !waiting.is_empty() {
        out.push_str(&format!(" Waiting: {}.", list_numbers(&waiting)));
    }
    out
}

/// Whether the queue runs, and what it is doing.
fn check_text(data: &Value) -> String {
    let active = data.get("active").and_then(Value::as_bool).unwrap_or(false);
    let running = ids(data, "running");
    let waiting = ids(data, "order");
    let mut out = if active {
        "The queue is running.".to_string()
    } else {
        "The queue is not running.".to_string()
    };
    if running.is_empty() {
        out.push_str(" Nothing is running right now.");
    } else {
        out.push_str(&format!(" It is running {}.", list_numbers(&running)));
    }
    if waiting.is_empty() {
        out.push_str(" Nothing is waiting.");
    } else {
        out.push_str(&format!(" Next in line: {}.", list_numbers(&waiting)));
    }
    out
}

/// The numbers of a list field.
fn ids(data: &Value, key: &str) -> Vec<u64> {
    data.get(key)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

/// The numbers as a reader would read them.
fn list_numbers(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<String>>()
        .join(", ")
}

/// A number of a field, or zero when it is not there.
fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The name of a file, from its path.
fn file_name(dest: &str) -> &str {
    dest.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(dest)
}

/// A size in the unit a reader would use.
fn bytes(value: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// A text field that has to be there.
fn required_text(arguments: &Value, key: &str) -> Result<String, String> {
    match arguments.get(key) {
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        Some(Value::String(_)) => Err(format!("`{key}` must not be empty.")),
        Some(_) => Err(format!("`{key}` must be a string.")),
        None => Err(format!("`{key}` is needed.")),
    }
}

/// A text field that may be left out.
fn optional_text(arguments: &Value, key: &str) -> Result<Option<String>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.trim().to_string())),
        Some(_) => Err(format!("`{key}` must be a string.")),
    }
}

/// The number of a download, which has to be there.
fn required_id(arguments: &Value) -> Result<u64, String> {
    match arguments.get("id") {
        Some(Value::Number(number)) => match number.as_u64() {
            Some(id) if id > 0 => Ok(id),
            _ => Err("`id` must be the number of a download.".to_string()),
        },
        Some(_) => Err("`id` must be a number.".to_string()),
        None => Err("`id` is needed.".to_string()),
    }
}

/// How many connections to use, between one and thirty-two.
fn optional_connections(arguments: &Value) -> Result<Option<u32>, String> {
    match arguments.get("connections") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => match number.as_u64() {
            Some(value) if (1..=32).contains(&value) => Ok(Some(value as u32)),
            _ => Err("`connections` must be between 1 and 32.".to_string()),
        },
        Some(_) => Err("`connections` must be a number.".to_string()),
    }
}

/// A flag that may be left out.
fn optional_flag(arguments: &Value, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(format!("`{key}` must be true or false.")),
    }
}

/// Whether a text is a link the engine may fetch.
fn is_link(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "ftp"
    ) {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_name_a_description_and_a_schema() {
        for tool in list() {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.schema["type"], "object");
            assert!(tool.schema["properties"].is_object());
        }
    }

    #[test]
    fn the_tool_list_is_the_shape_the_protocol_asks_for() {
        let schemas = schemas();
        let tools = schemas["tools"].as_array().expect("the tools");
        assert_eq!(tools.len(), list().len());
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert!(tool["inputSchema"]["type"] == "object");
        }
    }

    #[test]
    fn a_link_is_needed_to_add_a_download() {
        let error = parse("add_download", &json!({})).expect_err("no url");
        assert_eq!(
            error,
            ParseError::BadArguments("`url` is needed.".to_string())
        );
        assert!(parse("add_download", &json!({ "url": 7 })).is_err());
        assert!(parse("add_download", &json!({ "url": "example.org/file" })).is_err());
    }

    #[test]
    fn the_number_of_connections_is_bounded() {
        assert!(parse(
            "add_download",
            &json!({ "url": "https://example.org/f", "connections": 0 })
        )
        .is_err());
        assert!(parse(
            "add_download",
            &json!({ "url": "https://example.org/f", "connections": 33 })
        )
        .is_err());
        assert!(parse(
            "add_download",
            &json!({ "url": "https://example.org/f", "connections": 1 })
        )
        .is_ok());
        assert!(parse(
            "add_download",
            &json!({ "url": "https://example.org/f", "connections": 32 })
        )
        .is_ok());
    }

    #[test]
    fn a_download_keeps_its_file_unless_the_caller_says_otherwise() {
        let call = parse("remove_download", &json!({ "id": 3 })).expect("the call");
        assert_eq!(
            call,
            Call::RemoveDownload {
                id: 3,
                delete_file: false
            }
        );
        let call =
            parse("remove_download", &json!({ "id": 3, "delete_file": true })).expect("the call");
        assert_eq!(
            call,
            Call::RemoveDownload {
                id: 3,
                delete_file: true
            }
        );
    }

    #[test]
    fn the_number_of_a_download_is_needed() {
        assert!(parse("pause_download", &json!({})).is_err());
        assert!(parse("resume_download", &json!({ "id": 0 })).is_err());
        assert_eq!(
            parse("pause_download", &json!({ "id": 4 })).expect("the call"),
            Call::PauseDownload { id: 4 }
        );
    }

    #[test]
    fn an_unknown_tool_is_refused() {
        let error = parse("make_coffee", &json!({})).expect_err("no tool");
        assert_eq!(error, ParseError::UnknownTool("make_coffee".to_string()));
    }

    #[test]
    fn the_downloads_are_readable() {
        let text = render(
            &Call::ListDownloads,
            &json!({
                "downloads": [{
                    "id": 1,
                    "dest": "C:\\Downloads\\holiday.mp4",
                    "total": 41943040,
                    "downloaded": 10485760,
                    "status": "running",
                    "speedBps": 1048576,
                }],
                "active": 1,
            }),
        );
        assert!(text.contains("holiday.mp4"));
        assert!(text.contains("10.0 MB of 40.0 MB (25%)"));
        assert!(text.contains("running"));
        assert!(text.contains("1.0 MB/s"));
    }

    #[test]
    fn an_empty_list_says_so() {
        let text = render(&Call::ListDownloads, &json!({ "downloads": [] }));
        assert_eq!(text, "There are no downloads.");
    }

    #[test]
    fn the_queue_is_readable() {
        let text = render(
            &Call::QueueStatus,
            &json!({ "order": [4, 5], "running": [3], "maxSimultaneous": 3, "active": true }),
        );
        assert!(text.contains("2 waiting, 1 running"));
        assert!(text.contains("up to 3 at a time"));

        let text = render(
            &Call::CheckQueue,
            &json!({ "order": [4], "running": [], "maxSimultaneous": 3, "active": false }),
        );
        assert!(text.contains("The queue is not running."));
    }
}
