//! Small cross-platform headless scene driver. No native runner or IPC.
use rndr_core::{
    set_parent, App, Color, Component, Entity, Hidden, Label, Layout, Size, Viewport, World,
};
use rndr_render::{
    capture::{self, Target},
    encode_png,
    observe::{ObservationContent, ObservationSensitive},
    Exporter,
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::PathBuf,
};

const HELP: &str = "rndr-capture --scene FILE --output PREFIX (--ui-id U64 | --entity INDEX:GEN) [--parent INDEX:GEN]\nRenders a public declarative scene without a window. Writes PREFIX.png and PREFIX.json (never overwrites).\nSee docs/component-capture.md for the bounded scene format, privacy and composite semantics.";
#[derive(Debug)]
struct Args {
    scene: PathBuf,
    output: PathBuf,
    key: Option<u64>,
    entity: Option<(u32, u32)>,
    parent: Option<(u32, u32)>,
}
fn pair(s: &str) -> Result<(u32, u32), String> {
    let (a, b) = s.split_once(':').ok_or("expected INDEX:GEN")?;
    Ok((
        a.parse().map_err(|_| "invalid entity index")?,
        b.parse().map_err(|_| "invalid entity generation")?,
    ))
}
fn args(values: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut it = values.into_iter();
    let (mut scene, mut output, mut key, mut entity, mut parent) = (None, None, None, None, None);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or("missing option value")?;
        match flag.as_str() {
            "--scene" if scene.is_none() => scene = Some(PathBuf::from(value)),
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            "--ui-id" if key.is_none() => {
                key = Some(value.parse::<u64>().map_err(|_| "invalid UI key")?)
            }
            "--entity" if entity.is_none() => entity = Some(pair(&value)?),
            "--parent" if parent.is_none() => parent = Some(pair(&value)?),
            _ => return Err("unknown or repeated option".into()),
        }
    }
    if key.is_some() == entity.is_some() || parent.is_some() && key.is_none() {
        return Err("choose exactly one target; parent requires UI key".into());
    }
    Ok(Args {
        scene: scene.ok_or("missing --scene")?,
        output: output.ok_or("missing --output")?,
        key,
        entity,
        parent,
    })
}
#[derive(Clone, Copy)]
struct Key(u64);
impl Component for Key {}
fn ids(w: &World, e: Entity) -> Option<u64> {
    w.get::<Key>(e).map(|k| k.0)
}
fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let map = value.as_object().ok_or("expected object")?;
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("unknown scene field".into());
    }
    Ok(())
}
fn numbers<const N: usize>(v: &Value) -> Result<[f32; N], String> {
    let a = v.as_array().ok_or("expected numeric array")?;
    if a.len() != N {
        return Err("wrong numeric array length".into());
    }
    let mut result = [0.0; N];
    for (to, from) in result.iter_mut().zip(a) {
        *to = from.as_f64().ok_or("expected number")? as f32;
        if !to.is_finite() {
            return Err("nonfinite number".into());
        }
    }
    Ok(result)
}
fn flag(node: &Value, name: &str) -> Result<bool, String> {
    match node.get(name) {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| "expected boolean".into()),
    }
}
fn scene(value: &Value) -> Result<(App, Vec<Entity>), String> {
    fields(value, &["version", "public", "viewport", "nodes"])?;
    if value["version"] != 1 {
        return Err("unsupported scene version".into());
    }
    let [width, height, scale] = numbers::<3>(&value["viewport"])?;
    if width <= 0.0 || height <= 0.0 || scale <= 0.0 {
        return Err("invalid viewport".into());
    }
    let nodes = value["nodes"].as_array().ok_or("missing nodes")?;
    if nodes.len() > 256 {
        return Err("too many scene nodes".into());
    }
    let public = flag(value, "public")?;
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport {
        width,
        height,
        scale,
        ..Viewport::default()
    });
    app.world_mut().insert_resource(if public {
        ObservationContent::Public
    } else {
        ObservationContent::Deny
    });
    let mut entities = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        fields(
            node,
            &[
                "id",
                "parent",
                "size",
                "padding",
                "gap",
                "color",
                "label",
                "hidden",
                "sensitive",
            ],
        )?;
        let [w, h] = numbers::<2>(&node["size"])?;
        if w < 0.0 || h < 0.0 {
            return Err("negative layout size".into());
        }
        let mut layout = Layout {
            width: Size::Px(w),
            height: Size::Px(h),
            padding: 0.0,
            gap: 0.0,
            ..Layout::column()
        };
        for (name, to) in [("padding", &mut layout.padding), ("gap", &mut layout.gap)] {
            if let Some(v) = node.get(name) {
                *to = v.as_f64().ok_or("invalid spacing")? as f32;
                if !to.is_finite() || *to < 0.0 {
                    return Err("invalid spacing".into());
                }
            }
        }
        let e = app.world_mut().spawn((layout,));
        if let Some(id) = node.get("id") {
            let id = id
                .as_str()
                .ok_or("UI id must be a decimal string")?
                .parse::<u64>()
                .map_err(|_| "invalid UI id")?;
            app.world_mut().insert(e, Key(id));
        }
        if let Some(parent) = node.get("parent") {
            let p = usize::try_from(parent.as_u64().ok_or("invalid parent node index")?)
                .map_err(|_| "invalid parent node index")?;
            if p >= index {
                return Err("parent must precede child".into());
            }
            set_parent(app.world_mut(), e, entities[p]);
        }
        if let Some(color) = node.get("color") {
            let [r, g, b, a] = numbers::<4>(color)?;
            if [r, g, b, a].iter().any(|x| !(0.0..=1.0).contains(x)) {
                return Err("invalid color".into());
            }
            app.world_mut().insert(e, Color { r, g, b, a });
        }
        if let Some(label) = node.get("label") {
            let label = label.as_str().ok_or("expected label string")?;
            if label.len() > 4096 {
                return Err("label too long".into());
            }
            app.world_mut().insert(e, Label::new(label));
        }
        if flag(node, "hidden")? {
            app.world_mut().insert(e, Hidden);
        }
        if flag(node, "sensitive")? {
            app.world_mut().insert(e, ObservationSensitive);
        }
        entities.push(e);
    }
    Ok((app, entities))
}
fn resolve(entities: &[Entity], pair: (u32, u32)) -> Result<Entity, (i32, String)> {
    entities
        .iter()
        .copied()
        .find(|e| (e.index(), e.generation()) == pair)
        .ok_or((3, "stale_identity".into()))
}
fn create_output(path: &std::ffi::OsStr) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn write_output(prefix: &std::path::Path, image: &[u8], metadata: &[u8]) -> Result<(), String> {
    // Append rather than replace an extension. Never overwrite captures/source.
    let mut png = prefix.as_os_str().to_os_string();
    png.push(".png");
    let mut json = prefix.as_os_str().to_os_string();
    json.push(".json");
    let mut a = create_output(&png).map_err(|_| "cannot create PNG output")?;
    let result = (|| {
        let mut b = create_output(&json).map_err(|_| "cannot create metadata output")?;
        let written = a
            .write_all(image)
            .and_then(|_| b.write_all(metadata))
            .and_then(|_| a.sync_all())
            .and_then(|_| b.sync_all());
        drop(b);
        if written.is_err() {
            let _ = std::fs::remove_file(&json);
            return Err("cannot write capture output");
        }
        Ok(())
    })();
    drop(a);
    if result.is_err() {
        let _ = std::fs::remove_file(&png);
    }
    result.map_err(str::to_owned)
}
fn run(a: Args) -> Result<(), (i32, String)> {
    let file = std::fs::File::open(a.scene).map_err(|_| (2, "cannot read scene".into()))?;
    let mut bytes = Vec::new();
    file.take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| (2, "cannot read scene".into()))?;
    if bytes.len() > 65_536 {
        return Err((2, "scene exceeds 64 KiB".into()));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| (2, "invalid scene JSON".into()))?;
    let (mut app, entities) = scene(&value).map_err(|e| (2, e))?;
    // Privacy precedes target lookup, including diagnostics, just like the API.
    if !matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Public)
    ) || !app
        .world()
        .entities_with::<ObservationSensitive>()
        .is_empty()
    {
        return Err((4, "sensitive_frame".into()));
    }
    let target = if let Some(id) = a.key {
        Target::UiId {
            id,
            parent: a.parent.map(|p| resolve(&entities, p)).transpose()?,
            expected: None,
        }
    } else {
        Target::Entity(resolve(&entities, a.entity.unwrap())?)
    };
    let mut exporter = Exporter::new().map_err(|_| (5, "headless_adapter_unavailable".into()))?;
    let captured = capture::capture(&mut app, &mut exporter, target, ids).map_err(|e| {
        let code = match e {
            capture::Error::Sensitive => 4,
            capture::Error::Export(_) => 5,
            _ => 3,
        };
        (code, e.to_string())
    })?;
    let png = encode_png(&captured.image).map_err(|_| (5, "PNG encode failed".into()))?;
    let json = serde_json::to_vec_pretty(&captured.metadata.to_json())
        .map_err(|_| (5, "metadata encode failed".into()))?;
    write_output(&a.output, &png, &json).map_err(|e| (1, e))?;
    println!(
        "captured {}x{} offscreen pixels",
        captured.image.width, captured.image.height
    );
    Ok(())
}
fn main() {
    let values: Vec<_> = std::env::args().skip(1).collect();
    if values == ["--help"] || values == ["-h"] {
        println!("{HELP}");
        return;
    }
    match args(values).map_err(|e| (2, e)).and_then(run) {
        Ok(()) => (),
        Err((code, error)) => {
            eprintln!("{error}");
            std::process::exit(code);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> Result<Args, String> {
        args(s.split_whitespace().map(str::to_owned))
    }
    #[test]
    fn arguments_are_strict() {
        assert!(parse("--scene a --output b --ui-id 7").is_ok());
        for bad in [
            "",
            "--scene a --output b",
            "--scene a --output b --ui-id 7 --entity 0:0",
            "--scene a --output b --entity 0:0 --parent 1:0",
            "--scene a --output b --ui-id NaN",
            "--scene a --output b --entity 2",
            "--scene a --scene b --ui-id 1 --output c",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn scene_rejects_unsupported_input() {
        let value = serde_json::json!({"version":1,"public":true,"viewport":[40,40,1],"nodes":[{"id":"7","size":[10,10],"parent":0}]});
        assert!(scene(&value).is_err());
        let value = serde_json::json!({"version":1,"public":true,"viewport":[40,40,0],"nodes":[]});
        assert!(scene(&value).is_err());
        assert!(scene(&serde_json::json!({"version":2})).is_err());
    }
}
