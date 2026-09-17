//! `chf` / `chf2`: the Swiss money formatter every page renders amounts with.
//!
//! Spec (the design export's `chf(n, dp)`): `’` (U+2019) thousands groups, `.`
//! decimal, `−` (U+2212) for negatives, exactly `dp` decimals, rounded half away
//! from zero like `Number.toFixed`, zero-padded past the two centime digits.

use phosk_core::money::Money;

use crate::data::{chf, chf2};

fn m(centimes: i64) -> Money {
    Money::from_centimes(centimes)
}

#[test]
fn zero_renders_with_the_requested_decimals() {
    assert_eq!(chf2(Money::ZERO), "0.00");
    assert_eq!(chf(Money::ZERO, 0), "0");
    assert_eq!(chf(Money::ZERO, 1), "0.0");
    assert_eq!(chf(Money::ZERO, 4), "0.0000");
}

#[test]
fn chf2_is_chf_with_two_decimals() {
    for c in [0, 5, -5, 123_450, -99_999_999, i64::MAX, i64::MIN] {
        assert_eq!(chf2(m(c)), chf(m(c), 2), "centimes {c}");
    }
}

#[test]
fn groups_thousands_with_a_right_single_quote() {
    assert_eq!(chf2(m(99_999)), "999.99");
    assert_eq!(chf2(m(100_000)), "1\u{2019}000.00");
    assert_eq!(chf2(m(123_450)), "1\u{2019}234.50");
    assert_eq!(chf2(m(123_456_789)), "1\u{2019}234\u{2019}567.89");
    assert_eq!(chf(m(100_000_000), 0), "1\u{2019}000\u{2019}000");
}

#[test]
fn negatives_use_the_unicode_minus_sign() {
    assert_eq!(chf2(m(-1_250)), "\u{2212}12.50");
    assert_eq!(chf2(m(-5)), "\u{2212}0.05");
    assert_eq!(chf2(m(-123_450)), "\u{2212}1\u{2019}234.50");
    assert!(!chf2(m(-1)).contains('-'), "ASCII hyphen must never render");
}

#[test]
fn two_decimals_keep_every_centime() {
    assert_eq!(chf2(m(5)), "0.05");
    assert_eq!(chf2(m(50)), "0.50");
    assert_eq!(chf2(m(5_875)), "58.75");
    assert_eq!(chf2(m(261_440)), "2\u{2019}614.40");
}

#[test]
fn zero_decimals_round_half_away_from_zero() {
    assert_eq!(chf(m(1_249), 0), "12");
    assert_eq!(chf(m(1_250), 0), "13");
    assert_eq!(chf(m(1_299), 0), "13");
    assert_eq!(chf(m(-1_250), 0), "\u{2212}13");
    assert_eq!(chf(m(-1_249), 0), "\u{2212}12");
    assert_eq!(chf(m(168_000), 0), "1\u{2019}680");
}

#[test]
fn rounding_carries_into_a_new_thousands_group() {
    assert_eq!(chf(m(99_950), 0), "1\u{2019}000");
    assert_eq!(chf(m(99_995), 1), "1\u{2019}000.0");
    assert_eq!(chf(m(-99_950), 0), "\u{2212}1\u{2019}000");
}

#[test]
fn one_decimal_rounds_the_centimes() {
    assert_eq!(chf(m(1_234), 1), "12.3");
    assert_eq!(chf(m(1_235), 1), "12.4");
    assert_eq!(chf(m(1_299), 1), "13.0");
    assert_eq!(chf(m(5), 1), "0.1");
    assert_eq!(chf(m(-1_235), 1), "\u{2212}12.4");
}

#[test]
fn more_than_two_decimals_pad_with_zeros() {
    assert_eq!(chf(m(1_234), 3), "12.340");
    assert_eq!(chf(m(-7), 5), "\u{2212}0.07000");
}

#[test]
fn i64_extremes_format_exactly_without_overflow() {
    // i64::MAX = 92'233'720'368'547'758.07 CHF.
    assert_eq!(
        chf2(m(i64::MAX)),
        "92\u{2019}233\u{2019}720\u{2019}368\u{2019}547\u{2019}758.07"
    );
    assert_eq!(
        chf(m(i64::MAX), 0),
        "92\u{2019}233\u{2019}720\u{2019}368\u{2019}547\u{2019}758"
    );
    // i64::MIN has no positive i64 counterpart; it must still render exactly.
    assert_eq!(
        chf2(m(i64::MIN)),
        "\u{2212}92\u{2019}233\u{2019}720\u{2019}368\u{2019}547\u{2019}758.08"
    );
    assert_eq!(
        chf(m(i64::MIN), 1),
        "\u{2212}92\u{2019}233\u{2019}720\u{2019}368\u{2019}547\u{2019}758.1"
    );
    assert_eq!(
        chf(m(i64::MIN), 0),
        "\u{2212}92\u{2019}233\u{2019}720\u{2019}368\u{2019}547\u{2019}758"
    );
}
