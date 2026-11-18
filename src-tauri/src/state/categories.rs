//! The category rules: which file type goes to which folder.

use std::path::{Path, PathBuf};

use super::{AppState, CategoryRule};

impl AppState {
    /// Give every category its own destination.
    ///
    /// The window owns the list; the backend only applies it, so a browser
    /// hand-over lands where the options window says.
    pub fn set_categories(&self, rules: Vec<CategoryRule>) {
        let _gate = self.start_gate();
        *self
            .category_rules
            .lock()
            .expect("category rules lock poisoned") = rules;
    }

    /// The category a file name belongs to, in the order the window sent the
    /// rules: a rule the reader wrote wins over a built-in one, the way the
    /// categories pane detects one. A type nobody claims belongs to the
    /// category the window marked as the fallback, which is what the add
    /// dialog would have shown for it.
    pub fn detect_category(&self, dest: &Path) -> Option<String> {
        let rules = self
            .category_rules
            .lock()
            .expect("category rules lock poisoned");
        let owns = |rule: &CategoryRule, extension: &str| {
            rule.extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        };
        let matched = dest
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .and_then(|extension| {
                rules
                    .iter()
                    .filter(|rule| !rule.builtin)
                    .find(|rule| owns(rule, &extension))
                    .or_else(|| rules.iter().find(|rule| owns(rule, &extension)))
            });
        matched
            .or_else(|| rules.iter().find(|rule| rule.fallback))
            .map(|rule| rule.name.clone())
    }

    /// The folder a category saves to, given the default folder in force.
    ///
    /// A folder set on the category wins; a relative one is ignored, it would
    /// depend on where the app was started. Without a folder of its own, every
    /// category except the general buckets saves in a folder of its own name
    /// inside the default folder — the same rule the add dialog applies, so a
    /// hand-over with no dialog lands where the dialog would have put it. The
    /// general buckets keep the default folder itself, which the caller has
    /// already chosen.
    pub fn category_folder_in(&self, name: &str, default: Option<&Path>) -> Option<PathBuf> {
        let rules = self
            .category_rules
            .lock()
            .expect("category rules lock poisoned");
        let rule = rules.iter().find(|rule| rule.name == name)?;
        let folder = rule.folder.trim();
        if !folder.is_empty() {
            let path = PathBuf::from(folder);
            if path.is_absolute() {
                return Some(path);
            }
            // A relative folder is not a folder; the category derives its
            // own below, the way the window's saved preference would.
        }
        if rule.root || rule.fallback {
            return None;
        }
        let root = default.filter(|path| path.is_absolute())?;
        Some(root.join(&rule.name))
    }
}
