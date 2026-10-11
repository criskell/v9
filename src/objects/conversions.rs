//! Ported from `src/numbers/conversions.cc` (`DoubleToCString` and friends).
//!
//! `Number::toString` is not `format!("{}")`. ECMA-262 §6.1.6.1.20 pins the
//! output exactly: the shortest decimal that round-trips, with a specific
//! rule for when to switch to exponential notation and a specific spelling
//! of the exponent. Rust's `{}` already gives the shortest round-tripping
//! decimal, which is the hard half; the rest is the formatting rules.

/// `DoubleToCString` / `Number::ToString`.
pub fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value == 0.0 {
        // Both zeroes print as "0": `String(-0)` is "0", even though
        // `Object.is(-0, 0)` is false.
        return "0".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }

    if value < 0.0 {
        return format!("-{}", number_to_string(-value));
    }

    // The spec splits on the decimal exponent (§6.1.6.1.20): writing the
    // value as `s x 10^(n-k)` with `k` significant digits, a plain decimal
    // is used when `-6 < n <= 21` and exponential notation otherwise.
    //
    // The exponent is taken from Rust's own exponential formatting rather
    // than from `log10().floor()`, which is off by one for exact powers of
    // ten whose logarithm rounds down (`log10(1e21)` is not exactly 21).
    let exponent = decimal_exponent(value);
    if exponent >= 21 || exponent < -6 {
        return exponential_notation(value);
    }

    let formatted = format!("{value}");
    // Rust switches to its own exponential form for very large and very
    // small values; inside the spec's plain-decimal window it must not.
    if formatted.contains('e') {
        return expand_rust_exponential(&formatted);
    }
    formatted
}

/// The power of ten of the leading digit: `floor(log10(value))`, computed
/// exactly by reading it off Rust's exponential formatting.
fn decimal_exponent(value: f64) -> i32 {
    let formatted = format!("{value:e}");
    match formatted.split_once('e') {
        Some((_, exponent)) => exponent.parse().unwrap_or(0),
        None => 0,
    }
}

/// The `1e+21` form the spec requires outside the plain-decimal window.
/// Note the explicit `+`, which Rust's `{:e}` omits.
fn exponential_notation(value: f64) -> String {
    let formatted = format!("{value:e}");
    match formatted.split_once('e') {
        Some((mantissa, exponent)) => {
            let exponent: i32 = exponent.parse().unwrap_or(0);
            if exponent >= 0 {
                format!("{mantissa}e+{exponent}")
            } else {
                format!("{mantissa}e-{}", -exponent)
            }
        }
        None => formatted,
    }
}

/// Turns Rust's `1e-7`-style output back into plain decimal digits, for the
/// values the spec says must not use exponential notation.
fn expand_rust_exponential(formatted: &str) -> String {
    let Some((mantissa, exponent)) = formatted.split_once('e') else {
        return formatted.to_string();
    };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (integer_part, fraction_part) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer.to_string(), fraction.to_string()),
        None => (mantissa.to_string(), String::new()),
    };
    let digits = format!("{integer_part}{fraction_part}");
    // Where the decimal point lands after applying the exponent.
    let point = integer_part.len() as i32 + exponent;

    if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if (point as usize) >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_print_without_a_decimal_point() {
        assert_eq!(number_to_string(0.0), "0");
        assert_eq!(number_to_string(1.0), "1");
        assert_eq!(number_to_string(-1.0), "-1");
        assert_eq!(number_to_string(42.0), "42");
        assert_eq!(number_to_string(1e20), "100000000000000000000");
    }

    #[test]
    fn negative_zero_prints_as_zero() {
        // `String(-0) === "0"`, even though -0 is a distinct value.
        assert_eq!(number_to_string(-0.0), "0");
    }

    #[test]
    fn non_finite_values() {
        assert_eq!(number_to_string(f64::NAN), "NaN");
        assert_eq!(number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(number_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn fractions_round_trip_shortest() {
        assert_eq!(number_to_string(1.5), "1.5");
        assert_eq!(number_to_string(0.1), "0.1");
        // The famous one: 0.1 + 0.2 is not 0.3, and must print as the
        // shortest decimal that identifies the actual bits.
        assert_eq!(number_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number_to_string(1.0 / 3.0), "0.3333333333333333");
    }

    #[test]
    fn the_spec_switches_to_exponential_at_1e21() {
        // The boundary ECMA-262 picks, and the `+` in the exponent.
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e22), "1e+22");
        assert_eq!(number_to_string(-1e21), "-1e+21");
    }

    #[test]
    fn small_values_stay_decimal_down_to_1e_minus_6() {
        assert_eq!(number_to_string(0.000001), "0.000001");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(1.5e-7), "1.5e-7");
    }

    #[test]
    fn the_exponent_is_exact_at_the_boundaries() {
        // `log10(1e21).floor()` is 20, not 21, on a binary float; reading the
        // exponent off the formatted value avoids guessing.
        assert_eq!(decimal_exponent(1e21), 21);
        assert_eq!(decimal_exponent(1e20), 20);
        assert_eq!(decimal_exponent(1e-6), -6);
        assert_eq!(decimal_exponent(1e-7), -7);
        assert_eq!(decimal_exponent(9.999), 0);
        // ...and the boundary lands on the right side.
        assert!(!number_to_string(1e20).contains('e'));
        assert!(number_to_string(1e21).contains('e'));
        assert!(!number_to_string(1e-6).contains('e'));
        assert!(number_to_string(1e-7).contains('e'));
    }

    #[test]
    fn values_rust_would_print_exponentially_but_the_spec_would_not() {
        // Rust is free to choose its own threshold; the spec is not.
        for value in [1e-6, 5e-7_f64 * 10.0, 0.0000015] {
            let printed = number_to_string(value);
            assert!(
                !printed.contains('e'),
                "{value} should print as plain decimal, got {printed}"
            );
            assert_eq!(
                printed.parse::<f64>().unwrap(),
                value,
                "{printed} must round-trip"
            );
        }
    }

    #[test]
    fn everything_round_trips() {
        for value in [
            1.0,
            -1.0,
            0.5,
            1e10,
            1e-10,
            1e21,
            1e-21,
            f64::MAX,
            f64::MIN_POSITIVE,
            123456789.123456,
            2f64.powi(53),
        ] {
            let printed = number_to_string(value);
            let reparsed: f64 = printed.parse().unwrap_or_else(|_| panic!("{printed}"));
            assert_eq!(reparsed, value, "{value} printed as {printed}");
        }
    }
}
