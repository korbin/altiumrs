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

fn kind_label(file: &AltiumFile) -> &'static str {
    match file {
        AltiumFile::PcbLibrary(_) => ".PcbLib",
        AltiumFile::SchLibrary(_) => ".SchLib",
        AltiumFile::PcbDocument(_) => ".PcbDoc",
        AltiumFile::SchDocument(_) => ".SchDoc",
    }
}
