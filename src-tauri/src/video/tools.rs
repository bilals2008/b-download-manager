//! Test-only helpers shared by the media modules.
//!
//! The remux and verification paths are fully native now; there is no external
//! video tool to locate or run. The one thing the tests still share is a lock
//! around the `BDM_FFMPEG` override the older tests used, so a stray variable
//! from one test cannot leak into another.

/// One lock for every test that manipulates a process-wide override. Tests
/// share a process, so an environment variable set by one must not reach
/// another module's test.
#[cfg(test)]
pub fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One EBML element with a one- or two-byte size, for building fixtures.
#[cfg(test)]
pub fn ebml(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    let size = body.len();
    if size < 0x7F {
        out.push(0x80 | size as u8);
    } else {
        out.push(0x40 | (size >> 8) as u8);
        out.push(size as u8);
    }
    out.extend_from_slice(body);
    out
}

/// A whole one-second WebM: one picture track holding one frame.
///
/// It is assembled from the bytes rather than encoded, so it is the same file
/// on every machine and needs no video tool to exist.
#[cfg(test)]
pub fn tiny_webm() -> Vec<u8> {
    let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
    let mut info = ebml(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]);
    info.extend(ebml(&[0x44, 0x89], &1000.0f64.to_be_bytes()));
    let mut entry = ebml(&[0xD7], &[1]);
    entry.extend(ebml(&[0x83], &[1]));
    entry.extend(ebml(&[0x86], b"V_VP9"));
    let mut cluster = ebml(&[0xE7], &[0]);
    cluster.extend(ebml(&[0xA3], &[0x81, 0, 0, 0x80, 1, 2, 3]));

    let mut out = header;
    out.extend([0x18, 0x53, 0x80, 0x67]);
    out.extend([0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    out.extend(ebml(&[0x15, 0x49, 0xA9, 0x66], &info));
    out.extend(ebml(&[0x16, 0x54, 0xAE, 0x6B], &ebml(&[0xAE], &entry)));
    out.extend(ebml(&[0x1F, 0x43, 0xB6, 0x75], &cluster));
    out
}
