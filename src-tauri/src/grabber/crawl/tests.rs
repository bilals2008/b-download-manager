use super::request::check_target;
use std::collections::HashMap;
use reqwest::{Client, Url};
use tokio::sync::watch;
use super::super::policy::{self, Accepted, Candidate, Exclusions, FileKind, MAX_DEPTH, MAX_PAGES};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    fn request(url: &str) -> Request {
        Request {
            url: url.to_string(),
            ..Request::default()
        }
    }

    #[test]
    fn the_window_defaults_are_the_safe_ones() {
        let request = Request::default();
        assert_eq!(request.depth, 1, "one page, unless the user asks for more");
        assert_eq!(request.pages, 50);
        assert_eq!(
            request.types,
            vec!["video", "music", "documents", "archives", "programs"]
        );
        assert!(!request.types.iter().any(|kind| kind == "images"));
    }

    #[test]
    fn a_request_reads_its_limits_and_clamps_them() {
        let mut request = request("https://example.org/a/b");
        request.depth = 99;
        request.pages = 10_000;
        let settings = Settings::read(&request).expect("a crawl");
        assert_eq!(settings.depth, MAX_DEPTH);
        assert_eq!(settings.pages, MAX_PAGES);
        assert_eq!(settings.hosts, vec!["example.org", "www.example.org"]);
        assert_eq!(settings.start.path(), "/a/b");

        request.depth = 0;
        request.pages = 0;
        let settings = Settings::read(&request).expect("a crawl");
        assert_eq!(settings.depth, 1);
        assert_eq!(settings.pages, 1);
    }

    #[test]
    fn the_start_must_be_a_public_page() {
        assert!(Settings::read(&request("")).is_err());
        assert!(Settings::read(&request("not an address")).is_err());
        assert!(Settings::read(&request("ftp://example.org/a.zip")).is_err());
        assert!(Settings::read(&request("http://127.0.0.1/a")).is_err());
        assert!(Settings::read(&request("http://localhost/a")).is_err());
        assert!(Settings::read(&request("http://intranet/a")).is_err());
        assert!(
            Settings::read(&request("https://example.org/film.mp4")).is_err(),
            "a file is not a page to read"
        );
        assert!(Settings::read(&request("https://example.org/index.html")).is_ok());
    }

    #[test]
    fn a_request_without_a_file_type_is_refused() {
        let mut request = request("https://example.org/");
        request.types.clear();
        request.extensions.clear();
        assert!(Settings::read(&request).is_err());

        request.extensions = vec!["jpeg".to_string()];
        assert!(Settings::read(&request).is_ok());
    }

    #[test]
    fn the_fragment_of_the_start_is_not_part_of_the_crawl() {
        let settings =
            Settings::read(&request("https://example.org/docs#section")).expect("a crawl");
        assert_eq!(settings.start.as_str(), "https://example.org/docs");
    }

    #[test]
    fn an_event_reads_the_way_the_window_expects() {
        let progress = serde_json::to_value(Event::Progress {
            pages_read: 2,
            pages_queued: 3,
            files: 4,
            page_limit: 50,
        })
        .expect("an event");
        assert_eq!(progress["kind"], "progress");
        assert_eq!(progress["pagesRead"], 2);
        assert_eq!(progress["pagesQueued"], 3);
        assert_eq!(progress["pageLimit"], 50);
        assert!(progress.get("pages_read").is_none());

        let file = Candidate::from_url(
            &Url::parse("https://example.org/files/a.zip").unwrap(),
            "https://example.org/",
        )
        .expect("a file");
        let found = serde_json::to_value(Event::File { file }).expect("an event");
        assert_eq!(found["kind"], "file");
        assert_eq!(found["file"]["name"], "a.zip");
        assert_eq!(found["file"]["category"], "Compressed");
        assert_eq!(found["file"]["kind"], "archives");
        assert!(found["file"].get("page").is_some());
    }

    #[test]
    fn the_notes_carry_a_readable_address() {
        let url = Url::parse("https://example.org/files/a.zip?token=secret").unwrap();
        let line = policy::short(&url);
        assert_eq!(line, "example.org/files/a.zip");
        assert!(!line.contains("secret"));
    }

    /* ------------------------------------------------- a server on loopback */

    #[derive(Clone)]
    struct Reply {
        status: u16,
        location: Option<String>,
        body: String,
        delay: Duration,
    }

    impl Reply {
        fn html(body: &str) -> Self {
            Self {
                status: 200,
                location: None,
                body: body.to_string(),
                delay: Duration::ZERO,
            }
        }

        fn redirect(to: &str) -> Self {
            Self {
                status: 302,
                location: Some(to.to_string()),
                body: String::new(),
                delay: Duration::ZERO,
            }
        }

        fn slow(milliseconds: u64) -> Self {
            Self {
                status: 200,
                location: None,
                body: "<html></html>".to_string(),
                delay: Duration::from_millis(milliseconds),
            }
        }
    }

    /// A tiny HTTP server for the crawl to read.
    ///
    /// It answers one request per connection, which is all a crawl needs, and
    /// remembers every path it was asked for. A path that was not registered
    /// answers 404, which is what a site without robots.txt does.
    async fn serve(routes: Vec<(&str, Reply)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let port = listener.local_addr().expect("the port").port();
        let routes: Arc<HashMap<String, Reply>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, reply)| (path.to_string(), reply))
                .collect(),
        );
        let hits: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    continue;
                };
                let routes = routes.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 8_192];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let target = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    log.lock().unwrap().push(target.clone());
                    let path = target.split('?').next().unwrap_or("/").to_string();
                    let reply = routes.get(&path).cloned().unwrap_or(Reply {
                        status: 404,
                        location: None,
                        body: String::new(),
                        delay: Duration::ZERO,
                    });
                    if reply.delay > Duration::ZERO {
                        tokio::time::sleep(reply.delay).await;
                    }
                    let reason = match reply.status {
                        200 => "OK",
                        302 => "Found",
                        404 => "Not Found",
                        _ => "Error",
                    };
                    let mut head = format!(
                        "HTTP/1.1 {} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    if let Some(location) = reply.location {
                        head.push_str(&format!("Location: {location}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(reply.body.as_bytes()).await;
                });
            }
        });

        (format!("http://127.0.0.1:{port}"), hits)
    }

    fn client() -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .expect("a client")
    }

    /// The settings a test crawls with.
    ///
    /// A loopback address is refused by [`Settings::read`], which is the point
    /// of the production policy, so the test builds the same settings around
    /// its own server and switches on the loopback allowance.
    fn crawl_settings(
        start: &str,
        depth: u8,
        pages: usize,
        types: &[&str],
        exclude: &[&str],
    ) -> Settings {
        let start = Url::parse(start).expect("a test address");
        let kinds = types.iter().filter_map(|kind| FileKind::from_token(kind));
        Settings {
            hosts: policy::site_hosts(&start),
            start,
            depth: depth.clamp(1, MAX_DEPTH),
            pages: pages.clamp(1, MAX_PAGES),
            accepted: Accepted::new(kinds, &[]),
            exclusions: Exclusions::parse(
                &exclude
                    .iter()
                    .map(|rule| (*rule).to_string())
                    .collect::<Vec<_>>(),
            ),
            test_allow_local: true,
        }
    }

    async fn crawl(
        start: &str,
        depth: u8,
        pages: usize,
        types: &[&str],
        exclude: &[&str],
    ) -> (Outcome, Vec<Event>) {
        let settings = crawl_settings(start, depth, pages, types, exclude);
        let (_sender, receiver) = watch::channel(false);
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut emit = move |event: Event| sink.lock().unwrap().push(event);
        let outcome = run(settings, client(), receiver, &mut emit).await;
        let events = events.lock().unwrap().clone();
        (outcome, events)
    }

    fn files_of(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::File { file } => Some(file.name.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_site_is_read_once_and_only_as_deep_as_asked() {
        let (base, hits) = serve(vec![
            (
                "/",
                Reply::html(
                    r#"<a href="/a.html">A</a><a href="/a.html">A again</a>
                       <a href="/files/one.zip">1</a><a href="/files/one.zip#again">1 again</a>
                       <a href="/private/skip.zip">skip</a>
                       <a href="/files/two.pdf">2</a>"#,
                ),
            ),
            (
                "/a.html",
                Reply::html(
                    r#"<a href="/b.html">B</a><a href="/files/two.pdf">2</a>
                       <a href="/deep.rar">r</a>"#,
                ),
            ),
            (
                "/b.html",
                Reply::html(r#"<a href="/files/late.zip">late</a>"#),
            ),
            ("/files/one.zip", Reply::html("not read")),
        ])
        .await;

        let (outcome, events) =
            crawl(&base, 2, 10, &["documents", "archives"], &["/private"]).await;
        assert_eq!(outcome.status, "done");
        assert_eq!(outcome.stats.pages_read, 2);
        assert_eq!(
            files_of(&events),
            vec!["one.zip", "two.pdf", "deep.rar"],
            "each address once, and only from the pages that were read"
        );
        assert_eq!(
            outcome.stats.duplicates, 1,
            "two.pdf is linked from both pages; the page reader already folds the repeats on one page"
        );
        assert!(
            outcome.stats.excluded >= 1,
            "the excluded path was left out"
        );

        let hits = hits.lock().unwrap().clone();
        assert!(hits.contains(&"/".to_string()));
        assert!(hits.contains(&"/a.html".to_string()));
        assert!(
            !hits.contains(&"/b.html".to_string()),
            "depth 2 reads the start page and the pages it links to, no further: {hits:?}"
        );
        assert!(
            !hits.iter().any(|hit| hit.contains("one.zip")),
            "the crawl offers files, it does not fetch them"
        );
    }

    #[tokio::test]
    async fn the_page_limit_stops_the_walk_and_says_so() {
        let mut home = String::new();
        for index in 0..10 {
            home.push_str(&format!(r#"<a href="/p{index}.html">p</a>"#));
        }
        let pages: Vec<String> = (0..10)
            .map(|index| format!(r#"<a href="/p{index}.html">p</a>"#))
            .collect();
        let paths: Vec<String> = (0..10).map(|index| format!("/p{index}.html")).collect();
        let mut routes: Vec<(&str, Reply)> = vec![("/", Reply::html(&home))];
        for (path, body) in paths.iter().zip(pages.iter()) {
            routes.push((path.as_str(), Reply::html(body)));
        }
        let (base, _hits) = serve(routes).await;

        let (outcome, _events) = crawl(&base, 3, 3, &["documents"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 3);
        assert!(
            outcome.stats.truncated,
            "the limit was reached, so the site is not finished"
        );
    }

    #[tokio::test]
    async fn a_page_robots_txt_forbids_is_not_read() {
        let (base, hits) = serve(vec![
            (
                "/",
                Reply::html(r#"<a href="/private/page.html">p</a><a href="/files/a.zip">a</a>"#),
            ),
            (
                "/private/page.html",
                Reply::html(r#"<a href="/files/b.zip">b</a>"#),
            ),
            (
                "/robots.txt",
                Reply::html("User-agent: *\nDisallow: /private\n"),
            ),
        ])
        .await;

        let (outcome, events) = crawl(&base, 2, 10, &["archives"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 1);
        assert_eq!(files_of(&events), vec!["a.zip"]);
        assert!(outcome.stats.restricted >= 1);
        assert!(
            events.iter().any(|event| matches!(
                event,
                Event::Note { message, .. } if message.contains("robots.txt")
            )),
            "the refusal is explained in the log"
        );
        let hits = hits.lock().unwrap().clone();
        assert!(
            !hits.contains(&"/private/page.html".to_string()),
            "a disallowed page is never requested: {hits:?}"
        );
    }

    #[tokio::test]
    async fn a_redirect_on_the_site_is_followed_and_rechecked() {
        let (base, hits) = serve(vec![
            ("/go", Reply::redirect("/landing.html")),
            (
                "/landing.html",
                Reply::html(r#"<a href="/files/a.zip">a</a>"#),
            ),
        ])
        .await;

        let (outcome, events) = crawl(&format!("{base}/go"), 1, 5, &["archives"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 1);
        assert_eq!(files_of(&events), vec!["a.zip"]);
        let hits = hits.lock().unwrap().clone();
        assert!(hits.contains(&"/landing.html".to_string()));
    }

    #[tokio::test]
    async fn a_redirect_away_from_the_site_is_refused() {
        let (base, _hits) = serve(vec![(
            "/go",
            Reply::redirect("http://169.254.169.254/latest/meta-data/"),
        )])
        .await;

        let (outcome, events) = crawl(&format!("{base}/go"), 1, 5, &["documents"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 0);
        assert!(outcome.stats.restricted >= 1);
        let notes: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert!(
            notes.iter().any(|note| note.contains("redirected away")),
            "the redirect was stopped at the site boundary: {notes:?}"
        );
    }

    #[tokio::test]
    async fn a_private_address_is_still_refused_when_the_test_policy_is_on() {
        let request = Request {
            url: "https://example.org/".to_string(),
            ..Request::default()
        };
        let mut settings = Settings::read(&request).expect("settings");
        settings.test_allow_local = true;
        let metadata = Url::parse("http://169.254.169.254/latest/meta-data/").expect("an address");
        assert!(
            check_target(&metadata, &settings).await.is_err(),
            "a cloud metadata endpoint is never crawled"
        );
        let local = Url::parse("http://127.0.0.1:9/").expect("an address");
        assert!(check_target(&local, &settings).await.is_ok());
    }

    #[tokio::test]
    async fn a_crawl_stops_when_it_is_cancelled() {
        let (base, _hits) = serve(vec![("/", Reply::slow(500))]).await;
        let settings = crawl_settings(&base, 1, 5, &["documents"], &[]);
        let (sender, receiver) = watch::channel(false);
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();

        let task = tokio::spawn(async move {
            let mut emit = move |event: Event| sink.lock().unwrap().push(event);
            run(settings, client(), receiver, &mut emit).await
        });

        tokio::time::sleep(Duration::from_millis(80)).await;
        sender.send(true).expect("the crawl is listening");
        let outcome = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("the crawl stops without waiting for the page")
            .expect("the task finished");
        assert_eq!(outcome.status, "cancelled");
        assert_eq!(outcome.stats.pages_read, 0);

        let events = events.lock().unwrap().clone();
        assert!(matches!(
            events.last(),
            Some(Event::Done {
                status: "cancelled",
                ..
            })
        ));
    }
