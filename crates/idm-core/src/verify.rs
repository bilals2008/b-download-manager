//! The check that the bytes of a chunk belong to the chunk.
//!
//! The reference download manager keeps the head of the file and compares the
//! head of every chunk with it. A chunk that starts with the head of the file
//! is a chunk whose range was not honoured: the server sent the whole file
//! instead of the range that was asked for. Such a chunk is rolled back — the
//! bytes it had are given back and the chunk is fetched again.
//!
//! Evidence: `FUN_00566f10`, the strings "Strart data for this chunk is
//! matched with start of the file!!!", "There is real matching" and "no
//! match, step 4". See `ANALYSIS.md`, section 10.2.

/// How many bytes of the head of the file the reference manager keeps:
/// `0x80c`.
pub const HEAD_BUFFER: usize = 0x80c;

/// How many bytes the reference manager counts as a clean match: `0x200`.
///
/// The comparison in the reference binary (`FUN_00566730`) runs over the head
/// of the file and stops at `0x200` bytes; a match that is longer is counted
/// as a mismatch of its own.
pub const HEAD_BYTES: usize = 0x200;

/// What the check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The chunk holds the data of its own range.
    Good,
    /// The chunk holds the head of the file: the range was not honoured.
    Repeated,
}

/// Compare the head of a chunk with the head of the file.
///
/// The first range of a download *is* the head of the file, so it is never
/// reported as a repeat. A chunk that is shorter than the head is not
/// judged: there is not enough data to tell a repeat from a coincidence, and
/// a wrong verdict would throw away good data.
pub fn check(head: &[u8], chunk: &[u8], is_first: bool) -> Verdict {
    if is_first || head.len() < HEAD_BYTES || chunk.len() < HEAD_BYTES {
        return Verdict::Good;
    }
    if chunk[..HEAD_BYTES] == head[..HEAD_BYTES] {
        Verdict::Repeated
    } else {
        Verdict::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> Vec<u8> {
        (0..HEAD_BYTES).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn a_chunk_with_other_data_passes() {
        let mut chunk = head();
        chunk[0] = 42;
        assert_eq!(check(&head(), &chunk, false), Verdict::Good);
    }

    #[test]
    fn a_chunk_with_the_head_of_the_file_is_reported() {
        assert_eq!(check(&head(), &head(), false), Verdict::Repeated);
    }

    #[test]
    fn the_first_range_is_never_reported() {
        assert_eq!(check(&head(), &head(), true), Verdict::Good);
    }

    #[test]
    fn a_short_chunk_is_not_judged() {
        let chunk = head()[..100].to_vec();
        assert_eq!(check(&head(), &chunk, false), Verdict::Good);
    }
}
