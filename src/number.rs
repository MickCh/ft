//! Numbers read out of text, for `--sort --numeric` and the summaries.
//!
//! An `f64` alone cannot tell 9007199254740993 from 9007199254740992
//! (2^53 + 1 rounds to 2^53), so a number that is written as an integer
//! stays one: [`Number::Int`] holds it exactly, and only a value with a
//! fraction, an exponent or beyond `i128` falls back to [`Number::Float`].
//! Comparing an integer with a float is exact too, which keeps the
//! ordering total — a sort may rely on it.

use std::cmp::Ordering;
use std::fmt;
use std::ops::Add;

/// A parsed number: exact while it is an integer, a float otherwise.
/// A `Float` is never `NaN`, which is what makes the ordering total.
#[derive(Debug, Clone, Copy)]
pub enum Number {
    Int(i128),
    Float(f64),
}

/// 2^127: the first float past `i128::MAX`, and exactly representable.
const I128_BOUND: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;

impl Number {
    /// Read a number from `text`, ignoring surrounding whitespace.
    /// `None` when it is not a number at all — `NaN` included, since it
    /// has no place in an ordering or a sum.
    pub fn parse(text: &str) -> Option<Number> {
        let text = text.trim();
        if let Ok(integer) = text.parse::<i128>() {
            return Some(Number::Int(integer));
        }
        text.parse::<f64>()
            .ok()
            .filter(|value| !value.is_nan())
            .map(Number::Float)
    }

    /// The value as a float, rounding an integer too large for one.
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(integer) => integer as f64,
            Number::Float(value) => value,
        }
    }
}

/// The sum, exact while both are integers and it does not overflow.
impl Add for Number {
    type Output = Number;

    fn add(self, other: Number) -> Number {
        match (self, other) {
            (Number::Int(this), Number::Int(that)) => match this.checked_add(that) {
                Some(sum) => Number::Int(sum),
                None => Number::Float(this as f64 + that as f64),
            },
            _ => Number::Float(self.as_f64() + other.as_f64()),
        }
    }
}

/// Compare an integer with a float without rounding either: the float's
/// whole part fits an `i128` once it is known to lie within its range,
/// and its fraction breaks the tie.
fn cmp_int_float(integer: i128, value: f64) -> Ordering {
    if value >= I128_BOUND {
        return Ordering::Less;
    }
    if value < -I128_BOUND {
        return Ordering::Greater;
    }
    let whole = value.trunc();
    //within [-2^127, 2^127), so the cast is exact
    integer
        .cmp(&(whole as i128))
        .then_with(|| {
            whole
                .partial_cmp(&value)
                .expect("not NaN")
        })
}

impl Ord for Number {
    fn cmp(&self, other: &Number) -> Ordering {
        match (*self, *other) {
            (Number::Int(this), Number::Int(that)) => this.cmp(&that),
            //`partial_cmp` rather than `total_cmp`: -0.0 must equal 0.0,
            //as it equals the integer 0, or the ordering stops being total
            (Number::Float(this), Number::Float(that)) => this
                .partial_cmp(&that)
                .expect("a Number is never NaN"),
            (Number::Int(this), Number::Float(that)) => cmp_int_float(this, that),
            (Number::Float(this), Number::Int(that)) => cmp_int_float(that, this).reverse(),
        }
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Number) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Number {
    fn eq(&self, other: &Number) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Number {}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Int(integer) => integer.fmt(formatter),
            Number::Float(value) => value.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(text: &str) -> Number {
        Number::parse(text).expect("a number")
    }

    #[test]
    fn integers_beyond_f64_precision_stay_exact() {
        // Given two integers an f64 cannot tell apart
        let (low, high) = (number("9007199254740992"), number("9007199254740993"));

        // When / Then they still compare and print as written
        assert!(low < high);
        assert_eq!(high.to_string(), "9007199254740993");
    }

    #[test]
    fn parse_accepts_floats_and_rejects_nan_and_text() {
        // Given / When / Then
        assert!(matches!(number(" 1.5 "), Number::Float(value) if value == 1.5));
        assert!(matches!(number("+7"), Number::Int(7)));
        assert!(matches!(number("1e3"), Number::Float(_)));
        assert!(Number::parse("NaN").is_none());
        assert!(Number::parse("abc").is_none());
        assert!(Number::parse("").is_none());
    }

    #[test]
    fn integer_and_float_compare_exactly() {
        // Given an integer one past 2^53 and the float 2^53
        let integer = Number::Int(9_007_199_254_740_993);
        let float = Number::Float(9_007_199_254_740_992.0);

        // When / Then the integer is greater, though `as f64` would tie
        assert!(integer > float);
        assert!(float < integer);
        assert_eq!(Number::Int(2), Number::Float(2.0));
        assert!(Number::Int(2) < Number::Float(2.5));
        assert!(Number::Int(-2) > Number::Float(-2.5));
        assert!(Number::Int(i128::MAX) < Number::Float(f64::INFINITY));
        assert!(Number::Int(i128::MIN) > Number::Float(f64::NEG_INFINITY));
        assert!(Number::Int(i128::MIN) == Number::Float(-I128_BOUND));
    }

    #[test]
    fn negative_zero_equals_zero() {
        // Given / When / Then: otherwise Int(0) == both while they differ
        assert_eq!(Number::Float(-0.0), Number::Float(0.0));
        assert_eq!(Number::Int(0), Number::Float(-0.0));
    }

    #[test]
    fn add_stays_exact_until_it_overflows() {
        // Given / When
        let exact = number("9007199254740993") + number("1");
        let mixed = number("1") + number("0.5");
        let overflow = Number::Int(i128::MAX) + Number::Int(1);

        // Then
        assert_eq!(exact.to_string(), "9007199254740994");
        assert_eq!(mixed.to_string(), "1.5");
        assert!(matches!(overflow, Number::Float(_)));
    }
}
