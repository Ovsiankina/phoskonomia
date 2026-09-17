//! `data::cycle`: the shared current-cycle window.

use super::support::json;
use crate::data::cycle::{get_cycle, CycleDto};

#[tokio::test]
async fn get_cycle_frames_the_seeded_june_cycle() {
    let c = get_cycle().await.expect("cycle");
    assert_eq!(
        c,
        CycleDto {
            label: "JUN 2026".into(),
            day: 18,
            days: 30,
            days_left: 12,
            as_of: "18 JUN".into(),
            start_date: "2026-06-01".into(),
            end_date: "2026-06-30".into(),
        }
    );
    let wire = json(&c);
    assert_eq!(wire["daysLeft"], 12, "camelCase on the wire");
    assert_eq!(wire["asOf"], "18 JUN");
    assert_eq!(wire["startDate"], "2026-06-01");
}
