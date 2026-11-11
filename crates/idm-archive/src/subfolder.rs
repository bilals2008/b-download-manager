//! Where the contents of an archive go: straight into the destination, or a
//! folder named after the archive.
//!
//! The three answers are the ones the reference download manager gives, and
//! the ones the file managers do: always a folder, never a folder, or a
//! folder only when the archive holds more than one thing at its top (so a
//! zip with a single file in it does not get a folder of its own).

/// Whether the contents of an archive go into a folder named after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Subfolder {
    /// A folder named after the archive when it holds more than one thing at
    /// its top (KDE Ark, 7-Zip's "smart"): one folder or one file goes
    /// straight into the destination.
    #[default]
    Smart,
    /// Always a folder named after the archive.
    Always,
    /// Never a folder: everything goes straight into the destination.
    Never,
}

impl Subfolder {
    /// Whether a folder named after the archive is made for `top_level`
    /// things at its top.
    pub fn wants_folder(self, top_level: usize) -> bool {
        match self {
            Subfolder::Always => true,
            Subfolder::Never => false,
            Subfolder::Smart => top_level > 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_made_when_the_archive_holds_more_than_one_thing() {
        assert!(Subfolder::Smart.wants_folder(2));
        assert!(!Subfolder::Smart.wants_folder(1));
        assert!(!Subfolder::Smart.wants_folder(0));
    }

    #[test]
    fn always_and_never_do_not_look() {
        assert!(Subfolder::Always.wants_folder(0));
        assert!(Subfolder::Always.wants_folder(1));
        assert!(!Subfolder::Never.wants_folder(0));
        assert!(!Subfolder::Never.wants_folder(9));
    }

    #[test]
    fn smart_is_the_default() {
        assert_eq!(Subfolder::default(), Subfolder::Smart);
    }
}
