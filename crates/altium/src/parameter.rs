//! Pipe-delimited parameter records.
//!
//! `KEY=VALUE` pairs separated by `|` (or `` ` `` when nested). Keys prefixed
//! with `%UTF8%` carry UTF-8 values; the prefix is stripped on read and
//! re-attached on write through [`ParameterMap::is_utf8`].

use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;

use crate::Coord;

/// Top-level entry separator used in parameter strings.
pub const SEP_TOP: char = '|';
/// Separator used inside nested parameter blocks (rare).
pub const SEP_NESTED: char = '`';
/// `%UTF8%` prefix that flags a value as UTF-8 encoded.
pub const UTF8_PREFIX: &str = "%UTF8%";

/// A single `name=value` entry, borrowed from the input string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterEntry<'a> {
    /// Parameter name with the `%UTF8%` prefix (if any) already stripped.
    pub name: &'a str,
    /// Parameter value as it appeared in the input (no decoding performed).
    pub value: &'a str,
    /// Whether the original key carried the `%UTF8%` prefix.
    pub is_utf8: bool,
}

/// Zero-allocation iterator over a parameter string.
///
/// ```
/// # use altium::parameter::Parameters;
/// let mut iter = Parameters::new("|FOO=bar|BAZ=qux");
/// let first = iter.next().unwrap();
/// assert_eq!((first.name, first.value), ("FOO", "bar"));
/// let second = iter.next().unwrap();
/// assert_eq!((second.name, second.value), ("BAZ", "qux"));
/// assert!(iter.next().is_none());
/// ```
#[derive(Debug, Clone)]
pub struct Parameters<'a> {
    remaining: &'a str,
    sep: char,
}

impl<'a> Parameters<'a> {
    /// Iterate top-level (`|`-separated) entries.
    pub const fn new(s: &'a str) -> Self {
        Self {
            remaining: s,
            sep: SEP_TOP,
        }
    }

    /// Iterate nested (`` ` ``-separated) entries.
    pub const fn nested(s: &'a str) -> Self {
        Self {
            remaining: s,
            sep: SEP_NESTED,
        }
    }
}

impl<'a> Iterator for Parameters<'a> {
    type Item = ParameterEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Trim any leading separators.
            let trimmed = self.remaining.trim_start_matches(self.sep);
            if trimmed.is_empty() {
                self.remaining = "";
                return None;
            }
            // The separator characters are ASCII so byte indices align with chars.
            let (entry, rest) = match trimmed.find(self.sep) {
                Some(idx) => (&trimmed[..idx], &trimmed[idx + 1..]),
                None => (trimmed, ""),
            };
            self.remaining = rest;

            // Only the tail of the whole record is trimmed: Altium writes
            // values such as `FR-4\r` mid-record and reads them back verbatim.
            let entry = if rest.is_empty() {
                entry.trim_end_matches(['\r', '\n', ' ', '\t'])
            } else {
                entry
            };
            if entry.is_empty() {
                continue;
            }

            // Find `=` separating name from value.
            let Some(eq) = entry.find('=') else {
                // Some Altium files contain stray name-only tokens; surface them
                // as empty-value entries so callers can skip or preserve them.
                return Some(ParameterEntry {
                    name: "",
                    value: entry,
                    is_utf8: false,
                });
            };
            let name = &entry[..eq];
            let value = &entry[eq + 1..];

            let (name, is_utf8) = strip_utf8_prefix(name);
            return Some(ParameterEntry {
                name,
                value,
                is_utf8,
            });
        }
    }
}

fn trim_ascii_end(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b'\r' | b'\n' | b' ' | b'\t') {
        end -= 1;
    }
    &bytes[..end]
}

fn strip_utf8_prefix(name: &str) -> (&str, bool) {
    let len = UTF8_PREFIX.len();
    if name.len() >= len && name[..len].eq_ignore_ascii_case(UTF8_PREFIX) {
        (&name[len..], true)
    } else {
        (name, false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// Original-cased name (without `%UTF8%` prefix).
    name: String,
    value: String,
    is_utf8: bool,
}

/// Owned, insertion-ordered, case-insensitive parameter map.
///
/// Lookup is case-insensitive (Altium's convention); iteration preserves the
/// order entries were inserted in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParameterMap {
    /// Index keyed by uppercase name → position in `entries`.
    index: IndexMap<String, usize>,
    entries: Vec<Entry>,
}

#[cfg(feature = "serde")]
impl serde::Serialize for ParameterMap {
    /// Ordered `[name, value, is_utf8]` triples — preserves insertion order
    /// and original casing exactly.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.entries.iter().map(|e| (&e.name, &e.value, e.is_utf8)))
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ParameterMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let entries = Vec::<(String, String, bool)>::deserialize(d)?;
        let mut map = ParameterMap::default();
        for (name, value, is_utf8) in entries {
            map.index
                .entry(name.to_uppercase())
                .or_insert(map.entries.len());
            map.entries.push(Entry {
                name,
                value,
                is_utf8,
            });
        }
        Ok(map)
    }
}

impl ParameterMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a top-level parameter string.
    pub fn parse(s: &str) -> Self {
        Self::parse_at(s, SEP_TOP)
    }

    /// Parse with an explicit separator (`|` or `` ` ``).
    pub fn parse_at(s: &str, sep: char) -> Self {
        let mut map = Self::new();
        let iter = Parameters { remaining: s, sep };
        for entry in iter {
            map.insert_parsed(entry.name, entry.value, entry.is_utf8);
        }
        map
    }

    /// Parse a raw parameter record straight from its bytes.
    ///
    /// Altium stores every value that contains non-ASCII text twice: a
    /// `%UTF8%`-prefixed UTF-8 copy, then two empty entries, then a plain
    /// twin under the bare key. Decoding the whole record as Windows-1252
    /// first would turn the UTF-8 copy into mojibake, so each value is decoded
    /// according to its own key and the UTF-8 copy wins over the twin.
    pub fn parse_bytes(bytes: &[u8], sep: u8) -> Self {
        let mut map = Self::new();
        let chunks: Vec<&[u8]> = bytes.split(|&b| b == sep).collect();
        for (i, raw) in chunks.iter().enumerate() {
            // Only the tail of the whole record is trimmed (see `Parameters`).
            let entry = if i + 1 == chunks.len() {
                trim_ascii_end(raw)
            } else {
                raw
            };
            if entry.is_empty() {
                continue;
            }
            let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                // Stray name-only token; keep it like the string parser does.
                map.insert_parsed("", &crate::encoding::decode(entry), false);
                continue;
            };
            let key = crate::encoding::decode(&entry[..eq]);
            let (name, is_utf8) = strip_utf8_prefix(&key);
            let value = if is_utf8 {
                String::from_utf8_lossy(&entry[eq + 1..]).into_owned()
            } else {
                crate::encoding::decode(&entry[eq + 1..])
            };
            map.insert_parsed(name, &value, is_utf8);
        }
        map
    }

    /// Insert an entry read from a file. A plain twin never overrides a
    /// `%UTF8%` copy that was already seen for the same name.
    fn insert_parsed(&mut self, name: &str, value: &str, is_utf8: bool) {
        let key = name.to_ascii_uppercase();
        if let Some(&pos) = self.index.get(&key) {
            if self.entries[pos].is_utf8 && !is_utf8 {
                return;
            }
            // `RECORD` marks the start of a sub-record; Altium repeats it
            // inside one string (a `Board` record restarts it every few
            // layers). Every marker stays in place; lookups see the first.
            if key == "RECORD" {
                self.entries.push(Entry {
                    name: name.to_string(),
                    value: value.to_string(),
                    is_utf8,
                });
                return;
            }
        }
        self.insert_with_utf8_flag(name, value, is_utf8);
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether a parameter with the given name exists (case-insensitive).
    pub fn contains_key(&self, name: &str) -> bool {
        self.index.contains_key(&name.to_ascii_uppercase())
    }

    /// Borrow the value for `name`, if present.
    pub fn get(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_uppercase();
        self.index
            .get(&key)
            .map(|i| self.entries[*i].value.as_str())
    }

    /// Whether the entry was originally tagged with `%UTF8%`.
    pub fn is_utf8(&self, name: &str) -> bool {
        let key = name.to_ascii_uppercase();
        self.index
            .get(&key)
            .is_some_and(|i| self.entries[*i].is_utf8)
    }

    /// Iterate `(name, value, is_utf8)` in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.entries
            .iter()
            .map(|e| (e.name.as_str(), e.value.as_str(), e.is_utf8))
    }

    /// Insert or replace an entry. New entries take the next position; updates
    /// keep their original position. The `%UTF8%` flag is auto-set for any
    /// value containing non-ASCII text: that is what Altium itself does (a
    /// `-55°C` value is stored as `%UTF8%Text=` even though `°` fits in
    /// Windows-1252), and it keeps the legacy code-page twin from ever being
    /// the only copy of a character that some other locale would misread.
    pub fn insert(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        let is_utf8 = !value.is_ascii();
        self.insert_with_utf8_flag(name, &value, is_utf8);
    }

    /// Insert or replace, marking the entry as UTF-8.
    pub fn insert_utf8(&mut self, name: &str, value: impl Into<String>) {
        self.insert_with_utf8_flag(name, &value.into(), true);
    }

    fn insert_with_utf8_flag(&mut self, name: &str, value: &str, is_utf8: bool) {
        let key = name.to_ascii_uppercase();
        if let Some(&pos) = self.index.get(&key) {
            self.entries[pos].value = value.to_owned();
            self.entries[pos].is_utf8 = is_utf8;
            return;
        }
        let pos = self.entries.len();
        self.entries.push(Entry {
            name: name.to_owned(),
            value: value.to_owned(),
            is_utf8,
        });
        self.index.insert(key, pos);
    }

    /// Remove an entry. Subsequent inserts keep their relative order.
    pub fn remove(&mut self, name: &str) -> Option<String> {
        let key = name.to_ascii_uppercase();
        let pos = self.index.shift_remove(&key)?;
        let removed = self.entries.remove(pos);
        // Patch indices of entries after the removed slot.
        for value in self.index.values_mut() {
            if *value > pos {
                *value -= 1;
            }
        }
        Some(removed.value)
    }

    // Typed accessors

    /// Parse `name` as `T: FromStr`, returning `None` if absent or unparseable.
    pub fn get_parsed<T: FromStr>(&self, name: &str) -> Option<T> {
        self.get(name)?.trim().parse().ok()
    }

    pub fn get_i32(&self, name: &str) -> Option<i32> {
        self.get_parsed(name)
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        self.get_parsed(name)
    }

    pub fn get_u32(&self, name: &str) -> Option<u32> {
        self.get_parsed(name)
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        self.get_parsed(name)
    }

    /// Parse a `T`/`F` or `TRUE`/`FALSE` value (case-insensitive). Missing keys
    /// resolve to `false` to match Altium's "absent means default" convention.
    pub fn get_bool(&self, name: &str) -> bool {
        match self.get(name) {
            None => false,
            Some(s) => parse_bool(s),
        }
    }

    pub fn get_coord(&self, name: &str) -> Option<Coord> {
        self.get(name)?.parse::<Coord>().ok()
    }
}

/// Parse the Altium boolean encoding: empty/absent is `false`,
/// `T`/`t`/`TRUE`/`true` is `true`, anything else is `false`.
pub fn parse_bool(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if t.len() == 1 {
        return matches!(t.as_bytes()[0], b'T' | b't');
    }
    t.eq_ignore_ascii_case("TRUE")
}

/// Format the Altium boolean encoding using the long form (`TRUE`/`FALSE`).
pub fn format_bool_long(b: bool) -> &'static str {
    if b { "TRUE" } else { "FALSE" }
}

/// Format the Altium boolean encoding using the short form (`T`/`F`).
pub fn format_bool_short(b: bool) -> &'static str {
    if b { "T" } else { "F" }
}

/// Render a parameter map into a textual block.
///
/// Each entry is preceded by `sep`, `%UTF8%`-flagged entries get the prefix
/// re-attached. Pass [`SEP_TOP`] for the standard `|`-separated form.
pub fn write_block(map: &ParameterMap, sep: char) -> String {
    let mut s = String::new();
    write_block_into(&mut s, map, sep);
    s
}

/// As [`write_block`], but appends to an existing buffer.
pub fn write_block_into(buf: &mut String, map: &ParameterMap, sep: char) {
    use std::fmt::Write;
    for entry in &map.entries {
        buf.push(sep);
        if entry.is_utf8 {
            buf.push_str(UTF8_PREFIX);
        }
        buf.push_str(&entry.name);
        buf.push('=');
        let _ = write!(buf, "{}", entry.value);
    }
}

/// Render a parameter map directly to bytes, choosing the encoding per entry:
/// `%UTF8%`-flagged values are emitted as a UTF-8 copy plus the plain twin
/// Altium expects (see [`ParameterMap::parse_bytes`]), everything else goes
/// through Windows-1252 (with `&#NNNN;` numeric-reference fallback for any
/// stray non-Latin-1 chars). This is the byte-correct path for property
/// blocks that mix latin-1 keys with international values.
pub fn write_block_bytes(buf: &mut Vec<u8>, map: &ParameterMap, sep: char) {
    let mut sep_buf = [0u8; 4];
    let sep_bytes = sep.encode_utf8(&mut sep_buf).as_bytes();
    for entry in &map.entries {
        buf.extend_from_slice(sep_bytes);
        if entry.is_utf8 {
            // Altium's layout: the UTF-8 copy, two empty entries, then a plain
            // twin carrying the same bytes under the bare key.
            let name = crate::encoding::encode(&entry.name);
            buf.extend_from_slice(UTF8_PREFIX.as_bytes());
            buf.extend_from_slice(&name);
            buf.push(b'=');
            buf.extend_from_slice(entry.value.as_bytes());
            for _ in 0..3 {
                buf.extend_from_slice(sep_bytes);
            }
            buf.extend_from_slice(&name);
            buf.push(b'=');
            buf.extend_from_slice(entry.value.as_bytes());
        } else {
            buf.extend_from_slice(&crate::encoding::encode(&entry.name));
            buf.push(b'=');
            buf.extend_from_slice(&crate::encoding::encode(&entry.value));
        }
    }
}

impl fmt::Display for ParameterMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in &self.entries {
            f.write_str("|")?;
            if entry.is_utf8 {
                f.write_str(UTF8_PREFIX)?;
            }
            f.write_str(&entry.name)?;
            f.write_str("=")?;
            f.write_str(&entry.value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterates_basic_pairs() {
        let raw = "|FOO=bar|BAZ=42|EMPTY=";
        let entries: Vec<_> = Parameters::new(raw).collect();
        assert_eq!(entries.len(), 3);
        assert_eq!((entries[0].name, entries[0].value), ("FOO", "bar"));
        assert_eq!((entries[1].name, entries[1].value), ("BAZ", "42"));
        assert_eq!((entries[2].name, entries[2].value), ("EMPTY", ""));
    }

    #[test]
    fn handles_no_leading_separator() {
        let entries: Vec<_> = Parameters::new("FOO=1|BAR=2").collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "FOO");
        assert_eq!(entries[1].name, "BAR");
    }

    #[test]
    fn skips_empty_segments() {
        let entries: Vec<_> = Parameters::new("|||FOO=1||BAR=2|||").collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn strips_trailing_crlf() {
        let entries: Vec<_> = Parameters::new("|FOO=bar\r\n").collect();
        assert_eq!(entries[0].value, "bar");
    }

    #[test]
    fn detects_utf8_prefix() {
        let entries: Vec<_> = Parameters::new("|%UTF8%TEXT=hello|TEXT=plain").collect();
        assert_eq!(entries[0].name, "TEXT");
        assert!(entries[0].is_utf8);
        assert_eq!(entries[1].name, "TEXT");
        assert!(!entries[1].is_utf8);
    }

    #[test]
    fn insert_promotes_any_non_ascii_value_to_utf8() {
        let mut map = ParameterMap::new();
        map.insert("ASCII", "plain text");
        // Altium promotes every non-ASCII value, even ones that fit Win-1252:
        // a `-55°C` parameter is stored as `%UTF8%Text=` in real libraries.
        map.insert("EURO", "Costs €5");
        map.insert("LATIN1", "Café résumé");
        map.insert("OMEGA", "value Ω");

        assert!(!map.is_utf8("ASCII"));
        assert!(map.is_utf8("EURO"));
        assert!(map.is_utf8("LATIN1"));
        assert!(map.is_utf8("OMEGA"));
    }

    #[test]
    fn parse_bytes_prefers_utf8_copy_over_plain_twin() {
        let raw = b"|RECORD=41|%UTF8%Text=-55\xc2\xb0C|||Text=-55\xb0C|Name=Temp";
        let map = ParameterMap::parse_bytes(raw, b'|');
        assert_eq!(map.get("Text"), Some("-55°C"));
        assert!(map.is_utf8("Text"));
        assert_eq!(map.get("Name"), Some("Temp"));
        assert_eq!(map.len(), 3);

        // Plain-only legacy values still decode as Windows-1252.
        let plain = ParameterMap::parse_bytes(b"|Text=22\xb5F", b'|');
        assert_eq!(plain.get("Text"), Some("22µF"));
        assert!(!plain.is_utf8("Text"));
    }

    #[test]
    fn write_block_bytes_emits_utf8_copy_and_plain_twin() {
        let mut map = ParameterMap::new();
        map.insert("RECORD", "41");
        map.insert("Text", "100Ω");
        let mut out = Vec::new();
        write_block_bytes(&mut out, &map, '|');
        assert_eq!(
            out,
            b"|RECORD=41|%UTF8%Text=100\xce\xa9|||Text=100\xce\xa9".to_vec()
        );
        let back = ParameterMap::parse_bytes(&out, b'|');
        assert_eq!(back.get("Text"), Some("100Ω"));
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn nested_separator() {
        let entries: Vec<_> = Parameters::nested("`A=1`B=2").collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "A");
    }

    #[test]
    fn map_lookup_case_insensitive() {
        let map = ParameterMap::parse("|FOO=bar|Baz=qux");
        assert_eq!(map.get("foo"), Some("bar"));
        assert_eq!(map.get("FOO"), Some("bar"));
        assert_eq!(map.get("BAZ"), Some("qux"));
    }

    #[test]
    fn map_preserves_order() {
        let map = ParameterMap::parse("|A=1|B=2|C=3");
        let names: Vec<_> = map.iter().map(|(n, _, _)| n).collect();
        assert_eq!(names, ["A", "B", "C"]);
    }

    #[test]
    fn map_insert_replaces_in_place() {
        let mut map = ParameterMap::parse("|A=1|B=2|C=3");
        map.insert("B", "new");
        let names: Vec<_> = map.iter().map(|(n, v, _)| (n, v)).collect();
        assert_eq!(names, [("A", "1"), ("B", "new"), ("C", "3")]);
    }

    #[test]
    fn map_remove_keeps_order() {
        let mut map = ParameterMap::parse("|A=1|B=2|C=3|D=4");
        assert_eq!(map.remove("b"), Some("2".to_owned()));
        let names: Vec<_> = map.iter().map(|(n, _, _)| n).collect();
        assert_eq!(names, ["A", "C", "D"]);
        // Subsequent lookups still work.
        assert_eq!(map.get("D"), Some("4"));
    }

    #[test]
    fn map_round_trip_through_display() {
        let original = "|A=1|B=hello|%UTF8%C=café";
        let map = ParameterMap::parse(original);
        assert_eq!(map.to_string(), original);
    }

    #[test]
    fn map_typed_accessors() {
        let map = ParameterMap::parse("|N=42|F=2.5|YES=T|NO=F|LONG=TRUE|MIL=10mil|MM=2.54mm");
        assert_eq!(map.get_i32("N"), Some(42));
        assert_eq!(map.get_f64("F"), Some(2.5));
        assert!(map.get_bool("YES"));
        assert!(!map.get_bool("NO"));
        assert!(map.get_bool("LONG"));
        assert!(!map.get_bool("MISSING"));
        assert_eq!(map.get_coord("MIL"), Some(Coord::from_mils(10.0)));
        assert_eq!(map.get_coord("MM"), Some(Coord::from_mm(2.54)));
    }

    #[test]
    fn parse_bool_helpers() {
        assert!(parse_bool("T"));
        assert!(parse_bool("t"));
        assert!(parse_bool("TRUE"));
        assert!(parse_bool("True"));
        assert!(!parse_bool("F"));
        assert!(!parse_bool("FALSE"));
        assert!(!parse_bool(""));
        assert!(!parse_bool("anything"));
    }

    #[test]
    fn write_block_round_trip() {
        let map = ParameterMap::parse("|A=1|B=hi|%UTF8%C=café");
        let written = write_block(&map, '|');
        assert_eq!(written, "|A=1|B=hi|%UTF8%C=café");
    }
}
