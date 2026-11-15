//! The reader's leave-out rules and the file types a crawl offers.

use std::collections::BTreeSet;

use reqwest::Url;

use super::kinds::{Candidate, FileKind};

/// The paths and patterns the user asked to leave out.
#[derive(Debug, Clone, Default)]
pub struct Exclusions {
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
struct Pattern {
    text: String,
    /// A pattern beginning with `/` matches the path only; anything else may
    /// also match the host and scheme.
    path_only: bool,
}

impl Exclusions {
    /// Read the typed list. Junk, duplicates and anything past the limit are
    /// dropped rather than refused: one bad line should not stop a crawl.
    pub fn parse(values: &[String]) -> Self {
        const LIMIT: usize = 32;
        let mut patterns: Vec<Pattern> = Vec::new();
        for value in values {
            let text = value.trim();
            if text.is_empty() || text.chars().count() > 200 {
                continue;
            }
            if patterns
                .iter()
                .any(|known| known.text.eq_ignore_ascii_case(text))
            {
                continue;
            }
            patterns.push(Pattern {
                text: text.to_string(),
                path_only: text.starts_with('/') && !text.contains("://"),
            });
            if patterns.len() >= LIMIT {
                break;
            }
        }
        Self { patterns }
    }

    /// Whether one link matches any pattern. The path and query are checked,
    /// and a pattern without a leading `/` is also checked against the host.
    pub fn matches(&self, url: &Url) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let mut path = url.path().to_string();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        let whole = url.as_str();
        let lowered = whole.to_ascii_lowercase();
        self.patterns.iter().any(|pattern| {
            if pattern.path_only {
                // A path rule is a prefix, like a rule for a folder: `/private`
                // leaves out `/private/a` as well as `/private` itself.
                wildcard_prefix(&pattern.text, &path)
            } else {
                // Without a leading `/` a rule may name a host, a fragment of
                // an address, or a pattern. All three readings are tried.
                lowered.contains(&pattern.text.to_ascii_lowercase())
                    || wildcard(&pattern.text, &path)
                    || wildcard(&pattern.text, whole)
            }
        })
    }
}

/// A small glob: `*` matches any run of characters, everything else is taken
/// literally. The comparison ignores ASCII case, because a path on a server
/// may be either and a rule typed by a person should not depend on it.
pub(super) fn wildcard(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let text: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while text_at < text.len() {
        if pattern_at < pattern.len() && pattern[pattern_at] == text[text_at] {
            pattern_at += 1;
            text_at += 1;
        } else if pattern_at < pattern.len() && pattern[pattern_at] == '*' {
            star = Some(pattern_at);
            pattern_at += 1;
            resume = text_at;
        } else if let Some(position) = star {
            pattern_at = position + 1;
            resume += 1;
            text_at = resume;
        } else {
            return false;
        }
    }
    while pattern_at < pattern.len() && pattern[pattern_at] == '*' {
        pattern_at += 1;
    }
    pattern_at == pattern.len()
}

/// A glob that only has to match the beginning of the text.
fn wildcard_prefix(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let text: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    loop {
        if pattern_at == pattern.len() {
            return true;
        }
        if text_at < text.len() && pattern[pattern_at] == text[text_at] {
            pattern_at += 1;
            text_at += 1;
        } else if pattern[pattern_at] == '*' {
            star = Some(pattern_at);
            pattern_at += 1;
            resume = text_at;
        } else if let Some(position) = star {
            if resume >= text.len() {
                return false;
            }
            resume += 1;
            text_at = resume;
            pattern_at = position + 1;
        } else {
            return false;
        }
    }
}

/// The file types a crawl should offer.
#[derive(Debug, Clone)]
pub struct Accepted {
    kinds: BTreeSet<FileKind>,
    extensions: BTreeSet<String>,
}

impl Accepted {
    pub fn new(kinds: impl IntoIterator<Item = FileKind>, extensions: &[String]) -> Self {
        let extensions = extensions
            .iter()
            .map(|extension| {
                extension
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|extension| {
                !extension.is_empty()
                    && extension.len() <= 16
                    && extension.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_')
                    })
            })
            .collect();
        Self {
            kinds: kinds.into_iter().collect(),
            extensions,
        }
    }

    /// Whether a candidate is one the user asked the grabber to collect.
    pub fn takes(&self, candidate: &Candidate) -> bool {
        self.kinds.contains(&candidate.kind) || self.extensions.contains(&candidate.extension)
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty() && self.extensions.is_empty()
    }
}
