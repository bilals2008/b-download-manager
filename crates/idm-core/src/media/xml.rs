//! A tiny XML reader for MPEG-DASH manifests.
//!
//! Only what an MPD contains is read: elements, attributes, text and the
//! predefined entities. There is no DTD, no entity declaration and no
//! namespace resolution: a manifest that asks for any of those is refused
//! before an element is built, which keeps an untrusted document from
//! expanding into something else. Depth, element count, attribute count and
//! attribute size are capped as well.

use super::MediaError;

/// The deepest element tree an MPD may have.
const MAX_DEPTH: usize = 64;

/// The most elements one manifest may hold.
const MAX_ELEMENTS: usize = 50_000;

/// The most attributes one element may carry.
const MAX_ATTRIBUTES: usize = 128;

/// One element of a manifest.
#[derive(Debug, Clone, Default)]
pub struct Element {
    /// The name without its namespace prefix.
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// The text directly inside the element, trimmed.
    pub text: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| local_name(key).eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children
            .iter()
            .find(|child| child.name.eq_ignore_ascii_case(name))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children
            .iter()
            .filter(move |child| child.name.eq_ignore_ascii_case(name))
    }

    /// Whether any element below this one (this one included) is named `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self.children.iter().any(|child| child.contains(name))
    }
}

/// Read one document. The first element is the root.
pub fn parse(text: &str) -> Result<Element, MediaError> {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("<!doctype") || lowered.contains("<!entity") {
        return Err(MediaError::Malformed(
            "the manifest declares a document type, which is not read".to_string(),
        ));
    }

    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut elements = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'<' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            if let Some(top) = stack.last_mut() {
                let piece = decode_entities(&text[start..index]);
                if !piece.trim().is_empty() {
                    if !top.text.is_empty() {
                        top.text.push(' ');
                    }
                    top.text.push_str(piece.trim());
                }
            }
            continue;
        }

        if text[index..].starts_with("<!--") {
            index = find(text, index + 4, "-->")
                .map(|at| at + 3)
                .ok_or_else(|| MediaError::Malformed("a comment does not end".to_string()))?;
            continue;
        }
        if text[index..].starts_with("<?") {
            index = find(text, index + 2, "?>")
                .map(|at| at + 2)
                .ok_or_else(|| MediaError::Malformed("a declaration does not end".to_string()))?;
            continue;
        }
        if text[index..].starts_with("<![CDATA[") {
            let start = index + 9;
            let end = find(text, start, "]]>")
                .ok_or_else(|| MediaError::Malformed("a CDATA block does not end".to_string()))?;
            if let Some(top) = stack.last_mut() {
                let piece = &text[start..end];
                if !piece.trim().is_empty() {
                    top.text.push_str(piece.trim());
                }
            }
            index = end + 3;
            continue;
        }
        if text[index..].starts_with("<!") {
            index = find(text, index + 2, ">")
                .map(|at| at + 1)
                .ok_or_else(|| MediaError::Malformed("a declaration does not end".to_string()))?;
            continue;
        }

        if text[index..].starts_with("</") {
            let end = text[index..]
                .find('>')
                .map(|offset| index + offset)
                .ok_or_else(|| MediaError::Malformed("an end tag does not end".to_string()))?;
            let name = local_name(text[index + 2..end].trim()).to_string();
            let Some(finished) = stack.pop() else {
                return Err(MediaError::Malformed(format!(
                    "an end tag for {name} has no start"
                )));
            };
            if !finished.name.eq_ignore_ascii_case(&name) {
                return Err(MediaError::Malformed(format!(
                    "an end tag for {name} closes {}",
                    finished.name
                )));
            }
            if let Some(parent) = stack.last_mut() {
                parent.children.push(finished);
            } else if root.is_none() {
                root = Some(finished);
            } else {
                return Err(MediaError::Malformed(
                    "the document has more than one root".to_string(),
                ));
            }
            index = end + 1;
            continue;
        }

        // A start tag.
        let end = find_tag_end(text, index + 1)
            .ok_or_else(|| MediaError::Malformed("a start tag does not end".to_string()))?;
        let inner = &text[index + 1..end];
        let self_closing = inner.trim_end().ends_with('/');
        let inner = inner.trim_end().trim_end_matches('/');
        let (name, attrs) = parse_start_tag(inner)?;
        if stack.len() >= MAX_DEPTH {
            return Err(MediaError::Malformed(
                "the manifest is nested too deeply".to_string(),
            ));
        }
        elements += 1;
        if elements > MAX_ELEMENTS {
            return Err(MediaError::Malformed(
                "the manifest has too many elements".to_string(),
            ));
        }
        let element = Element {
            name,
            attrs,
            children: Vec::new(),
            text: String::new(),
        };
        if self_closing {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(element);
            } else if root.is_none() {
                root = Some(element);
            } else {
                return Err(MediaError::Malformed(
                    "the document has more than one root".to_string(),
                ));
            }
        } else {
            stack.push(element);
        }
        index = end + 1;
    }

    if !stack.is_empty() {
        return Err(MediaError::Malformed(
            "the document ends before its elements close".to_string(),
        ));
    }
    root.ok_or_else(|| MediaError::Malformed("the document has no root".to_string()))
}

/// The name of an element or attribute without its namespace prefix.
pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

fn find_tag_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote = 0u8;
    let mut index = from;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = byte;
        } else if byte == b'>' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_start_tag(inner: &str) -> Result<(String, Vec<(String, String)>), MediaError> {
    let bytes = inner.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let name = local_name(inner[..index].trim()).to_string();
    if name.is_empty() {
        return Err(MediaError::Malformed("an element has no name".to_string()));
    }

    let mut attrs = Vec::new();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let name_start = index;
        while index < bytes.len() && bytes[index] != b'=' && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let key = local_name(inner[name_start..index].trim()).to_string();
        if key.is_empty() {
            return Err(MediaError::Malformed(
                "an attribute has no name".to_string(),
            ));
        }
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            // A bare attribute has no value; it carries nothing an MPD needs.
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let quote = bytes[index];
        if quote != b'"' && quote != b'\'' {
            return Err(MediaError::Malformed(
                "an attribute value is not quoted".to_string(),
            ));
        }
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != quote {
            index += 1;
        }
        if index >= bytes.len() {
            return Err(MediaError::Malformed(
                "an attribute value does not end".to_string(),
            ));
        }
        if attrs.len() >= MAX_ATTRIBUTES {
            return Err(MediaError::Malformed(
                "an element has too many attributes".to_string(),
            ));
        }
        if index - value_start > 4096 {
            return Err(MediaError::Malformed(
                "an attribute value is too long".to_string(),
            ));
        }
        attrs.push((key, decode_entities(&inner[value_start..index])));
        index += 1;
    }
    Ok((name, attrs))
}

fn find(text: &str, from: usize, needle: &str) -> Option<usize> {
    text.get(from..)?.find(needle).map(|offset| from + offset)
}

/// The five predefined entities and numeric character references.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => out.push(character),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_element_tree_is_read() {
        let root = parse(
            r#"<?xml version="1.0"?>
            <MPD mediaPresentationDuration="PT10S">
              <Period><AdaptationSet contentType="video"><Representation id="1" bandwidth="100"/></AdaptationSet></Period>
            </MPD>"#,
        )
        .expect("a document");
        assert_eq!(root.name, "MPD");
        assert_eq!(root.attr("mediaPresentationDuration"), Some("PT10S"));
        assert!(root.contains("Representation"));
        let representation = root
            .child("Period")
            .and_then(|period| period.child("AdaptationSet"))
            .and_then(|set| set.child("Representation"))
            .expect("a representation");
        assert_eq!(representation.attr("id"), Some("1"));
        assert_eq!(representation.attr("bandwidth"), Some("100"));
    }

    #[test]
    fn namespaces_are_ignored_and_entities_are_read() {
        let root = parse(r#"<dash:MPD xlink:href="a&amp;b"><dash:Period/></dash:MPD>"#)
            .expect("a document");
        assert_eq!(root.name, "MPD");
        assert_eq!(root.attr("href"), Some("a&b"));
        assert!(root.contains("Period"));
    }

    #[test]
    fn a_document_type_is_refused_before_it_is_read() {
        let error = parse(r#"<!DOCTYPE MPD [<!ENTITY x "y">]><MPD/>"#).expect_err("a DTD");
        assert!(matches!(error, MediaError::Malformed(_)));
    }

    #[test]
    fn a_truncated_document_is_refused() {
        assert!(parse("<MPD><Period>").is_err());
        assert!(parse("<MPD></NOPE>").is_err());
    }

    #[test]
    fn a_comment_and_a_declaration_are_skipped() {
        let root = parse("<!-- hi --><?xml version=\"1.0\"?><MPD><!-- x --><Period/></MPD>")
            .expect("a document");
        assert_eq!(root.children.len(), 1);
    }
}
