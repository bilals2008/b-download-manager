//! The crawl: from one page to the files it leads to.
//!
//! The walk is breadth-first and patient. It reads the pages of one site, a
//! page at a time, with a pause between requests, and it stops at the depth,
//! page and file limits the user set. Every page and every redirect goes
//! through the address policy, every page is checked against the site's
//! `robots.txt`, and a cancellation is noticed between chunks, not after the
//! whole request.
//!
//! This module never creates a download. It only offers candidates; the window
//! hands the selected ones to the engine.
//!
//! The pieces: [`request`] reads the window's ask into addresses and limits,
//! [`events`] is what the window hears, [`fetch`] reads one page and one
//! robots file, and [`walk`] is the breadth-first loop that joins them.

mod events;
mod fetch;
mod request;
mod walk;

pub use events::{Event, Outcome};
#[cfg(test)]
pub use events::Stats;
pub use request::{Request, Settings};
pub use walk::run;

#[cfg(test)]
mod tests;
