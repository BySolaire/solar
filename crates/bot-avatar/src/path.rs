//! Restricted SVG path geometry: parser, normalizer and flattener (port of `path.ts`).
//!
//! Accepts the path command set M L H V C S Q T A Z (absolute and relative) and nothing else.
//! The parsed result is re-serialized from numbers, so the renderer never echoes model-written
//! text into markup.

use std::cell::RefCell;
use std::fmt;

use crate::jsmath::{self, number_to_string};

/// A point in viewBox units.
pub type Point = [f64; 2];

/// One normalized, absolute path segment (`Segment` in path.ts).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    M {
        x: f64,
        y: f64,
    },
    L {
        x: f64,
        y: f64,
    },
    C {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Q {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    A {
        rx: f64,
        ry: f64,
        rot: f64,
        large: bool,
        sweep: bool,
        x: f64,
        y: f64,
    },
    Z,
}

/// `PATH_LIMITS`.
pub mod limits {
    pub const MAX_CHARS: usize = 6000;
    pub const MAX_SEGMENTS: usize = 400;
    pub const MAX_SUBPATHS: usize = 16;
    pub const COORD_MIN: f64 = -20.0;
    pub const COORD_MAX: f64 = 120.0;
}

/// A path that cannot be parsed (`PathError`). The message is the TS one, word for word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathError(pub String);

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PathError {}

fn err<T>(msg: impl Into<String>) -> Result<T, PathError> {
    Err(PathError(msg.into()))
}

/// JavaScript's `\s` (WhiteSpace and LineTerminator).
pub(crate) fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(js_space)
}

/// `JSON.stringify` of a string.
pub(crate) fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// `JSON.stringify` of one UTF-16 code unit of `c` (the first, as a non-unicode regex matches it).
fn json_code_unit(c: char) -> String {
    let mut buf = [0u16; 2];
    let units = c.encode_utf16(&mut buf);
    if units.len() == 1 {
        json_string(&c.to_string())
    } else {
        format!("\"\\u{:04x}\"", units[0])
    }
}

const COMMANDS: &str = "MmLlHhVvCcSsQqTtAaZz";

#[derive(Clone, Copy, Debug)]
enum Token {
    Cmd(u8),
    Num(f64),
}

fn allowed(c: char) -> bool {
    COMMANDS.contains(c)
        || c.is_ascii_digit()
        || matches!(c, '.' | ',' | 'e' | 'E' | '+' | '-')
        || js_space(c)
}

/// Length of the number token at the start of `s` (the regex
/// `[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?`), or 0.
fn number_len(s: &[u8]) -> usize {
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let digits = |i: usize| -> usize {
        let mut j = i;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    let j = digits(i);
    if j > i {
        i = j;
        if i < s.len() && s[i] == b'.' {
            i = digits(i + 1);
        }
    } else if i < s.len() && s[i] == b'.' {
        let j = digits(i + 1);
        if j == i + 1 {
            return 0;
        }
        i = j;
    } else {
        return 0;
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut k = i + 1;
        if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
            k += 1;
        }
        let j = digits(k);
        if j > k {
            i = j;
        }
    }
    i
}

fn tokenize(d: &str) -> Result<Vec<Token>, PathError> {
    if let Some(bad) = d.chars().find(|&c| !allowed(c)) {
        return err(format!(
            "unsupported character {} (allowed commands: M L H V C S Q T A Z)",
            json_code_unit(bad)
        ));
    }
    let mut tokens = Vec::new();
    let mut i = 0;
    let bytes = d.as_bytes();
    while i < d.len() {
        let rest = &d[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if COMMANDS.contains(c) {
            tokens.push(Token::Cmd(c as u8));
            i += 1;
            continue;
        }
        let n = number_len(&bytes[i..]);
        if n > 0 {
            let v: f64 = d[i..i + n].parse().unwrap_or(f64::NAN);
            tokens.push(Token::Num(v));
            i += n;
            continue;
        }
        let mut j = 0;
        for ch in rest.chars() {
            if ch == ',' || js_space(ch) {
                j += ch.len_utf8();
            } else {
                break;
            }
        }
        if j == 0 {
            let near: String = rest.chars().take(12).collect();
            return err(format!("cannot parse path near {}", json_string(&near)));
        }
        i += j;
    }
    Ok(tokens)
}

fn arg_count(up: u8) -> usize {
    match up {
        b'M' | b'L' | b'T' => 2,
        b'H' | b'V' => 1,
        b'C' => 6,
        b'S' | b'Q' => 4,
        b'A' => 7,
        _ => 0,
    }
}

/// Parse and convert to absolute M/L/C/Q/A/Z segments (`parsePath`). `bounded` (the TS default)
/// rejects coordinates far outside the 0..100 viewBox.
pub fn parse_path(d: &str, bounded: bool) -> Result<Vec<Segment>, PathError> {
    if js_trim(d).is_empty() {
        return err("path is empty");
    }
    if d.encode_utf16().count() > limits::MAX_CHARS {
        return err(format!("path longer than {} characters", limits::MAX_CHARS));
    }
    let tokens = tokenize(d)?;
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0;
    let (mut cx, mut cy, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut prev_c: Option<Point> = None;
    let mut prev_q: Option<Point> = None;
    let mut cmd: u8 = 0;
    let mut subpaths = 0usize;
    let mut open = false;

    while i < tokens.len() {
        match tokens[i] {
            Token::Cmd(c) => {
                cmd = c;
                i += 1;
                if cmd == b'Z' || cmd == b'z' {
                    if !open {
                        return err("Z without an open subpath");
                    }
                    out.push(Segment::Z);
                    cx = sx;
                    cy = sy;
                    prev_c = None;
                    prev_q = None;
                    open = false;
                    continue;
                }
            }
            Token::Num(_) => {
                if cmd == 0 {
                    return err("path must start with M");
                } else if cmd == b'Z' || cmd == b'z' {
                    return err("numbers after Z");
                }
            }
        }
        let up = cmd.to_ascii_uppercase();
        let rel = cmd != up;
        if out.is_empty() && up != b'M' {
            return err("path must start with M");
        }
        let n = arg_count(up);
        let mut args = [0.0f64; 7];
        for (k, arg) in args.iter_mut().enumerate().take(n) {
            match tokens.get(i + k) {
                Some(Token::Num(v)) => {
                    if !v.is_finite() {
                        return err("non-finite number");
                    }
                    *arg = *v;
                }
                _ => return err(format!("command {} expects {} numbers", cmd as char, n)),
            }
        }
        i += n;
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        let mut next_c: Option<Point> = None;
        let mut next_q: Option<Point> = None;
        match up {
            b'M' => {
                if open {
                    out.push(Segment::Z);
                }
                cx = args[0] + ox;
                cy = args[1] + oy;
                sx = cx;
                sy = cy;
                out.push(Segment::M { x: cx, y: cy });
                subpaths += 1;
                open = true;
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                cx = args[0] + ox;
                cy = args[1] + oy;
                out.push(Segment::L { x: cx, y: cy });
            }
            b'H' => {
                cx = args[0] + ox;
                out.push(Segment::L { x: cx, y: cy });
            }
            b'V' => {
                cy = args[0] + oy;
                out.push(Segment::L { x: cx, y: cy });
            }
            b'C' => {
                let (x1, y1, x2, y2, x, y) = (
                    args[0] + ox,
                    args[1] + oy,
                    args[2] + ox,
                    args[3] + oy,
                    args[4] + ox,
                    args[5] + oy,
                );
                out.push(Segment::C {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
                next_c = Some([x2, y2]);
                cx = x;
                cy = y;
            }
            b'S' => {
                let (x1, y1) = match prev_c {
                    Some(p) => (2.0 * cx - p[0], 2.0 * cy - p[1]),
                    None => (cx, cy),
                };
                let (x2, y2, x, y) = (args[0] + ox, args[1] + oy, args[2] + ox, args[3] + oy);
                out.push(Segment::C {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
                next_c = Some([x2, y2]);
                cx = x;
                cy = y;
            }
            b'Q' => {
                let (x1, y1, x, y) = (args[0] + ox, args[1] + oy, args[2] + ox, args[3] + oy);
                out.push(Segment::Q { x1, y1, x, y });
                next_q = Some([x1, y1]);
                cx = x;
                cy = y;
            }
            b'T' => {
                let (x1, y1) = match prev_q {
                    Some(p) => (2.0 * cx - p[0], 2.0 * cy - p[1]),
                    None => (cx, cy),
                };
                let (x, y) = (args[0] + ox, args[1] + oy);
                out.push(Segment::Q { x1, y1, x, y });
                next_q = Some([x1, y1]);
                cx = x;
                cy = y;
            }
            b'A' => {
                let (rx, ry, rot, la, sw) = (args[0], args[1], args[2], args[3], args[4]);
                if rx < 0.0 || ry < 0.0 {
                    return err("arc radii must be non-negative");
                }
                if (la != 0.0 && la != 1.0) || (sw != 0.0 && sw != 1.0) {
                    return err("arc flags must be 0 or 1, written as separate numbers");
                }
                cx = args[5] + ox;
                cy = args[6] + oy;
                out.push(Segment::A {
                    rx,
                    ry,
                    rot,
                    large: la == 1.0,
                    sweep: sw == 1.0,
                    x: cx,
                    y: cy,
                });
            }
            _ => return err(format!("unsupported command {}", cmd as char)),
        }
        prev_c = next_c;
        prev_q = next_q;
        if out.len() > limits::MAX_SEGMENTS {
            return err(format!("more than {} segments", limits::MAX_SEGMENTS));
        }
    }
    if open {
        out.push(Segment::Z);
    }
    if subpaths > limits::MAX_SUBPATHS {
        return err(format!("more than {} subpaths", limits::MAX_SUBPATHS));
    }
    if bounded {
        for s in &out {
            let check = |v: f64| -> Result<(), PathError> {
                if v < limits::COORD_MIN || v > limits::COORD_MAX {
                    return err(format!(
                        "coordinate {} far outside the 0..100 viewBox",
                        number_to_string(v)
                    ));
                }
                Ok(())
            };
            match *s {
                Segment::M { x, y } | Segment::L { x, y } => {
                    check(x)?;
                    check(y)?;
                }
                Segment::C {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    for v in [x1, y1, x2, y2, x, y] {
                        check(v)?;
                    }
                }
                Segment::Q { x1, y1, x, y } => {
                    for v in [x1, y1, x, y] {
                        check(v)?;
                    }
                }
                Segment::A { rx, ry, x, y, .. } => {
                    for v in [rx, ry, x, y] {
                        check(v)?;
                    }
                }
                Segment::Z => {}
            }
        }
    }
    Ok(out)
}

/// Compact absolute serialization built only from parsed numbers (`serializePath`).
pub fn serialize_path(segs: &[Segment]) -> String {
    let mut d = String::new();
    write_segments(&mut d, segs);
    d
}

/// Appends `serializePath(segs)` to `d`.
pub fn write_segments(d: &mut String, segs: &[Segment]) {
    use jsmath::push_f as p;
    for s in segs {
        match *s {
            Segment::M { x, y } => {
                d.push('M');
                p(d, x);
                d.push(' ');
                p(d, y);
            }
            Segment::L { x, y } => {
                d.push('L');
                p(d, x);
                d.push(' ');
                p(d, y);
            }
            Segment::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                d.push('C');
                for (k, v) in [x1, y1, x2, y2, x, y].into_iter().enumerate() {
                    if k > 0 {
                        d.push(' ');
                    }
                    p(d, v);
                }
            }
            Segment::Q { x1, y1, x, y } => {
                d.push('Q');
                for (k, v) in [x1, y1, x, y].into_iter().enumerate() {
                    if k > 0 {
                        d.push(' ');
                    }
                    p(d, v);
                }
            }
            Segment::A {
                rx,
                ry,
                rot,
                large,
                sweep,
                x,
                y,
            } => {
                d.push('A');
                p(d, rx);
                d.push(' ');
                p(d, ry);
                d.push(' ');
                p(d, rot);
                d.push_str(if large { " 1" } else { " 0" });
                d.push_str(if sweep { " 1 " } else { " 0 " });
                p(d, x);
                d.push(' ');
                p(d, y);
            }
            Segment::Z => d.push('Z'),
        }
    }
}

/// Largest gap, viewBox units, between a curve and the polygon that stands for it (`FLATNESS`).
pub const FLATNESS: f64 = 0.02;

/// Endpoint arc → sampled points (SVG implementation notes, F.6.5) (`sampleArc`).
#[allow(clippy::too_many_arguments)]
fn sample_arc(
    x0: f64,
    y0: f64,
    rx0: f64,
    ry0: f64,
    rot: f64,
    large: bool,
    sweep: bool,
    x: f64,
    y: f64,
    out: &mut Vec<Point>,
) {
    let (mut rx, mut ry) = (rx0, ry0);
    if rx == 0.0 || ry == 0.0 || (x0 == x && y0 == y) {
        out.push([x, y]);
        return;
    }
    let phi = (rot * std::f64::consts::PI) / 180.0;
    let cos = jsmath::cos(phi);
    let sin = jsmath::sin(phi);
    let dx = (x0 - x) / 2.0;
    let dy = (y0 - y) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    rx = rx.abs();
    ry = ry.abs();
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = jsmath::max(0.0, num / den).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = (coef * rx * y1p) / ry;
    let cyp = (-coef * ry * x1p) / rx;
    let cx = cos * cxp - sin * cyp + (x0 + x) / 2.0;
    let cy = sin * cxp + cos * cyp + (y0 + y) / 2.0;
    let ang =
        |ux: f64, uy: f64, vx: f64, vy: f64| jsmath::atan2(ux * vy - uy * vx, ux * vx + uy * vy);
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    let tau = 2.0 * std::f64::consts::PI;
    if !sweep && dt > 0.0 {
        dt -= tau;
    }
    if sweep && dt < 0.0 {
        dt += tau;
    }
    let step = 2.0 * jsmath::acos(jsmath::max(-1.0, 1.0 - FLATNESS / jsmath::max(rx, ry)));
    let steps = jsmath::min(256.0, jsmath::max(4.0, (dt.abs() / step).ceil())) as usize;
    for k in 1..=steps {
        let t = t1 + (dt * k as f64) / steps as f64;
        let ex = rx * jsmath::cos(t);
        let ey = ry * jsmath::sin(t);
        out.push([cos * ex - sin * ey + cx, sin * ex + cos * ey + cy]);
    }
}

/// Flatten into closed polygons, one per subpath (`flattenPath`).
pub fn flatten_path(segs: &[Segment]) -> Vec<Vec<Point>> {
    let mut polys: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let (mut px, mut py) = (0.0f64, 0.0f64);
    for s in segs {
        match *s {
            Segment::M { x, y } => {
                if !cur.is_empty() {
                    polys.push(std::mem::take(&mut cur));
                }
                cur = vec![[x, y]];
                px = x;
                py = y;
            }
            Segment::L { x, y } => {
                cur.push([x, y]);
                px = x;
                py = y;
            }
            Segment::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let m = jsmath::max(
                    jsmath::hypot(px - 2.0 * x1 + x2, py - 2.0 * y1 + y2),
                    jsmath::hypot(x1 - 2.0 * x2 + x, y1 - 2.0 * y2 + y),
                );
                let n = jsmath::min(
                    128.0,
                    jsmath::max(2.0, ((0.75 * m) / FLATNESS).sqrt().ceil()),
                ) as usize;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    cur.push([
                        u * u * u * px
                            + 3.0 * u * u * t * x1
                            + 3.0 * u * t * t * x2
                            + t * t * t * x,
                        u * u * u * py
                            + 3.0 * u * u * t * y1
                            + 3.0 * u * t * t * y2
                            + t * t * t * y,
                    ]);
                }
                px = x;
                py = y;
            }
            Segment::Q { x1, y1, x, y } => {
                let n = jsmath::min(
                    128.0,
                    jsmath::max(
                        2.0,
                        ((0.25 * jsmath::hypot(px - 2.0 * x1 + x, py - 2.0 * y1 + y)) / FLATNESS)
                            .sqrt()
                            .ceil(),
                    ),
                ) as usize;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    cur.push([
                        u * u * px + 2.0 * u * t * x1 + t * t * x,
                        u * u * py + 2.0 * u * t * y1 + t * t * y,
                    ]);
                }
                px = x;
                py = y;
            }
            Segment::A {
                rx,
                ry,
                rot,
                large,
                sweep,
                x,
                y,
            } => {
                sample_arc(px, py, rx, ry, rot, large, sweep, x, y, &mut cur);
                px = x;
                py = y;
            }
            Segment::Z => {
                if !cur.is_empty() {
                    px = cur[0][0];
                    py = cur[0][1];
                    polys.push(std::mem::take(&mut cur));
                }
                cur = Vec::new();
            }
        }
    }
    if !cur.is_empty() {
        polys.push(cur);
    }
    polys.retain(|p| p.len() >= 3);
    polys
}

/// Nonzero winding number of a point against all subpaths (`winding`).
pub fn winding(polys: &[Vec<Point>], x: f64, y: f64) -> i32 {
    let mut w = 0;
    for poly in polys {
        let n = poly.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let [xi, yi] = poly[i];
            let [xj, yj] = poly[j];
            if yj <= y {
                if yi > y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) > 0.0 {
                    w += 1;
                }
            } else if yi <= y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) < 0.0 {
                w -= 1;
            }
            j = i;
        }
    }
    w
}

/// `isInside`.
pub fn is_inside(polys: &[Vec<Point>], x: f64, y: f64) -> bool {
    winding(polys, x, y) != 0
}

/// The edges of a set of polygons listed by the horizontal bands they cross (`EdgeBands`): the
/// answers of `winding` / `insideRow` bit for bit, faster.
pub struct EdgeBands {
    e: Vec<f64>,
    start: Vec<i32>,
    items: Vec<i32>,
    y0: f64,
    inv: f64,
    n: i32,
    hits: RefCell<(Vec<f64>, Vec<i8>)>,
}

impl EdgeBands {
    pub fn new(polys: &[Vec<Point>]) -> EdgeBands {
        let count: usize = polys.iter().map(|p| p.len()).sum();
        let mut e = vec![0.0f64; 4 * count];
        let mut k = 0;
        let (mut lo, mut hi, mut climb) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for poly in polys {
            let n = poly.len();
            let mut j = n.wrapping_sub(1);
            for i in 0..n {
                let yj = poly[j][1];
                let yi = poly[i][1];
                e[k] = poly[j][0];
                e[k + 1] = yj;
                e[k + 2] = poly[i][0];
                e[k + 3] = yi;
                k += 4;
                if yj < lo {
                    lo = yj;
                }
                if yj > hi {
                    hi = yj;
                }
                climb += (yi - yj).abs();
                j = i;
            }
        }
        let tall = if climb > 0.0 {
            ((3.0 * count as f64 * (hi - lo)) / climb).floor()
        } else {
            f64::INFINITY
        };
        let n = jsmath::max(
            1.0,
            jsmath::min(jsmath::min(512.0, (count as f64 / 4.0).ceil()), tall),
        ) as i32;
        let y0 = if count > 0 { lo } else { 0.0 };
        let inv = if hi > lo { n as f64 / (hi - lo) } else { 0.0 };
        let mut bands = EdgeBands {
            e,
            start: vec![0; n as usize + 1],
            items: Vec::new(),
            y0,
            inv,
            n,
            hits: RefCell::new((vec![0.0; 16], vec![0; 16])),
        };
        let mut start = vec![0i32; n as usize + 1];
        let mut k = 0;
        while k < bands.e.len() {
            let a = bands.band(jsmath::min(bands.e[k + 1], bands.e[k + 3]));
            let b = bands.band(jsmath::max(bands.e[k + 1], bands.e[k + 3]));
            for q in a..=b {
                start[q as usize + 1] += 1;
            }
            k += 4;
        }
        for q in 0..n as usize {
            start[q + 1] += start[q];
        }
        let mut items = vec![0i32; start[n as usize] as usize];
        let mut next: Vec<i32> = start[..n as usize].to_vec();
        let mut k = 0;
        while k < bands.e.len() {
            let a = bands.band(jsmath::min(bands.e[k + 1], bands.e[k + 3]));
            let b = bands.band(jsmath::max(bands.e[k + 1], bands.e[k + 3]));
            for q in a..=b {
                items[next[q as usize] as usize] = k as i32;
                next[q as usize] += 1;
            }
            k += 4;
        }
        bands.start = start;
        bands.items = items;
        bands
    }

    #[inline]
    fn band(&self, y: f64) -> i32 {
        let q = ((y - self.y0) * self.inv).floor();
        if q >= self.n as f64 {
            self.n - 1
        } else if q > 0.0 {
            q as i32
        } else {
            0
        }
    }

    /// Same as `winding(polys, x, y)`.
    pub fn winding(&self, x: f64, y: f64) -> i32 {
        let e = &self.e;
        let b = self.band(y) as usize;
        let mut w = 0;
        for q in self.start[b]..self.start[b + 1] {
            let k = self.items[q as usize] as usize;
            let (xj, yj, xi, yi) = (e[k], e[k + 1], e[k + 2], e[k + 3]);
            if yj <= y {
                if yi > y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) > 0.0 {
                    w += 1;
                }
            } else if yi <= y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) < 0.0 {
                w -= 1;
            }
        }
        w
    }

    /// The row's crossings sorted by x, into `hits`. Returns how many.
    fn crossings(&self, y: f64, hits: &mut (Vec<f64>, Vec<i8>)) -> usize {
        let e = &self.e;
        let b = self.band(y) as usize;
        let (s, end) = (self.start[b] as usize, self.start[b + 1] as usize);
        if end - s > hits.0.len() {
            hits.0 = vec![0.0; 2 * (end - s)];
            hits.1 = vec![0; 2 * (end - s)];
        }
        let (hx, hd) = (&mut hits.0, &mut hits.1);
        let mut n = 0;
        for &item in &self.items[s..end] {
            let k = item as usize;
            let (xj, yj, xi, yi) = (e[k], e[k + 1], e[k + 2], e[k + 3]);
            let dir: i8 = if yj <= y && yi > y {
                1
            } else if yi <= y && yj > y {
                -1
            } else {
                continue;
            };
            let x = xj + ((y - yj) * (xi - xj)) / (yi - yj);
            let mut m = n;
            n += 1;
            while m > 0 && hx[m - 1] > x {
                hx[m] = hx[m - 1];
                hd[m] = hd[m - 1];
                m -= 1;
            }
            hx[m] = x;
            hd[m] = dir;
        }
        n
    }

    /// Same as `insideRow(polys, y, xs, out)`.
    pub fn inside_row(&self, y: f64, xs: &[f64], out: &mut [u8]) {
        let mut hits = self.hits.borrow_mut();
        let n = self.crossings(y, &mut hits);
        let (hx, hd) = (&hits.0, &hits.1);
        let mut w = 0i32;
        let mut h = 0;
        for k in 0..xs.len() {
            while h < n && hx[h] < xs[k] {
                w += hd[h] as i32;
                h += 1;
            }
            out[k] = u8::from(w != 0);
        }
    }

    /// One flag of `insideRow` (`insideRowAt`).
    pub fn inside_row_at(&self, x: f64, y: f64) -> bool {
        let mut hits = self.hits.borrow_mut();
        let n = self.crossings(y, &mut hits);
        let mut w = 0i32;
        let mut h = 0;
        while h < n && hits.0[h] < x {
            w += hits.1[h] as i32;
            h += 1;
        }
        w != 0
    }
}

/// Axis-aligned bounds (`Bounds`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// `bounds`.
pub fn bounds(polys: &[Vec<Point>]) -> Bounds {
    let mut b = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for p in polys {
        for &[x, y] in p {
            if x < b.min_x {
                b.min_x = x;
            }
            if y < b.min_y {
                b.min_y = y;
            }
            if x > b.max_x {
                b.max_x = x;
            }
            if y > b.max_y {
                b.max_y = y;
            }
        }
    }
    b
}

/// Filled area estimated on a 1-unit sample grid, nonzero rule (`filledArea`).
pub fn filled_area(polys: &[Vec<Point>], b: Bounds) -> f64 {
    let mut n = 0u64;
    let mut xs = Vec::new();
    let mut x = b.min_x.floor() + 0.5;
    while x < b.max_x {
        xs.push(x);
        x += 1.0;
    }
    let mut row = vec![0u8; xs.len()];
    let index = EdgeBands::new(polys);
    let mut y = b.min_y.floor() + 0.5;
    while y < b.max_y {
        index.inside_row(y, &xs, &mut row);
        n += row.iter().map(|&v| v as u64).sum::<u64>();
        y += 1.0;
    }
    n as f64
}

/// Split normalized segments into one segment list per subpath (`splitSubpaths`).
pub fn split_subpaths(segs: &[Segment]) -> Vec<Vec<Segment>> {
    let mut out: Vec<Vec<Segment>> = Vec::new();
    for s in segs {
        if matches!(s, Segment::M { .. }) {
            out.push(Vec::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes() {
        let segs = parse_path("M20 20 H80 V80 H20 Z", true).unwrap();
        assert_eq!(serialize_path(&segs), "M20 20L80 20L80 80L20 80Z");
        let segs = parse_path("m10 10 l5 0 5 5z", true).unwrap();
        assert_eq!(serialize_path(&segs), "M10 10L15 10L20 15Z");
        assert_eq!(
            parse_path("L1 2", true).unwrap_err().0,
            "path must start with M"
        );
        assert_eq!(
            parse_path("M1 2 X", true).unwrap_err().0,
            "unsupported character \"X\" (allowed commands: M L H V C S Q T A Z)"
        );
        assert_eq!(
            parse_path("M1 200", true).unwrap_err().0,
            "coordinate 200 far outside the 0..100 viewBox"
        );
        assert_eq!(
            parse_path("M1 2e", true).unwrap_err().0,
            "cannot parse path near \"e\""
        );
    }
}
