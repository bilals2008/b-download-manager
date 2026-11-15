//! Protection markers: where a stream says it is protected, and what to say.
//!
//! Detection only. Nothing here fetches a key, opens a licence or decrypts a
//! byte: the module answers one question — is this protected, and by what —
//! so the refusal can name the reason before a single byte is downloaded.
//!
//! Every marker lives here, so the playlist reader, the manifest reader and
//! the download path say the same thing about the same bytes. A false
//! positive is worse than a late failure: a marker is required, never a
//! guess from a content type.

use super::hls::Attributes;
use super::xml::{self, Element};
use super::MediaError;

/// YouTube's UMP transport.
///
/// UMP is the player's own protocol rather than a file, so it has no address
/// to download and is never called encryption.
pub const UMP: &str = "YouTube's UMP protocol";

/// The protection a playlist names, when it names one.
///
/// A `#EXT-X-KEY` whose method is not `NONE` names a protection; the key is
/// never fetched and the bytes are never touched. `METHOD=NONE` is the
/// absence of encryption and is not a protection.
pub fn in_hls_playlist(text: &str) -> Option<&'static str> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("#EXT-X-KEY:")
            .or_else(|| line.strip_prefix("#EXT-X-SESSION-KEY:"))
        else {
            continue;
        };
        if let Some(scheme) = hls_scheme(rest) {
            return Some(scheme);
        }
    }
    None
}

/// The protection a manifest names, when it names one.
///
/// A `ContentProtection` element names a system by `schemeIdUri`. The
/// well-known systems are named; a system nobody knows is named as nothing,
/// and the refusal says only that the stream is protected.
pub fn in_dash_manifest(text: &str) -> Option<&'static str> {
    let root = xml::parse(text).ok()?;
    in_dash_element(&root)
}

/// The protection a parsed manifest names.
pub fn in_dash_element(root: &Element) -> Option<&'static str> {
    let mut schemes = Vec::new();
    collect_schemes(root, &mut schemes);
    if schemes.is_empty() {
        return None;
    }
    let names: Vec<&'static str> = schemes.iter().map(|scheme| protection_name(scheme)).collect();
    for known in [
        "Widevine",
        "PlayReady",
        "FairPlay",
        "Clear Key",
        "MPEG Common Encryption",
    ] {
        if names.contains(&known) {
            return Some(known);
        }
    }
    Some("")
}

/// The protection an init segment carries, when it carries one.
///
/// `pssh` is the common-encryption header and `encv`/`enca` are protected
/// sample entries; either one means the bytes are encrypted. Nothing here
/// decrypts anything or looks at a licence.
pub fn in_container(bytes: &[u8]) -> Option<&'static str> {
    let mut sample_entry = false;
    for window in bytes.windows(4) {
        match window {
            b"pssh" => return Some("MPEG Common Encryption"),
            b"encv" | b"enca" => sample_entry = true,
            _ => {}
        }
    }
    sample_entry.then_some("an encrypted sample entry")
}

/// Whether a container or a media type is YouTube's UMP transport.
pub fn is_ump(container_or_mime: &str) -> bool {
    container_or_mime.to_ascii_lowercase().contains("yt-ump")
}

/// The refusal a marker earns.
pub fn error(reason: &str) -> MediaError {
    if reason == UMP {
        MediaError::PlayerTransport(UMP.to_string())
    } else {
        MediaError::protected(reason)
    }
}

/// The reader's sentence for a marker.
///
/// One sentence, with no line break in it: the panel shows it as it is.
pub fn sentence(reason: &str) -> String {
    error(reason).to_string()
}

/// The name of the protection a `#EXT-X-KEY` names.
///
/// The key format names the system behind the method (`Widevine`,
/// `FairPlay`, `PlayReady`); without one the method is the name. A method
/// nobody knows is named as nothing, and the refusal says only that the
/// stream is protected.
fn hls_scheme(rest: &str) -> Option<&'static str> {
    let attrs = Attributes::parse(rest);
    let method = attrs.get("METHOD").unwrap_or("").trim().to_ascii_uppercase();
    if method.is_empty() || method == "NONE" {
        return None;
    }
    let format = attrs.get("KEYFORMAT").unwrap_or("").to_ascii_lowercase();
    if format.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed") || format.contains("widevine") {
        return Some("Widevine");
    }
    if format.contains("com.apple.streamingkeydelivery") || format.contains("fairplay") {
        return Some("FairPlay");
    }
    if format.contains("com.microsoft.playready") || format.contains("playready") {
        return Some("PlayReady");
    }
    Some(match method.as_str() {
        "AES-128" => "AES-128",
        "AES-256" => "AES-256",
        "AES-CTR" => "AES-CTR",
        "SAMPLE-AES" => "SAMPLE-AES",
        "SAMPLE-AES-CTR" => "SAMPLE-AES-CTR",
        "SAMPLE-AES-CENC" => "SAMPLE-AES-CENC",
        "ISO-23001-7" => "ISO-23001-7",
        // `IDENTITY` is the format's own name for no key format at all, and
        // the format still calls it a key.
        _ => "",
    })
}

/// The `schemeIdUri` of every `ContentProtection` element below this one.
fn collect_schemes(element: &Element, out: &mut Vec<String>) {
    if element.name.eq_ignore_ascii_case("ContentProtection") {
        let scheme = element
            .attr("schemeIdUri")
            .or_else(|| element.attr("value"))
            .unwrap_or("");
        out.push(scheme.trim().to_ascii_lowercase());
    }
    for child in &element.children {
        collect_schemes(child, out);
    }
}

/// The readable name of one `schemeIdUri`.
fn protection_name(scheme: &str) -> &'static str {
    if scheme.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed") || scheme.contains("widevine") {
        return "Widevine";
    }
    if scheme.contains("9a04f079-9840-4286-ab92-e65be0885f95") || scheme.contains("playready") {
        return "PlayReady";
    }
    if scheme.contains("94ce86fb-07ff-4f43-adb8-93d2fa968ca2") || scheme.contains("fairplay") {
        return "FairPlay";
    }
    if scheme.contains("1077efec-c0b2-4d02-ace3-3c1e52e2fb4b") || scheme.contains("clearkey") {
        return "Clear Key";
    }
    if scheme.contains("mp4protection") {
        return "MPEG Common Encryption";
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::fixtures;

    #[test]
    fn a_protected_playlist_is_named_and_a_clear_one_is_not() {
        assert_eq!(in_hls_playlist(fixtures::HLS_ENCRYPTED), Some("AES-128"));
        assert_eq!(in_hls_playlist(fixtures::HLS_MASTER), None);
        assert_eq!(in_hls_playlist(fixtures::HLS_MEDIA), None);
    }

    #[test]
    fn a_key_of_method_none_is_the_absence_of_encryption() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:2,\na.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(in_hls_playlist(text), None);
    }

    #[test]
    fn a_key_format_names_the_system_behind_the_method() {
        let widevine = "#EXTM3U\n\
            #EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\n";
        assert_eq!(in_hls_playlist(widevine), Some("Widevine"));

        let fairplay = "#EXTM3U\n\
            #EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n";
        assert_eq!(in_hls_playlist(fairplay), Some("FairPlay"));
    }

    #[test]
    fn a_protected_manifest_is_named_and_a_clear_one_is_not() {
        assert_eq!(
            in_dash_manifest(fixtures::DASH_PROTECTED),
            Some("MPEG Common Encryption")
        );
        assert_eq!(in_dash_manifest(fixtures::DASH_MANIFEST), None);
    }

    #[test]
    fn a_protection_system_is_named_instead_of_one_blanket_sentence() {
        let text = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <ContentProtection schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"/>
        </AdaptationSet></Period></MPD>"#;
        assert_eq!(in_dash_manifest(text), Some("Widevine"));
    }

    #[test]
    fn a_protection_box_is_recognised_in_an_init_segment() {
        assert_eq!(
            in_container(b"\0\0\0\x20pssh\0\0\0\0"),
            Some("MPEG Common Encryption")
        );
        assert_eq!(in_container(b"....encv...."), Some("an encrypted sample entry"));
        assert_eq!(in_container(b"\0\0\0\x20ftypisom"), None);
    }

    #[test]
    fn the_ump_transport_has_no_address_and_is_not_encryption() {
        assert!(is_ump("vnd.yt-ump"));
        assert!(is_ump("application/vnd.yt-ump"));
        assert!(!is_ump("video/mp4"));

        let sentence = sentence(UMP);
        assert!(sentence.contains("no downloadable address"), "{sentence}");
        assert!(!sentence.contains("protected"), "{sentence}");
        assert!(!sentence.contains("DRM"), "{sentence}");
        assert!(matches!(error(UMP), MediaError::PlayerTransport(_)));
    }

    #[test]
    fn every_sentence_is_one_sentence() {
        for reason in ["Widevine", "", "MPEG Common Encryption", UMP] {
            let sentence = sentence(reason);
            assert!(!sentence.is_empty());
            assert!(!sentence.contains('\n'));
            assert!(
                sentence.chars().filter(|character| *character == '.').count() <= 1,
                "one sentence: {sentence}"
            );
        }
    }
}
