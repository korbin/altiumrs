//! Data-transfer objects: one struct per on-disk record kind.

pub mod pcb;
pub mod sch;

#[cfg(test)]
mod tests {
    use crate::parameter::ParameterMap;
    use crate::{AltiumRecord, Coord};

    #[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
    #[altium(record = "test-record")]
    struct Sample {
        #[altium(name = "OWNERINDEX")]
        owner_index: i32,
        #[altium(name = "FLAG")]
        flag: bool,
        #[altium(name = "ANGLE")]
        angle: f64,
        #[altium(name = "LABEL")]
        label: String,
        #[altium(name = "NAME")]
        name: Option<String>,
        #[altium(name = "WIDTH")]
        width: Coord,
    }

    #[test]
    fn record_const_emitted() {
        assert_eq!(Sample::RECORD, "test-record");
    }

    #[test]
    fn from_params_reads_present_fields() {
        let mut params = ParameterMap::new();
        params.insert("OWNERINDEX", "42");
        params.insert("FLAG", "T");
        params.insert("ANGLE", "12.5");
        params.insert("LABEL", "hello");
        params.insert("NAME", "Pin1");
        params.insert("WIDTH", "10mil");

        let s = Sample::from_params(&params);
        assert_eq!(s.owner_index, 42);
        assert!(s.flag);
        assert_eq!(s.angle, 12.5);
        assert_eq!(s.label, "hello");
        assert_eq!(s.name.as_deref(), Some("Pin1"));
        assert_eq!(s.width, Coord::from_mils(10.0));
    }

    #[test]
    fn from_params_uses_defaults_for_missing() {
        let params = ParameterMap::new();
        let s = Sample::from_params(&params);
        assert_eq!(s, Sample::default());
    }

    #[test]
    fn to_params_skips_defaults() {
        let s = Sample::default();
        let mut params = ParameterMap::new();
        s.to_params(&mut params);
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn to_params_emits_set_fields() {
        let s = Sample {
            owner_index: 7,
            flag: true,
            angle: 1.5,
            label: "abc".to_string(),
            name: Some("xyz".to_string()),
            width: Coord::from_mils(5.0),
        };
        let mut params = ParameterMap::new();
        s.to_params(&mut params);

        assert_eq!(params.get("OWNERINDEX"), Some("7"));
        assert_eq!(params.get("FLAG"), Some("T"));
        assert_eq!(params.get("ANGLE"), Some("1.5"));
        assert_eq!(params.get("LABEL"), Some("abc"));
        assert_eq!(params.get("NAME"), Some("xyz"));
        assert_eq!(params.get("WIDTH"), Some("5mil"));
    }

    #[test]
    fn round_trip() {
        let original = Sample {
            owner_index: -3,
            flag: false,
            angle: 2.5,
            label: String::new(),
            name: Some("foo".to_string()),
            width: Coord::ZERO,
        };
        let mut params = ParameterMap::new();
        original.to_params(&mut params);
        let parsed = Sample::from_params(&params);
        assert_eq!(parsed, original);
    }
}
