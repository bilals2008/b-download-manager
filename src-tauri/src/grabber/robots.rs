//! The part of `robots.txt` a grabber has to obey.
//!
//! The file is a set of groups: one or more `User-agent` lines, then the rules
//! that apply to them. A crawler picks the group for its own name, or the `*`
//! group when there is none, and asks whether a path is allowed. The longest
//! matching rule wins, and an `Allow` wins a tie, which is the rule RFC 9309
//! settled on and the one every large crawler follows.
//!
//! `Crawl-delay` is not part of that standard, but sites still write it, so it
//! is honoured within a sane ceiling. Everything else in the file is ignored.

use std::time::Duration;

use super::policy::{MAX_CRAWL_DELAY, ROBOTS_AGENT};

#[derive(Debug, Clone, Default)]
pub struct Robots {
    rules: Vec<Rule>,
    delay: Option<Duration>,
}

#[derive(Debug, Clone)]
struct Rule {
    allow: bool,
    pattern: String,
}

#[derive(Debug, Clone, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    delay: Option<Duration>,
}

impl Robots {
    /// Whether a path may be read. An empty file allows everything.
    pub fn allows(&self, path_and_query: &str) -> bool {
        let mut best: Option<(usize, bool)> = None;
        for rule in &self.rules {
            if !rule_matches(&rule.pattern, path_and_query) {
                continue;
            }
            let length = rule.pattern.trim_end_matches('$').chars().count();
            best = match best {
                Some((known, allow)) if length < known => Some((known, allow)),
                Some((known, allow)) if length == known => Some((known, allow || rule.allow)),
                _ => Some((length, rule.allow)),
            };
        }
        best.map(|(_, allow)| allow).unwrap_or(true)
    }

    /// The pause the site asked for, within the ceiling of the grabber.
    pub fn delay(&self) -> Option<Duration> {
        self.delay
    }
}

/// Read a `robots.txt` body for one crawler name.
///
/// The name is the product token, so `BDM-SiteGrabber/0.1` in a request is
/// matched by `User-agent: BDM-SiteGrabber`. A group naming the crawler wins
/// over the `*` group, even when it is empty: a site that writes a group for a
/// crawler has said something about that crawler.
pub fn parse(text: &str, agent: &str) -> Robots {
    let groups = groups(text);
    let wanted = token(agent);
    let mut chosen: Vec<&Group> = groups
        .iter()
        .filter(|group| group.agents.contains(&wanted))
        .collect();
    if chosen.is_empty() {
        chosen = groups
            .iter()
            .filter(|group| group.agents.iter().any(|name| name == "*"))
            .collect();
    }

    let mut rules: Vec<Rule> = Vec::new();
    let mut delay: Option<Duration> = None;
    for group in chosen {
        for rule in &group.rules {
            if !rules
                .iter()
                .any(|known| known.allow == rule.allow && known.pattern == rule.pattern)
            {
                rules.push(rule.clone());
            }
        }
        if let Some(group_delay) = group.delay {
            delay = Some(delay.map_or(group_delay, |known| known.max(group_delay)));
        }
    }

    Robots { rules, delay }
}

/// Split a file into its groups. Rules that come before any `User-agent` line
/// belong to nobody and are dropped.
fn groups(text: &str) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    let mut current = Group::default();
    let mut seen_agent = false;

    let flush = |current: &mut Group, out: &mut Vec<Group>| {
        if !current.agents.is_empty() {
            out.push(std::mem::take(current));
        } else {
            *current = Group::default();
        }
    };

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match key.as_str() {
            "user-agent" => {
                // A new name after rules starts the next group.
                if seen_agent && !current.rules.is_empty() {
                    flush(&mut current, &mut out);
                    seen_agent = false;
                }
                let name = token(value);
                if !name.is_empty() {
                    current.agents.push(name);
                    seen_agent = true;
                }
            }
            "allow" | "disallow" => {
                if current.agents.is_empty() || value.is_empty() {
                    // `Disallow:` with nothing after it allows everything, and
                    // a rule before any name belongs to nobody.
                    continue;
                }
                current.rules.push(Rule {
                    allow: key == "allow",
                    pattern: value.to_string(),
                });
            }
            "crawl-delay" => {
                // A delay before any name is nobody's delay.
                if current.agents.is_empty() {
                    continue;
                }
                if let Ok(seconds) = value.parse::<f64>() {
                    if seconds > 0.0 {
                        let delay =
                            Duration::from_secs_f64(seconds.min(MAX_CRAWL_DELAY.as_secs_f64()));
                        current.delay = Some(delay);
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut current, &mut out);
    out
}

/// The product token of a user-agent line or of the grabber itself: everything
/// before the version, lower-cased.
fn token(value: &str) -> String {
    value
        .split(['/', ' ', '\t'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Whether a robots pattern matches a path.
///
/// `*` is a wildcard and a trailing `$` anchors the end. Without the anchor a
/// pattern matches a prefix, so `Disallow: /private` covers `/private/a` as
/// well as `/private`.
fn rule_matches(pattern: &str, path: &str) -> bool {
    let (body, anchored) = match pattern.strip_suffix('$') {
        Some(body) => (body, true),
        None => (pattern, false),
    };
    if anchored {
        wildcard(
            &body.chars().collect::<Vec<_>>(),
            &path.chars().collect::<Vec<_>>(),
            true,
        )
    } else {
        wildcard(
            &body.chars().collect::<Vec<_>>(),
            &path.chars().collect::<Vec<_>>(),
            false,
        )
    }
}

/// A wildcard match that either covers the whole text or, when `exact` is
/// false, only its beginning.
fn wildcard(pattern: &[char], text: &[char], exact: bool) -> bool {
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    loop {
        if pattern_at == pattern.len() {
            return !exact || text_at == text.len();
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

/// The token the applied rules are matched under. The window names the agent
/// in one place, so a test can ask about the same name the request carries.
pub fn agent() -> &'static str {
    ROBOTS_AGENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_allows_everything() {
        let robots = parse("", agent());
        assert!(robots.allows("/anything"));
        assert!(robots.delay().is_none());
    }

    #[test]
    fn a_disallowed_folder_is_refused_and_its_sibling_is_not() {
        let robots = parse(
            "User-agent: *\nDisallow: /private/\nDisallow: /tmp\n",
            agent(),
        );
        assert!(!robots.allows("/private/a.zip"));
        assert!(!robots.allows("/tmp"));
        assert!(!robots.allows("/tmp/file"));
        assert!(robots.allows("/public/a.zip"));
        assert!(
            !robots.allows("/tmpfile"),
            "a robots rule is a prefix, so `/tmp` covers `/tmpfile` too"
        );
    }

    #[test]
    fn the_longest_rule_wins_and_allow_wins_a_tie() {
        let robots = parse(
            "User-agent: *\nDisallow: /private/\nAllow: /private/open/\nDisallow: /same\nAllow: /same\n",
            agent(),
        );
        assert!(!robots.allows("/private/a"));
        assert!(robots.allows("/private/open/a"));
        assert!(robots.allows("/same"));
    }

    #[test]
    fn a_wildcard_and_an_anchor_match_what_they_name() {
        let robots = parse(
            "User-agent: *\nDisallow: /*.pdf$\nDisallow: /*/secret\n",
            agent(),
        );
        assert!(!robots.allows("/files/a.pdf"));
        assert!(robots.allows("/files/a.pdf?download=1"));
        assert!(robots.allows("/files/a.pdfx"));
        assert!(!robots.allows("/files/secret"));
        assert!(!robots.allows("/x/y/secret"));
    }

    #[test]
    fn a_group_for_the_grabber_wins_over_the_star_group() {
        let robots = parse(
            "User-agent: *\nDisallow: /\n\nUser-agent: BDM-SiteGrabber/0.1\nDisallow: /only-here\n",
            agent(),
        );
        assert!(robots.allows("/elsewhere"));
        assert!(!robots.allows("/only-here/a"));
    }

    #[test]
    fn an_empty_group_for_the_grabber_allows_everything() {
        let robots = parse(
            "User-agent: *\nDisallow: /\n\nUser-agent: BDM-SiteGrabber\n",
            agent(),
        );
        assert!(robots.allows("/anything"));
    }

    #[test]
    fn an_unknown_agent_falls_back_to_every_crawler() {
        let robots = parse("User-agent: OtherBot\nDisallow: /\n", agent());
        assert!(
            robots.allows("/anything"),
            "another crawler's rules do not apply"
        );

        let robots = parse("User-agent: *\nDisallow: /\n", "SomeAgent/2");
        assert!(!robots.allows("/anything"));
    }

    #[test]
    fn comments_and_rules_before_a_name_are_ignored() {
        let robots = parse(
            "# a comment\nDisallow: /\nUser-agent: * # every crawler\nDisallow: /x # the x folder\n",
            agent(),
        );
        assert!(robots.allows("/"));
        assert!(!robots.allows("/x/a"));
    }

    #[test]
    fn a_crawl_delay_is_read_and_capped() {
        let robots = parse("User-agent: *\nCrawl-delay: 1.5\n", agent());
        assert_eq!(robots.delay(), Some(Duration::from_millis(1500)));

        let robots = parse("User-agent: *\nCrawl-delay: 900\n", agent());
        assert_eq!(robots.delay(), Some(MAX_CRAWL_DELAY));

        let robots = parse("User-agent: *\nCrawl-delay: soon\n", agent());
        assert_eq!(robots.delay(), None);
    }

    #[test]
    fn an_empty_disallow_allows_everything() {
        let robots = parse("User-agent: *\nDisallow:\n", agent());
        assert!(robots.allows("/a"));
    }
}
