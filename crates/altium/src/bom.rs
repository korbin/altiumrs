//! Bill-of-materials documents (`.BomDoc`).
//!
//! Plain-text, Windows-1252 / CRLF, one record per line. Each line is a pipe-
//! delimited list of `KEY=VALUE` pairs whose first pair is `RECORD=<Kind>`.
//! Within `CatalogItem` records, `COMPONENTPARAMETERS` carries a CSV-style
//! list of `Key=Value` entries (double-quoted when an entry contains a comma
//! or an embedded `=`).
//!
//! Round-trip: parse → mutate → re-emit. Unknown record kinds are preserved
//! verbatim via the generic [`BomRecord`].

use std::path::Path;

use indexmap::IndexMap;

use crate::encoding;
use crate::error::{Error, Result};
use crate::parameter::ParameterMap;

/// `RECORD=` values for record kinds we surface as typed views.
pub mod kinds {
    pub const BOM: &str = "BOM";
    pub const GENERAL_OPTIONS: &str = "GeneralOptions";
    pub const CATALOG_ITEM: &str = "CatalogItem";
}

/// One line of the document. Generic — every record kind is stored the same
/// way, then projected through typed views ([`BomHeader`], [`BomItem`], …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomRecord {
    /// `RECORD=` value (e.g. `"BOM"`, `"CatalogItem"`, `"GeneralOptions"`).
    pub kind: String,
    /// Remaining `|KEY=VALUE` pairs in insertion order.
    pub parameters: ParameterMap,
}

impl BomRecord {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            parameters: ParameterMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.parameters.get(key)
    }

    pub fn get_or_empty(&self, key: &str) -> &str {
        self.parameters.get(key).unwrap_or("")
    }
}

/// Top-level BomDoc.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BomDocument {
    pub records: Vec<BomRecord>,
}

impl BomDocument {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::parse(&encoding::decode(&bytes))
    }

    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut records = Vec::new();
        for (lineno, raw_line) in text.split_inclusive('\n').enumerate() {
            let line = raw_line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                continue;
            }
            let params = ParameterMap::parse(line);
            let Some(kind) = params.get("RECORD") else {
                return Err(Error::corrupt(format!(
                    "BomDoc line {} missing RECORD= prefix",
                    lineno + 1
                )));
            };
            let kind = kind.to_string();
            // Re-emit without the leading RECORD= so .parameters carries only
            // the remaining fields. Keeps round-trip clean.
            let mut without_record = ParameterMap::new();
            for (name, value, is_utf8) in params.iter() {
                if name.eq_ignore_ascii_case("RECORD") {
                    continue;
                }
                if is_utf8 {
                    without_record.insert_utf8(name, value);
                } else {
                    without_record.insert(name, value);
                }
            }
            records.push(BomRecord {
                kind,
                parameters: without_record,
            });
        }
        Ok(Self { records })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for rec in &self.records {
            out.push(b'|');
            out.extend_from_slice(b"RECORD=");
            out.extend_from_slice(&encoding::encode(&rec.kind));
            crate::parameter::write_block_bytes(&mut out, &rec.parameters, '|');
            out.extend_from_slice(b"\r\n");
        }
        Ok(out)
    }

    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.to_bytes()?).await?;
        Ok(())
    }

    /// The `BOM` header record (version, filename, currency, …).
    pub fn header(&self) -> Option<BomHeader<'_>> {
        self.records
            .iter()
            .find(|r| r.kind.eq_ignore_ascii_case(kinds::BOM))
            .map(|r| BomHeader { record: r })
    }

    /// The `GeneralOptions` record.
    pub fn general_options(&self) -> Option<&BomRecord> {
        self.records
            .iter()
            .find(|r| r.kind.eq_ignore_ascii_case(kinds::GENERAL_OPTIONS))
    }

    /// Iterator over every `CatalogItem` record, each surfaced as a typed [`BomItem`].
    pub fn items(&self) -> impl Iterator<Item = BomItem<'_>> {
        self.records
            .iter()
            .filter(|r| r.kind.eq_ignore_ascii_case(kinds::CATALOG_ITEM))
            .map(|r| BomItem { record: r })
    }

    /// Count of `CatalogItem` records.
    pub fn item_count(&self) -> usize {
        self.items().count()
    }
}

/// Typed view over the `BOM` header record.
#[derive(Debug, Clone, Copy)]
pub struct BomHeader<'a> {
    pub record: &'a BomRecord,
}

impl<'a> BomHeader<'a> {
    pub fn version(&self) -> Option<i32> {
        self.record.parameters.get_i32("VERSION")
    }
    pub fn filename(&self) -> &str {
        self.record.get_or_empty("FILENAME")
    }
    pub fn kind(&self) -> &str {
        self.record.get_or_empty("KIND")
    }
    pub fn date(&self) -> &str {
        self.record.get_or_empty("DATE")
    }
    pub fn time(&self) -> &str {
        self.record.get_or_empty("TIME")
    }
    pub fn currency(&self) -> &str {
        self.record.get_or_empty("CURRENCY")
    }
    pub fn production_quantity(&self) -> Option<i32> {
        self.record.parameters.get_i32("PRODUCTIONQUANTITY")
    }
}

/// Typed view over a `CatalogItem` record.
#[derive(Debug, Clone, Copy)]
pub struct BomItem<'a> {
    pub record: &'a BomRecord,
}

impl<'a> BomItem<'a> {
    pub fn item_type(&self) -> &str {
        self.record.get_or_empty("ITEMTYPE")
    }
    pub fn unique_id(&self) -> &str {
        self.record.get_or_empty("UNIQUEID")
    }
    pub fn design_item_id(&self) -> &str {
        self.record.get_or_empty("DESIGNITEMID")
    }
    pub fn item_source(&self) -> &str {
        self.record.get_or_empty("ITEMSOURCE")
    }
    pub fn status(&self) -> &str {
        self.record.get_or_empty("STATUS")
    }
    pub fn description(&self) -> &str {
        self.record.get_or_empty("DESCRIPTION")
    }
    pub fn user_comments(&self) -> &str {
        self.record.get_or_empty("USERCOMMENTS")
    }
    pub fn line_number(&self) -> &str {
        self.record.get_or_empty("LINENUMBER")
    }

    /// Parse the CSV-style `COMPONENTPARAMETERS` blob into an ordered map.
    pub fn component_parameters(&self) -> IndexMap<String, String> {
        parse_csv_params(self.record.get_or_empty("COMPONENTPARAMETERS"))
    }

    /// Parse the CSV-style `CUSTOMPARAMETERS` blob into an ordered map.
    pub fn custom_parameters(&self) -> IndexMap<String, String> {
        parse_csv_params(self.record.get_or_empty("CUSTOMPARAMETERS"))
    }

    /// Convenience: `COMPONENTPARAMETERS["Footprint"]`.
    pub fn footprint(&self) -> Option<String> {
        component_param(self.record.get_or_empty("COMPONENTPARAMETERS"), "Footprint")
    }

    pub fn comment(&self) -> Option<String> {
        component_param(self.record.get_or_empty("COMPONENTPARAMETERS"), "Comment")
    }

    pub fn manufacturer(&self) -> Option<String> {
        component_param(
            self.record.get_or_empty("COMPONENTPARAMETERS"),
            "Manufacturer",
        )
    }

    pub fn manufacturer_part_number(&self) -> Option<String> {
        component_param(
            self.record.get_or_empty("COMPONENTPARAMETERS"),
            "Manufacturer Part Number",
        )
    }

    pub fn supplier(&self) -> Option<String> {
        component_param(self.record.get_or_empty("COMPONENTPARAMETERS"), "Supplier")
    }

    pub fn supplier_part_number(&self) -> Option<String> {
        component_param(
            self.record.get_or_empty("COMPONENTPARAMETERS"),
            "Supplier Part Number",
        )
    }

    pub fn value(&self) -> Option<String> {
        component_param(self.record.get_or_empty("COMPONENTPARAMETERS"), "Value")
    }

    pub fn library_reference(&self) -> Option<String> {
        component_param(
            self.record.get_or_empty("COMPONENTPARAMETERS"),
            "Library Reference",
        )
    }
}

// CSV-style entry parser for COMPONENTPARAMETERS / CUSTOMPARAMETERS.
//
// Tokens are comma-separated, optionally wrapped in `"…"`. Inside quotes,
// `""` escapes a literal `"`. Each token is then split on the first `=` into
// (key, value).
fn parse_csv_params(s: &str) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    for token in csv_tokens(s) {
        match token.split_once('=') {
            Some((k, v)) => {
                out.insert(k.to_string(), v.to_string());
            }
            None if !token.is_empty() => {
                out.insert(token, String::new());
            }
            None => {}
        }
    }
    out
}

fn component_param(s: &str, key: &str) -> Option<String> {
    for token in csv_tokens(s) {
        if let Some((k, v)) = token.split_once('=') {
            if k.eq_ignore_ascii_case(key) {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn csv_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        // Skip a leading separator only at the very start of the next token.
        let mut entry = String::new();
        match chars.peek() {
            None => break,
            Some('"') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        if chars.peek() == Some(&'"') {
                            entry.push('"');
                            chars.next();
                        } else {
                            break;
                        }
                    } else {
                        entry.push(c);
                    }
                }
                // Consume the separator after the closing quote (if any).
                if chars.peek() == Some(&',') {
                    chars.next();
                }
            }
            Some(_) => {
                while let Some(&c) = chars.peek() {
                    if c == ',' {
                        chars.next();
                        break;
                    }
                    entry.push(c);
                    chars.next();
                }
            }
        }
        out.push(entry);
    }
    out
}

/// Re-emit a CSV-style parameter block. Tokens are wrapped in quotes when the
/// `Key=Value` contains a comma, `=` after the first split, or a `"`.
pub fn encode_csv_params(entries: &IndexMap<String, String>) -> String {
    let mut out = String::new();
    let mut first = true;
    for (k, v) in entries {
        if !first {
            out.push(',');
        }
        first = false;
        let token = format!("{k}={v}");
        if needs_quoting(&token) {
            out.push('"');
            for c in token.chars() {
                if c == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(c);
                }
            }
            out.push('"');
        } else {
            out.push_str(&token);
        }
    }
    out
}

fn needs_quoting(token: &str) -> bool {
    // Altium quotes tokens containing a comma, double-quote, or whitespace.
    // Extra `=` signs (common in URLs like `?src=…&k=v`) are left unquoted.
    token.chars().any(|c| matches!(c, ',' | '"') || c.is_whitespace())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_items() {
        let text = "|RECORD=BOM|VERSION=6|KIND=ALTIUM_DESIGNER_LIVEBOM|CURRENCY=USD\r\n\
                    |RECORD=GeneralOptions|OPENEXPORTED=False\r\n\
                    |RECORD=CatalogItem|UNIQUEID=Lib\\R1|DESIGNITEMID=R1|DESCRIPTION=10k 0402|USERCOMMENTS=10k|COMPONENTPARAMETERS=Comment=10k,\"Component Kind=Standard\",Footprint=R0402|CUSTOMPARAMETERS=\r\n";
        let doc = BomDocument::parse(text).unwrap();
        assert_eq!(doc.records.len(), 3);

        let h = doc.header().unwrap();
        assert_eq!(h.version(), Some(6));
        assert_eq!(h.kind(), "ALTIUM_DESIGNER_LIVEBOM");
        assert_eq!(h.currency(), "USD");

        assert!(doc.general_options().is_some());

        let items: Vec<_> = doc.items().collect();
        assert_eq!(items.len(), 1);
        let it = items[0];
        assert_eq!(it.unique_id(), "Lib\\R1");
        assert_eq!(it.design_item_id(), "R1");
        assert_eq!(it.description(), "10k 0402");
        assert_eq!(it.user_comments(), "10k");
        assert_eq!(it.footprint().as_deref(), Some("R0402"));
        assert_eq!(it.comment().as_deref(), Some("10k"));
        let params = it.component_parameters();
        assert_eq!(params.get("Component Kind").map(String::as_str), Some("Standard"));
    }

    #[test]
    fn round_trip_through_bytes() {
        let text = "|RECORD=BOM|VERSION=6|KIND=ALTIUM_DESIGNER_LIVEBOM\r\n\
                    |RECORD=CatalogItem|UNIQUEID=Lib\\R1|DESCRIPTION=res|COMPONENTPARAMETERS=Comment=10k,\"Component Kind=Standard\"|CUSTOMPARAMETERS=\r\n";
        let doc = BomDocument::parse(text).unwrap();
        let bytes = doc.to_bytes().unwrap();
        let back = BomDocument::from_bytes(bytes).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn csv_tokens_handles_quotes_and_commas() {
        let tokens = csv_tokens("Comment=10k,\"Component Kind=Standard\",Footprint=R0402");
        assert_eq!(
            tokens,
            vec!["Comment=10k", "Component Kind=Standard", "Footprint=R0402"]
        );
    }

    #[test]
    fn csv_tokens_handles_escaped_quote() {
        let tokens = csv_tokens("\"Quoted \"\"name\"\"=value\"");
        assert_eq!(tokens, vec!["Quoted \"name\"=value"]);
    }

    #[test]
    fn empty_input_parses_to_empty_doc() {
        let doc = BomDocument::parse("").unwrap();
        assert!(doc.records.is_empty());
    }

    #[test]
    fn needs_quoting_triggers_on_comma_quote_and_whitespace() {
        assert!(needs_quoting("Key=val,ue"));
        assert!(needs_quoting("Component Kind=Standard"));
        assert!(!needs_quoting("Footprint=R0402"));
        assert!(needs_quoting("Quoted \"hello\"=v"));
        // URL-style values with extra `=` are not quoted in real Altium output.
        assert!(!needs_quoting(
            "ComponentLink1URL=https://example.com/x?src-supplier=Foo"
        ));
    }

    #[test]
    fn encode_csv_params_round_trip() {
        let mut m = IndexMap::new();
        m.insert("Comment".to_string(), "10k".to_string());
        m.insert("Component Kind".to_string(), "Standard".to_string());
        m.insert("Footprint".to_string(), "R0402".to_string());
        let encoded = encode_csv_params(&m);
        assert_eq!(
            encoded,
            "Comment=10k,\"Component Kind=Standard\",Footprint=R0402"
        );
        let parsed = parse_csv_params(&encoded);
        assert_eq!(parsed, m);
    }

    #[test]
    fn item_typed_accessors_pull_from_component_parameters() {
        let text = "|RECORD=CatalogItem|UNIQUEID=X|COMPONENTPARAMETERS=\"Manufacturer=ACME\",\"Manufacturer Part Number=PN-1\",\"Supplier=Mouser\",\"Supplier Part Number=SP-1\",Value=10k,\"Library Reference=R10K\"|\r\n";
        let doc = BomDocument::parse(text).unwrap();
        let it = doc.items().next().unwrap();
        assert_eq!(it.manufacturer().as_deref(), Some("ACME"));
        assert_eq!(it.manufacturer_part_number().as_deref(), Some("PN-1"));
        assert_eq!(it.supplier().as_deref(), Some("Mouser"));
        assert_eq!(it.supplier_part_number().as_deref(), Some("SP-1"));
        assert_eq!(it.value().as_deref(), Some("10k"));
        assert_eq!(it.library_reference().as_deref(), Some("R10K"));
    }
}
