//! What one crawl may reach, and what it may offer.
//!
//! A grabber aims a request at whatever address it is given, and then follows
//! what that page points at. Both halves are a hazard: an address may name the
//! machine the application runs on, a name may resolve into the router's
//! network, and a redirect may send the reader somewhere the first check never
//! saw. Every request therefore goes through [`check_public`], and every hop of
//! a redirect is checked again before it is followed.
//!
//! The rules are deliberately plain and in one place, so the window can explain
//! a refusal with the same sentence the crawl used.
//!
//! The module is cut by responsibility: [`kinds`] reads a link as a file,
//! [`site`] decides what belongs to the site that was asked for, [`net`]
//! decides whether an address may be reached at all, and [`exclusions`] holds
//! the reader's leave-out rules and the accepted file types.

mod exclusions;
mod kinds;
mod net;
mod site;

pub use exclusions::{Accepted, Exclusions};
pub use kinds::{extension_of, is_page_document, kind_of, short, Candidate, FileKind};
pub use net::{check_literal, check_public};
#[cfg(test)]
pub use net::check_web;
pub use site::{same_site, site_hosts};

use std::time::Duration;

/// The most pages one crawl reads.
pub const MAX_PAGES: usize = 500;

/// The most links a page may hold before the rest are left unread.
pub const MAX_PAGE_LINKS: usize = 2_000;

/// The deepest a crawl may follow page links. One level is the start page.
pub const MAX_DEPTH: u8 = 5;

/// The most files one crawl offers. Beyond this the list is frozen and the
/// window says so, instead of growing until the window crawls.
pub const MAX_FILES: usize = 2_000;

/// The pause between two page requests. A grabber that fires as fast as the
/// site answers is a small denial of service, and every crawl is one user.
pub const PAGE_DELAY: Duration = Duration::from_millis(300);

/// The longest page accepted. A page is a directory, not a download.
pub const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

/// The redirects one page may travel through.
pub const MAX_REDIRECTS: usize = 5;

/// The time one page request may take, body included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The longest `robots.txt` read.
pub const MAX_ROBOTS_BYTES: usize = 512 * 1024;

/// The longest `Crawl-delay` honoured. A directive asking for more than this
/// is reported instead of followed; the crawl never waits minutes per page.
pub const MAX_CRAWL_DELAY: Duration = Duration::from_secs(5);

/// The User-Agent every request of the grabber carries.
///
/// It names the grabber honestly and gives robots.txt a token to match. A site
/// that only serves browsers may refuse it; that is the site's decision, and
/// the window reports the refusal.
pub const USER_AGENT: &str = "BDM-SiteGrabber/0.1";

/// The token robots.txt groups match against.
pub const ROBOTS_AGENT: &str = "BDM-SiteGrabber";

#[cfg(test)]
mod tests;
