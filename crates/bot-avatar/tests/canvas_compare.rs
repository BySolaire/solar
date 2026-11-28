//! Painter cross-check, step 2 (see tests/golden/canvas/README.md): paints the cases of
//! `<OBA_CANVAS_DIR>/cases.json` with `bot_avatar::paint` into `<dir>/rust/<name>.rgba`
//! (unpremultiplied RGBA, `size` × `size`) for the browser page to compare.

mod common;

use bot_avatar::{example, gallery_avatar, paint, solar_avatar, PreparedAvatar, Tuning};
use serde_json::Value;

#[test]
#[ignore = "needs OBA_CANVAS_DIR from tests/golden/canvas/frames.mjs"]
fn canvas_compare() {
    let dir = std::path::PathBuf::from(std::env::var("OBA_CANVAS_DIR").expect("OBA_CANVAS_DIR"));
    let cases: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("cases.json")).expect("cases.json"))
            .expect("json");
    for case in cases.as_array().expect("cases") {
        let id = case["id"].as_str().unwrap_or("");
        let def = match case["source"].as_str() {
            Some("example") => example(id),
            Some("solar") => solar_avatar(id).cloned(),
            _ => gallery_avatar(id).cloned(),
        }
        .expect("definition");
        let mut o = common::options_of(&case["o"]);
        o.tuning = Tuning::Upstream;
        let p = PreparedAvatar::new(&def, o.depth).expect("valid");
        let px = paint(
            &p.avatar_frame(&o),
            case["size"].as_u64().unwrap_or(160) as u32,
        );
        let mut rgba = Vec::with_capacity(px.data().len());
        for c in px.pixels() {
            let c = c.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        std::fs::write(
            dir.join("rust")
                .join(format!("{}.rgba", case["name"].as_str().unwrap_or("case"))),
            rgba,
        )
        .expect("write");
    }
}
