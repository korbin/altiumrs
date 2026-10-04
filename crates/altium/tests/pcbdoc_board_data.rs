//! Board-level data of a real `.PcbDoc`, read through `ALTIUM_TEST_PCBDOC`
//! (a board outside the tree; the test passes where the variable is unset):
//! the V9 layer stack, the outline, pad expansion modes, the view
//! configurations, the Pad/Via Library and the shape-based regions.

use altium::MaskExpansionMode;
use altium::pcb::{self, StackLayerKind, ViaTemplateLink};

fn board() -> Option<pcb::Document> {
    let path = std::env::var("ALTIUM_TEST_PCBDOC").ok()?;
    let bytes = std::fs::read(path).ok()?;
    Some(pcb::Document::from_bytes(bytes).expect("the board reads"))
}

#[test]
fn a_real_boards_board_data_reads() {
    let Some(doc) = board() else {
        println!("ALTIUM_TEST_PCBDOC names no board");
        return;
    };
    let stack = doc.layer_stack().expect("a stack");
    let coppers = stack
        .layers
        .iter()
        .filter(|layer| layer.kind == StackLayerKind::Copper)
        .count();
    println!("stack: {} layers, {coppers} copper", stack.layers.len());
    for layer in &stack.layers {
        println!(
            "  {:?} {:<16} id {:#x} cop {:?} diel {:?} {} used {:?}",
            layer.kind,
            layer.name,
            layer.layer_id,
            layer.copper_thickness.map(|c| c.to_mm()),
            layer.dielectric_height.map(|c| c.to_mm()),
            layer.dielectric_material,
            layer.used_by_primitives
        );
    }
    assert!(coppers >= 2, "a stack with its coppers");
    assert!(
        stack
            .layers
            .iter()
            .all(|layer| layer.kind != StackLayerKind::Copper || layer.copper_thickness.is_some())
    );

    let outline = doc
        .board_outline()
        .expect("the outline parses")
        .expect("an outline");
    let arcs = outline
        .iter()
        .filter(|v| v.kind == pcb::VertexKind::Arc)
        .count();
    println!("outline: {} vertices, {arcs} arcs", outline.len());

    let mut modes = std::collections::BTreeMap::new();
    for pad in &doc.pads {
        *modes
            .entry((
                format!("{:?}", pad.paste_mask_expansion_mode),
                format!("{:?}", pad.solder_mask_expansion_mode),
            ))
            .or_insert(0) += 1;
    }
    println!("pad (paste, mask) modes: {modes:?}");
    assert!(
        doc.pads
            .iter()
            .all(|pad| !matches!(pad.paste_mask_expansion_mode, MaskExpansionMode::Unknown(_)))
    );

    if let Some(three) = doc.view_config_3d() {
        println!(
            "3D: system colours {:?}; mask top {:?} bottom {:?}; core {:?}; copper {:?}; silk {:?}; show silk {:?}/{:?}",
            three.use_system_colors,
            three.top_solder_mask,
            three.bottom_solder_mask,
            three.board_core,
            three.copper,
            three.top_silkscreen,
            three.show_top_silkscreen,
            three.show_bottom_silkscreen
        );
    }
    if let Some(two) = doc.view_config_2d() {
        println!(
            "2D: {} layers stated, {} hidden",
            two.layers_shown.len(),
            two.layers_shown.values().filter(|shown| !**shown).count()
        );
    }

    let libraries = doc.pad_via_libraries();
    println!(
        "pad/via library: {} template(s), {} link(s), {} cached librar(ies)",
        libraries.templates.len(),
        libraries.links.len(),
        libraries.cached.len()
    );
    for template in &libraries.templates {
        let via = template.via.as_ref();
        println!(
            "  {} {} structure {:?} ({:?}) features {}",
            template.id,
            template.name,
            via.and_then(|v| v.structure),
            via.and_then(|v| v.structure).map(|s| s.to_string()),
            via.map_or(0, |v| v.features.len())
        );
    }
    let mut resolved = std::collections::BTreeMap::new();
    for index in 0..doc.vias.len() {
        let key = match doc.via_template(&libraries, index) {
            ViaTemplateLink::None => "no template".to_string(),
            ViaTemplateLink::Resolved(template) => format!(
                "{} {:?}",
                template.name,
                template.via.as_ref().and_then(|v| v.structure)
            ),
            ViaTemplateLink::Unresolved { template_id, .. } => format!("unresolved {template_id}"),
        };
        *resolved.entry(key).or_insert(0) += 1;
    }
    println!("vias by template: {resolved:?}");
    // A linked via's own record names the template its link does.
    for link in libraries.links.iter().filter(|link| link.object == "Via") {
        let own = doc.vias[link.primitive_index].template_id.as_deref();
        assert_eq!(
            own.map(str::to_ascii_uppercase),
            Some(link.template_id.to_ascii_uppercase())
        );
    }

    let shaped = doc
        .shape_based_regions()
        .expect("the shape-based regions parse");
    let arcs: usize = shaped
        .iter()
        .map(|r| r.vertices.iter().filter(|v| v.is_round).count())
        .sum();
    println!("shape-based regions: {}, {arcs} arc vertices", shaped.len());
    assert!(
        shaped
            .iter()
            .all(|r| r.vertices.first().map(|v| v.point) == r.vertices.last().map(|v| v.point))
    );
}

#[test]
fn a_real_boards_custom_pads_name_their_pads() {
    let Some(doc) = board() else {
        println!("ALTIUM_TEST_PCBDOC names no board");
        return;
    };
    let linked: Vec<&pcb::Region> = doc.regions.iter().filter(|r| r.pad_ref.is_some()).collect();
    println!("regions that are a pad's shape: {}", linked.len());
    for region in &linked {
        let pad = &doc.pads[region.pad_ref.expect("linked")];
        let (xs, ys): (Vec<i32>, Vec<i32>) = region.outline.iter().map(|p| (p.x.0, p.y.0)).unzip();
        let inside = (xs.iter().min().unwrap()..=xs.iter().max().unwrap()).contains(&&pad.location.x.0)
            && (ys.iter().min().unwrap()..=ys.iter().max().unwrap()).contains(&&pad.location.y.0);
        assert!(inside, "a custom pad's region holds its pad's centre");
        assert_eq!(region.component_index, pad.component_index);
        assert!(
            region
                .additional_parameters
                .as_ref()
                .is_none_or(|extra| !extra.keys().any(|k| k.eq_ignore_ascii_case("PADINDEX")))
        );
    }
    // Written and read again, every link names the same pad.
    let again = pcb::Document::from_bytes(doc.to_bytes().expect("the board writes")).expect("and reads");
    let refs = |d: &pcb::Document| d.regions.iter().map(|r| r.pad_ref).collect::<Vec<_>>();
    assert_eq!(refs(&doc), refs(&again));
}
