//! JavaScript number semantics the engine depends on, so frames match the TypeScript bit for bit.
//!
//! V8 implements `Math.sin`, `Math.cos`, `Math.atan2`, `Math.exp`, `Math.cbrt` and `Math.acos`
//! with fdlibm (`src/base/ieee754.cc`), which differs from the platform libm Rust's `f64` methods
//! call in the last bit for a few percent of arguments. Those functions are ported here from
//! fdlibm as V8 ships it. `Math.pow` (`**`) agrees with Rust's `powf`, and `Math.hypot` is V8's
//! own scaled Kahan sum (see [`hypot`]), which differs from libm's `hypot`.
//!
//! Also here: `Math.round` (ties toward +∞), `Number.prototype.toString` (shortest round trip,
//! JS exponent rules), `Math.imul`, `ToInt32`, `toString(36)` and the renderer's 2-decimal
//! number formatting `f()`.

// fdlibm's constants are kept digit for digit (some equal std's within a bit, by design), and
// its `x - x` / `(x - x) / (x - x)` produce NaN exactly as the C does.
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::approx_constant,
    clippy::eq_op
)]

use std::f64::consts::PI;

#[inline]
fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as u32 as i32
}

#[inline]
fn low(x: f64) -> u32 {
    x.to_bits() as u32
}

#[inline]
fn from_words(hi: u32, lo: u32) -> f64 {
    f64::from_bits(((hi as u64) << 32) | lo as u64)
}

// ---------------------------------------------------------------------------
// sin / cos (fdlibm k_sin.c, k_cos.c, e_rem_pio2.c, s_sin.c, s_cos.c)

const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

fn kernel_cos(x: f64, y: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3FD33333 {
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3fe90000 {
            0.28125
        } else {
            from_words((ix - 0x00200000) as u32, 0)
        };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - (z * r - x * y))
    }
}

const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

const INVPIO2: f64 = 6.36619772367581382433e-01;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;
const PIO2_2: f64 = 6.07710050630396597660e-11;
const PIO2_2T: f64 = 2.02226624879595063154e-21;
const PIO2_3: f64 = 2.02226624871116645580e-21;
const PIO2_3T: f64 = 8.47842766036889956997e-32;

/// `__ieee754_rem_pio2` for |x| up to 2^19·π/2. Returns None for larger arguments (fdlibm's
/// `__kernel_rem_pio2` path, which the engine never reaches with sane clocks).
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return Some((0, x, 0.0));
    }
    if ix < 0x4002d97c {
        if hx > 0 {
            let mut z = x - PIO2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z - PIO2_1T;
                y1 = (z - y0) - PIO2_1T;
            } else {
                z -= PIO2_2;
                y0 = z - PIO2_2T;
                y1 = (z - y0) - PIO2_2T;
            }
            return Some((1, y0, y1));
        } else {
            let mut z = x + PIO2_1;
            let (y0, y1);
            if ix != 0x3ff921fb {
                y0 = z + PIO2_1T;
                y1 = (z - y0) + PIO2_1T;
            } else {
                z += PIO2_2;
                y0 = z + PIO2_2T;
                y1 = (z - y0) + PIO2_2T;
            }
            return Some((-1, y0, y1));
        }
    }
    if ix <= 0x413921fb {
        let t = x.abs();
        let n = (t * INVPIO2 + 0.5) as i32;
        let fnn = n as f64;
        let mut r = t - fnn * PIO2_1;
        let mut w = fnn * PIO2_1T;
        let mut y0;
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y0 = r - w;
        } else {
            let j = ix >> 20;
            y0 = r - w;
            let mut i = j - ((high(y0) >> 20) & 0x7ff);
            if i > 16 {
                let t = r;
                w = fnn * PIO2_2;
                r = t - w;
                w = fnn * PIO2_2T - ((t - r) - w);
                y0 = r - w;
                i = j - ((high(y0) >> 20) & 0x7ff);
                if i > 49 {
                    let t = r;
                    w = fnn * PIO2_3;
                    r = t - w;
                    w = fnn * PIO2_3T - ((t - r) - w);
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        if hx < 0 {
            return Some((-n, -y0, -y1));
        }
        return Some((n, y0, y1));
    }
    None
}

/// `Math.sin` as V8 computes it.
pub fn sin(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return kernel_sin(x, 0.0, 0);
    }
    if ix >= 0x7ff00000 {
        return x - x;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_sin(y0, y1, 1),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, 1),
            _ => -kernel_cos(y0, y1),
        },
        None => x.sin(),
    }
}

/// `Math.cos` as V8 computes it.
pub fn cos(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7ff00000 {
        return x - x;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, 1),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, 1),
        },
        None => x.cos(),
    }
}

// ---------------------------------------------------------------------------
// atan / atan2 (fdlibm s_atan.c, e_atan2.c)

const ATANHI: [f64; 4] = [
    4.63647609000806093515e-01,
    7.85398163397448278999e-01,
    9.82793723247329054082e-01,
    1.57079632679489655800e+00,
];
const ATANLO: [f64; 4] = [
    2.26987774529616870924e-17,
    3.06161699786838301793e-17,
    1.39033110312309984516e-17,
    6.12323399573676603587e-17,
];
const AT: [f64; 11] = [
    3.33333333333329318027e-01,
    -1.99999999998764832476e-01,
    1.42857142725034663711e-01,
    -1.11111104054623557880e-01,
    9.09088713343650656196e-02,
    -7.69187620504482999495e-02,
    6.66107313738753120669e-02,
    -5.83357013379057348645e-02,
    4.97687799461593236017e-02,
    -3.65315727442169155270e-02,
    1.62858201153657823623e-02,
];

/// `Math.atan` as V8 computes it.
pub fn atan(x0: f64) -> f64 {
    let mut x = x0;
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    let id: i32;
    if ix >= 0x44100000 {
        if ix > 0x7ff00000 || (ix == 0x7ff00000 && low(x) != 0) {
            return x + x;
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    if ix < 0x3fdc0000 {
        if ix < 0x3e400000 {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff30000 {
            if ix < 0x3fe60000 {
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x40038000 {
            id = 2;
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let id = id as usize;
    let z = ATANHI[id] - ((x * (s1 + s2) - ATANLO[id]) - x);
    if hx < 0 {
        -z
    } else {
        z
    }
}

const PI_O_4: f64 = 7.8539816339744827900E-01;
const PI_O_2: f64 = 1.5707963267948965580E+00;
const PI_LO: f64 = 1.2246467991473531772E-16;

/// `Math.atan2(y, x)` as V8 computes it.
pub fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let hx = high(x);
    let lx = low(x);
    let ix = hx & 0x7fffffff;
    let hy = high(y);
    let ly = low(y);
    let iy = hy & 0x7fffffff;
    if hx.wrapping_sub(0x3ff00000) == 0 && lx == 0 {
        return atan(y);
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if iy == 0 && ly == 0 {
        return match m {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    if ix == 0 && lx == 0 {
        return if hy < 0 { -PI_O_2 } else { PI_O_2 };
    }
    if ix == 0x7ff00000 {
        if iy == 0x7ff00000 {
            return match m {
                0 => PI_O_4,
                1 => -PI_O_4,
                2 => 3.0 * PI_O_4,
                _ => -3.0 * PI_O_4,
            };
        }
        return match m {
            0 => 0.0,
            1 => -0.0,
            2 => PI,
            _ => -PI,
        };
    }
    if iy == 0x7ff00000 {
        return if hy < 0 { -PI_O_2 } else { PI_O_2 };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        m &= 1;
        PI_O_2 + 0.5 * PI_LO
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

// ---------------------------------------------------------------------------
// exp (fdlibm e_exp.c, with V8's exact exp(1))

const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
const U_THRESHOLD: f64 = -7.45133219101941108420e+02;
const LN2HI: [f64; 2] = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
const LN2LO: [f64; 2] = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
const INVLN2: f64 = 1.44269504088896338700e+00;
const P1: f64 = 1.66666666666666019037e-01;
const P2: f64 = -2.77777777770155933842e-03;
const P3: f64 = 6.61375632143793436117e-05;
const P4: f64 = -1.65339022054652515390e-06;
const P5: f64 = 4.13813679705723846039e-08;

/// `Math.exp` as V8 computes it.
pub fn exp(x0: f64) -> f64 {
    let mut x = x0;
    let mut hx = high(x) as u32;
    let xsb = ((hx >> 31) & 1) as usize;
    hx &= 0x7fffffff;
    let mut hi = 0.0;
    let mut lo = 0.0;
    let mut k: i32 = 0;
    if hx >= 0x40862E42 {
        if hx >= 0x7ff00000 {
            if ((hx & 0xfffff) | low(x)) != 0 {
                return x + x;
            }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return f64::INFINITY;
        }
        if x < U_THRESHOLD {
            return 0.0;
        }
    }
    if hx > 0x3fd62e42 {
        if hx < 0x3FF0A2B2 {
            if x == 1.0 {
                return std::f64::consts::E;
            }
            hi = x - LN2HI[xsb];
            lo = LN2LO[xsb];
            k = 1 - xsb as i32 - xsb as i32;
        } else {
            let half = if xsb == 0 { 0.5 } else { -0.5 };
            k = (INVLN2 * x + half) as i32;
            let t = k as f64;
            hi = x - t * LN2HI[0];
            lo = t * LN2LO[0];
        }
        x = hi - lo;
    } else if hx < 0x3e300000 {
        return 1.0 + x;
    }
    let t = x * x;
    let twopk = if k >= -1021 {
        from_words((0x3ff00000 + (k << 20)) as u32, 0)
    } else {
        from_words((0x3ff00000 + ((k + 1000) << 20)) as u32, 0)
    };
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * from_words(0x7fe00000, 0);
        }
        y * twopk
    } else {
        y * twopk * from_words(0x01700000, 0)
    }
}

// ---------------------------------------------------------------------------
// cbrt (FreeBSD s_cbrt.c, as in V8)

const B1: u32 = 715094163;
const B2: u32 = 696219795;
const CP0: f64 = 1.87595182427177009643;
const CP1: f64 = -1.88497979543377169875;
const CP2: f64 = 1.621429720105354466140;
const CP3: f64 = -0.758397934778766047437;
const CP4: f64 = 0.145996192886612446982;

/// `Math.cbrt` as V8 computes it.
pub fn cbrt(x: f64) -> f64 {
    let hx0 = high(x) as u32;
    let lo = low(x);
    let sign = hx0 & 0x80000000;
    let hx = hx0 & 0x7fffffff;
    if hx >= 0x7ff00000 {
        return x + x;
    }
    let mut t;
    if hx < 0x00100000 {
        if (hx | lo) == 0 {
            return x;
        }
        t = from_words(0x43500000, 0);
        t *= x;
        let h = high(t) as u32;
        t = from_words(sign | ((h & 0x7fffffff) / 3 + B2), 0);
    } else {
        t = from_words(sign | (hx / 3 + B1), 0);
    }
    let r = (t * t) * (t / x);
    t *= (CP0 + r * (CP1 + r * CP2)) + ((r * r) * r) * (CP3 + r * CP4);
    let bits = (t.to_bits().wrapping_add(0x80000000)) & 0xffffffffc0000000;
    t = f64::from_bits(bits);
    let s = t * t;
    let mut r = x / s;
    let w = t + t;
    r = (r - t) / (w + r);
    t + t * r
}

// ---------------------------------------------------------------------------
// acos (fdlibm e_acos.c)

const PIO2_HI: f64 = 1.57079632679489655800e+00;
const PIO2_LO: f64 = 6.12323399573676603587e-17;
const PS0: f64 = 1.66666666666666657415e-01;
const PS1: f64 = -3.25565818622400915405e-01;
const PS2: f64 = 2.01212532134862925881e-01;
const PS3: f64 = -4.00555345006794114027e-02;
const PS4: f64 = 7.91534994289814532176e-04;
const PS5: f64 = 3.47933107596021167570e-05;
const QS1: f64 = -2.40339491173441421878e+00;
const QS2: f64 = 2.02094576023350569471e+00;
const QS3: f64 = -6.88283971605453293030e-01;
const QS4: f64 = 7.70381505559019352791e-02;

/// `Math.acos` as V8 computes it.
pub fn acos(x: f64) -> f64 {
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix >= 0x3ff00000 {
        if (ix - 0x3ff00000) == 0 && low(x) == 0 {
            return if hx > 0 { 0.0 } else { PI + 2.0 * PIO2_LO };
        }
        return (x - x) / (x - x);
    }
    let pq = |z: f64| {
        let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
        let q = 1.0 + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
        p / q
    };
    if ix < 0x3fe00000 {
        if ix <= 0x3c600000 {
            return PIO2_HI + PIO2_LO;
        }
        let z = x * x;
        let r = pq(z);
        PIO2_HI - (x - (PIO2_LO - x * r))
    } else if hx < 0 {
        let z = (1.0 + x) * 0.5;
        let s = z.sqrt();
        let r = pq(z);
        let w = r * s - PIO2_LO;
        PI - 2.0 * (s + w)
    } else {
        let z = (1.0 - x) * 0.5;
        let s = z.sqrt();
        let df = f64::from_bits(s.to_bits() & 0xffffffff00000000);
        let c = (z - df * df) / (s + df);
        let r = pq(z);
        let w = r * s + c;
        2.0 * (df + w)
    }
}

/// `Math.tanh`. V8 uses fdlibm here too; this uses the platform libm, which agrees with it on
/// all but about one argument in ten thousand (by one ulp). Only `pointer_gaze` uses it.
pub fn tanh(x: f64) -> f64 {
    x.tanh()
}

// ---------------------------------------------------------------------------
// Plain JS number helpers

/// `Math.hypot(...values)` exactly as V8's builtin: abs values scaled by the largest, a Kahan
/// sum of their squares, then `sqrt(sum) * max`.
pub fn hypot_n(values: &[f64]) -> f64 {
    let mut max = 0.0f64;
    let mut nan = false;
    for &v in values {
        if v.is_nan() {
            nan = true;
        } else if v.abs() > max {
            max = v.abs();
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for &v in values {
        let n = v.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `Math.hypot(a, b)` (see [`hypot_n`]).
#[inline]
pub fn hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a.is_nan() || b.is_nan() {
        if a == f64::INFINITY || b == f64::INFINITY {
            return f64::INFINITY;
        }
        return f64::NAN;
    }
    let max = if a > b { a } else { b };
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if max == 0.0 {
        return 0.0;
    }
    let n = a / max;
    let sum = n * n;
    let n = b / max;
    let summand = n * n; // compensation after the first term is exactly 0
    let preliminary = sum + summand;
    preliminary.sqrt() * max
}

/// `Math.hypot(a, b, c)` (see [`hypot_n`]).
pub fn hypot3(a: f64, b: f64, c: f64) -> f64 {
    hypot_n(&[a, b, c])
}

/// `Math.round`: the nearest integer, ties toward +∞, keeping -0 (V8's Float64Round).
#[inline]
pub fn round(x: f64) -> f64 {
    let r = x.ceil();
    if r - 0.5 > x {
        r - 1.0
    } else {
        r
    }
}

/// `Math.max(a, b)`: NaN wins, +0 beats -0.
#[inline]
pub fn max(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else if b > a {
        b
    } else if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && a.is_sign_negative() {
        b
    } else {
        a
    }
}

/// `Math.min(a, b)`: NaN wins, -0 beats +0.
#[inline]
pub fn min(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else if b < a {
        b
    } else if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && a.is_sign_positive() {
        b
    } else {
        a
    }
}

/// `Math.max(...values)` (`-Infinity` for none).
pub fn max_of(values: impl IntoIterator<Item = f64>) -> f64 {
    values.into_iter().fold(f64::NEG_INFINITY, max)
}

/// `Math.min(...values)` (`Infinity` for none).
pub fn min_of(values: impl IntoIterator<Item = f64>) -> f64 {
    values.into_iter().fold(f64::INFINITY, min)
}

/// `Math.sign`.
#[inline]
pub fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `clamp` as the engine writes it: `Math.min(hi, Math.max(lo, v))`.
#[inline]
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    min(hi, max(lo, v))
}

/// `smooth` (smoothstep of a clamped value), as several engine modules define it.
#[inline]
pub fn smooth(u: f64) -> f64 {
    let x = clamp(u, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// ECMAScript `ToInt32`.
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() || x == 0.0 {
        return 0;
    }
    let t = x.trunc();
    let m = t.rem_euclid(4294967296.0);
    (m as u64 as u32) as i32
}

/// `Math.imul`.
#[inline]
pub fn imul(a: u32, b: u32) -> u32 {
    a.wrapping_mul(b)
}

/// `n.toString(36)` for a non-negative integer.
pub fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `Number.prototype.toString()` (radix 10): the shortest digits that round-trip, written with
/// JavaScript's rules for when to use an exponent.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    // Rust's LowerExp prints the shortest round-trip digits: d[.ddd]e<exp>.
    let s = format!("{:e}", x.abs());
    let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1;
    if k <= n && n <= 21 {
        out.push_str(&digits);
        for _ in 0..(n - k) {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        let e = n - 1;
        out.push(if e < 0 { '-' } else { '+' });
        out.push_str(&e.abs().to_string());
    }
    out
}

/// The value the renderer's `f()` writes: rounded to hundredths, -0 as 0.
#[inline]
pub fn round2(n: f64) -> f64 {
    let r = round(n * 100.0) / 100.0;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// The renderer's number formatting `f()` (render.ts, eyes.ts, effects.ts, marks.ts, and `fmt` in
/// path.ts): `Math.round(n * 100) / 100`, -0 written as 0.
pub fn f(n: f64) -> String {
    number_to_string(round2(n))
}

/// Appends `f(n)` to `out`.
pub fn push_f(out: &mut String, n: f64) {
    out.push_str(&f(n));
}

/// `Math.tan(0.6)` as V8 computes it (fdlibm), the only tangent the engine takes.
pub const TAN_0_6: f64 = 0.6841368083416923;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_strings() {
        assert_eq!(number_to_string(1.0), "1");
        assert_eq!(number_to_string(-12.34), "-12.34");
        assert_eq!(number_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(1.5e-7), "1.5e-7");
        assert_eq!(number_to_string(0.000001), "0.000001");
        assert_eq!(
            number_to_string(123456789012345680000.0),
            "123456789012345680000"
        );
        assert_eq!(f(-0.001), "0");
        assert_eq!(f(2.345), "2.35");
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(2.5), 3.0);
        assert!(round(-0.4).is_sign_negative());
        assert_eq!(round(0.49999999999999994), 0.0);
        assert_eq!(to_base36(2166136261), "ztntfp");
    }
}
