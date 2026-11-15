//! Probing the server: how big the file is, and where the server is.

use super::*;

/// Measure where the server that answers is, without holding the download.
///
/// The measurement opens one connection of its own, so it runs beside the
/// transfer and reports into the row when it is done.
pub(super) fn measure_location(download: &Arc<Download>, host: Option<String>) {
    let Some(host) = host.filter(|host| measurable(host)) else {
        return;
    };
    let Ok(url) = reqwest::Url::parse(&download.url) else {
        return;
    };
    let Some(port) = url.port_or_known_default() else {
        return;
    };
    let download = download.clone();
    tokio::spawn(async move {
        let Ok(reach) = crate::geo::measure(&host, port).await else {
            return;
        };
        let country = reach
            .address
            .parse::<std::net::IpAddr>()
            .ok()
            .and_then(crate::geo::country)
            .map(|country| country.code().to_string());
        download.set_location(country, Some(reach.rtt_ms));
    });
}

/// Whether asking a server where it is tells the reader anything.
///
/// A server on this computer is not far away and is in no country: a probe of
/// the loopback interface spends a connection to learn nothing. That is also
/// where the test servers live, and a download from them must not open a
/// second connection of its own.
fn measurable(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" {
        return false;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(address) => !address.is_loopback() && !address.is_unspecified(),
        Err(_) => true,
    }
}

/// Ask the server how big the file is, and ask again when it says no.
///
/// A server that is busy answers `429`; the answer carries a body, and that
/// body is not the size of the file. The probe therefore fails on such an
/// answer, and the download waits and asks again.
pub(super) async fn probe_with_retries(download: &Arc<Download>, epoch: u64) -> Result<Probe, String> {
    let mut attempt = 0u32;

    loop {
        if download.is_cancelled(epoch) {
            return Err("cancelled".to_string());
        }
        match download.source.probe().await {
            Ok(probe) => {
                download.note_status(None);
                return Ok(probe);
            }
            Err(error) => {
                attempt += 1;
                download.note_status(error.status);
                if !error.is_transient() || attempt >= download.max_attempts.load(Ordering::SeqCst)
                {
                    return Err(error.message);
                }
                let wait = error.retry_after.unwrap_or_else(|| http::backoff(attempt));
                if !wait_while_active(download, epoch, wait).await {
                    return Err("cancelled".to_string());
                }
            }
        }
    }
}
