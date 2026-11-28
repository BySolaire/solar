//! Embedded definitions: the upstream gallery (`gallery/*.json`, open-bot-avatar commit e10afe4,
//! MIT, Dominik Scholz) and the Solar avatars (`solar/*.json`, see `solar/LICENSE-MIT.txt`).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::validate::AvatarDefinition;

/// `(id, JSON)` of every upstream gallery definition, sorted by id.
const GALLERY: &[(&str, &str)] = &[
    ("book", include_str!("../gallery/book.json")),
    ("brain", include_str!("../gallery/brain.json")),
    ("bull", include_str!("../gallery/bull.json")),
    ("bunny", include_str!("../gallery/bunny.json")),
    ("camera", include_str!("../gallery/camera.json")),
    ("cat", include_str!("../gallery/cat.json")),
    ("cloud", include_str!("../gallery/cloud.json")),
    ("egg", include_str!("../gallery/egg.json")),
    ("ghost", include_str!("../gallery/ghost.json")),
    ("magnifier", include_str!("../gallery/magnifier.json")),
    ("paint-brush", include_str!("../gallery/paint-brush.json")),
    ("palette", include_str!("../gallery/palette.json")),
    ("rocket", include_str!("../gallery/rocket.json")),
    ("shield", include_str!("../gallery/shield.json")),
    ("star", include_str!("../gallery/star.json")),
    ("witch-hat", include_str!("../gallery/witch-hat.json")),
];

/// `(id, JSON)` of every Solar avatar, sorted by id.
const SOLAR: &[(&str, &str)] = &[
    ("cat", include_str!("../solar/cat.json")),
    ("crab", include_str!("../solar/crab.json")),
    ("dino", include_str!("../solar/dino.json")),
    ("frog", include_str!("../solar/frog.json")),
    ("ghost", include_str!("../solar/ghost.json")),
    ("invader", include_str!("../solar/invader.json")),
    ("mushroom", include_str!("../solar/mushroom.json")),
    ("robot", include_str!("../solar/robot.json")),
    ("rocket", include_str!("../solar/rocket.json")),
    ("skull", include_str!("../solar/skull.json")),
];

const GALLERY_COLORS: &str = include_str!("../gallery/colors.json");

fn parse(list: &'static [(&'static str, &'static str)]) -> Vec<AvatarDefinition> {
    list.iter()
        .filter_map(|(_, json)| serde_json::from_str(json).ok())
        .collect()
}

/// The upstream gallery definitions, sorted by id (16 avatars).
pub fn gallery() -> &'static [AvatarDefinition] {
    static ALL: OnceLock<Vec<AvatarDefinition>> = OnceLock::new();
    ALL.get_or_init(|| parse(GALLERY))
}

/// One gallery definition by id.
pub fn gallery_avatar(id: &str) -> Option<&'static AvatarDefinition> {
    gallery().iter().find(|d| d.id == id)
}

/// The colour upstream shows each example and gallery avatar in (`gallery/colors.json`).
pub fn gallery_color(id: &str) -> Option<&'static str> {
    static COLORS: OnceLock<HashMap<String, String>> = OnceLock::new();
    COLORS
        .get_or_init(|| serde_json::from_str(GALLERY_COLORS).unwrap_or_default())
        .get(id)
        .map(String::as_str)
}

/// The ten Solar avatars, sorted by id.
pub fn solar_avatars() -> &'static [AvatarDefinition] {
    static ALL: OnceLock<Vec<AvatarDefinition>> = OnceLock::new();
    ALL.get_or_init(|| parse(SOLAR))
}

/// One Solar avatar by id (invader, ghost, robot, cat, skull, crab, mushroom, rocket, dino, frog).
pub fn solar_avatar(id: &str) -> Option<&'static AvatarDefinition> {
    solar_avatars().iter().find(|d| d.id == id)
}

/// The raw JSON of a gallery or Solar definition, for `validate_avatar`.
pub fn definition_json(id: &str) -> Option<&'static str> {
    GALLERY
        .iter()
        .chain(SOLAR.iter())
        .find(|(n, _)| *n == id)
        .map(|(_, j)| *j)
}
