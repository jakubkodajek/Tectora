//! The x87 behaviour the core depends on beyond IEEE double arithmetic.
//!
//! The core runs with the x87 precision control at 53 bits (the CRT default
//! control word), so `+ - * / sqrt` between two values that are already
//! doubles are exactly Rust's `f64` operations. Two things are not:
//!
//! * The transcendental instructions (`fsin`, `fcos`, `fptan`, `fpatan`)
//!   ignore the precision control: they leave a result with a 64-bit
//!   mantissa in the register. The next `fmul`/`fadd`/`fdiv` then rounds
//!   the exact result of "64-bit value op double" to 53 bits, and an
//!   `fstp dword` rounds the 64-bit value straight to 24 bits. Computing
//!   `cos` as an `f64` first would round twice.
//! * `fsin`/`fcos`/`fptan` reduce their argument with the hardware's 66-bit
//!   approximation of pi, not with the true pi.
//!
//! [`X80`] is that register value (sign, 64-bit mantissa, exponent), and its
//! methods are the mixed operations rounded once, exactly like the FPU. The
//! transcendentals are evaluated in double-double arithmetic (about 104
//! bits) and then rounded to the 64-bit mantissa, ties to even. That is
//! what the emulator used as the oracle does (it evaluates them with 256-bit
//! mpmath and rounds to 64 bits), so the two agree except when the exact
//! value lies within about 2^-100 of a rounding boundary.

/// An x87 register value: `(-1)^neg * mant * 2^exp`, `mant` normalised
/// (top bit set) unless the value is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X80 {
    pub neg: bool,
    pub mant: u64,
    pub exp: i32,
}

/// Rounds `m * 2^e` to `p` significant bits, ties to even. `sticky` is a
/// flag for nonzero bits below `m` (it only breaks ties). Returns the
/// rounded mantissa (at most `p` bits) and its exponent.
fn round_bits(m: u128, e: i32, p: u32, sticky: bool) -> (u128, i32) {
    if m == 0 {
        return (0, 0);
    }
    let n = 128 - m.leading_zeros();
    if n <= p {
        return (m, e);
    }
    let sh = n - p;
    let q = m >> sh;
    let rem = m & ((1u128 << sh) - 1);
    let half = 1u128 << (sh - 1);
    let up = rem > half || (rem == half && (sticky || q & 1 == 1));
    let mut q = q + up as u128;
    let mut e = e + sh as i32;
    if q >> p != 0 {
        q >>= 1;
        e += 1;
    }
    (q, e)
}

/// `m * 2^e` as an `f64`; `m` has at most 53 bits. Exact for normal
/// results (the only ones the core produces).
fn make_f64(neg: bool, m: u128, e: i32) -> f64 {
    if m == 0 {
        return if neg { -0.0 } else { 0.0 };
    }
    let n = 128 - m.leading_zeros() as i32;
    let m = (m << (53 - n)) as u64;
    let e = e - (53 - n);
    let be = e + 52 + 1023;
    let v = if (1..2047).contains(&be) {
        f64::from_bits(((be as u64) << 52) | (m & ((1u64 << 52) - 1)))
    } else {
        // Outside the double range: not reached by the core; scale in two steps.
        (m as f64) * 2f64.powi(e / 2) * 2f64.powi(e - e / 2)
    };
    if neg { -v } else { v }
}

fn make_f32(neg: bool, m: u128, e: i32) -> f32 {
    if m == 0 {
        return if neg { -0.0 } else { 0.0 };
    }
    let n = 128 - m.leading_zeros() as i32;
    let m = (m << (24 - n)) as u32;
    let e = e - (24 - n);
    let be = e + 23 + 127;
    let v = if (1..255).contains(&be) {
        f32::from_bits(((be as u32) << 23) | (m & ((1u32 << 23) - 1)))
    } else {
        ((m as f64) * 2f64.powi(e)) as f32
    };
    if neg { -v } else { v }
}

/// Exact `(neg, 64-bit normalised mantissa, exponent)` of a finite double.
fn split(x: f64) -> (bool, u64, i32) {
    let b = x.to_bits();
    let neg = b >> 63 != 0;
    let be = ((b >> 52) & 0x7ff) as i32;
    let frac = b & ((1u64 << 52) - 1);
    if be == 0 {
        if frac == 0 {
            return (neg, 0, 0);
        }
        let lz = frac.leading_zeros();
        return (neg, frac << lz, -1074 - lz as i32);
    }
    let m = (frac | (1u64 << 52)) << 11;
    (neg, m, be - 1023 - 52 - 11)
}

/// Exact sum of two values given as `(neg, mant, exp)`, rounded to `p` bits.
fn add_exact(a: (bool, u64, i32), b: (bool, u64, i32), p: u32) -> (bool, u128, i32) {
    if a.1 == 0 && b.1 == 0 {
        return (a.0 && b.0, 0, 0);
    }
    if b.1 == 0 {
        let (m, e) = round_bits(a.1 as u128, a.2, p, false);
        return (a.0, m, e);
    }
    if a.1 == 0 {
        let (m, e) = round_bits(b.1 as u128, b.2, p, false);
        return (b.0, m, e);
    }
    // Normalise both to a 64-bit mantissa; the larger exponent goes first.
    let norm = |v: (bool, u64, i32)| {
        let lz = v.1.leading_zeros();
        (v.0, v.1 << lz, v.2 - lz as i32)
    };
    let (mut a, mut b) = (norm(a), norm(b));
    if b.2 > a.2 {
        std::mem::swap(&mut a, &mut b);
    }
    let d = (a.2 - b.2) as u32;
    let big = (a.1 as i128) << 62;
    let small: i128 = if d <= 62 {
        (b.1 as i128) << (62 - d)
    } else if d - 62 >= 64 {
        1 // only a sticky bit remains
    } else {
        let sh = d - 62;
        let q = b.1 >> sh;
        let lost = b.1 & ((1u64 << sh) - 1) != 0;
        (q | lost as u64) as i128
    };
    let sa: i128 = if a.0 { -big } else { big };
    let sb: i128 = if b.0 { -small } else { small };
    let s = sa + sb;
    if s == 0 {
        // Exact cancellation gives +0 in round-to-nearest.
        return (false, 0, 0);
    }
    let (m, e) = round_bits(s.unsigned_abs(), a.2 - 62, p, false);
    (s < 0, m, e)
}

impl X80 {
    pub const ZERO: X80 = X80 {
        neg: false,
        mant: 0,
        exp: 0,
    };

    /// The exact register value of a double (an `fld qword`).
    pub fn from_f64(x: f64) -> X80 {
        let (neg, mant, exp) = split(x);
        X80 { neg, mant, exp }
    }

    fn parts(self) -> (bool, u64, i32) {
        (self.neg, self.mant, self.exp)
    }

    /// `fstp qword`: rounded to 53 bits.
    pub fn to_f64(self) -> f64 {
        let (m, e) = round_bits(self.mant as u128, self.exp, 53, false);
        make_f64(self.neg, m, e)
    }

    /// `fstp dword`: rounded to 24 bits once.
    pub fn to_f32(self) -> f32 {
        let (m, e) = round_bits(self.mant as u128, self.exp, 24, false);
        make_f32(self.neg, m, e)
    }

    /// `fmul` with a double operand at 53-bit precision.
    pub fn fmul(self, b: f64) -> f64 {
        let (bn, bm, be) = split(b);
        let m = self.mant as u128 * bm as u128;
        let (m, e) = round_bits(m, self.exp + be, 53, false);
        make_f64(self.neg ^ bn, m, e)
    }

    /// `self / b` at 53-bit precision.
    pub fn fdiv(self, b: f64) -> f64 {
        div_parts(self.parts(), split(b))
    }

    /// `a / self` at 53-bit precision (`fdivr`).
    pub fn fdivr(self, a: f64) -> f64 {
        div_parts(split(a), self.parts())
    }

    /// `self + b` at 53-bit precision.
    pub fn fadd(self, b: f64) -> f64 {
        let (n, m, e) = add_exact(self.parts(), split(b), 53);
        make_f64(n, m, e)
    }

    /// `self - b` at 53-bit precision.
    pub fn fsub(self, b: f64) -> f64 {
        self.fadd(-b)
    }

    /// The 10-byte memory image (`fstp tword`).
    pub fn to_bytes(self) -> [u8; 10] {
        let mut out = [0u8; 10];
        if self.mant == 0 {
            if self.neg {
                out[9] = 0x80;
            }
            return out;
        }
        let be = (self.exp + 63 + 16383) as u16 | if self.neg { 0x8000 } else { 0 };
        out[..8].copy_from_slice(&self.mant.to_le_bytes());
        out[8..].copy_from_slice(&be.to_le_bytes());
        out
    }

    /// From the 10-byte memory image (normal numbers and zero).
    pub fn from_bytes(b: [u8; 10]) -> X80 {
        let mant = u64::from_le_bytes(b[..8].try_into().unwrap());
        let se = u16::from_le_bytes([b[8], b[9]]);
        let neg = se & 0x8000 != 0;
        let e = (se & 0x7fff) as i32;
        if mant == 0 {
            return X80 { neg, mant: 0, exp: 0 };
        }
        X80 {
            neg,
            mant,
            exp: e.max(1) - 16383 - 63,
        }
    }

    fn from_dd(d: DD) -> X80 {
        let (neg, m, e) = add_exact(split(d.hi), split(d.lo), 64);
        if m == 0 {
            return X80 { neg, mant: 0, exp: 0 };
        }
        let lz = (m as u64).leading_zeros();
        X80 {
            neg,
            mant: (m as u64) << lz,
            exp: e - lz as i32,
        }
    }
}

fn div_parts(a: (bool, u64, i32), b: (bool, u64, i32)) -> f64 {
    let neg = a.0 ^ b.0;
    if a.1 == 0 {
        return if neg { -0.0 } else { 0.0 };
    }
    assert!(b.1 != 0, "x87 division by zero");
    let (la, lb) = (a.1.leading_zeros(), b.1.leading_zeros());
    let (ma, ea) = (a.1 << la, a.2 - la as i32);
    let (mb, eb) = (b.1 << lb, b.2 - lb as i32);
    let num = (ma as u128) << 64;
    let q = num / mb as u128;
    let r = num % mb as u128;
    let (m, e) = round_bits((q << 1) | (r != 0) as u128, ea - 64 - eb - 1, 53, false);
    make_f64(neg, m, e)
}

// ---------------------------------------------------------------------------------------------
// Double-double arithmetic for the transcendentals.

#[derive(Debug, Clone, Copy)]
struct DD {
    hi: f64,
    lo: f64,
}

fn two_sum(a: f64, b: f64) -> DD {
    let s = a + b;
    let bb = s - a;
    let e = (a - (s - bb)) + (b - bb);
    DD { hi: s, lo: e }
}

fn quick_two_sum(a: f64, b: f64) -> DD {
    let s = a + b;
    DD {
        hi: s,
        lo: b - (s - a),
    }
}

fn split_dekker(a: f64) -> (f64, f64) {
    let t = 134217729.0 * a;
    let hi = t - (t - a);
    (hi, a - hi)
}

fn two_prod(a: f64, b: f64) -> DD {
    let p = a * b;
    let (ah, al) = split_dekker(a);
    let (bh, bl) = split_dekker(b);
    let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    DD { hi: p, lo: e }
}

impl DD {
    fn new(x: f64) -> DD {
        DD { hi: x, lo: 0.0 }
    }
    fn add(self, b: DD) -> DD {
        let s = two_sum(self.hi, b.hi);
        let t = two_sum(self.lo, b.lo);
        let s = quick_two_sum(s.hi, s.lo + t.hi);
        quick_two_sum(s.hi, s.lo + t.lo)
    }
    fn neg(self) -> DD {
        DD {
            hi: -self.hi,
            lo: -self.lo,
        }
    }
    fn sub(self, b: DD) -> DD {
        self.add(b.neg())
    }
    fn mul(self, b: DD) -> DD {
        let p = two_prod(self.hi, b.hi);
        quick_two_sum(p.hi, p.lo + (self.hi * b.lo + self.lo * b.hi))
    }
    fn mul_f(self, b: f64) -> DD {
        let p = two_prod(self.hi, b);
        quick_two_sum(p.hi, p.lo + self.lo * b)
    }
    fn div(self, b: DD) -> DD {
        let q1 = self.hi / b.hi;
        let r = self.sub(b.mul_f(q1));
        let q2 = r.hi / b.hi;
        let r = r.sub(b.mul_f(q2));
        let q3 = r.hi / b.hi;
        quick_two_sum(q1, q2).add(DD::new(q3))
    }
    fn div_f(self, b: f64) -> DD {
        self.div(DD::new(b))
    }
}

/// sin and cos of a reduced argument (|r| <= about pi/4) by their Taylor
/// series, to about 2^-104.
fn sin_cos_dd(r: DD) -> (DD, DD) {
    let r2 = r.mul(r);
    // sin: r - r^3/3! + ...
    let mut term = r;
    let mut sin = r;
    let mut k = 1.0f64;
    loop {
        term = term.mul(r2).div_f(-((k + 1.0) * (k + 2.0)));
        k += 2.0;
        sin = sin.add(term);
        if term.hi.abs() < 1e-34 * sin.hi.abs().max(1e-300) || k > 80.0 {
            break;
        }
    }
    let mut term = DD::new(1.0);
    let mut cos = DD::new(1.0);
    let mut k = 0.0f64;
    loop {
        term = term.mul(r2).div_f(-((k + 1.0) * (k + 2.0)));
        k += 2.0;
        cos = cos.add(term);
        if term.hi.abs() < 1e-34 || k > 80.0 {
            break;
        }
    }
    (sin, cos)
}

/// The hardware's pi: the 66-bit rounding of pi, as `N * 2^-64`.
const PI66_N: u128 = 0x3_243F_6A88_85A3_08D3; // 66 bits

/// `x - q * pi66/2` exactly, with `q` the nearest integer to `x / (pi66/2)`
/// (no reduction for |x| <= pi/4), and q mod 4.
fn reduce66(x: f64) -> (DD, u32) {
    if x.abs() <= std::f64::consts::FRAC_PI_4 {
        return (DD::new(x), 0);
    }
    assert!(x.abs() < 1e9, "x87 argument reduction outside the modelled range");
    // Work in units of 2^-65 (pi66/2 = PI66_N * 2^-65).
    let (neg, m, e) = split(x);
    let sh = e + 65;
    assert!(sh >= 0);
    let mut xi = (m as i128) << sh as u32;
    if neg {
        xi = -xi;
    }
    let n = PI66_N as i128;
    let mut q = (x / std::f64::consts::FRAC_PI_2).round() as i128;
    let mut r = xi - q * n;
    while 2 * r > n {
        q += 1;
        r -= n;
    }
    while 2 * r < -n {
        q -= 1;
        r += n;
    }
    // r has at most 66 bits: exact as a double-double.
    let hi = r as f64;
    let lo = (r - hi as i128) as f64;
    let s = 2f64.powi(-65);
    (
        DD {
            hi: hi * s,
            lo: lo * s,
        },
        q.rem_euclid(4) as u32,
    )
}

/// `fsin` of a double argument.
pub fn fsin(x: f64) -> X80 {
    let (r, q) = reduce66(x);
    let (s, c) = sin_cos_dd(r);
    X80::from_dd(match q {
        0 => s,
        1 => c,
        2 => s.neg(),
        _ => c.neg(),
    })
}

/// `fcos` of a double argument.
pub fn fcos(x: f64) -> X80 {
    let (r, q) = reduce66(x);
    let (s, c) = sin_cos_dd(r);
    X80::from_dd(match q {
        0 => c,
        1 => s.neg(),
        2 => c.neg(),
        _ => s,
    })
}

/// `fptan` of a double argument (the tangent it leaves in ST1).
pub fn fptan(x: f64) -> X80 {
    let (r, q) = reduce66(x);
    let (s, c) = sin_cos_dd(r);
    X80::from_dd(if q % 2 == 0 { s.div(c) } else { c.div(s).neg() })
}

/// pi/2 in three parts (about 160 bits), for the reduction inside `fpatan`.
const PIO2: [f64; 3] = [
    std::f64::consts::FRAC_PI_2,
    6.123233995736766e-17,
    -1.4973849048591698e-33,
];

/// sin and cos of a double with the true pi (used by `fpatan`).
fn sin_cos_true(a: f64) -> (DD, DD) {
    let k = (a / PIO2[0]).round();
    let r = DD::new(a)
        .sub(two_prod(k, PIO2[0]))
        .sub(two_prod(k, PIO2[1]))
        .sub(DD::new(k * PIO2[2]));
    let (s, c) = sin_cos_dd(r);
    match (k as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, s.neg()),
        2 => (s.neg(), c.neg()),
        _ => (c.neg(), s),
    }
}

/// `fpatan`: atan2(y, x) with y in ST1 and x in ST0, both doubles.
pub fn fpatan(y: f64, x: f64) -> X80 {
    if y == 0.0 && x == 0.0 {
        // atan2(+-0, +0) = +-0, atan2(+-0, -0) = +-pi
        if x.is_sign_negative() {
            let pi = DD {
                hi: 2.0 * PIO2[0],
                lo: 2.0 * PIO2[1],
            };
            return X80::from_dd(if y.is_sign_negative() { pi.neg() } else { pi });
        }
        return X80::from_f64(y);
    }
    let a0 = y.atan2(x);
    let (s, c) = sin_cos_true(a0);
    let num = two_prod(y, 1.0).mul(c).sub(two_prod(x, 1.0).mul(s));
    let den = DD::new(x).mul(c).add(DD::new(y).mul(s));
    let a1 = DD::new(a0).add(num.div(den));
    X80::from_dd(a1)
}

/// The CRT `acos` (an intrinsic taking its argument on the x87 stack):
/// `fpatan(sqrt((1+x)(1-x)), x)` for |x| < 1, 0 for 1, pi for -1, else NaN.
pub fn crt_acos(x: f64) -> X80 {
    if x.abs() < 1.0 {
        let t = ((1.0 + x) * (1.0 - x)).sqrt();
        fpatan(t, x)
    } else if x == 1.0 {
        X80::ZERO
    } else if x == -1.0 {
        // fldpi: pi rounded to 64 bits
        X80 {
            neg: false,
            mant: 0xC90F_DAA2_2168_C235,
            exp: -62,
        }
    } else {
        X80 {
            neg: true,
            mant: 0xC000_0000_0000_0000,
            exp: 0x7fff - 16383 - 63,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_of_mixed_operations() {
        // A 64-bit value times a double is rounded once.
        let c = X80 {
            neg: false,
            mant: 0x8000_0000_0000_0401,
            exp: -63,
        }; // 1 + 2^-53 + 2^-63
        assert_eq!(c.fmul(1.0), 1.0 + f64::EPSILON); // rounds up because of the 2^-63 bit
        assert_eq!(c.to_f64(), 1.0 + f64::EPSILON);
        let c2 = X80 {
            neg: false,
            mant: 0x8000_0000_0000_0400,
            exp: -63,
        }; // exact tie: to even
        assert_eq!(c2.to_f64(), 1.0);
        assert_eq!(X80::from_f64(0.1).to_f64(), 0.1);
        assert_eq!(X80::from_f64(3.0).fadd(-3.0), 0.0);
        assert_eq!(X80::from_f64(1.5).fdiv(3.0), 0.5);
        assert_eq!(X80::from_f64(3.0).fdivr(1.5), 0.5);
    }

    #[test]
    fn bytes_roundtrip() {
        for x in [1.0, -2.5, 0.1, 1e-10, 12345.678] {
            let v = X80::from_f64(x);
            assert_eq!(X80::from_bytes(v.to_bytes()), v);
            assert_eq!(X80::from_bytes(v.to_bytes()).to_f64(), x);
        }
    }

    #[test]
    fn transcendentals_are_close_to_libm() {
        for i in -40..40 {
            let x = i as f64 * 0.173;
            assert!((fsin(x).to_f64() - x.sin()).abs() <= 2e-16, "{x}");
            assert!((fcos(x).to_f64() - x.cos()).abs() <= 2e-16, "{x}");
            let y = (i as f64 * 0.31).sin();
            assert!((fpatan(y, 0.7).to_f64() - y.atan2(0.7)).abs() <= 2e-16);
        }
        assert!((fptan(0.6981317).to_f64() - 0.6981317f64.tan()).abs() < 1e-15);
    }

    #[test]
    fn cos_of_right_angle_uses_the_hardware_pi() {
        // cos(f64(pi/2)) with the 66-bit pi: 6.123...e-17 differs from the
        // true value only far down; both are tiny and positive.
        let c = fcos(std::f64::consts::FRAC_PI_2);
        assert!(!c.neg && c.to_f64() > 6.12e-17 && c.to_f64() < 6.13e-17);
    }
}
