//! Verifies `#[altium(crate = ...)]` overrides the default `::altium` path.

mod altium_alias {
    pub use altium::*;
}

use altium::AltiumRecord;

#[derive(Default, AltiumRecord)]
#[altium(record = "99", crate = crate::altium_alias)]
struct AliasedRecord {
    name: String,
    count: i32,
}

#[test]
fn derive_uses_crate_override() {
    assert_eq!(AliasedRecord::RECORD, "99");

    let mut params = altium::parameter::ParameterMap::new();
    params.insert("NAME", "thingy");
    params.insert("COUNT", "42");

    let r = AliasedRecord::from_params(&params);
    assert_eq!(r.name, "thingy");
    assert_eq!(r.count, 42);

    let mut out = altium::parameter::ParameterMap::new();
    r.to_params(&mut out);
    assert_eq!(out.get("NAME"), Some("thingy"));
    assert_eq!(out.get("COUNT"), Some("42"));
}
