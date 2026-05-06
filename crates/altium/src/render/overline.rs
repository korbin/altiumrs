//! Parse Altium's overline-marker syntax.
//!
//! - A backslash `\` after a character marks **that character** as overlined.
//! - Consecutive overlined characters render as a continuous bar.
//!
//! ```text
//! "Reset"      → "Reset"
//! "RESET\"     → "RESET" (last char overlined)
//! "R\E\S\E\T\" → "RESET" (every char overlined → one continuous bar)
//! "R\ESET"     → "R" overlined, "ESET" plain
//! ```

/// One run of identically-decorated characters.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub overline: bool,
}

/// Parse `input` into runs of `(text, overline)`. Adjacent runs always have
/// different `overline` values.
pub fn parse(input: &str) -> Vec<Segment> {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();

    let mut decorated = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '\\' {
            i += 1;
            continue;
        }
        let overlined = i + 1 < n && chars[i + 1] == '\\';
        decorated.push((c, overlined));
        i += 1;
    }

    let mut out: Vec<Segment> = Vec::new();
    for (c, ol) in decorated {
        match out.last_mut() {
            Some(s) if s.overline == ol => s.text.push(c),
            _ => out.push(Segment {
                text: c.to_string(),
                overline: ol,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(input: &str) -> Vec<(String, bool)> {
        parse(input)
            .into_iter()
            .map(|s| (s.text, s.overline))
            .collect()
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(segs("Reset"), vec![("Reset".into(), false)]);
    }

    #[test]
    fn last_char_overlined() {
        assert_eq!(
            segs("RESET\\"),
            vec![("RESE".into(), false), ("T".into(), true),]
        );
    }

    #[test]
    fn fully_overlined() {
        // Each char followed by `\` → all chars overlined → single segment.
        assert_eq!(segs("R\\E\\S\\E\\T\\"), vec![("RESET".into(), true)]);
    }

    #[test]
    fn first_char_overlined() {
        assert_eq!(
            segs("R\\ESET"),
            vec![("R".into(), true), ("ESET".into(), false),]
        );
    }

    #[test]
    fn empty() {
        assert_eq!(segs(""), Vec::<(String, bool)>::new());
    }

    #[test]
    fn dangling_backslash() {
        assert_eq!(segs("\\"), Vec::<(String, bool)>::new());
    }
}
