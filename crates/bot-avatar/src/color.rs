//! Minimal hex color handling for the renderer (port of `color.ts`). Only #rgb / #rrggbb are
//! accepted.

use crate::jsmath;

/// An sRGB color, 0..255 per channel (not rounded).
pub type Rgb = [f64; 3];

/// `parseHex`.
pub fn parse_hex(input: &str) -> Option<Rgb> {
    let s = crate::path::js_trim(input);
    let h = s.strip_prefix('#')?;
    if !(h.len() == 3 || h.len() == 6) || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    let ch = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).map(f64::from).ok();
    Some([ch(0)?, ch(2)?, ch(4)?])
}

/// `toHex`.
pub fn to_hex(c: Rgb) -> String {
    let ch = |v: f64| jsmath::round(jsmath::min(255.0, jsmath::max(0.0, v))) as u32;
    format!("#{:02x}{:02x}{:02x}", ch(c[0]), ch(c[1]), ch(c[2]))
}

/// `mix`.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Darken while keeping the hue rich (`shade`).
pub fn shade(c: Rgb, t: f64) -> Rgb {
    let max = jsmath::max_of(c);
    let deep = [
        c[0] * 0.35 - (max - c[0]) * 0.1,
        c[1] * 0.35 - (max - c[1]) * 0.1,
        c[2] * 0.35 - (max - c[2]) * 0.1,
    ];
    mix(c, deep, t)
}

/// `tint`.
pub fn tint(c: Rgb, t: f64) -> Rgb {
    mix(c, [255.0, 255.0, 255.0], t)
}

/// WCAG relative luminance (`luminance`).
pub fn luminance(c: Rgb) -> f64 {
    let ch = |v: f64| {
        let s = v / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

/// `contrast`.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    (jsmath::max(la, lb) + 0.05) / (jsmath::min(la, lb) + 0.05)
}

pub const DARK_INK: Rgb = [20.0, 22.0, 34.0];
pub const LIGHT_INK: Rgb = [250.0, 250.0, 255.0];

/// Pick the eye ink with the best contrast against the body color (`autoEyeColor`).
pub fn auto_eye_color(body: Rgb) -> Rgb {
    if contrast(body, DARK_INK) >= contrast(body, LIGHT_INK) {
        DARK_INK
    } else {
        LIGHT_INK
    }
}

fn to_linear(v: f64) -> f64 {
    let s = v / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> f64 {
    255.0
        * if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
}

/// A color as OKLCH: lightness 0..1, chroma, hue in degrees (`toOklch`).
pub fn to_oklch(c: Rgb) -> [f64; 3] {
    let [r, g, b] = c.map(to_linear);
    let l = jsmath::cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    let m = jsmath::cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    let s = jsmath::cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    let ll = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
    let aa = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
    let bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
    let h = (jsmath::atan2(bb, aa) * 180.0) / std::f64::consts::PI;
    [
        ll,
        jsmath::hypot(aa, bb),
        if h < 0.0 { h + 360.0 } else { h },
    ]
}

fn oklch_linear(l0: f64, c: f64, h: f64) -> [f64; 3] {
    let a = c * jsmath::cos((h * std::f64::consts::PI) / 180.0);
    let b = c * jsmath::sin((h * std::f64::consts::PI) / 180.0);
    let l = (l0 + 0.3963377774 * a + 0.2158037573 * b).powf(3.0);
    let m = (l0 - 0.1055613458 * a - 0.0638541728 * b).powf(3.0);
    let s = (l0 - 0.0894841775 * a - 1.291485548 * b).powf(3.0);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ]
}

/// An OKLCH color in sRGB, its chroma lowered as far as it takes to fit (`oklch`).
pub fn oklch(l: f64, c: f64, h: f64) -> Rgb {
    let fits = |c: f64| {
        oklch_linear(l, c, h)
            .iter()
            .all(|&v| (-1e-4..=1.0 + 1e-4).contains(&v))
    };
    let (mut lo, mut hi) = (0.0, c);
    if !fits(hi) {
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi = lo;
    }
    oklch_linear(l, hi, h).map(from_linear)
}
