//! What the window hears from a crawl.

use serde::Serialize;

use super::super::policy::Candidate;

/// One step of the crawl, for the window.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    Started {
        url: String,
        site: String,
        pages: usize,
        depth: u8,
    },
    /// One page was read.
    Page {
        url: String,
        depth: u8,
        links: usize,
        files: usize,
    },
    /// One file was found.
    File { file: Candidate },
    /// Something was left out, with the sentence the window shows.
    Note {
        level: &'static str,
        url: String,
        message: String,
    },
    Progress {
        pages_read: usize,
        pages_queued: usize,
        files: usize,
        page_limit: usize,
    },
    Done {
        status: &'static str,
        message: String,
        pages_read: usize,
        pages_queued: usize,
        files: usize,
        duplicates: usize,
        restricted: usize,
        excluded: usize,
        unsupported: usize,
        truncated: bool,
    },
}

/// What the crawl counted, for the final line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub pages_read: usize,
    pub pages_queued: usize,
    pub files: usize,
    pub duplicates: usize,
    pub restricted: usize,
    pub excluded: usize,
    pub unsupported: usize,
    pub truncated: bool,
}

/// How the run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// `done`, `cancelled` or `failed`.
    pub status: &'static str,
    pub message: String,
    pub stats: Stats,
}

impl Outcome {
    pub(super) fn cancelled(stats: Stats) -> Self {
        Self {
            status: "cancelled",
            message: "The crawl was stopped.".to_string(),
            stats,
        }
    }
}
