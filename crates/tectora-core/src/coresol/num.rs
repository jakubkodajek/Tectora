//! Number handling shared by the readers and writers: float32 stores,
//! DACAD's tolerance tests, the CRT's `_hypot`, `sscanf("%f")` and
//! `printf("%f")`.
//!
//! DACAD keeps every coordinate as a float32 and computes on the x87 with
//! a 53-bit precision control, so the arithmetic is IEEE double with a
//! rounding to float32 wherever the binary stores a float ([`f32r`]).

/// Rounds to float32 and widens again: a float store.
#[inline]
pub fn f32r(x: f64) -> f64 {
    x as f32 as f64
}

/// The float32 constants of the core's tolerances.
pub const EPS: f64 = 0.01f32 as f64;
/// Angle tolerance, 0.5 degrees in radians.
pub const EPS_ANG: f64 = 0.008_726_646_192_371_845_f64 as f32 as f64;
/// Tolerance of the wall chain sort.
pub const EPS_CHAIN: f64 = 0.02f32 as f64;
/// pi/2 as the float the core stores.
pub const HALF_PI_F: f64 = std::f64::consts::FRAC_PI_2 as f32 as f64;
/// The double pi/180 the core converts degrees with.
pub const D2R: f64 = 0.017_453_292_519_943_295;
/// The double the core converts radians back to degrees with.
pub const R2D: f64 = 57.295_779_513_082_32;

/// `|a - b| < t` on the float difference (the core's equality test).
#[inline]
pub fn feq(a: f64, b: f64, t: f64) -> bool {
    f32r(a - b).abs() < t
}

/// `|a - b| >= t` on the float difference.
#[inline]
pub fn fne(a: f64, b: f64, t: f64) -> bool {
    t <= f32r(a - b).abs()
}

/// `|a| < t`.
#[inline]
pub fn is0(a: f64, t: f64) -> bool {
    a.abs() < t
}

/// The CRT's `_hypot`: both values are divided by the larger magnitude,
/// the root of the sum of squares is taken and scaled back. Each step is
/// rounded to double, so the result can differ from a correctly rounded
/// hypot in the last bit.
pub fn hypot(x: f64, y: f64) -> f64 {
    if !x.is_finite() || !y.is_finite() {
        return x.hypot(y);
    }
    let (ax, ay) = (x.abs(), y.abs());
    let m = if ay > ax { ay } else { ax };
    if m == 0.0 {
        return 0.0;
    }
    let (sx, sy) = (ax / m, ay / m);
    let s = sx * sx + sy * sy;
    m * s.sqrt()
}

/// `sscanf("%f")`: the decimal is converted to a double and stored as a
/// float. The CRT drops the sign of a zero, so "-0.00000" reads as +0.0.
pub fn scan_f32(tok: &str) -> Option<f32> {
    let v = tok.parse::<f64>().ok()? as f32;
    Some(if v == 0.0 { 0.0 } else { v })
}

/// `sscanf("%d")`.
pub fn scan_i32(tok: &str) -> Option<i32> {
    tok.parse::<i64>().ok().map(|v| v as i32)
}

/// What the wall stage reads back from a value the core printed with
/// `%10.5f`.
pub fn requantize(v: f32) -> f32 {
    let mut s = String::new();
    put_f(&mut s, v as f64, 0, 5);
    scan_f32(&s).unwrap_or(0.0)
}

/// Appends `v` as `printf("%<width>.<prec>f")` of DACAD's CRT does it.
///
/// That CRT first converts the value to 17 significant digits and then
/// rounds that digit string half up at the requested place, so an exact
/// tie such as 20.015625 prints as 20.01563 (glibc and Rust round it to
/// even: 20.01562). The sign of a negative zero is printed.
pub fn put_f(out: &mut String, v: f64, width: usize, prec: usize) {
    let neg = v.is_sign_negative();
    let a = v.abs();
    let s = format!("{a:.16e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let mut digits: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).collect();
    let mut decpt: i64 = exp.parse::<i64>().unwrap_or(0) + 1;
    if a == 0.0 {
        digits = vec![b'0'];
        decpt = 1;
    }
    let keep = decpt + prec as i64;
    let mut kept: Vec<u8> = if keep < 0 {
        Vec::new()
    } else {
        let k = keep as usize;
        let round_up = digits.get(k).is_some_and(|&d| d >= b'5');
        let mut d: Vec<u8> = digits.iter().copied().take(k).collect();
        while d.len() < k {
            d.push(b'0');
        }
        if round_up {
            let mut i = d.len();
            loop {
                if i == 0 {
                    d.insert(0, b'1');
                    decpt += 1;
                    break;
                }
                i -= 1;
                if d[i] == b'9' {
                    d[i] = b'0';
                } else {
                    d[i] += 1;
                    break;
                }
            }
        }
        d
    };
    if keep < 0 {
        decpt = 0;
    }
    // Integer part and fraction from the digit string.
    let mut body = String::new();
    if decpt <= 0 {
        body.push('0');
        if prec > 0 {
            body.push('.');
            let zeros = (-decpt) as usize;
            for _ in 0..zeros.min(prec) {
                body.push('0');
            }
            for &d in kept.iter().take(prec.saturating_sub(zeros)) {
                body.push(d as char);
            }
            while body.len() < prec + 2 {
                body.push('0');
            }
        }
    } else {
        let ip = decpt as usize;
        while kept.len() < ip + prec {
            kept.push(b'0');
        }
        for &d in &kept[..ip] {
            body.push(d as char);
        }
        if prec > 0 {
            body.push('.');
            for &d in &kept[ip..ip + prec] {
                body.push(d as char);
            }
        }
    }
    let len = body.len() + usize::from(neg);
    for _ in len..width {
        out.push(' ');
    }
    if neg {
        out.push('-');
    }
    out.push_str(&body);
}

/// `printf("%<width>.<prec>f")` as a new string.
pub fn fmt_f(v: f64, width: usize, prec: usize) -> String {
    let mut s = String::new();
    put_f(&mut s, v, width, prec);
    s
}

/// The `%10.5f` of nearly every number in these files.
pub fn f5(v: f64) -> String {
    fmt_f(v, 10, 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_rounds_ties_up() {
        assert_eq!(f5(20.015625), "  20.01563");
        assert_eq!(f5(0.046875), "   0.04688");
        assert_eq!(f5(-0.015625), "  -0.01563");
        assert_eq!(f5(0.0), "   0.00000");
        assert_eq!(f5(-0.0), "  -0.00000");
        assert_eq!(f5(-0.000001), "  -0.00000");
        assert_eq!(f5(0.000006), "   0.00001");
        assert_eq!(f5(9.999996), "  10.00000");
        assert_eq!(f5(1234.5), "1234.50000");
        assert_eq!(fmt_f(10.1f32 as f64, 10, 2), "     10.10");
        assert_eq!(fmt_f(0.5, 0, 0), "1");
        assert_eq!(fmt_f(123456.789, 3, 5), "123456.78900");
        assert_eq!(fmt_f(1e-9, 0, 5), "0.00000");
    }

    #[test]
    fn printf_matches_exact_rounding_off_ties() {
        for i in 0..20000 {
            let v = ((i as f64 * 0.731_234_567).sin() * 10f64.powi(i % 5)) as f32 as f64;
            let t = v * 200_000.0;
            if t.fract() == 0.0 && (t as i64) % 2 != 0 {
                continue; // an exact tie, rounded differently on purpose
            }
            let ours = f5(v);
            let std = format!("{v:10.5}");
            assert_eq!(ours, std, "{v}");
        }
    }

    #[test]
    fn scanf_drops_negative_zero() {
        assert!(scan_f32("-0.00000").unwrap().is_sign_positive());
        assert_eq!(scan_f32("-1.5"), Some(-1.5));
        assert_eq!(requantize(-0.000001), 0.0);
    }

    #[test]
    fn crt_hypot() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, -2.0), 2.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
    }
}
