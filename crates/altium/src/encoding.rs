//! Windows-1252 encode / decode helpers.
//!
//! [`encode`] is **lossy for non-Latin-1 text**: characters outside
//! Windows-1252 become numeric character references (e.g. `Ω` → `&#937;`).
//! Use [`fits_in_win1252`] to detect non-Latin-1 input ahead of time, or
//! [`try_encode_strict`] to encode-or-fail.
//! [`crate::parameter::ParameterMap::insert`] auto-promotes such values to
//! the `%UTF8%`-prefixed UTF-8 path.

use encoding_rs::WINDOWS_1252;

/// Decode a Windows-1252 byte slice into an owned `String`. Always succeeds —
/// the WHATWG mapping is total over all 256 byte values.
pub fn decode(bytes: &[u8]) -> String {
    let (cow, _, _) = WINDOWS_1252.decode(bytes);
    cow.into_owned()
}

/// Encode `s` as Windows-1252, falling back to numeric character references
/// (`&#NNNN;`) for characters outside the Windows-1252 repertoire.
///
/// **This is lossy for non-Latin-1 input.** Prefer [`try_encode_strict`] when
/// you want to surface the loss as an error, or [`fits_in_win1252`] to decide
/// up-front whether to take the `%UTF8%` path instead.
pub fn encode(s: &str) -> Vec<u8> {
    let (cow, _, _) = WINDOWS_1252.encode(s);
    cow.into_owned()
}

/// Whether every codepoint in `s` is representable as a single byte in
/// Windows-1252 (i.e., is in the `U+0000..=U+00FF` range, except for the
/// five C1 control codes that map to displayable CP1252 glyphs — those are
/// also representable). In practice this is the predicate "would [`encode`]
/// be lossless on this input?".
pub fn fits_in_win1252(s: &str) -> bool {
    // The WHATWG encoder maps every Latin-1 codepoint (U+0000..U+00FF) plus
    // a handful of higher codepoints that occupy the 0x80..0x9F slots in
    // Windows-1252 (Euro sign, smart quotes, etc.). Asking the encoder
    // itself is the only way to be exactly right.
    let (_, _, had_errors) = WINDOWS_1252.encode(s);
    !had_errors
}

/// Lossless variant of [`encode`]: returns `Err(EncodeError)` if any
/// character would have to be replaced with a numeric character reference.
pub fn try_encode_strict(s: &str) -> Result<Vec<u8>, EncodeError> {
    if fits_in_win1252(s) {
        Ok(encode(s))
    } else {
        Err(EncodeError {
            offending: s.chars().find(|&c| !is_single_char_win1252(c)),
        })
    }
}

fn is_single_char_win1252(c: char) -> bool {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    fits_in_win1252(s)
}

/// Returned by [`try_encode_strict`] when the input contains characters that
/// don't fit in Windows-1252.
#[derive(Debug, thiserror::Error)]
#[error(
    "string contains characters outside the Windows-1252 repertoire{}",
    offending.map(|c| format!(" (first offender: {c:?})")).unwrap_or_default()
)]
pub struct EncodeError {
    /// First character that couldn't be encoded losslessly, if known.
    pub offending: Option<char>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ascii() {
        let s = "Hello, world!";
        assert_eq!(decode(&encode(s)), s);
    }

    #[test]
    fn round_trip_latin1() {
        let s = "Café résumé naïve";
        assert_eq!(decode(&encode(s)), s);
    }

    #[test]
    fn omega_falls_back_to_numeric_ref() {
        // U+03A9 (Greek capital omega) isn't in Windows-1252.
        let bytes = encode("Ω");
        assert_eq!(bytes, b"&#937;");
    }

    #[test]
    fn cp1252_undefined_bytes_round_trip_to_c1_control_codes() {
        // The WHATWG mapping is total: 0x81/0x8D/0x8F/0x90/0x9D ↔
        // U+0081/U+008D/U+008F/U+0090/U+009D. Verify decode never produces
        // U+FFFD here, and encode round-trips cleanly.
        let bytes = vec![0x81, 0x8D, 0x8F, 0x90, 0x9D];
        let decoded = decode(&bytes);
        assert!(
            !decoded.contains('\u{FFFD}'),
            "decode should never produce U+FFFD for any single byte"
        );
        assert_eq!(encode(&decoded), bytes, "C1 round-trip");
    }

    #[test]
    fn fits_in_win1252_predicate() {
        assert!(fits_in_win1252(""));
        assert!(fits_in_win1252("plain ascii"));
        assert!(fits_in_win1252("Café"));
        assert!(fits_in_win1252("€")); // Win-1252 0x80
        assert!(fits_in_win1252("\u{0081}")); // C1 control mapped to 0x81
        assert!(!fits_in_win1252("Ω"));
        assert!(!fits_in_win1252("naïve Ω test"));
    }

    #[test]
    fn try_encode_strict_succeeds_on_latin1() {
        let bytes = try_encode_strict("Café").expect("Latin-1 encodes cleanly");
        assert_eq!(decode(&bytes), "Café");
    }

    #[test]
    fn try_encode_strict_fails_on_non_latin1() {
        let err = try_encode_strict("Hello Ω world").expect_err("must reject");
        assert_eq!(err.offending, Some('Ω'));
        // The error message names the offender so callers don't have to dig.
        assert!(err.to_string().contains("Ω"), "got: {err}");
    }
}
