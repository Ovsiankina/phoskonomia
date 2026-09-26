//! CHF-typed money (ADR §0 foundation — "no raw f64 money in the domain").
//!
//! [`Money`] is an exact amount in **centimes** (Swiss *rappen*, 1/100 CHF),
//! backed by an `i64`. All arithmetic is *checked*: an overflow is a real
//! [`PhoskError::Overflow`], never a wrap or a panic, and there is no float in
//! the value itself. `f64` appears only at [`Money::as_chf_f64`] — the HTTP edge
//! conversion to a JSON number — and never feeds back into a calculation.
//!
//! `Display` is for logs and the edge only: `CHF 1'234.50`, with the Swiss
//! apostrophe thousands separator and a leading `-` on negatives. Presentation
//! the frontend renders (whole-CHF heroes, percent deltas) is *not* here — that
//! is formatting at the edge (ADR-010); this type deals only in exact amounts.
//!
//! Splittable into `phosk_money` later if it earns a crate (ADR-005).

use std::fmt;

use crate::error::PhoskError;

/// An exact monetary amount in centimes (rappen), i64-backed.
///
/// `1 CHF == Money::from_centimes(100)`. Ordering and equality are the integer
/// centime ordering, so two amounts compare exactly with no float epsilon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money {
    /// The amount in centimes. Negative values are legal (refunds, deltas).
    centimes: i64,
}

impl Money {
    /// The zero amount (`CHF 0.00`). Useful as a fold identity for [`Money::sum`].
    pub const ZERO: Self = Self { centimes: 0 };

    /// Construct from a raw centime (rappen) count. This is the canonical
    /// constructor; everything else is a convenience over it.
    pub const fn from_centimes(centimes: i64) -> Self {
        Self { centimes }
    }

    /// Construct from a `(whole CHF, centimes)` pair, e.g. `(1234, 50)` →
    /// `CHF 1'234.50`.
    ///
    /// `cents` must be a valid sub-franc remainder, `0..=99`; anything else is a
    /// caller bug surfaced as [`PhoskError::Invalid`] rather than silently
    /// folded in. The franc/centime combination is range-checked so a huge
    /// `whole` cannot overflow the i64 backing silently — it becomes
    /// [`PhoskError::Overflow`].
    ///
    /// The sign is taken from `whole`: `(-5, 30)` is `CHF -5.30`. To express a
    /// purely sub-franc negative amount, use [`Money::from_centimes`].
    #[tracing::instrument(level = "trace", skip_all, fields(whole, cents))]
    pub fn from_chf(whole: i64, cents: u8) -> Result<Self, PhoskError> {
        if cents > 99 {
            return Err(PhoskError::Invalid(format!(
                "centimes component must be 0..=99, got {cents}"
            )));
        }
        let cents = i64::from(cents);
        // Sub-franc part carries the sign of the franc part so `(-5, 30)` reads
        // as a single negative amount rather than `-500 + 30`.
        let signed_cents = if whole < 0 { -cents } else { cents };
        whole
            .checked_mul(100)
            .and_then(|c| c.checked_add(signed_cents))
            .map(Self::from_centimes)
            .ok_or_else(|| PhoskError::Overflow(format!("CHF {whole}.{cents:02} exceeds range")))
    }

    /// Parse a user-typed CHF amount into exact centimes.
    ///
    /// Accepted, with optional surrounding whitespace:
    /// * an optional `CHF` prefix in any case (`CHF 1'234.50`);
    /// * an optional sign: `-`, `+`, or the typographic minus `−` (U+2212);
    /// * whole francs, optionally grouped in thousands with `'` or `’` (U+2019),
    ///   so both [`Display`](fmt::Display) output and the UI formatter parse back;
    /// * an optional `.` followed by one or two centime digits.
    ///
    /// Integer arithmetic only: no float is involved and nothing is rounded, so a
    /// third decimal is rejected rather than truncated. A comma is rejected rather
    /// than guessed at, because `1,000` could mean one franc or a thousand. The
    /// sign applies to the whole amount (`-0.05` is five negative centimes),
    /// unlike [`Money::from_chf`]. Whether a negative amount is acceptable is the
    /// caller's rule.
    ///
    /// # Errors
    /// [`PhoskError::Invalid`] for empty or malformed input, more than two
    /// decimals, or a magnitude above `i64::MAX` centimes. The accepted range is
    /// symmetric, `-i64::MAX..=i64::MAX`, so `i64::MIN` (displayed as
    /// `CHF -92'233'720'368'547'758.08`) does not parse back. That is caller
    /// input, so it is never [`PhoskError::Overflow`]. The message is a fixed hint
    /// that never repeats the input.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn parse_chf(input: &str) -> Result<Self, PhoskError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(parse_error(PARSE_EMPTY));
        }
        let (negative, s) = split_sign(strip_chf_prefix(s));
        if s.contains(',') {
            return Err(parse_error(PARSE_COMMA));
        }
        let (whole, frac) = s.split_once('.').map_or((s, None), |(w, f)| (w, Some(f)));
        let francs = parse_francs(whole)?;
        let cents = frac.map_or(Ok(0), parse_centimes)?;
        let magnitude = francs
            .checked_mul(100)
            .and_then(|c| c.checked_add(cents))
            .ok_or_else(|| parse_error(PARSE_RANGE))?;
        // `magnitude` is in 0..=i64::MAX, so negating it cannot overflow.
        Ok(Self::from_centimes(if negative {
            -magnitude
        } else {
            magnitude
        }))
    }

    /// The raw centime (rappen) count. The exact, lossless representation.
    pub const fn centimes(self) -> i64 {
        self.centimes
    }

    /// Lossy conversion to CHF as `f64`, for the JSON edge **only** (ADR-010).
    ///
    /// Never call this inside the domain: the result is float and must not feed
    /// back into a [`Money`] calculation.
    #[allow(
        clippy::cast_precision_loss,
        reason = "edge-only lossy conversion; centime magnitudes are far below f64's 2^53 exact-integer limit, and the result never re-enters Money math"
    )]
    pub fn as_chf_f64(self) -> f64 {
        self.centimes as f64 / 100.0
    }

    /// Checked addition. [`PhoskError::Overflow`] on i64 overflow, never wrapping.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn checked_add(self, rhs: Self) -> Result<Self, PhoskError> {
        self.centimes
            .checked_add(rhs.centimes)
            .map(Self::from_centimes)
            .ok_or_else(|| {
                PhoskError::Overflow(format!(
                    "{} + {} overflows CHF range",
                    self.centimes, rhs.centimes
                ))
            })
    }

    /// Checked subtraction. [`PhoskError::Overflow`] on i64 overflow, never wrapping.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn checked_sub(self, rhs: Self) -> Result<Self, PhoskError> {
        self.centimes
            .checked_sub(rhs.centimes)
            .map(Self::from_centimes)
            .ok_or_else(|| {
                PhoskError::Overflow(format!(
                    "{} - {} overflows CHF range",
                    self.centimes, rhs.centimes
                ))
            })
    }

    /// Checked sum of an iterator of amounts, starting from [`Money::ZERO`].
    /// [`PhoskError::Overflow`] the first time the running total overflows,
    /// never wrapping.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn sum<I>(amounts: I) -> Result<Self, PhoskError>
    where
        I: IntoIterator<Item = Self>,
    {
        amounts.into_iter().try_fold(Self::ZERO, Self::checked_add)
    }
}

impl fmt::Display for Money {
    /// Render as `CHF 1'234.50` — Swiss apostrophe thousands grouping on the
    /// whole-franc part, always two centime digits, leading `-` on negatives.
    /// For logs/edge only (ADR-010).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let negative = self.centimes < 0;
        // unsigned_abs() avoids the i64::MIN.abs() overflow panic.
        let abs = self.centimes.unsigned_abs();
        let whole = abs / 100;
        let cents = abs % 100;
        let grouped = group_thousands(whole);
        let sign = if negative { "-" } else { "" };
        write!(f, "CHF {sign}{grouped}.{cents:02}")
    }
}

impl std::str::FromStr for Money {
    type Err = PhoskError;

    /// Same grammar and errors as [`Money::parse_chf`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_chf(s)
    }
}

// ── parse_chf helpers ──────────────────────────────────────────────────────────
//
// The hints are fixed strings with no digits in them: they never echo what the
// user typed (an amount is financial data), and they read the same in any font.

const PARSE_EMPTY: &str = "enter an amount in CHF";
const PARSE_FORMAT: &str = "amount must be a number in CHF, with a dot before the centimes";
const PARSE_COMMA: &str = "use a dot, not a comma, before the centimes";
const PARSE_DECIMALS: &str = "amount can have at most two decimal places";
const PARSE_RANGE: &str = "amount is too large";

/// Thousands separators accepted in the franc part: ASCII `'` and U+2019 `’`.
const GROUP_SEPARATORS: [char; 2] = ['\'', '\u{2019}'];

fn parse_error(hint: &str) -> PhoskError {
    PhoskError::Invalid(hint.to_owned())
}

/// Drop a leading `CHF` (any case) and the whitespace after it.
fn strip_chf_prefix(s: &str) -> &str {
    match (s.get(..3), s.get(3..)) {
        (Some(prefix), Some(rest)) if prefix.eq_ignore_ascii_case("CHF") => rest.trim_start(),
        _ => s,
    }
}

/// Split off one leading sign: `(is_negative, rest)`.
fn split_sign(s: &str) -> (bool, &str) {
    s.strip_prefix('-')
        .or_else(|| s.strip_prefix('\u{2212}'))
        .map_or_else(
            || (false, s.strip_prefix('+').unwrap_or(s)),
            |rest| (true, rest),
        )
}

/// The franc part: plain digits, or digits grouped in threes (`1'234'567`).
fn parse_francs(whole: &str) -> Result<i64, PhoskError> {
    let grouped = whole.contains(GROUP_SEPARATORS);
    let mut digits = String::with_capacity(whole.len());
    for (i, group) in whole.split(GROUP_SEPARATORS).enumerate() {
        let size_ok = match (grouped, i) {
            (false, _) => !group.is_empty(),
            (true, 0) => (1..=3).contains(&group.len()),
            (true, _) => group.len() == 3,
        };
        if !size_ok || !group.bytes().all(|b| b.is_ascii_digit()) {
            return Err(parse_error(PARSE_FORMAT));
        }
        digits.push_str(group);
    }
    digits_value(&digits)
}

/// The centime part after the dot: one digit (tenths) or two.
fn parse_centimes(frac: &str) -> Result<i64, PhoskError> {
    if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(parse_error(PARSE_FORMAT));
    }
    if frac.len() > 2 {
        return Err(parse_error(PARSE_DECIMALS));
    }
    let value = digits_value(frac)?;
    Ok(if frac.len() == 1 { value * 10 } else { value })
}

/// Accumulate ASCII digits into an i64 with checked arithmetic.
fn digits_value(digits: &str) -> Result<i64, PhoskError> {
    digits.chars().try_fold(0_i64, |acc, c| {
        let digit = c.to_digit(10).ok_or_else(|| parse_error(PARSE_FORMAT))?;
        acc.checked_mul(10)
            .and_then(|a| a.checked_add(i64::from(digit)))
            .ok_or_else(|| parse_error(PARSE_RANGE))
    })
}

/// Group an unsigned integer into `'`-separated thousands: `1234567` → `1'234'567`.
fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push('\'');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constructors ─────────────────────────────────────────────────────
    #[test]
    fn from_centimes_is_lossless() {
        assert_eq!(Money::from_centimes(12345).centimes(), 12345);
        assert_eq!(Money::from_centimes(-7).centimes(), -7);
        assert_eq!(Money::ZERO.centimes(), 0);
    }

    #[test]
    fn from_chf_combines_whole_and_cents() {
        let m = Money::from_chf(1234, 50).expect("valid amount");
        assert_eq!(m.centimes(), 123_450);
    }

    #[test]
    fn from_chf_zero_cents() {
        let m = Money::from_chf(1680, 0).expect("valid amount");
        assert_eq!(m.centimes(), 168_000);
    }

    #[test]
    fn from_chf_negative_whole_carries_sign_to_cents() {
        let m = Money::from_chf(-5, 30).expect("valid amount");
        assert_eq!(m.centimes(), -530);
        assert_eq!(m.to_string(), "CHF -5.30");
    }

    #[test]
    fn from_chf_rejects_cents_over_99() {
        let err = Money::from_chf(10, 100).expect_err("100 cents is invalid");
        assert_eq!(err.http_status(), 400);
        assert_eq!(err.code(), "invalid_input");
    }

    #[test]
    fn from_chf_rejects_overflowing_whole() {
        let err = Money::from_chf(i64::MAX, 0).expect_err("must overflow");
        assert_eq!(err.code(), "overflow");
    }

    // ── parse_chf (user-typed CHF text) ──────────────────────────────────────
    fn parsed(s: &str) -> i64 {
        let result = Money::parse_chf(s);
        assert!(result.is_ok(), "{s:?} should parse, got {result:?}");
        result.expect("checked above").centimes()
    }

    fn rejected(s: &str) -> PhoskError {
        let result = Money::parse_chf(s);
        assert!(result.is_err(), "{s:?} should be rejected, got {result:?}");
        result.expect_err("checked above")
    }

    #[test]
    fn parse_chf_whole_francs() {
        assert_eq!(parsed("350"), 35_000);
        assert_eq!(parsed("0"), 0);
        assert_eq!(parsed("007"), 700);
    }

    #[test]
    fn parse_chf_one_or_two_centime_digits() {
        assert_eq!(parsed("350.50"), 35_050);
        assert_eq!(parsed("350.5"), 35_050);
        assert_eq!(parsed("0.05"), 5);
        assert_eq!(parsed("12.30"), 1_230);
        assert_eq!(parsed("0.00"), 0);
    }

    #[test]
    fn parse_chf_trims_and_accepts_a_chf_prefix() {
        assert_eq!(parsed("  42  "), 4_200);
        assert_eq!(parsed("CHF 42"), 4_200);
        assert_eq!(parsed("chf42.10"), 4_210);
    }

    #[test]
    fn parse_chf_accepts_swiss_thousands_grouping() {
        assert_eq!(parsed("1'234"), 123_400);
        assert_eq!(parsed("1'234'567.89"), 123_456_789);
        // U+2019, the separator the UI formatter renders.
        assert_eq!(parsed("1\u{2019}234\u{2019}567.89"), 123_456_789);
    }

    #[test]
    fn parse_chf_sign_applies_to_the_whole_amount() {
        assert_eq!(parsed("-12.50"), -1_250);
        assert_eq!(parsed("\u{2212}12.50"), -1_250);
        assert_eq!(parsed("-0.05"), -5);
        assert_eq!(parsed("+7"), 700);
        assert_eq!(parsed("-0"), 0);
    }

    #[test]
    fn parse_chf_round_trips_display() {
        for c in [0, 5, 50, 100, 123_450, 123_456_789, -530, -123_450] {
            let m = Money::from_centimes(c);
            assert_eq!(parsed(&m.to_string()), c, "round-trip of {m}");
        }
    }

    #[test]
    fn parse_chf_is_exact_at_the_i64_edge() {
        assert_eq!(parsed("92233720368547758.07"), i64::MAX);
        assert_eq!(parsed("-92233720368547758.07"), -i64::MAX);
    }

    #[test]
    fn parse_chf_rejects_empty_input() {
        for s in ["", "   ", "CHF", "-", "CHF -"] {
            assert_eq!(rejected(s).code(), "invalid_input", "{s:?}");
        }
    }

    #[test]
    fn parse_chf_rejects_garbage() {
        for s in [
            "abc",
            "12abc",
            "1 000",
            "--5",
            "- 5",
            ".50",
            "12.",
            "12.5.0",
            "1.-5",
            "1e3",
            "0x10",
            "NaN",
            "inf",
            "\u{0661}\u{0662}",
            "12 CHF",
            "CHF CHF 1",
        ] {
            let err = rejected(s);
            assert_eq!(err.code(), "invalid_input", "{s:?}");
            assert_eq!(err.http_status(), 400, "{s:?}");
        }
    }

    #[test]
    fn parse_chf_rejects_misplaced_grouping() {
        for s in [
            "'123",
            "1''234",
            "1'234'",
            "12'34",
            "1'2345",
            "1234'567",
            "1'234.5'0",
        ] {
            assert_eq!(rejected(s).code(), "invalid_input", "{s:?}");
        }
    }

    #[test]
    fn parse_chf_rejects_more_than_two_decimals_instead_of_rounding() {
        let err = rejected("12.345");
        assert_eq!(err.code(), "invalid_input");
        assert!(err.to_string().contains("decimal"), "{err}");
        assert_eq!(rejected("12.000").code(), "invalid_input");
    }

    #[test]
    fn parse_chf_rejects_a_decimal_comma_instead_of_guessing() {
        for s in ["12,50", "1,000"] {
            let err = rejected(s);
            assert_eq!(err.code(), "invalid_input");
            assert!(err.to_string().contains("comma"), "{s:?}: {err}");
        }
    }

    #[test]
    fn parse_chf_out_of_range_is_caller_input_not_an_internal_overflow() {
        for s in [
            "92233720368547758.08",
            "99999999999999999999",
            "-92233720368547758.08",
        ] {
            let err = rejected(s);
            assert_eq!(err.code(), "invalid_input", "{s:?}");
            assert!(err.to_string().contains("too large"), "{s:?}: {err}");
        }
    }

    #[test]
    fn parse_chf_errors_never_echo_the_input() {
        for s in ["12,50", "12.345", "99999999999999999999", "4711abc", ""] {
            let msg = rejected(s).to_string();
            assert!(!msg.chars().any(|c| c.is_ascii_digit()), "{s:?}: {msg}");
        }
    }

    #[test]
    fn money_from_str_uses_parse_chf() {
        let m: Money = "CHF 1'234.50".parse().expect("valid amount");
        assert_eq!(m.centimes(), 123_450);
        assert!("12,50".parse::<Money>().is_err());
    }

    // ── as_chf_f64 (edge only) ─────────────────────────────────────────────
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "these centime values divide to exactly-representable f64s, so == is exact here"
    )]
    fn as_chf_f64_divides_by_100() {
        assert_eq!(Money::from_centimes(12345).as_chf_f64(), 123.45);
        assert_eq!(Money::from_centimes(-50).as_chf_f64(), -0.5);
        assert_eq!(Money::ZERO.as_chf_f64(), 0.0);
    }

    // ── Arithmetic ─────────────────────────────────────────────────────────
    #[test]
    fn checked_add_sums_exactly() {
        let a = Money::from_centimes(5875);
        let b = Money::from_centimes(4430);
        assert_eq!(a.checked_add(b).expect("no overflow").centimes(), 10305);
    }

    #[test]
    fn checked_sub_subtracts_exactly() {
        let budget = Money::from_centimes(420_000);
        let spent = Money::from_centimes(261_440);
        assert_eq!(
            budget.checked_sub(spent).expect("no overflow").centimes(),
            158_560
        );
    }

    #[test]
    fn checked_sub_can_go_negative() {
        let a = Money::from_centimes(100);
        let b = Money::from_centimes(350);
        assert_eq!(a.checked_sub(b).expect("no overflow").centimes(), -250);
    }

    #[test]
    fn checked_add_rejects_overflow_not_wraps() {
        let big = Money::from_centimes(i64::MAX);
        let err = big
            .checked_add(Money::from_centimes(1))
            .expect_err("must overflow");
        assert_eq!(err.code(), "overflow");
        assert_eq!(err.http_status(), 500);
    }

    #[test]
    fn checked_sub_rejects_overflow_not_wraps() {
        let small = Money::from_centimes(i64::MIN);
        let err = small
            .checked_sub(Money::from_centimes(1))
            .expect_err("must overflow");
        assert_eq!(err.code(), "overflow");
    }

    #[test]
    fn sum_folds_an_iterator() {
        let amounts = [
            Money::from_centimes(5875),
            Money::from_centimes(4430),
            Money::from_centimes(3520),
        ];
        assert_eq!(Money::sum(amounts).expect("no overflow").centimes(), 13825);
    }

    #[test]
    fn sum_of_empty_is_zero() {
        let none: [Money; 0] = [];
        assert_eq!(Money::sum(none).expect("no overflow"), Money::ZERO);
    }

    #[test]
    fn sum_rejects_overflow_not_wraps() {
        let amounts = [Money::from_centimes(i64::MAX), Money::from_centimes(1)];
        let err = Money::sum(amounts).expect_err("must overflow");
        assert_eq!(err.code(), "overflow");
    }

    // ── Display / formatting ────────────────────────────────────────────────
    #[test]
    fn display_two_decimals_and_chf_prefix() {
        assert_eq!(Money::from_centimes(5).to_string(), "CHF 0.05");
        assert_eq!(Money::from_centimes(50).to_string(), "CHF 0.50");
        assert_eq!(Money::from_centimes(100).to_string(), "CHF 1.00");
        assert_eq!(Money::ZERO.to_string(), "CHF 0.00");
    }

    #[test]
    fn display_groups_thousands_with_apostrophe() {
        assert_eq!(Money::from_centimes(123_450).to_string(), "CHF 1'234.50");
        assert_eq!(
            Money::from_centimes(123_456_789).to_string(),
            "CHF 1'234'567.89"
        );
        assert_eq!(Money::from_centimes(100_000).to_string(), "CHF 1'000.00");
    }

    #[test]
    fn display_no_grouping_below_a_thousand() {
        assert_eq!(Money::from_centimes(99_900).to_string(), "CHF 999.00");
    }

    #[test]
    fn display_negative_has_leading_minus() {
        assert_eq!(Money::from_centimes(-530).to_string(), "CHF -5.30");
        assert_eq!(Money::from_centimes(-123_450).to_string(), "CHF -1'234.50");
    }

    #[test]
    fn display_i64_min_does_not_panic() {
        // i64::MIN.abs() would panic; unsigned_abs() must be used.
        let s = Money::from_centimes(i64::MIN).to_string();
        assert!(s.starts_with("CHF -"));
    }

    // ── Ordering / equality ─────────────────────────────────────────────────
    #[test]
    fn ordering_is_centime_ordering() {
        assert!(Money::from_centimes(100) < Money::from_centimes(101));
        assert!(Money::from_centimes(-1) < Money::ZERO);
        assert!(Money::from_centimes(420_000) > Money::from_centimes(261_440));
    }

    #[test]
    fn equality_is_exact() {
        assert_eq!(
            Money::from_centimes(100),
            Money::from_chf(1, 0).expect("ok")
        );
        assert_ne!(Money::from_centimes(100), Money::from_centimes(101));
    }

    #[test]
    fn sorting_a_slice() {
        let mut v = [
            Money::from_centimes(300),
            Money::from_centimes(-100),
            Money::from_centimes(200),
        ];
        v.sort();
        assert_eq!(
            v,
            [
                Money::from_centimes(-100),
                Money::from_centimes(200),
                Money::from_centimes(300),
            ]
        );
    }
}
