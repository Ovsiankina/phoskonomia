//! The allocation advice line under the Budgets allocation bar.
//!
//! **Computed, not generated.** The line is plain arithmetic over the category
//! caps and the monthly budget (what is allocated, what is left unassigned,
//! which categories have no cap), labelled [`COMPUTED_SOURCE`] so the screen
//! never passes it off as model output. Nothing to say → an empty line (the
//! page hides it).

use std::fmt::Write as _;

use phosk_core::error::PhoskError;
use phosk_core::money::Money;
use phosk_model::CategoryCap;

use crate::budgets::AllocAdviceDto;

/// The `source` label of advice computed from the data (no model involved).
pub const COMPUTED_SOURCE: &str = "COMPUTED";

/// How many uncapped category names the line lists before "and N more".
const NAMES_SHOWN: usize = 3;

/// The allocation advice for `caps` against the monthly `budget`.
///
/// # Errors
/// [`PhoskError::Overflow`] on checked centime overflow.
pub fn allocation_advice(
    caps: &[CategoryCap],
    budget: Money,
) -> Result<AllocAdviceDto, PhoskError> {
    let advice = |text: String| AllocAdviceDto {
        source: COMPUTED_SOURCE.to_owned(),
        text,
    };
    if caps.is_empty() {
        return Ok(advice(String::new()));
    }

    let allocated = Money::sum(caps.iter().filter_map(|c| c.cap))?;
    let mut text = if budget.centimes() <= 0 {
        if allocated.centimes() > 0 {
            format!(
                "No monthly budget is set; your caps add up to {allocated}. Set a budget to check them against it."
            )
        } else {
            "No monthly budget or caps are set yet. Set a budget, then give each category a cap."
                .to_owned()
        }
    } else if allocated > budget {
        let over = allocated.checked_sub(budget)?;
        format!(
            "Your caps add up to {allocated}, {over} more than your {budget} budget. Lower a cap to fit."
        )
    } else if allocated < budget {
        let free = budget.checked_sub(allocated)?;
        format!("{free} of your {budget} budget is not assigned to any cap.")
    } else {
        format!("Your caps add up to exactly your {budget} budget.")
    };

    let uncapped: Vec<&str> = caps
        .iter()
        .filter(|c| c.cap.is_none())
        .map(|c| c.name.as_str())
        .collect();
    // When nothing is capped the sentence above already says so.
    if !uncapped.is_empty() && allocated.centimes() > 0 {
        let mut names = uncapped
            .iter()
            .take(NAMES_SHOWN)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        if uncapped.len() > NAMES_SHOWN {
            let _ = write!(names, " and {} more", uncapped.len() - NAMES_SHOWN);
        }
        let lead = if uncapped.len() == 1 {
            "1 category has".to_owned()
        } else {
            format!("{} categories have", uncapped.len())
        };
        let _ = write!(text, " {lead} no cap: {names}.");
    }
    Ok(advice(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(name: &str, chf: Option<i64>) -> CategoryCap {
        CategoryCap {
            id: phosk_id::CategoryId::default(),
            slug: String::new(),
            name: name.to_owned(),
            cap: chf.map(|c| Money::from_centimes(c * 100)),
            fixed: false,
            glyph: String::new(),
            note: String::new(),
            provenance: phosk_model::Provenance::user_entered(),
        }
    }

    fn chf(c: i64) -> Money {
        Money::from_centimes(c * 100)
    }

    #[test]
    fn no_categories_means_no_advice() {
        let a = allocation_advice(&[], chf(1000)).expect("advice");
        assert!(a.text.is_empty());
        assert_eq!(a.source, COMPUTED_SOURCE);
    }

    #[test]
    fn nothing_set_says_so() {
        let a = allocation_advice(&[cap("Food", None)], Money::ZERO).expect("advice");
        assert_eq!(
            a.text,
            "No monthly budget or caps are set yet. Set a budget, then give each category a cap."
        );
    }

    #[test]
    fn unassigned_budget_and_uncapped_categories_are_named() {
        let caps = [cap("Rent", Some(1500)), cap("Food", None), cap("Fun", None)];
        let a = allocation_advice(&caps, chf(2000)).expect("advice");
        assert_eq!(
            a.text,
            "CHF 500.00 of your CHF 2'000.00 budget is not assigned to any cap. 2 categories have no cap: Food, Fun."
        );
    }

    #[test]
    fn over_allocation_is_flagged() {
        let caps = [cap("Rent", Some(1500)), cap("Food", Some(800))];
        let a = allocation_advice(&caps, chf(2000)).expect("advice");
        assert_eq!(
            a.text,
            "Your caps add up to CHF 2'300.00, CHF 300.00 more than your CHF 2'000.00 budget. Lower a cap to fit."
        );
    }

    #[test]
    fn long_uncapped_lists_are_shortened() {
        let caps = [
            cap("Rent", Some(2000)),
            cap("A", None),
            cap("B", None),
            cap("C", None),
            cap("D", None),
            cap("E", None),
        ];
        let a = allocation_advice(&caps, chf(2000)).expect("advice");
        assert_eq!(
            a.text,
            "Your caps add up to exactly your CHF 2'000.00 budget. 5 categories have no cap: A, B, C and 2 more."
        );
    }
}
