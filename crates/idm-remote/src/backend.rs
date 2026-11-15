//! What the remote asks of the application.
//!
//! The crate knows nothing about the engine, the settings or the window, so
//! it asks for what it needs through this trait and the application answers
//! it. Every method is synchronous and quick: the listener runs it on its own
//! task and answers the phone with the result.

use serde::{Deserialize, Serialize};

/// One download, as the phone sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDownload {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub state: String,
}

/// The application, as the remote sees it.
pub trait RemoteBackend: Send + Sync {
    /// Every download the application knows about.
    fn downloads(&self) -> Vec<RemoteDownload>;

    /// Add a link. It answers with the id of the new download.
    fn add(&self, url: &str, folder: Option<&str>) -> Result<String, String>;

    /// Pause one download.
    fn pause(&self, id: u64) -> Result<(), String>;

    /// Resume one download.
    fn resume(&self, id: u64) -> Result<(), String>;

    /// Remove one download and what it has written so far.
    fn remove(&self, id: u64) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend that counts what it was asked to do.
    #[derive(Default)]
    struct Fake {
        added: std::sync::Mutex<Vec<String>>,
    }

    impl RemoteBackend for Fake {
        fn downloads(&self) -> Vec<RemoteDownload> {
            Vec::new()
        }

        fn add(&self, url: &str, _folder: Option<&str>) -> Result<String, String> {
            self.added.lock().unwrap().push(url.to_string());
            Ok("7".to_string())
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

    #[test]
    fn a_download_is_written_in_camel_case() {
        let download = RemoteDownload {
            id: 1,
            name: "file".to_string(),
            size: 10,
            downloaded: 5,
            speed: 2,
            state: "running".to_string(),
        };
        let json = serde_json::to_string(&download).unwrap();
        assert!(json.contains("\"downloaded\":5"));
        assert!(json.contains("\"speed\":2"));
    }

    #[test]
    fn the_backend_is_reached() {
        let fake = Fake::default();
        assert_eq!(fake.add("https://example.org/file", None).unwrap(), "7");
        assert_eq!(fake.added.lock().unwrap().len(), 1);
    }
}
