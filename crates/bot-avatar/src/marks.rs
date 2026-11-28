//! Marks: what a body can give way to, the three dots of a thinking bot and the exclamation mark
//! of a blocked one (port of `marks.ts`).

use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::geom::{BoxF, PathCmd};
use crate::jsmath::{self, sin};

const TAU: f64 = std::f64::consts::PI * 2.0;

/// One part of a mark: a fixed outline about its own centre, and where that centre is this frame
/// (`MarkPart`). The outline is numeric; [`MarkPart::d`] writes it as the TS does.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkPart {
    /// Stable name of the outline, e.g. "dot".
    pub shape: String,
    /// The outline about the part's centre, absolute commands.
    pub cmds: Arc<Vec<PathCmd>>,
    pub bbox: BoxF,
    /// Centre, viewBox units, a rotation about the mark's centre (radians) and a scale about the part's own.
    pub x: f64,
    pub y: f64,
    pub turn: f64,
    pub scale: f64,
    /// The outline is a circle or an upright capsule that fills `bbox`.
    pub round: bool,
}

impl MarkPart {
    /// The outline as SVG path text (`MarkPart.d`).
    pub fn d(&self) -> String {
        crate::geom::path_d(&self.cmds)
    }
}

/// What a mark is laid out from (`MarkContext`).
#[derive(Clone, Copy, Debug)]
pub struct MarkContext {
    /// The length every mark is sized in, viewBox units.
    pub unit: f64,
    /// The animation clock, seconds, offset per avatar. 0 when `still`.
    pub time: f64,
    /// Hold the mark at rest (reduced motion).
    pub still: bool,
}

/// A mark (`AvatarMark`). Experimental upstream; the two built-ins are [`bang`] and [`dots`].
pub trait AvatarMark: Send + Sync {
    /// Names the mark; a pose lists its marks in the order of their ids.
    fn id(&self) -> &str;
    /// The mark's parts fully grown, `x` and `y` measured from the mark's centre.
    fn parts(&self, ctx: &MarkContext) -> Vec<MarkPart>;
}

/// A shared mark.
pub type MarkRef = Arc<dyn AvatarMark>;

/// A mark in a pose, and how far the body has given way to it, 0..1 (`PoseMark`).
#[derive(Clone)]
pub struct PoseMark {
    pub mark: MarkRef,
    pub weight: f64,
}

impl fmt::Debug for PoseMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PoseMark")
            .field("mark", &self.mark.id())
            .field("weight", &self.weight)
            .finish()
    }
}

impl PartialEq for PoseMark {
    fn eq(&self, other: &Self) -> bool {
        self.mark.id() == other.mark.id() && self.weight == other.weight
    }
}

/// `circle(r)`: `M r 0 A r r 0 1 1 -r 0 A r r 0 1 1 r 0 Z`.
pub fn circle(r: f64) -> Vec<PathCmd> {
    vec![
        PathCmd::M { x: r, y: 0.0 },
        PathCmd::A {
            rx: r,
            ry: r,
            rot: 0.0,
            large: true,
            sweep: true,
            x: -r,
            y: 0.0,
        },
        PathCmd::A {
            rx: r,
            ry: r,
            rot: 0.0,
            large: true,
            sweep: true,
            x: r,
            y: 0.0,
        },
        PathCmd::Z,
    ]
}

/// `capsule(w, h)`: an upright capsule about the origin.
pub fn capsule(w: f64, h: f64) -> Vec<PathCmd> {
    let r = w / 2.0;
    let y = h / 2.0 - r;
    vec![
        PathCmd::M { x: -r, y: -y },
        PathCmd::A {
            rx: r,
            ry: r,
            rot: 0.0,
            large: false,
            sweep: true,
            x: r,
            y: -y,
        },
        PathCmd::L { x: r, y },
        PathCmd::A {
            rx: r,
            ry: r,
            rot: 0.0,
            large: false,
            sweep: true,
            x: -r,
            y,
        },
        PathCmd::Z,
    ]
}

/// The exclamation mark (`BANG`): it jumps and shakes every couple of seconds.
pub struct Bang;

impl AvatarMark for Bang {
    fn id(&self) -> &str {
        "bang"
    }

    fn parts(&self, ctx: &MarkContext) -> Vec<MarkPart> {
        let (u, t, still) = (ctx.unit, ctx.time, ctx.still);
        let bar_w = 0.21 * u;
        let bar_h = 0.5 * u;
        let dot = 0.115 * u;
        let gap = 0.1 * u;
        let total = bar_h + gap + 2.0 * dot;
        let p = if still {
            1.0
        } else {
            ((t % 2.6) + 2.6) % 2.6 / 2.6
        };
        let alarm = if p < 0.3 { p / 0.3 } else { 1.0 };
        let jump = if still {
            0.0
        } else {
            sin(std::f64::consts::PI * alarm)
        };
        let turn = if still {
            0.0
        } else {
            0.2 * sin(TAU * 3.0 * alarm) * (1.0 - alarm)
        };
        let lift = -0.09 * u * jump;
        let bar_y = -total / 2.0 + bar_h / 2.0;
        let dot_y = total / 2.0 - dot;
        vec![
            MarkPart {
                shape: "bar".into(),
                cmds: Arc::new(capsule(bar_w, bar_h)),
                bbox: BoxF {
                    x: -bar_w / 2.0,
                    y: -bar_h / 2.0,
                    w: bar_w,
                    h: bar_h,
                },
                x: 0.0,
                y: bar_y + lift,
                turn,
                scale: 1.0,
                round: true,
            },
            MarkPart {
                shape: "dot".into(),
                cmds: Arc::new(circle(dot)),
                bbox: BoxF {
                    x: -dot,
                    y: -dot,
                    w: 2.0 * dot,
                    h: 2.0 * dot,
                },
                x: 0.0,
                y: dot_y + lift * 0.45,
                turn,
                scale: 1.0,
                round: true,
            },
        ]
    }
}

/// The three dots (`DOTS`): they take turns hopping.
pub struct Dots;

impl AvatarMark for Dots {
    fn id(&self) -> &str {
        "dots"
    }

    fn parts(&self, ctx: &MarkContext) -> Vec<MarkPart> {
        let (u, t, still) = (ctx.unit, ctx.time, ctx.still);
        let r = 0.1 * u;
        let step = 0.31 * u;
        let outline = Arc::new(circle(r));
        (0..3)
            .map(|i| {
                let i = i as f64;
                let p = if still {
                    0.0
                } else {
                    (((t - i * 0.17) % 1.25) + 1.25) % 1.25 / 1.25
                };
                let hop = if p < 0.4 {
                    sin(std::f64::consts::PI * (p / 0.4))
                } else {
                    0.0
                };
                MarkPart {
                    shape: "dot3".into(),
                    cmds: outline.clone(),
                    bbox: BoxF {
                        x: -r,
                        y: -r,
                        w: 2.0 * r,
                        h: 2.0 * r,
                    },
                    x: (i - 1.0) * step,
                    y: -0.16 * u * hop,
                    turn: 0.0,
                    scale: 1.0 + 0.12 * hop,
                    round: true,
                }
            })
            .collect()
    }
}

/// The built-in exclamation mark.
pub fn bang() -> MarkRef {
    static BANG: OnceLock<MarkRef> = OnceLock::new();
    BANG.get_or_init(|| Arc::new(Bang)).clone()
}

/// The built-in three dots.
pub fn dots() -> MarkRef {
    static DOTS: OnceLock<MarkRef> = OnceLock::new();
    DOTS.get_or_init(|| Arc::new(Dots)).clone()
}

fn by_id(m: &PoseMark, n: &PoseMark) -> std::cmp::Ordering {
    m.mark.id().cmp(n.mark.id())
}

/// The marks of a pose part way between two, each weight mixed, in the order of their ids
/// (`blendMarks`).
pub fn blend_marks(a: &[PoseMark], b: &[PoseMark], mix: impl Fn(f64, f64) -> f64) -> Vec<PoseMark> {
    let weight = |list: &[PoseMark], id: &str| {
        list.iter()
            .find(|m| m.mark.id() == id)
            .map_or(0.0, |m| m.weight)
    };
    let mut marks: Vec<MarkRef> = b.iter().map(|m| m.mark.clone()).collect();
    for m in a {
        if !b.iter().any(|n| n.mark.id() == m.mark.id()) {
            marks.push(m.mark.clone());
        }
    }
    let mut out: Vec<PoseMark> = marks
        .into_iter()
        .map(|mark| {
            let w = mix(weight(a, mark.id()), weight(b, mark.id()));
            PoseMark { mark, weight: w }
        })
        .filter(|m| m.weight > 0.0)
        .collect();
    out.sort_by(by_id);
    out
}

/// The marks a state's pose gave, as a pose holds them: one per id, in id order, each weight
/// within 0..1; entries whose weight does not count are left out (`poseMarks`).
pub fn pose_marks(given: &[PoseMark]) -> Vec<PoseMark> {
    let mut marks: Vec<PoseMark> = Vec::new();
    for entry in given {
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let counts = entry.weight > 0.0;
        if !counts || marks.iter().any(|m| m.mark.id() == entry.mark.id()) {
            continue;
        }
        marks.push(PoseMark {
            mark: entry.mark.clone(),
            weight: jsmath::min(1.0, entry.weight),
        });
    }
    marks.sort_by(by_id);
    marks
}

/// The parts a mark laid out, as the renderer can draw them (`markPartsOf`): a part without a
/// name, an outline or a box of some size is left out; a place, turn or scale that is not finite
/// is that of a part at rest.
pub fn mark_parts_of(given: Vec<MarkPart>) -> Vec<MarkPart> {
    let number = |v: f64, or: f64| if v.is_finite() { v } else { or };
    given
        .into_iter()
        .filter(|p| {
            !p.cmds.is_empty()
                && [p.bbox.x, p.bbox.y, p.bbox.w, p.bbox.h]
                    .iter()
                    .all(|v| v.is_finite())
                && p.bbox.w > 0.0
                && p.bbox.h > 0.0
        })
        .map(|p| MarkPart {
            x: number(p.x, 0.0),
            y: number(p.y, 0.0),
            turn: number(p.turn, 0.0),
            scale: number(p.scale, 1.0),
            ..p
        })
        .collect()
}
