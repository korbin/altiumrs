//! `#[derive(AltiumRecord)]`: generates `from_params` and `to_params`
//! methods that round-trip a struct through a `ParameterMap`.

use proc_macro::TokenStream;

mod codegen;
mod input;

/// Derive `from_params` / `to_params` for a struct of named fields.
///
/// Each field is read from the parameter map by its uppercased field name,
/// or by an explicit `#[altium(name = "...")]`. The field type drives the
/// decoding strategy:
///
/// - `bool` reads `T`/`TRUE` and writes `T`.
/// - integer types (`i8`..`i64`, `u8`..`u32`) parse + format with `to_string`.
/// - `f32` / `f64` parse + format with the standard Rust formatter.
/// - `String` and `Option<String>` round-trip the literal text.
/// - `Coord` uses `Coord::parse_altium` / `Display`.
/// - Anything else is assumed to be `TryFrom<i32>` and `Into<i32>` (i.e. a
///   `repr(i32)` enum).
///
/// On write, defaults are omitted: `0`, `false`, empty `String`, `None`,
/// `Coord::ZERO`, and zero-valued enums are not emitted, matching how Altium
/// rounds out absent parameters as defaults.
///
/// Per-struct attributes:
/// - `#[altium(record = "12")]` exposes `Self::RECORD: &'static str = "12"`.
/// - `#[altium(crate = path::to::altium)]` overrides the path the macro uses
///   to refer back to the `altium` crate (defaults to `::altium`). Set this
///   when the crate has been renamed in `Cargo.toml` or re-exported from a
///   parent crate.
///
/// Per-field attributes:
/// - `#[altium(name = "LOCATION.X")]` overrides the default key.
/// - `#[altium(skip)]` skips the field on both read and write (uses
///   `Default::default()` on read).
#[proc_macro_derive(AltiumRecord, attributes(altium))]
pub fn derive_altium_record(input: TokenStream) -> TokenStream {
    let parsed = match input::parse(input.into()) {
        Ok(p) => p,
        Err(err) => return err.to_compile_error().into(),
    };
    codegen::expand(&parsed).into()
}
