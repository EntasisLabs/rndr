//! Read-only viewer for a saved Create scene.
//!
//! The scene types are a mirror of schema `medousa-create` version 3. A git
//! dependency on `medousa-types` would pull that crate's whole module tree into
//! every workspace check, and `medousa-sdk` would pull the daemon client with
//! it. Fixtures in `fixtures/` keep `cargo test` offline. `--export` writes a
//! PNG through the offscreen renderer. Pass a scene path to open it.

mod model;
mod paint;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rndr_core::{TextEngine, World};
use rndr_render::{decode, load_rgba};

pub use model::{
    content_kind, utf16_to_byte, CreativeContent, CreativeNode, CreativeScene, SCHEMA, VERSION,
};
pub use paint::{
    export_scene, paint_scene, parse_color, pixel_rect, scene_bounds, scene_svg, Asset,
};

/// A scene plus the PNG assets it references.
#[derive(Clone, Debug)]
pub struct Project {
    /// Parsed document.
    pub scene: CreativeScene,
    /// `asset_id` to decoded pixels and the original PNG.
    pub assets: BTreeMap<String, Asset>,
}

/// Why a scene file did not load.
#[derive(Debug)]
pub struct LoadError(pub String);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LoadError {}

/// Directory of the committed fixture scenes.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Read a scene. Image nodes load `assets/{id}.png` next to the file.
pub fn load_project(path: &Path) -> Result<Project, LoadError> {
    let text =
        fs::read_to_string(path).map_err(|err| LoadError(format!("{}: {err}", path.display())))?;
    let scene: CreativeScene = serde_json::from_str(&text)
        .map_err(|err| LoadError(format!("{}: {err}", path.display())))?;
    if scene.schema != SCHEMA || scene.version != VERSION {
        return Err(LoadError(format!(
            "{}: expected {SCHEMA} version {VERSION}, found {} {}",
            path.display(),
            scene.schema,
            scene.version
        )));
    }
    let mut assets = BTreeMap::new();
    let dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("assets");
    for node in &scene.nodes {
        let CreativeContent::Image { asset_id, .. } = &node.content else {
            continue;
        };
        if assets.contains_key(asset_id) {
            continue;
        }
        let file = dir.join(format!("{asset_id}.png"));
        let png = fs::read(&file).map_err(|err| LoadError(format!("{}: {err}", file.display())))?;
        let raster =
            decode(&png).map_err(|_| LoadError(format!("{}: not a png", file.display())))?;
        assets.insert(
            asset_id.clone(),
            Asset {
                png,
                width: raster.width,
                height: raster.height,
                rgba: raster.rgba,
            },
        );
    }
    Ok(Project { scene, assets })
}

/// Stable ids for [`load_rgba`](rndr_render::load_rgba), starting at 1.
pub fn image_ids(project: &Project) -> BTreeMap<String, u64> {
    project
        .assets
        .keys()
        .enumerate()
        .map(|(index, key)| (key.clone(), index as u64 + 1))
        .collect()
}

/// Copy decoded assets into `world` and return their ids.
pub fn install_images(
    world: &mut World,
    project: &Project,
) -> Result<BTreeMap<String, u64>, LoadError> {
    let ids = image_ids(project);
    for (key, id) in &ids {
        let asset = &project.assets[key];
        load_rgba(world, *id, asset.width, asset.height, asset.rgba.clone())
            .map_err(|_| LoadError(format!("image {key}")))?;
    }
    Ok(ids)
}

/// Install the bundled text engine when the world does not already have one.
pub fn install_text(world: &mut World) {
    if world.resource::<Arc<dyn TextEngine>>().is_some() {
        return;
    }
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    world.insert_resource(engine);
}

/// Fixture scene paths, sorted by file name.
pub fn fixture_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let dir = fixture_dir();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_parse_and_cover_every_node_kind() {
        let mut kinds = std::collections::BTreeSet::new();
        let mut saw_shadow = false;
        let mut saw_opacity = false;
        let mut saw_rotation = false;
        let mut saw_hidden = false;
        let mut saw_clip = false;
        let mut saw_dash = false;
        let paths = fixture_paths();
        assert!(
            paths.len() >= 5,
            "expected fixture scenes, found {}",
            paths.len()
        );
        for path in &paths {
            let project = load_project(path).unwrap_or_else(|err| panic!("{err}"));
            for node in &project.scene.nodes {
                kinds.insert(content_kind(&node.content));
                if node
                    .appearance
                    .as_ref()
                    .and_then(|item| item.shadow.as_ref())
                    .is_some()
                {
                    saw_shadow = true;
                }
                if node.opacity < 0.999 {
                    saw_opacity = true;
                }
                if node.transform.rotation != 0.0 {
                    saw_rotation = true;
                }
                if !node.visible {
                    saw_hidden = true;
                }
                if matches!(node.content, CreativeContent::Frame { .. })
                    && node
                        .appearance
                        .as_ref()
                        .is_some_and(|item| item.corner_radius > 0.0)
                {
                    saw_clip = true;
                }
                if let CreativeContent::Shape {
                    line: Some(line), ..
                } = &node.content
                {
                    if line.dashed && (line.start_arrow || line.end_arrow) {
                        saw_dash = true;
                    }
                }
            }
        }
        for kind in [
            "rectangle",
            "ellipse",
            "line",
            "ink",
            "text",
            "image",
            "group",
            "frame",
        ] {
            assert!(kinds.contains(kind), "missing {kind}");
        }
        assert!(saw_shadow && saw_opacity && saw_rotation && saw_hidden && saw_clip && saw_dash);
    }

    #[test]
    fn a_dashed_arrow_svg_keeps_the_head_out_of_the_dash() {
        let project = load_project(&fixture_dir().join("arrows.json")).unwrap();
        let svg = scene_svg(
            &project.scene,
            &project.assets,
            pixel_rect(scene_bounds(&project.scene)),
        );
        assert!(svg.contains("stroke-dasharray="), "{svg}");
        assert!(svg.contains("data-arrowhead"), "{svg}");
        let shapes = load_project(&fixture_dir().join("shapes.json")).unwrap();
        let svg = scene_svg(
            &shapes.scene,
            &shapes.assets,
            pixel_rect(scene_bounds(&shapes.scene)),
        );
        assert!(
            !svg.contains("data-node-id=\"hidden\""),
            "a hidden node was emitted"
        );
    }
}
