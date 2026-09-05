//! `altium`: inspect, dump, render, and report on Altium Designer files.

use std::path::{Path, PathBuf};

use altium::compound::CompoundFile;
use altium::render::RenderOptions;
use altium::{AltiumFile, pcb, sch};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Debug, Parser)]
#[command(
    name = "altium",
    about = "Inspect Altium Designer files (.SchLib, .SchDoc, .PcbLib, .PcbDoc)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print a one-line summary of a file (kind + counts + bounds).
    Info {
        /// Path to a `.SchLib`, `.SchDoc`, `.PcbLib`, or `.PcbDoc`.
        path: PathBuf,
    },
    /// Print the full OLE compound-file tree (storages + stream sizes).
    Dump { path: PathBuf },
    /// Compare two Altium files stream by stream; footprint `Data` streams of
    /// a `.PcbLib` are compared record by record.
    Diff {
        a: PathBuf,
        b: PathBuf,
        /// List every differing record instead of per-footprint summaries.
        #[arg(long)]
        all: bool,
        /// Print the comparison as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List the raw records of one footprint's `Data` stream (kind, size,
    /// layer, text) with the bytes of each record's main block in hex.
    Records {
        path: PathBuf,
        /// Footprint name (its storage name when the two differ).
        component: String,
    },
    /// Hex-dump one stream of a compound file.
    Stream {
        path: PathBuf,
        /// Stream path inside the file, e.g. `XGL4040/WideStrings`.
        stream: String,
    },
    /// Check a `.PcbLib` for inconsistencies between its records and the
    /// tables Altium keeps next to them (header counts, primitive GUIDs, wide
    /// strings, the component TOC, embedded model checksums, legacy record
    /// layouts). Exits with status 1 when anything is wrong.
    Check {
        path: PathBuf,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Render to PNG or SVG. Pick the format from the output extension.
    Render {
        path: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Optional component name (defaults to the first component).
        #[arg(short, long)]
        component: Option<String>,
        #[arg(long, default_value_t = 1024)]
        width: u32,
        #[arg(long, default_value_t = 1024)]
        height: u32,
    },
    /// Pretty-print the modeled primitive lists for a component.
    Inspect {
        path: PathBuf,
        #[arg(short, long)]
        component: Option<String>,
    },
    /// Recursively dereference every embedded sub-board of a `.PcbDoc` and
    /// inline its primitives into a single self-contained output `.PcbDoc`.
    /// Sub-boards are looked up next to the input file (and any extra
    /// `--search-path` directories).
    Flatten {
        /// Source `.PcbDoc` carrying embedded board references.
        path: PathBuf,
        /// Destination `.PcbDoc`. Defaults to `<input>.flat.PcbDoc` next to
        /// the source. Use `-` to print a summary without writing.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Extra directories to search for sub-board files. Repeatable.
        #[arg(long)]
        search_path: Vec<PathBuf>,
    },
    /// Extract the netlist from a `.PcbDoc` or `.SchDoc`. PCB extraction is
    /// explicit (each pad carries its net name); SchDoc extraction traces
    /// the wire graph and names nets from net labels, power ports, ports and
    /// harness-connector entries (`<port>.<entry>`).
    Netlist {
        /// Source `.PcbDoc` or `.SchDoc`.
        path: PathBuf,
        /// Output file. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output format: `protel` (Altium .NET, default), `kicad`, `json`,
        /// or `csv`.
        #[arg(long, default_value = "protel")]
        format: String,
        /// SchDoc only: also emit every sheet-symbol entry as a connection
        /// (designator = sheet symbol name, pad = entry name) so a parent
        /// sheet's netlist shows which child entries are wired together.
        #[arg(long)]
        sheet_entries: bool,
        /// SchDoc only: also emit every port a net touches as a
        /// pseudo-connection (designator `PORT`, pad = port name; harness
        /// entries as `<bundle>.<entry>`), for binding child-sheet nets to
        /// the parent's sheet entries.
        #[arg(long)]
        ports: bool,
    },
    /// Export a `.BomDoc` to CSV (default) or JSON. One row per CatalogItem
    /// with the common BOM columns.
    Bom {
        /// Source `.BomDoc`.
        path: PathBuf,
        /// Output file. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output format: `csv` (default) or `json`.
        #[arg(long, default_value = "csv")]
        format: String,
    },
    /// Split an `.IntLib` into its constituent source files (`.SchLib`,
    /// `.PcbLib`, datasheets, …) plus a matching `.LibPkg` project file.
    Split {
        /// Source `.IntLib` file.
        path: PathBuf,
        /// Output directory. Created if it doesn't exist.
        #[arg(short, long)]
        out_dir: PathBuf,
        /// Basename for the emitted `.LibPkg` (defaults to the input file's
        /// stem).
        #[arg(long)]
        name: Option<String>,
    },
    /// Serialize any supported Altium file to JSON: `{"kind": ..,
    /// "document": ..}` with binary blobs as base64 strings.
    ToJson {
        /// Source file (`.PcbDoc`, `.SchDoc`, `.PcbLib`, `.SchLib`,
        /// `.IntLib`, `.LibPkg`, `.BomDoc`).
        path: PathBuf,
        /// Output file. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Single-line output instead of pretty-printed.
        #[arg(long)]
        compact: bool,
    },
    /// Rebuild an Altium file from `to-json` output.
    FromJson {
        /// JSON produced by `to-json`.
        path: PathBuf,
        /// Destination file; its format follows the JSON `kind`.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Export pick-and-place (centroid) data from a `.PcbDoc` as CSV.
    /// Coordinates are board-origin-relative; rotation uses Altium's
    /// convention on both sides.
    Pnp {
        /// Source `.PcbDoc`.
        path: PathBuf,
        /// Output file. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Dialect: `altium` (default), `jlc`, or `kicad`.
        #[arg(long, default_value = "altium")]
        format: String,
        /// Units: `mm` (default) or `mil`.
        #[arg(long, default_value = "mm")]
        units: String,
        /// Keep absolute workspace coordinates.
        #[arg(long)]
        absolute: bool,
        /// Also list Standard (No BOM) components. Like Altium's built-in
        /// export, only Standard components are listed by default; other
        /// kinds (mechanical, graphical, net ties, jumpers) never appear.
        #[arg(long)]
        include_no_bom: bool,
    },
    /// Extract every embedded 3D (STEP) model from a `.PcbDoc`, `.PcbLib`,
    /// or `.IntLib` into a directory, decompressed. Identical duplicates
    /// are written once; same-named distinct models get a numeric suffix.
    ExportModels {
        /// Source `.PcbDoc`, `.PcbLib`, or `.IntLib`.
        path: PathBuf,
        /// Output directory. Created if it doesn't exist.
        #[arg(short, long)]
        out_dir: PathBuf,
    },
    /// Run a jq filter over any supported file's JSON model (the same
    /// shape as `to-json`'s "document" object). Examples:
    /// `.components | length`, `[.nets[].name]`,
    /// `.components[] | select(.source_designator == "R16")`.
    Query {
        /// Source file (any kind `to-json` supports).
        path: PathBuf,
        /// jq filter expression.
        filter: String,
        /// Compact one-line-per-value output instead of pretty-printed.
        #[arg(long)]
        compact: bool,
        /// Print string results raw, without JSON quoting (like `jq -r`).
        #[arg(short = 'r', long)]
        raw: bool,
    },
    /// Print the design rules (DRC) of a `.PcbDoc`. Coordinate values are
    /// shown in the document's display unit (mm for metric documents).
    Rules {
        /// Source `.PcbDoc`.
        path: PathBuf,
        /// Output format: `text` (default) or `json`.
        #[arg(long, default_value = "text")]
        format: String,
        /// Unit for coordinate values: `auto` (document display unit,
        /// default), `mm`, or `mil`.
        #[arg(long, default_value = "auto")]
        units: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Info { path } => cmd_info(&path).await,
        Command::Dump { path } => cmd_dump(&path).await,
        Command::Diff { a, b, all, json } => cmd_diff(&a, &b, all, json).await,
        Command::Check { path, json } => cmd_check(&path, json).await,
        Command::Records { path, component } => cmd_records(&path, &component).await,
        Command::Stream { path, stream } => cmd_stream(&path, &stream).await,
        Command::Render {
            path,
            output,
            component,
            width,
            height,
        } => cmd_render(&path, &output, component.as_deref(), width, height).await,
        Command::Inspect { path, component } => cmd_inspect(&path, component.as_deref()).await,
        Command::Flatten {
            path,
            output,
            search_path,
        } => cmd_flatten(&path, output.as_deref(), &search_path).await,
        Command::ToJson {
            path,
            output,
            compact,
        } => cmd_to_json(&path, output.as_deref(), compact).await,
        Command::FromJson { path, output } => cmd_from_json(&path, &output).await,
        Command::Pnp {
            path,
            output,
            format,
            units,
            absolute,
            include_no_bom,
        } => cmd_pnp(&path, output.as_deref(), &format, &units, absolute, include_no_bom).await,
        Command::ExportModels { path, out_dir } => cmd_export_models(&path, &out_dir).await,
        Command::Query {
            path,
            filter,
            compact,
            raw,
        } => cmd_query(&path, &filter, compact, raw).await,
        Command::Rules {
            path,
            format,
            units,
        } => cmd_rules(&path, &format, &units).await,
        Command::Split {
            path,
            out_dir,
            name,
        } => cmd_split(&path, &out_dir, name.as_deref()).await,
        Command::Netlist {
            path,
            output,
            format,
            sheet_entries,
            ports,
        } => cmd_netlist(&path, output.as_deref(), &format, sheet_entries, ports).await,
        Command::Bom {
            path,
            output,
            format,
        } => cmd_bom(&path, output.as_deref(), &format).await,
    }
}

async fn cmd_info(path: &Path) -> Result<()> {
    let summary = match AltiumFile::read(path).await? {
        AltiumFile::PcbLibrary(lib) => json!({
            "kind": "PcbLib",
            "components": lib.components.len(),
            "models": lib.models.len(),
            "names": lib.components.iter().map(|c| &c.name).collect::<Vec<_>>(),
        }),
        AltiumFile::PcbDocument(doc) => json!({
            "kind": "PcbDoc",
            "components": doc.components.len(),
            "pads": doc.pads.len(),
            "tracks": doc.tracks.len(),
            "vias": doc.vias.len(),
            "arcs": doc.arcs.len(),
            "texts": doc.texts.len(),
            "fills": doc.fills.len(),
            "regions": doc.regions.len(),
            "bodies": doc.component_bodies.len(),
            "polygons": doc.polygons.len(),
            "nets": doc.nets.len(),
            "rules": doc.rules.len(),
            "classes": doc.classes.len(),
            "differential_pairs": doc.differential_pairs.len(),
            "rooms": doc.rooms.len(),
            "embedded_boards": doc.embedded_boards.len(),
        }),
        AltiumFile::SchLibrary(lib) => json!({
            "kind": "SchLib",
            "components": lib.components.len(),
            "embedded_images": lib.embedded_images.len(),
            "names": lib.components.iter().map(|c| &c.name).collect::<Vec<_>>(),
        }),
        AltiumFile::SchDocument(doc) => json!({
            "kind": "SchDoc",
            "raw_records": doc.raw_records.len(),
            "additional_streams": doc.additional_streams.len(),
        }),
        AltiumFile::IntegratedLibrary(intlib) => json!({
            "kind": "IntLib",
            "schematic_libraries": intlib.schematic_libraries.iter()
                .map(|e| serde_json::json!({ "name": e.name, "components": e.library.components.len() }))
                .collect::<Vec<_>>(),
            "footprint_libraries": intlib.footprint_libraries.iter()
                .map(|e| serde_json::json!({ "name": e.name, "components": e.library.components.len() }))
                .collect::<Vec<_>>(),
            "additional_files": intlib.additional_files.keys().collect::<Vec<_>>(),
            "manifest_keys": intlib.manifest.keys().collect::<Vec<_>>(),
        }),
        AltiumFile::LibraryPackage(pkg) => json!({
            "kind": "LibPkg",
            "documents": pkg.documents().iter().map(|d| &d.document_path).cloned().collect::<Vec<_>>(),
            "section_names": pkg.sections.keys().collect::<Vec<_>>(),
        }),
        AltiumFile::BomDocument(doc) => {
            let header = doc.header();
            json!({
                "kind": "BomDoc",
                "records": doc.records.len(),
                "items": doc.item_count(),
                "bom_kind": header.map(|h| h.kind().to_string()),
                "version": header.and_then(|h| h.version()),
                "currency": header.map(|h| h.currency().to_string()),
                "date": header.map(|h| h.date().to_string()),
            })
        }
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

async fn cmd_dump(path: &Path) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let mut cf = CompoundFile::open(bytes)?;
    walk(&mut cf, "/", 0)?;
    Ok(())
}

fn walk(cf: &mut CompoundFile, path: &str, depth: usize) -> Result<()> {
    let entries = cf.list_children(path)?;
    for entry in entries {
        let indent = "  ".repeat(depth);
        if entry.is_storage {
            println!("{indent}{}/", entry.name);
            let child_path = if path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", path, entry.name)
            };
            walk(cf, &child_path, depth + 1)?;
        } else {
            println!("{indent}{} ({} bytes)", entry.name, entry.len);
        }
    }
    Ok(())
}

async fn cmd_render(
    path: &Path,
    output: &Path,
    component: Option<&str>,
    width: u32,
    height: u32,
) -> Result<()> {
    let opts = RenderOptions {
        width,
        height,
        ..RenderOptions::default()
    };
    let svg_target = output
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));

    match AltiumFile::read(path).await? {
        AltiumFile::PcbLibrary(lib) => {
            let comp = pick_pcb_component(&lib, component)?;
            if svg_target {
                tokio::fs::write(output, comp.render_svg(opts)).await?;
            } else {
                let png = comp.render_png(opts).map_err(|e| anyhow!(e))?;
                tokio::fs::write(output, png).await?;
            }
        }
        AltiumFile::SchLibrary(lib) => {
            let comp = pick_sch_component(&lib, component)?;
            if svg_target {
                tokio::fs::write(output, comp.render_svg(opts)).await?;
            } else {
                let png = comp.render_png(opts).map_err(|e| anyhow!(e))?;
                tokio::fs::write(output, png).await?;
            }
        }
        AltiumFile::PcbDocument(doc) => {
            if component.is_some() {
                return Err(anyhow!(
                    "PcbDoc renders the whole board; --component is not supported here"
                ));
            }
            // If the PcbDoc carries any embedded board references, wire up a
            // FileBoardLoader rooted at the parent directory so siblings like
            // `Power Adapter Panel.PcbDoc` → `USB Power Adapter.PcbDoc` get
            // pulled in and rendered. Falls through to the placeholder
            // renderer when the document doesn't reference any sub-boards.
            let parent_dir = path.parent().unwrap_or(Path::new("."));
            let loader = altium::pcb::FileBoardLoader::new(parent_dir);
            if svg_target {
                tokio::fs::write(output, doc.render_svg_with_loader(opts, &loader)).await?;
            } else {
                let png = doc
                    .render_png_with_loader(opts, &loader)
                    .map_err(|e| anyhow!(e))?;
                tokio::fs::write(output, png).await?;
            }
        }
        AltiumFile::SchDocument(doc) => {
            if svg_target {
                tokio::fs::write(output, doc.render_svg(opts)).await?;
            } else {
                let png = doc.render_png(opts).map_err(|e| anyhow!(e))?;
                tokio::fs::write(output, png).await?;
            }
        }
        AltiumFile::IntegratedLibrary(intlib) => {
            // IntLibs bundle libraries — pick one schematic or footprint by
            // name (or the first if --component is omitted) and render that.
            // The render path doesn't extend across nested IntLib structure;
            // users wanting both halves can render twice.
            let want = component;
            let comp_pcb = intlib
                .footprint_libraries
                .iter()
                .find_map(|e| pick_pcb_component(&e.library, want).ok());
            let comp_sch = intlib
                .schematic_libraries
                .iter()
                .find_map(|e| pick_sch_component(&e.library, want).ok());
            if let Some(comp) = comp_pcb {
                if svg_target {
                    tokio::fs::write(output, comp.render_svg(opts)).await?;
                } else {
                    let png = comp.render_png(opts).map_err(|e| anyhow!(e))?;
                    tokio::fs::write(output, png).await?;
                }
            } else if let Some(comp) = comp_sch {
                if svg_target {
                    tokio::fs::write(output, comp.render_svg(opts)).await?;
                } else {
                    let png = comp.render_png(opts).map_err(|e| anyhow!(e))?;
                    tokio::fs::write(output, png).await?;
                }
            } else {
                return Err(anyhow!(
                    "IntLib has no renderable components matching {:?}",
                    want
                ));
            }
        }
        AltiumFile::LibraryPackage(_) => {
            return Err(anyhow!(
                "LibPkg is a project file with no rendered geometry; render the referenced .SchLib / .PcbLib instead"
            ));
        }
        AltiumFile::BomDocument(_) => {
            return Err(anyhow!(
                "BomDoc has no rendered geometry; use `altium bom` to export CSV/JSON instead"
            ));
        }
    }
    println!("wrote {}", output.display());
    Ok(())
}

fn pick_pcb_component<'a>(lib: &'a pcb::Library, name: Option<&str>) -> Result<&'a pcb::Component> {
    match name {
        Some(n) => lib
            .component(n)
            .ok_or_else(|| anyhow!("no component named {n:?} in library")),
        None => lib
            .components
            .first()
            .ok_or_else(|| anyhow!("library has no components")),
    }
}

fn pick_sch_component<'a>(lib: &'a sch::Library, name: Option<&str>) -> Result<&'a sch::Component> {
    match name {
        Some(n) => lib
            .component(n)
            .ok_or_else(|| anyhow!("no component named {n:?} in library")),
        None => lib
            .components
            .first()
            .ok_or_else(|| anyhow!("library has no components")),
    }
}

async fn cmd_inspect(path: &Path, component: Option<&str>) -> Result<()> {
    let summary = match AltiumFile::read(path).await? {
        AltiumFile::PcbLibrary(lib) => {
            let comp = pick_pcb_component(&lib, component)?;
            json!({
                "name": comp.name,
                "description": comp.description,
                "pads": comp.pads.len(),
                "tracks": comp.tracks.len(),
                "vias": comp.vias.len(),
                "arcs": comp.arcs.len(),
                "texts": comp.texts.len(),
                "fills": comp.fills.len(),
                "regions": comp.regions.len(),
                "bodies": comp.component_bodies.len(),
                "pad_designators": comp.pads.iter().filter_map(|p| p.designator.clone()).collect::<Vec<_>>(),
            })
        }
        AltiumFile::SchLibrary(lib) => {
            let comp = pick_sch_component(&lib, component)?;
            json!({
                "name": comp.name,
                "description": comp.description,
                "designator_prefix": comp.designator_prefix,
                "comment": comp.comment,
                "part_count": comp.part_count,
                "pins": comp.pins.len(),
                "lines": comp.lines.len(),
                "rectangles": comp.rectangles.len(),
                "rounded_rectangles": comp.rounded_rectangles.len(),
                "polygons": comp.polygons.len(),
                "polylines": comp.polylines.len(),
                "ellipses": comp.ellipses.len(),
                "elliptical_arcs": comp.elliptical_arcs.len(),
                "pies": comp.pies.len(),
                "arcs": comp.arcs.len(),
                "beziers": comp.beziers.len(),
                "labels": comp.labels.len(),
                "parameters": comp.parameters.len(),
                "text_frames": comp.text_frames.len(),
                "images": comp.images.len(),
                "implementations": comp.implementations.len(),
                "raw_records": comp.raw_records.len(),
                "pin_designators": comp.pins.iter().filter_map(|p| p.designator.clone()).collect::<Vec<_>>(),
            })
        }
        AltiumFile::PcbDocument(doc) => json!({
            "pads": doc.pads.len(),
            "tracks": doc.tracks.len(),
            "vias": doc.vias.len(),
            "arcs": doc.arcs.len(),
            "components": doc.components.len(),
        }),
        AltiumFile::SchDocument(doc) => json!({
            "raw_records": doc.raw_records.len(),
            "additional_streams_keys": doc.additional_streams.keys().collect::<Vec<_>>(),
        }),
        AltiumFile::IntegratedLibrary(intlib) => {
            // Surface the typed cross-reference table when it parses; fall
            // back to the raw token stream when the structure doesn't match.
            let cross_reference = match intlib.cross_reference_table() {
                Ok(table) => {
                    let symbols = table.symbols.iter().map(|s| {
                        serde_json::json!({
                            "libref": s.libref,
                            "internal_schlib_path": s.internal_schlib_path,
                            "description": s.description,
                            "source_schlib_path": s.source_schlib_path,
                            "footprints": s.footprints.iter().map(|f| serde_json::json!({
                                "name": f.name,
                                "kind": f.kind,
                                "internal_pcblib_path": f.internal_pcblib_path,
                                "source_pcblib_path": f.source_pcblib_path,
                            })).collect::<Vec<_>>(),
                        })
                    }).collect::<Vec<_>>();
                    json!({"symbols": symbols})
                }
                Err(e) => json!({ "error": e.to_string() }),
            };
            json!({
                "schematic_libraries": intlib.schematic_libraries.iter().map(|e| &e.name).collect::<Vec<_>>(),
                "footprint_libraries": intlib.footprint_libraries.iter().map(|e| &e.name).collect::<Vec<_>>(),
                "additional_files": intlib.additional_files.keys().collect::<Vec<_>>(),
                "manifest": intlib.manifest.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<std::collections::BTreeMap<_, _>>(),
                "version": intlib.version,
                "parameters_blocks": intlib.parameters_blocks().unwrap_or_default(),
                "cross_reference": cross_reference,
            })
        }
        AltiumFile::LibraryPackage(pkg) => json!({
            "documents": pkg.documents().iter().map(|d| serde_json::json!({
                "path": d.document_path,
                "annotation_enabled": d.annotation_enabled,
                "annotate_scope": d.annotate_scope,
            })).collect::<Vec<_>>(),
            "section_names": pkg.sections.keys().collect::<Vec<_>>(),
        }),
        AltiumFile::BomDocument(doc) => json!({
            "kind": "BomDoc",
            "records": doc.records.iter().map(|r| r.kind.clone()).collect::<Vec<_>>(),
            "items": doc.items().map(|it| serde_json::json!({
                "unique_id": it.unique_id(),
                "design_item_id": it.design_item_id(),
                "description": it.description(),
                "user_comments": it.user_comments(),
                "footprint": it.footprint(),
                "manufacturer": it.manufacturer(),
                "manufacturer_part_number": it.manufacturer_part_number(),
            })).collect::<Vec<_>>(),
        }),
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

/// Print or write `text`, following the CLI's stdout-by-default convention.
fn emit_output(output: Option<&Path>, text: &str) -> Result<()> {
    match output {
        Some(p) => {
            std::fs::write(p, text).with_context(|| format!("write {}", p.display()))?;
            println!("wrote {}", p.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

async fn cmd_to_json(path: &Path, output: Option<&Path>, compact: bool) -> Result<()> {
    let file = AltiumFile::read(path).await?;
    let value = match &file {
        AltiumFile::PcbDocument(d) => json!({"kind": "PcbDocument", "document": d}),
        AltiumFile::PcbLibrary(d) => json!({"kind": "PcbLibrary", "document": d}),
        AltiumFile::SchDocument(d) => json!({"kind": "SchDocument", "document": d}),
        AltiumFile::SchLibrary(d) => json!({"kind": "SchLibrary", "document": d}),
        AltiumFile::IntegratedLibrary(d) => json!({"kind": "IntegratedLibrary", "document": d}),
        AltiumFile::LibraryPackage(d) => json!({"kind": "LibraryPackage", "document": d}),
        AltiumFile::BomDocument(d) => json!({"kind": "BomDocument", "document": d}),
    };
    let mut text = if compact {
        serde_json::to_string(&value)?
    } else {
        serde_json::to_string_pretty(&value)?
    };
    text.push('\n');
    emit_output(output, &text)
}

async fn cmd_from_json(path: &Path, output: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let kind = value
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| anyhow!("missing \"kind\" field — is this `to-json` output?"))?
        .to_string();
    let document = value
        .get_mut("document")
        .ok_or_else(|| anyhow!("missing \"document\" field"))?
        .take();
    let bytes = match kind.as_str() {
        "PcbDocument" => serde_json::from_value::<pcb::Document>(document)?.to_bytes()?,
        "PcbLibrary" => serde_json::from_value::<pcb::Library>(document)?.to_bytes()?,
        "SchDocument" => serde_json::from_value::<sch::Document>(document)?.to_bytes()?,
        "SchLibrary" => serde_json::from_value::<sch::Library>(document)?.to_bytes()?,
        "IntegratedLibrary" => {
            serde_json::from_value::<altium::IntegratedLibrary>(document)?.to_bytes()?
        }
        "LibraryPackage" => {
            let pkg = serde_json::from_value::<altium::LibraryPackage>(document)?;
            pkg.write(output).await?;
            println!("wrote {}", output.display());
            return Ok(());
        }
        "BomDocument" => serde_json::from_value::<altium::BomDocument>(document)?.to_bytes()?,
        other => return Err(anyhow!("unsupported kind {other:?}")),
    };
    tokio::fs::write(output, bytes)
        .await
        .with_context(|| format!("write {}", output.display()))?;
    println!("wrote {}", output.display());
    Ok(())
}

async fn cmd_pnp(
    path: &Path,
    output: Option<&Path>,
    format: &str,
    units: &str,
    absolute: bool,
    include_no_bom: bool,
) -> Result<()> {
    use altium::pnp::{PnpFormat, PnpUnits, format_pnp_csv, pnp_entries};

    let file = AltiumFile::read(path).await?;
    let AltiumFile::PcbDocument(doc) = file else {
        return Err(anyhow!("pnp requires a .PcbDoc"));
    };
    let format = match format.to_ascii_lowercase().as_str() {
        "altium" => PnpFormat::Altium,
        "jlc" => PnpFormat::Jlc,
        "kicad" => PnpFormat::Kicad,
        other => return Err(anyhow!("unknown format {other:?} (altium|jlc|kicad)")),
    };
    let units = match units.to_ascii_lowercase().as_str() {
        "mm" => PnpUnits::Mm,
        "mil" => PnpUnits::Mil,
        other => return Err(anyhow!("unknown units {other:?} (mm|mil)")),
    };
    let entries = pnp_entries(&doc, absolute, include_no_bom);
    let skipped = doc.components.len() - entries.len();
    if skipped > 0 {
        eprintln!(
            "note: {skipped} component(s) excluded by kind{}",
            if include_no_bom {
                ""
            } else {
                " (--include-no-bom lists Standard (No BOM) parts)"
            }
        );
    }
    let missing = entries.iter().filter(|e| e.designator.is_empty()).count();
    if missing > 0 {
        eprintln!("note: {missing} component(s) have no designator (blank in output)");
    }
    emit_output(output, &format_pnp_csv(&entries, format, units))
}

/// The file's JSON model — the same object `to-json` puts in "document".
async fn document_json(path: &Path) -> Result<String> {
    let file = AltiumFile::read(path).await?;
    Ok(match &file {
        AltiumFile::PcbDocument(d) => serde_json::to_string(d)?,
        AltiumFile::PcbLibrary(d) => serde_json::to_string(d)?,
        AltiumFile::SchDocument(d) => serde_json::to_string(d)?,
        AltiumFile::SchLibrary(d) => serde_json::to_string(d)?,
        AltiumFile::IntegratedLibrary(d) => serde_json::to_string(d)?,
        AltiumFile::LibraryPackage(d) => serde_json::to_string(d)?,
        AltiumFile::BomDocument(d) => serde_json::to_string(d)?,
    })
}

async fn cmd_query(path: &Path, filter_code: &str, compact: bool, raw: bool) -> Result<()> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
    use jaq_json::{Val, read};

    let json = document_json(path).await?;
    let input = read::parse_single(json.as_bytes())
        .map_err(|e| anyhow!("internal: model JSON did not parse: {e:?}"))?;

    let program = File {
        code: filter_code,
        path: (),
    };
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(&arena, program)
        .map_err(|e| anyhow!("filter parse error: {e:?}"))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|e| anyhow!("filter compile error: {e:?}"))?;

    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    for v in filter.id.run((ctx, input)).map(unwrap_valr) {
        let v = v.map_err(|e| anyhow!("filter error: {e:?}"))?;
        match &v {
            Val::TStr(s) if raw => println!("{}", String::from_utf8_lossy(s)),
            _ if compact => println!("{v}"),
            _ => {
                // Pretty-print by re-parsing the compact form; fall back to
                // compact if the value isn't plain JSON.
                let compact_text = v.to_string();
                match serde_json::from_str::<serde_json::Value>(&compact_text) {
                    Ok(j) => println!("{}", serde_json::to_string_pretty(&j)?),
                    Err(_) => println!("{compact_text}"),
                }
            }
        }
    }
    Ok(())
}

/// `"0.09mm"`-style rendering with trailing zeros trimmed.
fn format_mm(c: altium::Coord) -> String {
    let mut s = format!("{:.4}", c.to_mm());
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s}mm")
}

/// Convert a rule parameter value for display: mil-suffixed coords become
/// mm, and the raw-unit clearance matrix (`OBJECTCLEARANCES`) is rewritten
/// pair by pair. Everything else passes through untouched.
fn rule_value_in_mm(key: &str, value: &str) -> String {
    if key.eq_ignore_ascii_case("OBJECTCLEARANCES") {
        return value
            .split(';')
            .map(|pair| match pair.rsplit_once(':') {
                Some((name, raw)) => match raw.trim().parse::<i32>() {
                    Ok(raw) => format!("{name}:{}", format_mm(altium::Coord::from_raw(raw))),
                    Err(_) => pair.to_string(),
                },
                None => pair.to_string(),
            })
            .collect::<Vec<_>>()
            .join(";");
    }
    if let Some(num) = value.strip_suffix("mil") {
        if let Ok(mils) = num.trim().parse::<f64>() {
            return format_mm(altium::Coord::from_mils(mils));
        }
    }
    value.to_string()
}

async fn cmd_rules(path: &Path, format: &str, units: &str) -> Result<()> {
    let file = AltiumFile::read(path).await?;
    let AltiumFile::PcbDocument(doc) = file else {
        return Err(anyhow!("rules requires a .PcbDoc"));
    };
    let metric = match units.to_ascii_lowercase().as_str() {
        "auto" => doc.display_unit() == Some(0),
        "mm" => true,
        "mil" => false,
        other => return Err(anyhow!("unknown units {other:?} (auto|mm|mil)")),
    };
    match format.to_ascii_lowercase().as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&doc.rules)?),
        "text" => {
            let mut rules: Vec<_> = doc.rules.iter().collect();
            rules.sort_by(|a, b| {
                (&a.rule_kind, a.priority, &a.name).cmp(&(&b.rule_kind, b.priority, &b.name))
            });
            let mut last_kind = "";
            for r in rules {
                if r.rule_kind != last_kind {
                    println!("[{}]", r.rule_kind);
                    last_kind = &r.rule_kind;
                }
                let flag = if r.enabled { "" } else { " [disabled]" };
                println!("  {} (priority {}){}", r.name, r.priority, flag);
                if !r.scope1_expression.is_empty() || !r.scope2_expression.is_empty() {
                    if r.scope2_expression.is_empty() {
                        println!("    scope: {}", r.scope1_expression);
                    } else {
                        println!(
                            "    scope: {}  vs  {}",
                            r.scope1_expression, r.scope2_expression
                        );
                    }
                }
                if !r.comment.is_empty() {
                    println!("    comment: {}", r.comment);
                }
                // Boilerplate and typed-field duplicates stay out of the
                // text view; `--format json` has everything.
                const NOISE: &[&str] = &[
                    "SELECTION",
                    "LAYER",
                    "LOCKED",
                    "POLYGONOUTLINE",
                    "USERROUTED",
                    "KEEPOUT",
                    "UNIONINDEX",
                    "RULEKIND",
                    "NAME",
                    "ENABLED",
                    "PRIORITY",
                    "COMMENT",
                    "UNIQUEID",
                    "SCOPE1EXPRESSION",
                    "SCOPE2EXPRESSION",
                    "DEFINEDBYLOGICALDOCUMENT",
                ];
                let params: Vec<String> = r
                    .parameters
                    .iter()
                    .filter(|(k, v)| !v.is_empty() && !NOISE.contains(&k.to_uppercase().as_str()))
                    .map(|(k, v)| {
                        if metric {
                            format!("{k}={}", rule_value_in_mm(k, v))
                        } else {
                            format!("{k}={v}")
                        }
                    })
                    .collect();
                if !params.is_empty() {
                    println!("    {}", params.join(" | "));
                }
            }
            println!("{} rule(s)", doc.rules.len());
        }
        other => return Err(anyhow!("unknown format {other:?} (text|json)")),
    }
    Ok(())
}

async fn cmd_export_models(path: &Path, out_dir: &Path) -> Result<()> {
    let file = AltiumFile::read(path).await?;
    // (source label, model) pairs; the label only disambiguates IntLib output.
    let mut models: Vec<(String, altium::pcb::Model3d)> = Vec::new();
    match file {
        AltiumFile::PcbDocument(doc) => {
            models.extend(doc.embedded_models()?.into_iter().map(|m| (String::new(), m)));
        }
        AltiumFile::PcbLibrary(lib) => {
            models.extend(lib.models.into_iter().map(|m| (String::new(), m)));
        }
        AltiumFile::IntegratedLibrary(intlib) => {
            for lib in intlib.footprint_libraries {
                models.extend(
                    lib.library
                        .models
                        .into_iter()
                        .map(|m| (lib.name.clone(), m)),
                );
            }
        }
        other => {
            return Err(anyhow!(
                "export-models requires .PcbDoc, .PcbLib, or .IntLib; got {}",
                kind_label(&other)
            ));
        }
    }

    let empty = models.iter().filter(|(_, m)| m.step_data.is_empty()).count();
    models.retain(|(_, m)| !m.step_data.is_empty());
    if models.is_empty() {
        println!("no embedded models found ({empty} entries without data)");
        return Ok(());
    }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;

    // Written name → content, for dedup and collision suffixing.
    let mut written: std::collections::BTreeMap<String, String> = Default::default();
    let mut saved = 0usize;
    let mut deduped = 0usize;
    for (i, (source, model)) in models.iter().enumerate() {
        let raw_name = if model.name.is_empty() {
            format!("model-{i}.step")
        } else {
            model.name.clone()
        };
        let base: String = raw_name
            .chars()
            .map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c })
            .collect();
        let base = if source.is_empty() {
            base
        } else {
            format!("{source}-{base}")
        };

        // Reuse the name for identical content; suffix distinct content.
        let mut name = base.clone();
        let mut n = 2;
        loop {
            match written.get(&name) {
                None => break,
                Some(existing) if existing == &model.step_data => break,
                Some(_) => {
                    let (stem, ext) = match base.rsplit_once('.') {
                        Some((s, e)) => (s.to_string(), format!(".{e}")),
                        None => (base.clone(), String::new()),
                    };
                    name = format!("{stem}-{n}{ext}");
                    n += 1;
                }
            }
        }
        if written.contains_key(&name) {
            deduped += 1;
            continue;
        }
        let dest = out_dir.join(&name);
        std::fs::write(&dest, model.step_data.as_bytes())
            .with_context(|| format!("write {}", dest.display()))?;
        println!("wrote {}", dest.display());
        written.insert(name, model.step_data.clone());
        saved += 1;
    }
    let mut summary = format!("{saved} model(s) written to {}", out_dir.display());
    if deduped > 0 {
        summary.push_str(&format!(", {deduped} identical duplicate(s) skipped"));
    }
    if empty > 0 {
        summary.push_str(&format!(", {empty} entr(ies) without embedded data"));
    }
    println!("{summary}");
    Ok(())
}

async fn cmd_flatten(
    path: &Path,
    output: Option<&Path>,
    extra_search_paths: &[PathBuf],
) -> Result<()> {
    use altium::pcb::FileBoardLoader;

    let parent_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Reading via AltiumFile::read so unrecognised extensions error early
    // with a structured message; only `.PcbDoc` makes sense for flattening.
    let file = AltiumFile::read(path).await?;
    let doc = match file {
        AltiumFile::PcbDocument(d) => d,
        other => {
            return Err(anyhow!(
                "flatten only supports .PcbDoc; got {}",
                kind_label(&other)
            ));
        }
    };
    let original_embedded = doc.embedded_boards.len();

    // FileBoardLoader resolves siblings against the parent directory plus
    // any extra search paths the user supplied. Mirrors the auto-resolver
    // the renderer uses.
    let mut loader = FileBoardLoader::new(&parent_dir);
    for extra in extra_search_paths {
        loader = loader.with_search_path(extra);
    }

    let flat = doc.flatten_embedded_boards_with(&loader);

    // Compact stats so the user can see what work the flatten did.
    let resolved = original_embedded.saturating_sub(flat.embedded_boards.len());
    eprintln!(
        "flattened {resolved}/{original_embedded} embedded boards from {}",
        path.display()
    );
    eprintln!(
        "  pads={} tracks={} arcs={} vias={} texts={} fills={} regions={} bodies={} polygons={} components={}",
        flat.pads.len(),
        flat.tracks.len(),
        flat.arcs.len(),
        flat.vias.len(),
        flat.texts.len(),
        flat.fills.len(),
        flat.regions.len(),
        flat.component_bodies.len(),
        flat.polygons.len(),
        flat.components.len(),
    );

    // Surface diagnostics — these are the boards that *didn't* flatten
    // (missing siblings, recursion cap, parse failure). Non-fatal.
    if !flat.diagnostics.is_empty() {
        eprintln!("diagnostics:");
        for d in &flat.diagnostics {
            eprintln!("  [{:?}] {}", d.severity, d.message);
        }
    }

    // `-o -` prints stats only and skips the write. No `-o` writes next to
    // the source as `<stem>.flat.PcbDoc`.
    let default_out;
    let out_path = match output {
        Some(p) if p.as_os_str() == "-" => return Ok(()),
        Some(p) => p,
        None => {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            default_out = parent_dir.join(format!("{stem}.flat.PcbDoc"));
            default_out.as_path()
        }
    };

    let bytes = flat
        .to_bytes()
        .with_context(|| format!("encode flattened {}", out_path.display()))?;
    tokio::fs::write(out_path, bytes)
        .await
        .with_context(|| format!("write {}", out_path.display()))?;
    println!("wrote {}", out_path.display());
    Ok(())
}

async fn cmd_netlist(
    path: &Path,
    output: Option<&Path>,
    format: &str,
    sheet_entries: bool,
    ports: bool,
) -> Result<()> {
    use altium::{Netlist, SchNetlistOptions};
    let file = AltiumFile::read(path).await?;
    let netlist = match file {
        AltiumFile::PcbDocument(doc) => Netlist::from_pcb_document(&doc),
        AltiumFile::SchDocument(doc) => Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_sheet_entries: sheet_entries,
                include_ports: ports,
            },
        ),
        other => {
            return Err(anyhow!(
                "netlist requires .PcbDoc or .SchDoc; got {}",
                kind_label(&other)
            ));
        }
    };

    let rendered = match format.to_ascii_lowercase().as_str() {
        "protel" | "altium" | "net" => netlist.to_protel(),
        "kicad" => netlist.to_kicad(),
        "json" => netlist.to_json(),
        "csv" | "tsv" => netlist.to_csv(),
        other => {
            return Err(anyhow!(
                "unknown format {other:?}; expected protel | kicad | json | csv"
            ));
        }
    };

    match output {
        Some(out) => {
            tokio::fs::write(out, rendered.as_bytes())
                .await
                .with_context(|| format!("write {}", out.display()))?;
            println!("wrote {}", out.display());
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

async fn cmd_bom(path: &Path, output: Option<&Path>, format: &str) -> Result<()> {
    use altium::bom::BomDocument;
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let doc = BomDocument::from_bytes(bytes)
        .with_context(|| format!("parse {}", path.display()))?;

    let rendered = match format.to_ascii_lowercase().as_str() {
        "csv" | "tsv" => bom_to_csv(&doc, format.eq_ignore_ascii_case("tsv")),
        "json" => bom_to_json(&doc)?,
        other => {
            return Err(anyhow!("unknown format {other:?}; expected csv | tsv | json"));
        }
    };

    match output {
        Some(out) => {
            tokio::fs::write(out, rendered.as_bytes())
                .await
                .with_context(|| format!("write {}", out.display()))?;
            println!("wrote {}", out.display());
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

fn bom_to_csv(doc: &altium::bom::BomDocument, tab: bool) -> String {
    let sep = if tab { '\t' } else { ',' };
    let headers = [
        "DesignItemId",
        "Comment",
        "Description",
        "Footprint",
        "Manufacturer",
        "Manufacturer Part Number",
        "Supplier",
        "Supplier Part Number",
        "ItemSource",
        "UniqueId",
    ];
    let mut out = String::new();
    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            out.push(sep);
        }
        out.push_str(&csv_field(h, sep));
    }
    out.push_str("\r\n");
    for it in doc.items() {
        let comment = it.comment().unwrap_or_else(|| it.user_comments().to_string());
        let row = [
            it.design_item_id().to_string(),
            comment,
            it.description().to_string(),
            it.footprint().unwrap_or_default(),
            it.manufacturer().unwrap_or_default(),
            it.manufacturer_part_number().unwrap_or_default(),
            it.supplier().unwrap_or_default(),
            it.supplier_part_number().unwrap_or_default(),
            it.item_source().to_string(),
            it.unique_id().to_string(),
        ];
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push(sep);
            }
            out.push_str(&csv_field(cell, sep));
        }
        out.push_str("\r\n");
    }
    out
}

fn csv_field(s: &str, sep: char) -> String {
    let needs_quotes = s.contains(sep) || s.contains('"') || s.contains('\n') || s.contains('\r');
    if needs_quotes {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn bom_to_json(doc: &altium::bom::BomDocument) -> Result<String> {
    let items: Vec<_> = doc
        .items()
        .map(|it| {
            json!({
                "unique_id": it.unique_id(),
                "design_item_id": it.design_item_id(),
                "item_source": it.item_source(),
                "description": it.description(),
                "user_comments": it.user_comments(),
                "comment": it.comment(),
                "footprint": it.footprint(),
                "value": it.value(),
                "manufacturer": it.manufacturer(),
                "manufacturer_part_number": it.manufacturer_part_number(),
                "supplier": it.supplier(),
                "supplier_part_number": it.supplier_part_number(),
                "library_reference": it.library_reference(),
                "component_parameters": it.component_parameters(),
            })
        })
        .collect();
    let header = doc.header().map(|h| {
        json!({
            "version": h.version(),
            "filename": h.filename(),
            "kind": h.kind(),
            "date": h.date(),
            "time": h.time(),
            "currency": h.currency(),
            "production_quantity": h.production_quantity(),
        })
    });
    let value = json!({ "header": header, "items": items });
    Ok(serde_json::to_string_pretty(&value)?)
}

async fn cmd_split(path: &Path, out_dir: &Path, name_override: Option<&str>) -> Result<()> {
    // Read whatever the user pointed at — IntLib explicitly, or anything else
    // we can crack open via the unified entry. Only IntLib actually contains
    // bundled libraries; the others get a clear error.
    let file = AltiumFile::read(path).await?;
    let intlib = match file {
        AltiumFile::IntegratedLibrary(i) => i,
        other => {
            return Err(anyhow!(
                "split only supports .IntLib; got {}",
                kind_label(&other)
            ));
        }
    };

    let stem = name_override
        .map(str::to_string)
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Library".to_string());

    let result = intlib
        .split_to_directory(out_dir, &stem)
        .await
        .with_context(|| format!("split {} into {}", path.display(), out_dir.display()))?;
    result.write_package().await.with_context(|| {
        format!("write {}", result.package_path.display())
    })?;

    eprintln!("split {} → {}", path.display(), out_dir.display());
    eprintln!("  {} embedded files written", result.written_files.len());
    eprintln!("  package: {}", result.package_path.display());
    for p in &result.written_files {
        eprintln!("  - {}", p.display());
    }
    println!("wrote {}", result.package_path.display());
    Ok(())
}

fn kind_label(file: &AltiumFile) -> &'static str {
    match file {
        AltiumFile::PcbLibrary(_) => ".PcbLib",
        AltiumFile::SchLibrary(_) => ".SchLib",
        AltiumFile::PcbDocument(_) => ".PcbDoc",
        AltiumFile::SchDocument(_) => ".SchDoc",
        AltiumFile::IntegratedLibrary(_) => ".IntLib",
        AltiumFile::LibraryPackage(_) => ".LibPkg",
        AltiumFile::BomDocument(_) => ".BomDoc",
    }
}

fn list_streams(cf: &mut CompoundFile, path: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in cf.list_children(path)? {
        let child = if path == "/" {
            entry.name.clone()
        } else {
            format!("{path}/{}", entry.name)
        };
        if entry.is_storage {
            list_streams(cf, &child, out)?;
        } else {
            out.push(child);
        }
    }
    Ok(())
}

fn first_diff(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()))
}

fn is_footprint_data(stream: &str) -> bool {
    let parts: Vec<&str> = stream.split('/').collect();
    parts.len() == 2 && parts[1] == "Data" && parts[0] != "Library" && parts[0] != "FileVersionInfo"
}

async fn cmd_diff(a: &Path, b: &Path, all: bool, json: bool) -> Result<()> {
    use altium::pcb::records::{kind_name, split_footprint_records};
    use std::collections::{BTreeMap, BTreeSet};
    let mut ca = CompoundFile::open(tokio::fs::read(a).await.with_context(|| format!("read {}", a.display()))?)?;
    let mut cb = CompoundFile::open(tokio::fs::read(b).await.with_context(|| format!("read {}", b.display()))?)?;
    let mut sa = Vec::new();
    let mut sb = Vec::new();
    list_streams(&mut ca, "/", &mut sa)?;
    list_streams(&mut cb, "/", &mut sb)?;
    let sa: BTreeSet<String> = sa.into_iter().collect();
    let sb: BTreeSet<String> = sb.into_iter().collect();
    let only_a: Vec<&String> = sa.difference(&sb).collect();
    let only_b: Vec<&String> = sb.difference(&sa).collect();
    let mut identical = 0usize;
    let mut stream_diffs = Vec::new(); // (stream, len a, len b, first diff)
    let mut record_diffs = Vec::new(); // json objects per footprint
    let mut changed_by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut added_by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut removed_by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    for s in sa.intersection(&sb) {
        let da = ca.read_stream(s)?;
        let db = cb.read_stream(s)?;
        if da == db {
            identical += 1;
            continue;
        }
        if is_footprint_data(s) {
            if let (Ok((name, ra)), Ok((_, rb))) = (split_footprint_records(&da), split_footprint_records(&db)) {
                let same_order = ra.iter().map(|r| r.kind).eq(rb.iter().map(|r| r.kind));
                let mut changed = Vec::new();
                if same_order {
                    for (i, (x, y)) in ra.iter().zip(&rb).enumerate() {
                        if x != y {
                            *changed_by_kind.entry(kind_name(x.kind)).or_default() += 1;
                            let fd = first_diff(&x.bytes, &y.bytes);
                            let offsets: Vec<usize> = x.bytes.iter().zip(&y.bytes).enumerate().filter(|(_, (p, q))| p != q).map(|(i, _)| i).take(32).collect();
                            let hex = |b: &[u8]| b[fd.min(b.len())..(fd + 16).min(b.len())].iter().map(|v| format!("{v:02x}")).collect::<String>();
                            changed.push(serde_json::json!({
                                "index": i, "kind": kind_name(x.kind), "layer": x.layer(), "layer_b": y.layer(),
                                "text": x.text(), "len_a": x.bytes.len(), "len_b": y.bytes.len(),
                                "first_diff": fd, "bytes_a": hex(&x.bytes), "bytes_b": hex(&y.bytes),
                                "diff_offsets": offsets,
                            }));
                        }
                    }
                }
                let mut added = Vec::new();
                let mut removed = Vec::new();
                if !same_order {
                    let mut pool: Vec<Option<&altium::pcb::records::RawRecord>> = ra.iter().map(Some).collect();
                    for (i, y) in rb.iter().enumerate() {
                        if let Some(slot) = pool.iter_mut().find(|p| p.map(|x| x == y).unwrap_or(false)) {
                            *slot = None;
                        } else {
                            *added_by_kind.entry(kind_name(y.kind)).or_default() += 1;
                            added.push(serde_json::json!({"index": i, "kind": kind_name(y.kind), "layer": y.layer(), "text": y.text()}));
                        }
                    }
                    for (i, x) in pool.iter().enumerate() {
                        if let Some(x) = x {
                            *removed_by_kind.entry(kind_name(x.kind)).or_default() += 1;
                            removed.push(serde_json::json!({"index": i, "kind": kind_name(x.kind), "layer": x.layer(), "text": x.text()}));
                        }
                    }
                }
                record_diffs.push(serde_json::json!({
                    "footprint": name, "stream": s, "records_a": ra.len(), "records_b": rb.len(),
                    "same_order": same_order, "changed": changed, "added": added, "removed": removed,
                }));
                continue;
            }
        }
        stream_diffs.push(serde_json::json!({"stream": s, "len_a": da.len(), "len_b": db.len(), "first_diff": first_diff(&da, &db)}));
    }
    let summary = serde_json::json!({
        "only_in_a": only_a, "only_in_b": only_b, "identical_streams": identical,
        "differing_streams": stream_diffs.len() + record_diffs.len(),
        "footprints_with_record_changes": record_diffs.len(),
        "changed_records_by_kind": changed_by_kind, "added_records_by_kind": added_by_kind,
        "removed_records_by_kind": removed_by_kind,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"summary": summary, "streams": stream_diffs, "footprints": record_diffs}))?);
        return Ok(());
    }
    if !only_a.is_empty() {
        println!("only in {}: {}", a.display(), only_a.len());
        for s in only_a.iter().take(if all { usize::MAX } else { 10 }) {
            println!("  {s}");
        }
    }
    if !only_b.is_empty() {
        println!("only in {}: {}", b.display(), only_b.len());
        for s in only_b.iter().take(if all { usize::MAX } else { 10 }) {
            println!("  {s}");
        }
    }
    for d in &stream_diffs {
        println!("{}: {} -> {} bytes, first difference at {}", d["stream"].as_str().unwrap_or(""), d["len_a"], d["len_b"], d["first_diff"]);
    }
    for f in &record_diffs {
        let ch = f["changed"].as_array().map(|v| v.len()).unwrap_or(0);
        let ad = f["added"].as_array().map(|v| v.len()).unwrap_or(0);
        let rm = f["removed"].as_array().map(|v| v.len()).unwrap_or(0);
        println!(
            "{}: {} -> {} records; changed {ch}, added {ad}, removed {rm}",
            f["footprint"].as_str().unwrap_or(""), f["records_a"], f["records_b"]
        );
        if all {
            for (label, list) in [("changed", &f["changed"]), ("added", &f["added"]), ("removed", &f["removed"])] {
                for r in list.as_array().into_iter().flatten() {
                    let text = r["text"].as_str().map(|t| format!(" {t:?}")).unwrap_or_default();
                    let extra = if label == "changed" {
                        format!(
                            " ({} -> {} bytes, first difference at {}: {} -> {}, layer {} -> {})",
                            r["len_a"], r["len_b"], r["first_diff"],
                            r["bytes_a"].as_str().unwrap_or(""), r["bytes_b"].as_str().unwrap_or(""),
                            r["layer"], r["layer_b"]
                        ) + &format!(" differing offsets {}", r["diff_offsets"])
                    } else {
                        String::new()
                    };
                    println!("    {label} #{} {} layer {}{text}{extra}", r["index"], r["kind"].as_str().unwrap_or(""), r["layer"]);
                }
            }
        }
    }
    println!(
        "identical streams: {identical}; differing: {}; footprints with record changes: {}; changed records {:?}; added {:?}; removed {:?}",
        stream_diffs.len() + record_diffs.len(), record_diffs.len(), changed_by_kind, added_by_kind, removed_by_kind
    );
    Ok(())
}

async fn cmd_check(path: &Path, json: bool) -> Result<()> {
    let bytes = tokio::fs::read(path).await.with_context(|| format!("read {}", path.display()))?;
    let mut cf = CompoundFile::open(bytes)?;
    let problems = altium::pcb::lint::check_pcblib(&mut cf)?;
    if json {
        let list: Vec<serde_json::Value> = problems
            .iter()
            .map(|p| serde_json::json!({"footprint": p.footprint, "message": p.message}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else {
        for p in &problems {
            match &p.footprint {
                Some(f) => println!("{f}: {}", p.message),
                None => println!("library: {}", p.message),
            }
        }
        println!("{} problem(s)", problems.len());
    }
    if problems.is_empty() {
        Ok(())
    } else {
        std::process::exit(1)
    }
}

async fn cmd_records(path: &Path, component: &str) -> Result<()> {
    use altium::pcb::records::{kind_name, split_footprint_records};
    let bytes = tokio::fs::read(path).await.with_context(|| format!("read {}", path.display()))?;
    let mut cf = CompoundFile::open(bytes)?;
    let stream = format!("{component}/Data");
    let data = cf
        .try_read_stream(&stream)?
        .ok_or_else(|| anyhow!("no stream {stream}"))?;
    let (name, records) = split_footprint_records(&data)?;
    println!("{name}: {} records", records.len());
    for (i, r) in records.iter().enumerate() {
        let body = r.main_block();
        let hex: String = body.iter().map(|b| format!("{b:02x}")).collect();
        let text = r.text().map(|t| format!(" {t:?}")).unwrap_or_default();
        println!("#{i} {} layer {} ({} bytes){text}: {hex}", kind_name(r.kind), r.layer().unwrap_or(0), body.len());
    }
    Ok(())
}

async fn cmd_stream(path: &Path, stream: &str) -> Result<()> {
    let bytes = tokio::fs::read(path).await.with_context(|| format!("read {}", path.display()))?;
    let mut cf = CompoundFile::open(bytes)?;
    let data = cf.try_read_stream(stream)?.ok_or_else(|| anyhow!("no stream {stream}"))?;
    println!("{stream}: {} bytes", data.len());
    for (i, chunk) in data.chunks(32).enumerate() {
        let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
        println!("{:06x}  {hex:<64}  {ascii}", i * 32);
    }
    Ok(())
}
