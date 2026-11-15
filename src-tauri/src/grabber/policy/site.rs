//! What belongs to the site that was asked for.

use std::net::IpAddr;

use reqwest::Url;

/// Whether a link stays on the site that was asked for.
///
/// A site is one host plus its `www.` twin, which is one site in the everyday
/// sense and two only in DNS. A subdomain is another host: following it would
/// turn one request into a crawl of whatever shares the domain, which is not
/// what the user pointed at.
pub fn site_hosts(start: &Url) -> Vec<String> {
    let Some(host) = start.host_str() else {
        return Vec::new();
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if let Some(rest) = host.strip_prefix("www.") {
        vec![host.clone(), rest.to_string()]
    } else if host.parse::<IpAddr>().is_ok() {
        vec![host]
    } else {
        vec![host.clone(), format!("www.{host}")]
    }
}

pub fn same_site(url: &Url, hosts: &[String]) -> bool {
    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        .is_some_and(|host| hosts.contains(&host))
}
