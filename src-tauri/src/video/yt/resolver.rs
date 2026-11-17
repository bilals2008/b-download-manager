//! Resolving a watch page: the remembered answer, the page read, and the learned throttling transforms.

use super::*;
use super::client::{ask_player, ios_profile, vr_profile};
use super::parse::{check_playable, digits_after, merge_failures, plan_from, text_between};

/// One remembered answer, so two callers asking at once share the page read.
struct Remembered {
    at: Instant,
    manifest: Manifest,
    source: SourcePlan,
}

fn answers() -> &'static Mutex<HashMap<String, Remembered>> {
    static ANSWERS: OnceLock<Mutex<HashMap<String, Remembered>>> = OnceLock::new();
    ANSWERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether any address of a plan is too near its own deadline to start with.
///
/// Only a plain-file plan carries leases; a playlist's segments are re-signed
/// by every read of the playlist, so their age is not this module's to know.
fn expiring(source: &SourcePlan, now: SystemTime) -> bool {
    match source {
        SourcePlan::Files { tracks } => tracks
            .iter()
            .any(|track| track.lease.expiring_at(EXPIRY_MARGIN, now)),
        _ => false,
    }
}

/// The transform programs this run has learned, one per player version.
///
/// A program belongs to the exact player that issued the address, so the key
/// carries both the client and the player's own version. A player that changes
/// therefore invalidates its own entry instead of being applied to output it
/// never produced.
pub(super) fn programs() -> &'static Mutex<challenge::Cache> {
    static PROGRAMS: OnceLock<Mutex<challenge::Cache>> = OnceLock::new();
    PROGRAMS.get_or_init(|| Mutex::new(challenge::Cache::new()))
}

/// Rewrite an address's throttling parameter with a learned program, if this
/// run has one for that player.
pub(super) fn rewrite_throttle(identity: &PlayerIdentity, url: &str) -> Option<String> {
    let mut cache = programs().lock().ok()?;
    match challenge::rewrite(&mut cache, identity, url) {
        Ok(rewritten) => rewritten,
        Err(error) => {
            // A program that does not fit this value is not applied. The
            // address is downloaded as it came, slowly but correctly.
            idm_core::log::write(
                "warn",
                &format!("the throttling transform was not applied: {error}"),
            );
            None
        }
    }
}
pub(super) async fn remember(
    id: &str,
    fallback_title: &str,
    fresh: bool,
) -> Result<(Manifest, SourcePlan), YtError> {
    if !fresh {
        if let Ok(answers) = answers().lock() {
            if let Some(answer) = answers.get(id) {
                // Age is only one reason to drop an answer. Every address in it
                // carries its own deadline, and a deadline is what a server
                // actually honours: handing one back that is about to pass
                // would fail the download on its first segment for no reason.
                if answer.at.elapsed() < ANSWER_TTL && !expiring(&answer.source, SystemTime::now())
                {
                    return Ok((answer.manifest.clone(), answer.source.clone()));
                }
            }
        }
    }
    let (manifest, source) = read_page(id, fallback_title).await?;
    if let Ok(mut answers) = answers().lock() {
        answers.retain(|_, answer| answer.at.elapsed() < ANSWER_TTL);
        answers.insert(
            id.to_string(),
            Remembered {
                at: Instant::now(),
                manifest: manifest.clone(),
                source: source.clone(),
            },
        );
    }
    Ok((manifest, source))
}

/// Read the watch page, ask each client, and keep the best answer.
async fn read_page(id: &str, fallback_title: &str) -> Result<(Manifest, SourcePlan), YtError> {
    let client = idm_core::net::client_builder(None, None, None)
        .map_err(YtError::Page)?
        .build()
        .map_err(|error| YtError::Page(error.to_string()))?;

    // 1. Read the watch page for the visitor data and the signature timestamp.
    let page = client
        .get(format!("https://www.youtube.com/watch?v={id}&hl=en&gl=US"))
        .header("user-agent", DESKTOP_UA)
        .header("accept-language", "en-US,en;q=0.9")
        .header("cookie", "CONSENT=YES+1")
        .send()
        .await
        .map_err(|error| YtError::Page(error.to_string()))?;
    let html = page
        .text()
        .await
        .map_err(|error| YtError::Page(error.to_string()))?;
    let visitor = text_between(&html, "\"VISITOR_DATA\":\"", '"')
        .or_else(|| text_between(&html, "\"visitorData\":\"", '"'))
        .ok_or(YtError::NoVisitor)?;
    let sts = digits_after(&html, "\"STS\":").unwrap_or(DEFAULT_STS);

    // 2. Ask each client in turn. iOS is first; the VR client answers when iOS
    //    does not, so one client losing its URL rights no longer stops every
    //    video on the page.
    let mut failures: Vec<YtError> = Vec::new();
    let mut fallback: Option<(Manifest, SourcePlan)> = None;
    let mut throttled_note = false;
    for profile in [ios_profile(visitor), vr_profile(visitor)] {
        let data = match ask_player(&client, id, visitor, sts, &profile).await {
            Ok(data) => data,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        if let Err(error) = check_playable(&data, id) {
            // A definite refusal is final: another client will say the same.
            if matches!(
                error,
                YtError::Protected(_) | YtError::Live | YtError::Identity { .. }
            ) {
                return Err(error);
            }
            failures.push(error);
            continue;
        }
        match plan_from(&data, fallback_title, &profile) {
            Ok(planned) => {
                // An answer whose addresses all need a transform this build
                // cannot verify is kept, but not preferred: a throttled
                // address downloads, only slowly.
                if planned.throttled {
                    throttled_note = true;
                    fallback.get_or_insert((planned.manifest, planned.source));
                } else {
                    return Ok((planned.manifest, planned.source));
                }
            }
            Err(error) => failures.push(error),
        }
    }
    if let Some(answer) = fallback {
        if throttled_note {
            idm_core::log::write(
                "info",
                "the player's throttling parameter is present and untransformed; the download may be slow",
            );
        }
        return Ok(answer);
    }
    Err(merge_failures(failures))
}
