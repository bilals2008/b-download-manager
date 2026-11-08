//!
//! The bounded execution of a player's string transform.
//!
//! Some addresses a YouTube player returns are only answered at full speed
//! when one query parameter is rewritten by a function the player carries. The
//! function is generated JavaScript; running it would mean running whatever a
//! remote page sends, which this build never does.
//!
//! What it does instead is narrower and testable: a transform is expressed as
//! a short, declarative program of the three operations those functions are
//! built from — `reverse`, `splice`, `swap` — and this module executes that
//! program under hard limits. A program with an operation it does not know, an
//! index out of range, or more steps than [`MAX_STEPS`] is refused with a
//! typed error, and the caller keeps the address it had.
//!
//! Keying matters as much as execution: a program belongs to one player
//! version. [`Cache`] drops every program the moment the player it was learned
//! from changes, so a stale program can never be applied to a new player's
//! output.

use std::collections::BTreeMap;

/// The most steps a program may hold. The real functions are a few dozen.
pub const MAX_STEPS: usize = 256;
/// The longest input a program may be applied to. A signed value is far
/// shorter; the cap exists so a malformed address cannot turn into work.
pub const MAX_INPUT: usize = 4096;

/// One step of a transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The whole string, backwards.
    Reverse,
    /// Remove `count` characters at `at`.
    Splice { at: usize, count: usize },
    /// Exchange the characters at `a` and `b`.
    Swap { a: usize, b: usize },
}

/// Why a transform could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeError {
    /// The program names an operation this build does not execute.
    UnknownOperation(String),
    /// The program is longer than [`MAX_STEPS`].
    TooManySteps(usize),
    /// An index does not point inside the value.
    OutOfRange { step: usize, index: usize },
    /// The value is longer than [`MAX_INPUT`].
    TooLong(usize),
    /// The program itself could not be read.
    Malformed(String),
}

impl std::fmt::Display for ChallengeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChallengeError::UnknownOperation(name) => {
                write!(
                    f,
                    "the player uses a transform step this build does not run ({name})"
                )
            }
            ChallengeError::TooManySteps(count) => {
                write!(
                    f,
                    "the player's transform has {count} steps, more than the {MAX_STEPS} allowed"
                )
            }
            ChallengeError::OutOfRange { step, index } => write!(
                f,
                "transform step {step} points at {index}, outside the value it rewrites"
            ),
            ChallengeError::TooLong(len) => {
                write!(
                    f,
                    "the value to transform is {len} bytes, more than the {MAX_INPUT} allowed"
                )
            }
            ChallengeError::Malformed(what) => write!(f, "the transform could not be read: {what}"),
        }
    }
}

impl std::error::Error for ChallengeError {}

/// A transform program: the steps, in the order the player applies them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    steps: Vec<Step>,
}

impl Program {
    // Nothing in this build reads a player's function, so no caller states a
    // program yet: `new` and `parse` are the two ways one arrives, and the
    // tests below are where they are exercised until a reader exists.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(steps: Vec<Step>) -> Result<Self, ChallengeError> {
        if steps.len() > MAX_STEPS {
            return Err(ChallengeError::TooManySteps(steps.len()));
        }
        Ok(Self { steps })
    }

    /// Read the compact declarative form: `reverse;splice:2,5;swap:0,3`.
    ///
    /// This is deliberately not JavaScript. A caller that has read a player's
    /// function has to state its steps here; anything it could not state is
    /// refused instead of guessed at.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn parse(text: &str) -> Result<Self, ChallengeError> {
        let mut steps = Vec::new();
        for (index, piece) in text
            .split(';')
            .map(str::trim)
            .filter(|piece| !piece.is_empty())
            .enumerate()
        {
            if index >= MAX_STEPS {
                return Err(ChallengeError::TooManySteps(index + 1));
            }
            let step = match piece.split_once(':') {
                None if piece.eq_ignore_ascii_case("reverse") => Step::Reverse,
                None => return Err(ChallengeError::UnknownOperation(piece.to_string())),
                Some((name, rest)) => {
                    let numbers: Vec<usize> =
                        rest.split(',')
                            .map(|number| {
                                number.trim().parse().map_err(|_| {
                                    ChallengeError::Malformed(format!("{name}: {rest}"))
                                })
                            })
                            .collect::<Result<_, _>>()?;
                    match (
                        name.trim().to_ascii_lowercase().as_str(),
                        numbers.as_slice(),
                    ) {
                        ("splice", [at, count]) => Step::Splice {
                            at: *at,
                            count: *count,
                        },
                        ("swap", [a, b]) => Step::Swap { a: *a, b: *b },
                        // A known operation with the wrong number of
                        // arguments is a broken program, not an unknown one.
                        ("splice" | "swap", _) => {
                            return Err(ChallengeError::Malformed(format!("{name}: {rest}")))
                        }
                        (other, _) => {
                            return Err(ChallengeError::UnknownOperation(other.to_string()))
                        }
                    }
                }
            };
            steps.push(step);
        }
        Ok(Self { steps })
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Apply the program to a value.
    ///
    /// Every index is checked against the value as it is *at that step*: a
    /// splice shortens the string, so an index that was valid for the input
    /// may not be valid any more. A bad program fails whole, and the caller
    /// keeps the value it had.
    pub fn apply(&self, value: &str) -> Result<String, ChallengeError> {
        if value.len() > MAX_INPUT {
            return Err(ChallengeError::TooLong(value.len()));
        }
        let mut chars: Vec<char> = value.chars().collect();
        for (index, step) in self.steps.iter().enumerate() {
            match *step {
                Step::Reverse => chars.reverse(),
                Step::Splice { at, count } => {
                    if at > chars.len() {
                        return Err(ChallengeError::OutOfRange {
                            step: index,
                            index: at,
                        });
                    }
                    let end = at.saturating_add(count).min(chars.len());
                    chars.drain(at..end);
                }
                Step::Swap { a, b } => {
                    if a >= chars.len() {
                        return Err(ChallengeError::OutOfRange {
                            step: index,
                            index: a,
                        });
                    }
                    if b >= chars.len() {
                        return Err(ChallengeError::OutOfRange {
                            step: index,
                            index: b,
                        });
                    }
                    chars.swap(a, b);
                }
            }
        }
        Ok(chars.into_iter().collect())
    }
}

/// Which player a program was learned from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerIdentity {
    /// The client the response came from, e.g. `iOS`.
    pub client: String,
    /// The player's own version string, when the response carried one.
    pub player_version: String,
}

impl PlayerIdentity {
    pub fn new(client: impl Into<String>, player_version: impl Into<String>) -> Self {
        Self {
            client: client.into(),
            player_version: player_version.into(),
        }
    }
}

/// The programs this run knows, one per player identity.
///
/// There is no size surprise here: the number of identities is the number of
/// player answers seen this run, and the map is cleared whenever an answer
/// names a player version the entry was not learned from.
#[derive(Debug, Default)]
pub struct Cache {
    programs: BTreeMap<PlayerIdentity, Program>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The program for this player, if one was learned for exactly this
    /// version.
    ///
    /// An entry learned for another version of the same client is dropped: a
    /// re-signed player invalidates its own transform, and applying the old
    /// one would produce a plausible, wrong value.
    pub fn get(&mut self, identity: &PlayerIdentity) -> Option<&Program> {
        if self.programs.contains_key(identity) {
            return self.programs.get(identity);
        }
        self.programs
            .retain(|known, _| known.client != identity.client);
        None
    }

    /// Remember a program for a player version.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn put(&mut self, identity: PlayerIdentity, program: Program) {
        self.programs
            .retain(|known, _| known.client != identity.client);
        self.programs.insert(identity, program);
    }

    /// Forget everything a client was learned for.
    ///
    /// `get` and `put` already drop another version of the same client; this
    /// answers a caller that knows the player changed without asking for it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn invalidate(&mut self, client: &str) {
        self.programs.retain(|known, _| known.client != client);
    }
}

/// Whether an address carries the throttling parameter.
///
/// The parameter is left exactly as the player returned it — this build never
/// rewrites a value it cannot verify — so a caller that sees `true` and has no
/// program for that player knows the address will be answered slowly, and can
/// prefer a profile whose addresses do not carry one.
pub fn throttled(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    parsed
        .query_pairs()
        .any(|(name, value)| name == "n" && !value.is_empty())
}

/// Rewrite the throttling parameter of an address with the program learned for
/// this exact player.
///
/// `Ok(None)` means "no program, keep the address": the caller downloads
/// slowly rather than guessing. `Err` means a program exists but does not fit
/// this value; the address is kept then too, so a bad program can cost speed
/// but never correctness.
///
/// The value is replaced textually inside the query string. Re-encoding the
/// whole query would change the bytes of every signed parameter, and a server
/// that signed those bytes refuses the request.
pub fn rewrite(
    cache: &mut Cache,
    identity: &PlayerIdentity,
    url: &str,
) -> Result<Option<String>, ChallengeError> {
    let program = match cache.get(identity) {
        Some(program) if !program.is_empty() => program.clone(),
        _ => return Ok(None),
    };
    let Some((head, query)) = url.split_once('?') else {
        return Ok(None);
    };
    let mut pieces: Vec<&str> = query.split('&').collect();
    let Some(piece) = pieces.iter_mut().find(|piece| piece.starts_with("n=")) else {
        return Ok(None);
    };
    let value = &piece[2..];
    if value.is_empty() {
        return Ok(None);
    }
    let transformed = program.apply(value)?;
    let replaced = format!("n={transformed}");
    *piece = &replaced;
    Ok(Some(format!("{head}?{}", pieces.join("&"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_player_steps_run_in_order() {
        let program = Program::parse("reverse;splice:1,2;swap:0,2").expect("a program");
        // "abcdef" reversed is "fedcba"; splice removes "ed" -> "fcba";
        // swapping the characters at 0 and 2 -> "bcfa".
        assert_eq!(program.apply("abcdef").expect("applied"), "bcfa");
    }

    #[test]
    fn an_unknown_step_is_refused_before_it_runs() {
        assert_eq!(
            Program::parse("reverse;rotate:1"),
            Err(ChallengeError::UnknownOperation("rotate".to_string()))
        );
        assert!(matches!(
            Program::parse("reverse;concat:1,2"),
            Err(ChallengeError::UnknownOperation(_))
        ));
        assert!(matches!(
            Program::parse("reverse;swap:1"),
            Err(ChallengeError::Malformed(_))
        ));
    }

    #[test]
    fn a_program_longer_than_the_limit_is_refused() {
        let text = (0..=MAX_STEPS)
            .map(|_| "reverse")
            .collect::<Vec<_>>()
            .join(";");
        assert!(matches!(
            Program::parse(&text),
            Err(ChallengeError::TooManySteps(_))
        ));
        assert!(matches!(
            Program::new(vec![Step::Reverse; MAX_STEPS + 1]),
            Err(ChallengeError::TooManySteps(_))
        ));
    }

    #[test]
    fn an_index_is_checked_against_the_value_at_that_step() {
        // 5 is inside "abcdef" but not inside "abc" after the splice.
        let program = Program::new(vec![
            Step::Splice { at: 3, count: 3 },
            Step::Swap { a: 0, b: 5 },
        ])
        .expect("a program");
        assert_eq!(
            program.apply("abcdef"),
            Err(ChallengeError::OutOfRange { step: 1, index: 5 })
        );
        let past_end = Program::new(vec![Step::Splice { at: 99, count: 0 }]).expect("a program");
        assert_eq!(
            past_end.apply("abc"),
            Err(ChallengeError::OutOfRange { step: 0, index: 99 })
        );
        // A splice past the end only removes what is there.
        let whole = Program::new(vec![Step::Splice { at: 0, count: 99 }]).expect("a program");
        assert_eq!(whole.apply("abc").expect("applied"), "");
    }

    #[test]
    fn an_oversized_value_is_refused_rather_than_rewritten() {
        let program = Program::parse("reverse").expect("a program");
        let value = "a".repeat(MAX_INPUT + 1);
        assert_eq!(
            program.apply(&value),
            Err(ChallengeError::TooLong(MAX_INPUT + 1))
        );
    }

    #[test]
    fn a_new_player_version_drops_the_old_program() {
        let mut cache = Cache::new();
        let old = PlayerIdentity::new("iOS", "20.11.6");
        cache.put(old.clone(), Program::parse("reverse").expect("a program"));
        assert!(cache.get(&old).is_some());

        let new = PlayerIdentity::new("iOS", "20.12.0");
        assert!(cache.get(&new).is_none(), "a stale transform is not reused");
        assert!(
            cache.get(&old).is_none(),
            "the entry of the old version went with it"
        );

        // Another client keeps its own program.
        let vr = PlayerIdentity::new("ANDROID_VR", "1.60.19");
        cache.put(vr.clone(), Program::parse("swap:0,1").expect("a program"));
        cache.put(old.clone(), Program::parse("reverse").expect("a program"));
        assert!(cache.get(&old).is_some());
        cache.invalidate("iOS");
        assert!(cache.get(&vr).is_some());
        assert!(cache.get(&old).is_none());
    }

    #[test]
    fn a_throttling_parameter_is_seen_wherever_it_sits() {
        assert!(throttled(
            "https://rr1.googlevideo.com/videoplayback?itag=137&n=AbC&expire=1"
        ));
        assert!(!throttled(
            "https://rr1.googlevideo.com/videoplayback?itag=137&expire=1"
        ));
        assert!(!throttled("https://rr1.googlevideo.com/videoplayback?n="));
        assert!(!throttled("not a url"));
    }

    #[test]
    fn an_address_is_rewritten_in_place_leaving_every_other_byte_alone() {
        let mut cache = Cache::new();
        let identity = PlayerIdentity::new("iOS", "20.11.6");
        // "abc" reversed is "cba".
        cache.put(
            identity.clone(),
            Program::parse("reverse").expect("a program"),
        );
        let rewritten = rewrite(
            &mut cache,
            &identity,
            "https://rr1.googlevideo.com/videoplayback?itag=137&n=abc&sig=SIGNED",
        )
        .expect("no error")
        .expect("a rewritten address");
        assert_eq!(
            rewritten, "https://rr1.googlevideo.com/videoplayback?itag=137&n=cba&sig=SIGNED",
            "the signed value must not be re-encoded"
        );
    }

    #[test]
    fn an_address_without_a_program_or_a_parameter_is_kept_as_it_is() {
        let mut cache = Cache::new();
        let identity = PlayerIdentity::new("ANDROID_VR", "1.60.19");
        let url = "https://rr1.googlevideo.com/videoplayback?itag=137&n=abc";
        assert_eq!(rewrite(&mut cache, &identity, url), Ok(None));

        cache.put(
            identity.clone(),
            Program::parse("reverse").expect("a program"),
        );
        assert_eq!(
            rewrite(
                &mut cache,
                &identity,
                "https://rr1.googlevideo.com/videoplayback?itag=137"
            ),
            Ok(None)
        );
        assert_eq!(
            rewrite(
                &mut cache,
                &identity,
                "https://rr1.googlevideo.com/videoplayback?n="
            ),
            Ok(None)
        );
    }

    #[test]
    fn a_program_that_does_not_fit_the_value_is_refused_and_the_address_kept() {
        let mut cache = Cache::new();
        let identity = PlayerIdentity::new("Web", "1");
        cache.put(
            identity.clone(),
            Program::parse("swap:0,99").expect("a program"),
        );
        let url = "https://cdn.example.org/a?n=abc";
        assert!(matches!(
            rewrite(&mut cache, &identity, url),
            Err(ChallengeError::OutOfRange { .. })
        ));
    }

    #[test]
    fn a_program_of_another_client_is_never_applied() {
        let mut cache = Cache::new();
        cache.put(
            PlayerIdentity::new("iOS", "20.11.6"),
            Program::parse("reverse").expect("a program"),
        );
        let other = PlayerIdentity::new("ANDROID_VR", "20.11.6");
        assert_eq!(
            rewrite(&mut cache, &other, "https://cdn.example.org/a?n=abc"),
            Ok(None)
        );
    }
}
