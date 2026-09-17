//! The momentum / trailing-N baseline helper (build-contract §6).
//!
//! ONE pair of pure helpers, owned here in `phosk_insights` and depended on by
//! `phosk_ledger` signals and `phosk_planning` `histAvg`. Every `deltaPct` /
//! `priorAvg` / `histAvg` derived field across the backend funnels through these
//! two fns — the formula is NOT reimplemented per feature.
//!
//! `N` (the number of trailing cycles to average) defaults to `3` and is read
//! from settings (`momentum_baseline_cycles`); callers pass it in so these helpers
//! stay pure (no adapter, no I/O).

use phosk_core::money::Money;

/// Trailing-N-cycle average of a per-cycle [`Money`] series (oldest → newest),
/// EXCLUDING the current in-progress cycle (callers pass only the *prior* cycles).
///
/// Takes the last `n` elements of `prior_cycles` (or all of them if fewer), sums
/// them (checked), and integer-divides by the count. Returns [`Money::ZERO`] when
/// there are no prior cycles.
///
/// `n` comes from `phosk_settings::momentum_baseline_cycles` (default `3`); it is
/// passed in so this helper stays pure.
///
/// # Errors
/// Surfaces [`PhoskError::Overflow`] if the checked sum of the trailing window
/// overflows `i64` centimes.
///
/// [`PhoskError::Overflow`]: phosk_core::error::PhoskError::Overflow
pub fn trailing_avg(
    prior_cycles: &[Money],
    n: u32,
) -> Result<Money, phosk_core::error::PhoskError> {
    // n == 0 selects an empty window; with no prior cycles there is nothing to
    // average. Either way the documented result is Money::ZERO (never a divide).
    let n = n as usize;
    if n == 0 || prior_cycles.is_empty() {
        return Ok(Money::ZERO);
    }
    // Take the trailing `n` (or all of them when fewer than `n` exist).
    let start = prior_cycles.len().saturating_sub(n);
    let window = &prior_cycles[start..];
    let count = i64::try_from(window.len()).map_err(|_| {
        phosk_core::error::PhoskError::Overflow("trailing window too large".to_owned())
    })?;
    let sum = Money::sum(window.iter().copied())?;
    // count >= 1 here (window is non-empty), so the division is always safe.
    Ok(Money::from_centimes(sum.centimes() / count))
}

/// `deltaPct = round(100 · (current − trailingAvg) / trailingAvg)` as a signed
/// integer percent; `0` when `trailing_avg` is `0` (no baseline to compare to).
///
/// The single source of truth for every momentum/`deltaPct` field; pair with
/// [`trailing_avg`].
// The casts compute a small unitless *percentage*, never money: the inputs are
// already exact integer centimes, and `f64::round() as i32` saturates rather than
// wrapping, so it can never produce a wrong-signed percent (ADR §0).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "computing a small integer percentage, not an amount; saturating cast"
)]
pub fn delta_pct(current: Money, trailing_avg: Money) -> i32 {
    let avg = trailing_avg.centimes();
    if avg == 0 {
        return 0;
    }
    let delta = current.centimes() - avg;
    let pct = 100.0 * delta as f64 / avg as f64;
    pct.round() as i32
}
