use super::*;

fn page(html: &str, url: &str) -> PageMedia {
    inspect(html, url)
}

#[test]
fn the_video_and_the_sound_of_a_page_are_found() {
    let media = page(
        r#"<html><body>
             <video src="/film.mp4" poster="/poster.jpg">
               <source src="/film.webm">
             </video>
             <audio src="/song.mp3"></audio>
             <a href="/readme.txt">notes</a>
             <img src="/poster.jpg">
           </body></html>"#,
        "https://example.org/watch",
    );

    let found: Vec<(&str, &str)> = media
        .files
        .iter()
        .map(|file| (file.kind, file.name.as_str()))
        .collect();
    assert_eq!(
        found,
        vec![
            ("video", "film.mp4"),
            ("video", "film.webm"),
            ("audio", "song.mp3")
        ]
    );
    assert_eq!(media.files[0].url, "https://example.org/film.mp4");
    assert!(media.video);
    assert!(!media.truncated);
    assert_eq!(media.page, "https://example.org/watch");
}

#[test]
fn a_page_that_only_says_so_in_its_metadata_is_still_a_video() {
    let media = page(
        r#"<meta property="og:video:type" content="video/mp4">
           <meta property="og:video" content="/media/clip.mp4">"#,
        "https://example.org/post/1",
    );
    assert!(media.video);
    assert_eq!(media.files.len(), 1);
    assert_eq!(media.files[0].url, "https://example.org/media/clip.mp4");
    assert_eq!(media.files[0].kind, "video");
}

#[test]
fn a_watch_page_with_nothing_in_its_markup_is_a_video() {
    let media = page(
        "<title>Never Gonna Give You Up</title>",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    );
    assert!(media.video);
    assert!(media.files.is_empty());
}

#[test]
fn a_plain_page_is_not_a_video() {
    let media = page(
        "<h1>Notes</h1><p>The meeting moved to Thursday.</p>",
        "https://example.org/notes",
    );
    assert!(!media.video);
    assert!(media.files.is_empty());
}

#[test]
fn a_script_that_mentions_a_player_is_not_evidence() {
    let media = page(
        r#"<script>var player = "<video src='/film.mp4'></video>";</script>"#,
        "https://example.org/notes",
    );
    assert!(!media.video);
    assert!(media.files.is_empty());
}

#[test]
fn a_pasted_playlist_is_a_video_and_not_a_file() {
    let media = page(
        "#EXTM3U\n#EXT-X-VERSION:3\n",
        "https://cdn.example.org/media/master.m3u8?token=1",
    );
    assert!(media.video);
    assert!(media.files.is_empty());

    let dash = page(
        r#"<?xml version="1.0"?><MPD xmlns="urn:mpeg:dash:schema:mpd:2011"></MPD>"#,
        "https://cdn.example.org/media/manifest.mpd",
    );
    assert!(dash.video);
    assert!(dash.files.is_empty());
}

#[test]
fn a_page_that_only_names_a_playlist_plays_a_video() {
    let media = page(
        r#"<video><source src="/media/master.m3u8"></video>"#,
        "https://example.org/watch",
    );
    assert!(media.video);
    assert!(media.files.is_empty());
}

#[test]
fn an_address_with_another_extension_is_not_media() {
    let media = page(
        r#"<a href="/files/a.zip">zip</a><a href="/img/a.jpg">picture</a>
           <a href="/page.html">page</a>"#,
        "https://example.org/downloads",
    );
    assert!(media.files.is_empty());
    assert!(!media.video);
}

#[test]
fn an_extension_is_read_in_any_case_and_a_query_is_not_part_of_the_name() {
    let media = page(
        r#"<a href="/FILM.MP4?token=1">a</a><a href="/Sound.Mp3">b</a>"#,
        "https://example.org/list",
    );
    let kinds: Vec<&str> = media.files.iter().map(|file| file.kind).collect();
    assert_eq!(kinds, vec!["video", "audio"]);
    assert_eq!(media.files[0].name, "FILM.MP4");
}

#[test]
fn the_same_address_is_listed_once() {
    let media = page(
        r#"<video src="/film.mp4"></video><a href="/film.mp4">again</a>"#,
        "https://example.org/watch",
    );
    assert_eq!(media.files.len(), 1);
}

#[test]
fn the_cap_stops_the_list_and_says_so() {
    let mut html = String::new();
    for index in 0..=MAX_MEDIA {
        html.push_str(&format!("<source src=\"/clip{index}.mp4\">"));
    }
    let media = page(&html, "https://example.org/watch");
    assert_eq!(media.files.len(), MAX_MEDIA);
    assert!(media.truncated);
}

#[test]
fn sound_tracks_follow_the_picture() {
    let media = page(
        r#"<audio src="/a.mp3"></audio><source src="/b.m4a">
           <video src="/c.mp4"></video>"#,
        "https://example.org/watch",
    );
    let kinds: Vec<&str> = media.files.iter().map(|file| file.kind).collect();
    assert_eq!(kinds, vec!["video", "audio", "audio"]);
}
