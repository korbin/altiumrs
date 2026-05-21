use altium::pcb::Document;
use std::collections::HashMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_comments <PcbDoc>");
    let bytes = std::fs::read(&path).expect("read file");
    let doc = Document::from_bytes(bytes).expect("parse PcbDoc");
    println!("# {} components, {} texts", doc.components.len(), doc.texts.len());

    let designators: std::collections::HashSet<String> = doc
        .components
        .iter()
        .filter_map(|c| c.source_designator.clone())
        .collect();

    let mut by_comp: HashMap<i32, Vec<&str>> = HashMap::new();
    for t in &doc.texts {
        if t.component_index < 0 {
            continue;
        }
        by_comp.entry(t.component_index).or_default().push(&t.text);
    }

    for (i, c) in doc.components.iter().enumerate() {
        let i32_i = i as i32;
        let des = c.source_designator.as_deref().unwrap_or("?");
        let texts = by_comp.get(&i32_i).cloned().unwrap_or_default();
        let comment_candidates: Vec<&&str> = texts
            .iter()
            .filter(|t| {
                let s: &str = t;
                s != ".Designator" && s != ".Comment" && !designators.contains(s) && s != des
            })
            .collect();
        println!(
            "[{i:>3}] {des:>6}  pattern={:?}  component.comment={:?}  texts={:?}  comment_candidates={:?}",
            c.pattern, c.comment, texts, comment_candidates,
        );
    }
}
