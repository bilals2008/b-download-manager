//! Proxy auto-config scripts: the script's `FindProxyForURL(url, host)` says
//! where a request goes.
//!
//! A PAC script is JavaScript, but only a small part of it is used: the
//! comparisons, the logical operators, the strings and the calls to the PAC
//! helper functions. This module has a parser and an interpreter for that
//! part — there is no JavaScript engine here, and the script gets no access
//! to anything but the values it builds itself.
//!
//! Anything outside that part is refused with an error. A wrong answer
//! would send the reader's downloads through a proxy the script did not
//! choose, which is worse than no answer at all.
//!
//! The answers are kept per script and per host, so a script is not run
//! again for every file of the same download.

mod helpers;
mod js;

pub use helpers::Clock;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

/// Where a request goes, in the script's order of preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyRoute {
    /// Straight to the server.
    Direct,
    /// A proxy address: `http://host:port`, `https://…`, `socks5://…` or
    /// `socks4://…`.
    Proxy(String),
}

/// Why a proxy auto-config script could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacError {
    /// The script has no `FindProxyForURL`, so it decides nothing.
    NoEntryPoint,
    /// The script is not the JavaScript this evaluator understands.
    Syntax {
        /// The line the trouble is on.
        line: usize,
        /// What was read wrong.
        message: String,
    },
    /// The script used something this evaluator does not have.
    Unsupported(String),
    /// The script answered something that is not a route list.
    Runtime(String),
}

impl PacError {
    /// A syntax error on that line.
    pub fn syntax(line: usize, message: impl Into<String>) -> Self {
        PacError::Syntax {
            line,
            message: message.into(),
        }
    }

    /// Something the evaluator does not have.
    pub fn unsupported(message: impl Into<String>) -> Self {
        PacError::Unsupported(message.into())
    }

    /// The script itself failed.
    pub fn runtime(message: impl Into<String>) -> Self {
        PacError::Runtime(message.into())
    }
}

impl std::fmt::Display for PacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PacError::NoEntryPoint => write!(f, "the script has no FindProxyForURL"),
            PacError::Syntax { line, message } => {
                write!(f, "the script cannot be read (line {line}): {message}")
            }
            PacError::Unsupported(what) => write!(
                f,
                "the script uses something this evaluator does not have: {what}"
            ),
            PacError::Runtime(message) => write!(f, "the script failed: {message}"),
        }
    }
}

impl std::error::Error for PacError {}

/// `FindProxyForURL`'s answer: `PROXY a:8080; SOCKS b:1080; DIRECT` becomes
/// routes, and the kinds that are not routes are skipped. An empty answer
/// means direct.
pub fn parse(answer: &str) -> Vec<ProxyRoute> {
    let mut out: Vec<ProxyRoute> = answer
        .split(';')
        .filter_map(|entry| {
            let mut it = entry.split_whitespace();
            let kind = it.next()?.to_ascii_uppercase();
            let at = it.next();
            match (kind.as_str(), at) {
                ("DIRECT", _) => Some(ProxyRoute::Direct),
                ("PROXY" | "HTTP", Some(a)) => Some(ProxyRoute::Proxy(format!("http://{a}"))),
                ("HTTPS", Some(a)) => Some(ProxyRoute::Proxy(format!("https://{a}"))),
                ("SOCKS" | "SOCKS5", Some(a)) => Some(ProxyRoute::Proxy(format!("socks5://{a}"))),
                ("SOCKS4", Some(a)) => Some(ProxyRoute::Proxy(format!("socks4://{a}"))),
                _ => None,
            }
        })
        .collect();
    if out.is_empty() {
        out.push(ProxyRoute::Direct);
    }
    out
}

/// The route that applies: the first one the script named, or direct when it
/// named none.
pub fn pick(routes: &[ProxyRoute]) -> ProxyRoute {
    routes.first().cloned().unwrap_or(ProxyRoute::Direct)
}

/// The routes for `url` under `script`, in the order the script prefers
/// them.
///
/// The answer is kept per script and per address. A proxy script sees the
/// whole address and may match on the path of it, so an answer for one
/// address says nothing about another address on the same host.
pub fn routes(script: &str, url: &str) -> Result<Vec<ProxyRoute>, PacError> {
    routes_with_clock(script, url, &Clock::now())
}

/// [`routes`] with the clock the date helpers read, for a caller that
/// knows it.
pub fn routes_with_clock(
    script: &str,
    url: &str,
    clock: &Clock,
) -> Result<Vec<ProxyRoute>, PacError> {
    let host = host_of(url)?;
    let key = (script_key(script), url.to_owned());
    if let Some(known) = cache()
        .as_ref()
        .and_then(|answers| answers.get(&key))
    {
        return Ok(known.clone());
    }
    let answer = js::call_entry(script, url, &host, clock)?;
    let found = parse(&answer);
    cache()
        .get_or_insert_with(HashMap::new)
        .insert(key, found.clone());
    Ok(found)
}

type Answers = HashMap<(u64, String), Vec<ProxyRoute>>;

static CACHE: Mutex<Option<Answers>> = Mutex::new(None);

/// The script in force, if the reader has chosen one.
///
/// The script decides per address, so it is asked for every address and its
/// answers are kept. The text is the script's own: the application fetches it
/// (a PAC file is a document, like any other) and hands it here.
static SCRIPT: Mutex<Option<String>> = Mutex::new(None);

/// Put a script in force. `None` means the reader has chosen none.
pub fn set_script(script: Option<String>) {
    let mut slot = match SCRIPT.lock() {
        Ok(slot) => slot,
        Err(poisoned) => poisoned.into_inner(),
    };
    *slot = script;
}

/// The script in force, if there is one.
pub fn script() -> Option<String> {
    match SCRIPT.lock() {
        Ok(slot) => slot.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// The route for `url`: what the script in force says about it.
///
/// Without a script the route is direct, and the static proxy the reader set
/// applies as it always did. A script that cannot answer is an error the
/// caller sees, never a silent fall-through to a route the script did not
/// choose.
pub fn route_for(url: &str) -> Result<ProxyRoute, PacError> {
    let Some(script) = script() else {
        return Ok(ProxyRoute::Direct);
    };
    Ok(pick(&routes(&script, url)?))
}

/// The answers kept so far. A poisoned cache is still a cache: the answers
/// in it are the ones that were read successfully.
fn cache() -> MutexGuard<'static, Option<Answers>> {
    match CACHE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The identity of a script: its text's hash. Two scripts with the same text
/// are the same script, whatever else was chosen.
fn script_key(script: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    script.hash(&mut hasher);
    hasher.finish()
}

/// The host of `url`, as the script's `host` argument: without the port, in
/// lower case.
fn host_of(url: &str) -> Result<String, PacError> {
    let after_scheme = match url.find("://") {
        Some(at) => &url[at + 3..],
        None => url,
    };
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // The user name and its password come first, when there are any.
    let authority = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    let host = if let Some(rest) = authority.strip_prefix('[') {
        // An IPv6 address in brackets: the port is after the bracket.
        match rest.find(']') {
            Some(at) => &rest[..at],
            None => rest,
        }
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err(PacError::runtime("the address has no host"));
    }
    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(script: &str, url: &str) -> Vec<ProxyRoute> {
        routes(script, url).expect("the script answers")
    }

    /// The typical script a company hands out.
    const TYPICAL: &str = r#"
        function FindProxyForURL(url, host) {
            if (isPlainHostName(host) || dnsDomainIs(host, ".intranet.test")) return "DIRECT";
            if (shExpMatch(url, "*/downloads/*.iso")) return "PROXY iso.proxy:3128";
            if (isInNet("10.1.2.3", "10.0.0.0", "255.0.0.0") && dnsDomainLevels(host) > 1)
                return "PROXY proxy.corp:8080; DIRECT";
            return "DIRECT";
        }"#;

    #[test]
    fn a_script_in_force_decides_per_address() {
        // The script is asked for every address: the answer for one says
        // nothing about another.
        set_script(Some(
            "function FindProxyForURL(url, host) { if (host == 'proxied.test') return 'PROXY 127.0.0.1:8080'; return 'DIRECT'; }"
                .into(),
        ));
        assert_eq!(
            route_for("http://proxied.test/a").expect("the script answers"),
            ProxyRoute::Proxy("http://127.0.0.1:8080".into())
        );
        assert_eq!(
            route_for("http://other.test/a").expect("the script answers"),
            ProxyRoute::Direct
        );
        // Without a script the route is direct, and the reader's own proxy
        // setting is the route as it always was.
        set_script(None);
        assert_eq!(
            route_for("http://proxied.test/a").expect("no script"),
            ProxyRoute::Direct
        );
    }

    #[test]
    fn a_typical_script_is_run() {
        assert_eq!(route(TYPICAL, "http://intranet/a"), vec![ProxyRoute::Direct]);
        assert_eq!(
            route(TYPICAL, "https://wiki.intranet.test/x"),
            vec![ProxyRoute::Direct]
        );
        assert_eq!(
            route(TYPICAL, "https://mirror.example.org/downloads/os.iso"),
            vec![ProxyRoute::Proxy("http://iso.proxy:3128".into())]
        );
        assert_eq!(
            route(TYPICAL, "https://cdn.example.org/a.zip"),
            vec![
                ProxyRoute::Proxy("http://proxy.corp:8080".into()),
                ProxyRoute::Direct
            ]
        );
    }

    #[test]
    fn a_script_may_use_the_string_methods() {
        let script = r#"
            function FindProxyForURL(url, host) {
                if (host.endsWith(".example.org")) return "PROXY org.proxy:8080";
                if (host.startsWith("www.")) return "PROXY www.proxy:3128";
                if (host.indexOf(".") < 0) return "DIRECT";
                return "PROXY other.proxy:1";
            }"#;
        assert_eq!(
            route(script, "https://files.example.org/a"),
            vec![ProxyRoute::Proxy("http://org.proxy:8080".into())]
        );
        assert_eq!(
            route(script, "https://www.elsewhere.test/a"),
            vec![ProxyRoute::Proxy("http://www.proxy:3128".into())]
        );
        assert_eq!(
            route(script, "https://somewhere.test/a"),
            vec![ProxyRoute::Proxy("http://other.proxy:1".into())]
        );
    }

    #[test]
    fn a_script_may_use_the_logic() {
        let script = r#"
            function FindProxyForURL(url, host) {
                var secure = (url.indexOf("https://") == 0);
                if (secure && host != "secure.example.com") return "PROXY s:1";
                else if (!secure || host == "secure.example.com") return "PROXY p:2";
                return "DIRECT";
            }"#;
        assert_eq!(
            route(script, "https://other.test/a"),
            vec![ProxyRoute::Proxy("http://s:1".into())]
        );
        assert_eq!(
            route(script, "https://secure.example.com/a"),
            vec![ProxyRoute::Proxy("http://p:2".into())]
        );
        assert_eq!(
            route(script, "http://other.test/a"),
            vec![ProxyRoute::Proxy("http://p:2".into())]
        );
    }

    #[test]
    fn a_script_may_use_the_hosts_of_the_network() {
        let script = r#"
            function FindProxyForURL(url, host) {
                if (localHostOrDomainIs(host, "www.example.org")) return "PROXY int:8080";
                if (isInNet(host, "192.168.0.0", "255.255.0.0")) return "PROXY lan:8080";
                return "DIRECT";
            }"#;
        assert_eq!(
            route(script, "http://www/a"),
            vec![ProxyRoute::Proxy("http://int:8080".into())]
        );
        assert_eq!(
            route(script, "http://192.168.1.20/a"),
            vec![ProxyRoute::Proxy("http://lan:8080".into())]
        );
        assert_eq!(route(script, "http://elsewhere.test/a"), vec![ProxyRoute::Direct]);
    }

    #[test]
    fn the_answer_is_kept_per_script_and_per_host() {
        let script = r#"
            function FindProxyForURL(url, host) {
                if (host == "one.test") return "PROXY one:1";
                if (host == "two.test") return "PROXY two:2";
                return "DIRECT";
            }"#;
        assert_eq!(
            route(script, "http://one.test/a"),
            vec![ProxyRoute::Proxy("http://one:1".into())]
        );
        assert_eq!(
            route(script, "http://two.test/a"),
            vec![ProxyRoute::Proxy("http://two:2".into())]
        );
        // The same host again: the answer is the one that was kept.
        assert_eq!(
            route(script, "http://one.test/a"),
            vec![ProxyRoute::Proxy("http://one:1".into())]
        );
    }

    #[test]
    fn a_script_that_cannot_be_run_is_an_error() {
        // No FindProxyForURL at all.
        assert_eq!(
            routes("var x = 1;", "http://x.test/"),
            Err(PacError::NoEntryPoint)
        );
        // Not JavaScript this evaluator reads.
        assert!(matches!(
            routes("this is not javascript (", "http://x.test/"),
            Err(PacError::Syntax { .. })
        ));
        // Something it does not have: a loop.
        assert!(matches!(
            routes(
                "function FindProxyForURL(u, h) { while (true) {} }",
                "http://x.test/"
            ),
            Err(PacError::Unsupported(_))
        ));
        // A name it does not know.
        assert!(matches!(
            routes(
                "function FindProxyForURL(u, h) { return somethingUnknown; }",
                "http://x.test/"
            ),
            Err(PacError::Unsupported(_))
        ));
        // An answer that is not a route list.
        assert!(matches!(
            routes(
                "function FindProxyForURL(u, h) { return 42; }",
                "http://x.test/"
            ),
            Err(PacError::Runtime(_))
        ));
    }

    #[test]
    fn the_unknown_answers_are_unknown_and_not_guesses() {
        let script = r#"
            function FindProxyForURL(url, host) {
                if (myIpAddress() === null) return "PROXY nowhere:1";
                if (dnsResolve(host) === null) return "PROXY unresolved:2";
                return "DIRECT";
            }"#;
        assert_eq!(
            route(script, "http://x.test/a"),
            vec![ProxyRoute::Proxy("http://nowhere:1".into())]
        );
    }

    #[test]
    fn answers_become_routes() {
        assert_eq!(
            parse("PROXY proxy.corp:8080; SOCKS s.corp:1080; DIRECT"),
            vec![
                ProxyRoute::Proxy("http://proxy.corp:8080".into()),
                ProxyRoute::Proxy("socks5://s.corp:1080".into()),
                ProxyRoute::Direct
            ]
        );
        assert_eq!(parse(""), vec![ProxyRoute::Direct]);
        assert_eq!(parse("WEIRD x; DIRECT"), vec![ProxyRoute::Direct]);
        assert_eq!(parse("SOCKS4 s:1080"), vec![ProxyRoute::Proxy("socks4://s:1080".into())]);
        assert_eq!(
            parse("HTTPS s:443"),
            vec![ProxyRoute::Proxy("https://s:443".into())]
        );
    }

    #[test]
    fn the_first_route_is_the_one_that_applies() {
        let routes = parse("PROXY a:1; PROXY b:2; DIRECT");
        assert_eq!(pick(&routes), ProxyRoute::Proxy("http://a:1".into()));
        assert_eq!(pick(&[]), ProxyRoute::Direct);
    }

    #[test]
    fn the_host_of_an_address_is_read_without_its_port() {
        assert_eq!(host_of("http://Example.ORG:8080/a?b").unwrap(), "example.org");
        assert_eq!(host_of("https://example.org").unwrap(), "example.org");
        assert_eq!(host_of("http://[2620:fe::fe]:80/x").unwrap(), "2620:fe::fe");
        assert_eq!(host_of("http://user:pw@example.org/x").unwrap(), "example.org");
        assert!(host_of("http://").is_err());
    }
}
