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
        /// Destination `.PcbDoc`. Use `-` to print a summary without writing.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Extra directories to search for sub-board files. Repeatable.
        #[arg(long)]
        search_path: Vec<PathBuf>,
    },
    /// Extract the netlist from a `.PcbDoc` or `.SchDoc`. PCB extraction is
    /// explicit (each pad carries its net name); SchDoc extraction traces
    /// the wire graph and uses net labels / power ports for naming.
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
        Command::Split {
            path,
            out_dir,
            name,
        } => cmd_split(&path, &out_dir, name.as_deref()).await,
        Command::Netlist {
            path,
            output,
            format,
        } => cmd_netlist(&path, output.as_deref(), &format).await,
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

    // `-o -` (or equivalent) prints stats only and skips the write. Anything
    // else writes a fresh `.PcbDoc` byte stream.
    let Some(out_path) = output else {
        return Ok(());
    };
    if out_path.as_os_str() == "-" {
        return Ok(());
    }

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
) -> Result<()> {
    use altium::Netlist;
    let file = AltiumFile::read(path).await?;
    let netlist = match file {
        AltiumFile::PcbDocument(doc) => Netlist::from_pcb_document(&doc),
        AltiumFile::SchDocument(doc) => Netlist::from_sch_document(&doc),
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
