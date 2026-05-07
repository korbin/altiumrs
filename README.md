# altium

Pure-Rust read/write for Altium Designer files: `.SchLib`, `.SchDoc`,
`.PcbLib`, `.PcbDoc`. Round-trips every fixture in `testdata/`.

PCB has a typed model: build a `PcbLib` / `PcbDoc` from scratch, mutate,
write. Schematic round-trips by replaying raw records; the typed model
isn't wired up for from-scratch authoring yet. PNG/SVG render lives behind
the `render` feature, for both PCB and schematic components.

## Crates

- `altium`: library.
- `altium-derive`: `#[derive(AltiumRecord)]`, generates `from_params` /
  `to_params` for record DTOs.
- `altium-cli`: `altium info | dump | render | inspect | flatten`.

## Usage

```rust
use altium::pcb;

#[tokio::main]
async fn main() -> altium::Result<()> {
    let lib = pcb::Library::read("parts.PcbLib").await?;
    println!("{} components", lib.components.len());

    let mut out = pcb::Library::default();
    let mut r0402 = pcb::Component::new("R0402");
    r0402.description = Some("Resistor 0402".into());
    out.components.push(r0402);
    out.write("/tmp/out.PcbLib").await?;
    Ok(())
}
```

All four file kinds: `from_bytes` / `to_bytes`, `read` / `write` (async).

```sh
$ cargo install --path crates/altium-cli
$ altium info parts.PcbLib
{ "kind": "PcbLib", "components": 1, "models": 1, "names": ["R0402"] }
$ altium render parts.PcbLib -o /tmp/out.svg
$ altium dump parts.PcbLib
$ altium inspect parts.PcbLib
$ altium flatten panel.PcbDoc -o panel-flat.PcbDoc
flattened 1/1 embedded boards from panel.PcbDoc
  pads=556 tracks=898 arcs=136 vias=48 ... components=60
wrote panel-flat.PcbDoc
```

`flatten` recursively dereferences every embedded sub-board (looking next
to the input file plus any `--search-path` directories), inlines and
transforms its primitives into the parent, and writes a single
self-contained `.PcbDoc`. Unresolved references stay as placeholders with a
diagnostic.

## Features

- `serde`: `Serialize` / `Deserialize` on the model types.
- `render`: `tiny-skia` + `cosmic-text`; adds `render_png` / `render_svg`
  on components and `altium render`.

## Testing

```sh
cargo test --workspace
cargo test --workspace --features render
cargo clippy --workspace --all-targets --features render -- -D warnings
```

## License

MIT or Apache-2.0.
