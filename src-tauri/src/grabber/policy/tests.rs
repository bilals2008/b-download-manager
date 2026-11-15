use reqwest::Url;
use super::exclusions::wildcard;
use super::kinds::KINDS;
use super::net::{addr_is_public, host_problem};
use super::*;

fn url(text: &str) -> Url {
    Url::parse(text).expect("a URL")
}

#[test]
fn every_file_kind_has_a_token_and_a_category() {
    for kind in KINDS {
        assert_eq!(FileKind::from_token(kind.token()), Some(kind));
        assert!(!kind.category().is_empty());
    }
    assert_eq!(FileKind::from_token("VIDEO"), Some(FileKind::Video));
    assert_eq!(FileKind::from_token("pictures"), None);
}

#[test]
fn a_type_belongs_to_the_kind_the_window_names() {
    assert_eq!(kind_of("MP4"), Some(FileKind::Video));
    assert_eq!(kind_of(".mp3"), Some(FileKind::Music));
    assert_eq!(kind_of("pdf"), Some(FileKind::Documents));
    assert_eq!(kind_of("webp"), Some(FileKind::Images));
    assert_eq!(kind_of("7z"), Some(FileKind::Archives));
    assert_eq!(kind_of("msi"), Some(FileKind::Programs));
    assert_eq!(kind_of("bin"), Some(FileKind::Other));
    assert_eq!(kind_of("html"), None, "a page is not a file to save");
    assert_eq!(kind_of("m3u8"), None, "a stream manifest is not a file");
    assert_eq!(kind_of("ts"), None);
    assert_eq!(kind_of(""), None);
}

#[test]
fn the_extension_comes_from_the_path_not_the_query() {
    assert_eq!(
        extension_of(&url("https://example.org/a/film.MP4?x=1")).as_deref(),
        Some("mp4")
    );
    assert_eq!(
        extension_of(&url("https://example.org/files/report%2Epdf")).as_deref(),
        Some("pdf")
    );
    assert_eq!(
        extension_of(&url("https://example.org/get?file=a.zip")),
        None
    );
    assert_eq!(extension_of(&url("https://example.org/files/")), None);
    assert_eq!(extension_of(&url("https://example.org/a.b%20c")), None);
    assert_eq!(
        extension_of(&url("https://example.org/a.verylongextension")),
        None
    );
}

#[test]
fn a_candidate_carries_the_name_and_category_the_list_needs() {
    let found = Candidate::from_url(
        &url("https://example.org/files/My%20Film.mp4?sig=1"),
        "https://example.org/",
    )
    .expect("a video");
    assert_eq!(found.name, "My Film.mp4");
    assert_eq!(found.extension, "mp4");
    assert_eq!(found.kind, FileKind::Video);
    assert_eq!(found.category, "Video");
    assert!(found.url.contains("sig=1"), "the address is kept whole");

    let archive =
        Candidate::from_url(&url("https://example.org/a.rar"), "https://example.org/")
            .expect("an archive");
    assert_eq!(archive.category, "Compressed");
}

#[test]
fn an_asset_is_not_a_page_to_read() {
    assert!(!is_page_document(&url("https://example.org/style.css")));
    assert!(!is_page_document(&url("https://example.org/app.js")));
    assert!(!is_page_document(&url("https://example.org/live.m3u8")));
    assert!(!is_page_document(&url("https://example.org/data.json")));
    assert!(is_page_document(&url("https://example.org/index.html")));
    assert!(is_page_document(&url("https://example.org/about")));
    assert!(is_page_document(&url("https://example.org/")));
}

#[test]
fn a_page_is_not_a_candidate() {
    assert!(Candidate::from_url(&url("https://example.org/about"), "x").is_none());
    assert!(Candidate::from_url(&url("https://example.org/index.html"), "x").is_none());
    assert!(Candidate::from_url(&url("https://example.org/a.m3u8"), "x").is_none());
}

#[test]
fn a_site_is_one_host_and_its_www_twin() {
    assert_eq!(
        site_hosts(&url("https://www.Example.org/a")),
        vec!["www.example.org", "example.org"]
    );
    assert_eq!(
        site_hosts(&url("https://example.org/a")),
        vec!["example.org", "www.example.org"]
    );
    assert_eq!(
        site_hosts(&url("http://93.184.216.34/")),
        vec!["93.184.216.34"]
    );

    let hosts = site_hosts(&url("https://example.org/"));
    assert!(same_site(&url("https://www.example.org/x"), &hosts));
    assert!(same_site(&url("http://example.org/x"), &hosts));
    assert!(
        !same_site(&url("https://files.example.org/x"), &hosts),
        "a subdomain is another host"
    );
    assert!(!same_site(&url("https://example.net/x"), &hosts));
}

#[test]
fn the_private_and_reserved_addresses_are_known() {
    for text in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "192.0.2.1",
        "198.18.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::",
        "fe80::1",
        "fc00::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "64:ff9b::7f00:1",
    ] {
        assert!(
            !addr_is_public(text.parse().expect("an address")),
            "{text} must not be reachable"
        );
    }
    for text in ["93.184.216.34", "8.8.8.8", "2606:4700::1111"] {
        assert!(
            addr_is_public(text.parse().expect("an address")),
            "{text} is public"
        );
    }
}

#[test]
fn local_names_and_metadata_endpoints_are_refused() {
    for host in [
        "localhost",
        "LOCALHOST.",
        "metadata.google.internal",
        "instance-data",
        "printer.local",
        "intranet",
        "something.internal",
        "127.0.0.1",
        "[::1]",
    ] {
        let host = host.trim_matches(['[', ']']);
        assert!(host_problem(host).is_some(), "{host} must be refused");
    }
    assert_eq!(host_problem("example.org"), None);
    assert_eq!(host_problem("93.184.216.34"), None);
}

#[test]
fn only_web_addresses_without_credentials_are_read() {
    assert!(check_literal(&url("https://example.org/a")).is_ok());
    assert!(check_literal(&url("ftp://example.org/a")).is_err());
    assert!(check_literal(&url("file:///etc/passwd")).is_err());
    assert!(check_literal(&url("javascript:alert(1)")).is_err());
    assert!(
        check_literal(&url("https://user:secret@example.org/a")).is_err(),
        "credentials in an address are a way to hide the host"
    );
    assert!(check_literal(&url("http://192.168.0.1/")).is_err());
}

#[test]
fn exclusion_patterns_match_paths_and_hosts() {
    let rules = Exclusions::parse(&[
        "/private/*".to_string(),
        "*.zip".to_string(),
        "cdn.example.org".to_string(),
        "/private/*".to_string(),
        "  ".to_string(),
        "/".to_string() + &"a".repeat(300),
    ]);
    assert!(rules.matches(&url("https://example.org/private/a/b.mp4")));
    assert!(!rules.matches(&url("https://example.org/public/a.mp4")));
    assert!(rules.matches(&url("https://example.org/files/a.ZIP")));
    assert!(rules.matches(&url("https://cdn.example.org/a.mp4")));
    assert!(!rules.matches(&url("https://example.org/a.mp4")));
    assert!(
        Exclusions::parse(&["/private".to_string()])
            .matches(&url("https://example.org/private/a.mp4")),
        "a path rule leaves out the pages under it"
    );
}

#[test]
fn an_empty_exclusion_list_matches_nothing() {
    let rules = Exclusions::parse(&[]);
    assert!(!rules.matches(&url("https://example.org/anything")));
}

#[test]
fn a_glob_anchors_neither_end() {
    assert!(wildcard("/private/*", "/private/a/b"));
    assert!(!wildcard("/private/*", "/public/private/a"));
    assert!(wildcard("*.mp4", "https://example.org/a.mp4"));
    assert!(wildcard("*", "anything"));
    assert!(wildcard("a*b*c", "axxbyyc"));
    assert!(!wildcard("a*b*c", "axxbyy"));
    assert!(!wildcard("", "x"));
}

#[test]
fn accepted_types_take_the_ticked_kinds_and_the_typed_extensions() {
    let accepted = Accepted::new([FileKind::Video], &["iso".to_string(), ".ZIP".to_string()]);
    let video = Candidate::from_url(&url("https://example.org/a.mp4"), "x").unwrap();
    let archive = Candidate::from_url(&url("https://example.org/a.iso"), "x").unwrap();
    let zip = Candidate::from_url(&url("https://example.org/a.zip"), "x").unwrap();
    let doc = Candidate::from_url(&url("https://example.org/a.pdf"), "x").unwrap();
    assert!(accepted.takes(&video));
    assert!(accepted.takes(&archive));
    assert!(accepted.takes(&zip));
    assert!(!accepted.takes(&doc));

    let none = Accepted::new([], &[]);
    assert!(none.is_empty());
    assert!(!none.takes(&video));
}
