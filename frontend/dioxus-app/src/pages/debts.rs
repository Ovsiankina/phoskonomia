//! Debts page (route `/debts`). Faithful port of React `pages/Debts.jsx`.
//!
//! Outstanding balances read as a DECAYING WAVEFORM: a payoff-trajectory hero up
//! top, a KPI band, a card/row grid of every debt with a payoff-progress meter,
//! a right-dock (or drawer) inspector, a strategy overlay (avalanche / snowball)
//! that marks which debt to target, and a personal-IOU net-position beam.
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg`, the
//! `.app-shell.swap` with the left `AiPanel`, the `.app-main` (TopBar + scroll),
//! the `.debts-wrap` (top bar, KPI band, payoff-trajectory hero, open-balances
//! section + card/row grid, IOU section + net beam + columns), then the docked /
//! drawered `DebtInspector`.
//!
//! JSX idioms → RSX (per the F4 playbook):
//!   * `useTweaks(...)` (the `@ds-adherence-ignore` authoring panel) is NOT a
//!     product surface — it is dropped. Its defaults become plain `use_signal` UI
//!     state (`view`, `sort`, `strategy`, `projection`, `group`, `iou_show`,
//!     `insp`) seeded from `DEBT_TWEAK_DEFAULTS`. The on-page `▦ CARDS / ≡ ROWS`
//!     buttons still drive `view`.
//!   * `useState` → `use_signal`; the resize listener → `use_effect` +
//!     `document::eval` posting `innerWidth<1280` back over `dioxus.send`.
//!   * `useGet(...)` → `use_resource(move || server_fn())`; the on-demand selected
//!     debt detail/payments → `use_resource` over the `sel` signal (refetch on
//!     change), exactly like the dashboard's signal detail.
//!   * writes (T39): debt create / edit / delete / instalment / extra payment
//!     and IOU create / edit / delete / partial payment / settle go through
//!     `data::debt_actions`; the forms live in `pages::debt_forms`, each in its
//!     own page-level signal, and every success refetches the reads it feeds.
//!     The plan (monthly / day / term) and the APR are create-only: changing
//!     them must run T17's `debt_plan` rules, which have no UI yet, so
//!     REFINANCE / ADJUST PLAN render disabled. REMIND had no backend and is
//!     replaced by RECORD PAYMENT.
//!   * SVG charts (`PayoffTrajectory`, `DecayLine`, `NetBeam`) are hand-written
//!     inline here, faithful to the JSX (Debts owns these page-specific charts;
//!     they are not shared F2 primitives). `Spark` (the card balance trace) IS a
//!     shared F2 primitive.
//!   * inline `style={{...}}` → `style: "k:v"` strings (token vars verbatim).
//!   * Money crosses as exact `Money`; chart geometry uses `f64` CHF via
//!     `as_chf_f64()`.

use dioxus::prelude::*;
use phosk_core::money::Money;

use crate::components::prims::{Dot, ScannerBg, Spark};
use crate::components::shell::{AiPanel, TopBar};
use crate::components::states::{Awaiting, InlineStatus};
use crate::data::chf;
use crate::data::cycle::{get_cycle, CycleDto};
use crate::data::debt_actions::{
    delete_debt, delete_iou, pay_debt, pay_debt_extra, pay_iou, settle_iou, DebtForm, IouForm,
};
use crate::data::debts::{
    get_debt, get_debt_payments, get_debt_stats, get_iou_stats, get_trajectory, list_debts,
    list_personal_ious, DebtDetailDto, DebtDto, DebtPaymentDto, DebtStatsDto, IouStatsDto,
    PersonalIouDto, TrajectoryDto,
};
use crate::pages::debt_forms::{
    debt_draft, iou_draft, new_debt_draft, new_iou_draft, submit, DebtFormPanel, DeleteConfirm,
    IouFormPanel, Panel, PayDraft, PayField,
};

// ── pure presentation helpers (faithful to the JSX) ─────────────────────────

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// `(month-index 0..11, year)` anchor for the cycle, parsed from its label.
///
/// Faithful to the JSX `cycleAnchor`: prefer the label's `MON` token + a 4-digit
/// year, else fall back to the seeded today (JUN 2026).
fn cycle_anchor(cycle: &CycleDto) -> (i32, i32) {
    let upper = cycle.label.to_uppercase();
    let mut mi: Option<i32> = None;
    let mut yr: Option<i32> = None;
    for tok in upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        if mi.is_none() {
            if let Some(idx) = MONTHS.iter().position(|m| *m == tok) {
                mi = Some(idx as i32);
            }
        }
        if yr.is_none() && tok.len() == 4 && tok.chars().all(|c| c.is_ascii_digit()) {
            yr = tok.parse::<i32>().ok();
        }
    }
    (mi.unwrap_or(5), yr.unwrap_or(2026))
}

/// Month label `offset` months ahead of the cycle anchor (e.g. `"NOV 28"`).
///
/// Faithful to the JSX `monthLabel`: `MON` + 2-digit year.
fn month_label(offset: i32, cycle: &CycleDto) -> String {
    let (a_m, a_y) = cycle_anchor(cycle);
    let mut m = a_m + offset;
    let mut y = a_y;
    while m > 11 {
        m -= 12;
        y += 1;
    }
    while m < 0 {
        m += 12;
        y -= 1;
    }
    let yy = y.rem_euclid(100);
    format!("{} {:02}", MONTHS[m as usize], yy)
}

/// status key → `(label, tone)`. Faithful to the JSX `debtStatus`.
fn debt_status(status: &str) -> (&'static str, &'static str) {
    match status {
        "high" => ("HIGH INTEREST", "coral"),
        "due" => ("DUE SOON", "warn"),
        "watch" => ("REVIEW", "warn"),
        _ => ("ON TRACK", "blue"),
    }
}

/// `Money` absolute value (centimes-exact; `Money` is `Ord` over i64 centimes).
fn money_abs(m: Money) -> Money {
    Money::from_centimes(m.centimes().abs())
}

/// Empty cycle default until the cycle read lands.
fn empty_cycle() -> CycleDto {
    CycleDto {
        label: String::new(),
        day: 0,
        days: 0,
        days_left: 0,
        as_of: String::new(),
        start_date: String::new(),
        end_date: String::new(),
    }
}

/// Defaults carried from the React `DEBT_TWEAK_DEFAULTS` (the tweaks authoring
/// panel itself is `@ds-adherence-ignore` and is NOT ported — these seed the
/// page's `use_signal` UI state instead).
const DEFAULT_VIEW: &str = "cards";
const DEFAULT_SORT: &str = "balance";
const DEFAULT_STRATEGY: &str = "avalanche";
const DEFAULT_PROJECTION: bool = true;
const DEFAULT_GROUP: bool = false;
const DEFAULT_IOU_SHOW: bool = true;
const DEFAULT_INSP_DOCK: bool = true; // "dock" (vs "drawer")
const DEFAULT_AI_OPEN: bool = true;

// ════════════════════════════════════════════════════════════════════════════

/// The Phoskonomia debts page.
#[component]
pub fn DebtsPage() -> Element {
    // ---- UI state (was: useTweaks defaults + useState) ----
    let mut ai_collapsed = use_signal(|| !DEFAULT_AI_OPEN);
    let mut sel = use_signal(|| Option::<String>::None);
    let mut drawer = use_signal(|| false);
    let mut view = use_signal(|| DEFAULT_VIEW.to_string());
    let sort = use_signal(|| DEFAULT_SORT.to_string());
    let strategy = use_signal(|| DEFAULT_STRATEGY.to_string());
    let projection = use_signal(|| DEFAULT_PROJECTION);
    let group = use_signal(|| DEFAULT_GROUP);
    let iou_show = use_signal(|| DEFAULT_IOU_SHOW);
    let insp_dock = use_signal(|| DEFAULT_INSP_DOCK);

    // ---- write panels (T39): one signal each, so no save closes another ----
    let mut debt_edit = use_signal(Panel::<DebtForm>::default);
    let debt_pay = use_signal(Panel::<PayDraft>::default);
    let debt_row = use_signal(Panel::<bool>::default);
    let mut iou_edit = use_signal(Panel::<IouForm>::default);
    let iou_pay = use_signal(Panel::<PayDraft>::default);
    let iou_row = use_signal(Panel::<bool>::default);
    let iou_settle = use_signal(Panel::<bool>::default);

    // Responsive dock vs drawer: narrow (<1280px) drawers the inspector.
    let narrow = use_signal(|| false);
    use_effect(move || {
        let mut set_narrow = narrow;
        let mut eval = document::eval(
            r#"
            function on(){ dioxus.send(window.innerWidth < 1280 ? 1 : 0); }
            on();
            window.addEventListener("resize", on);
            "#,
        );
        spawn(async move {
            while let Ok(v) = eval.recv::<i32>().await {
                set_narrow.set(v == 1);
            }
        });
    });

    // ---- backend data loads (was: useGet) ----
    let cycle = use_resource(get_cycle);
    let mut debts = use_resource(list_debts);
    let mut stats = use_resource(get_debt_stats);
    let mut ious = use_resource(list_personal_ious);
    let mut iou_stats = use_resource(get_iou_stats);

    // trajectory re-fetches when the strategy changes (params drive use_resource).
    let traj_strategy = match strategy().as_str() {
        "snowball" => "snowball".to_string(),
        "none" => "none".to_string(),
        _ => "avalanche".to_string(),
    };
    let mut traj = {
        let s = traj_strategy.clone();
        use_resource(move || get_trajectory(s.clone()))
    };

    // selected debt detail + payments — fetched on demand (refetch when sel changes).
    let mut detail = use_resource(move || async move {
        match sel() {
            Some(id) => Some(get_debt(id).await),
            None => None,
        }
    });
    let mut payments = use_resource(move || async move {
        match sel() {
            Some(id) => Some(get_debt_payments(id).await),
            None => None,
        }
    });

    // A write landed: refetch every read it feeds. `restart` keeps the previous
    // value until the new one arrives, so nothing unmounts meanwhile.
    let refresh_debts = use_callback(move |()| {
        debts.restart();
        stats.restart();
        traj.restart();
        detail.restart();
        payments.restart();
    });
    let refresh_ious = use_callback(move |()| {
        ious.restart();
        iou_stats.restart();
    });
    let on_debt_deleted = use_callback(move |()| {
        sel.set(None);
        drawer.set(false);
        refresh_debts.call(());
    });
    let pay_debt_cb = use_callback(move |(id, d): (String, PayDraft)| {
        let action = async move {
            if d.extra {
                pay_debt_extra(id, d.amount).await
            } else {
                pay_debt(id, d.amount).await
            }
        };
        submit(debt_pay, action, refresh_debts);
    });
    let delete_debt_cb = use_callback(move |id: String| {
        let (mut edit, mut pay) = (debt_edit, debt_pay);
        let action = async move {
            delete_debt(id.clone()).await?;
            // No panel may stay open on a record that is gone.
            if edit.peek().is_open_for(&id) {
                edit.write().close();
            }
            if pay.peek().is_open_for(&id) {
                pay.write().close();
            }
            Ok(())
        };
        submit(debt_row, action, on_debt_deleted);
    });
    let pay_iou_cb = use_callback(move |(id, d): (String, PayDraft)| {
        submit(iou_pay, pay_iou(id, d.amount), refresh_ious);
    });
    let delete_iou_cb = use_callback(move |id: String| {
        let (mut edit, mut pay) = (iou_edit, iou_pay);
        let action = async move {
            delete_iou(id.clone()).await?;
            if edit.peek().is_open_for(&id) {
                edit.write().close();
            }
            if pay.peek().is_open_for(&id) {
                pay.write().close();
            }
            Ok(())
        };
        submit(iou_row, action, refresh_ious);
    });
    // Its own panel: settling one IOU never disarms another's DELETE confirm.
    let settle_iou_cb = use_callback(move |id: String| {
        let mut settle = iou_settle;
        settle.write().open(&id, false);
        submit(iou_settle, settle_iou(id), refresh_ious);
    });

    // ---- read resources into owned snapshots (clone out of the borrow) ----
    let c = match &*cycle.read() {
        Some(Ok(v)) => v.clone(),
        _ => empty_cycle(),
    };
    let debts_v = debts.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let stats_v = stats.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let traj_v = traj.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let ious_v = ious.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let iou_stats_v = iou_stats
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();

    let debts_loading = debts.read().is_none();
    let traj_loading = traj.read().is_none();
    let ious_loading = ious.read().is_none();
    let iou_stats_loading = iou_stats.read().is_none();
    let detail_loading = detail.read().is_none();
    let payments_loading = payments.read().is_none();

    let detail_v: Option<DebtDetailDto> = match &*detail.read() {
        Some(Some(Ok(d))) => Some(d.clone()),
        _ => None,
    };
    let payments_list: Vec<DebtPaymentDto> = match &*payments.read() {
        Some(Some(Ok(p))) => p.clone(),
        _ => Vec::new(),
    };

    // ---- derived state ----
    let debts_list: Vec<DebtDto> = debts_v.clone().unwrap_or_default();
    let debts_ready = debts_v.is_some() && !debts_list.is_empty();
    let ious_list: Vec<PersonalIouDto> = ious_v.clone().unwrap_or_default();
    let ious_ready = ious_v.is_some() && !ious_list.is_empty();

    let dockable = !narrow() && insp_dock();
    let sel_id = sel();
    let sel_debt: Option<DebtDto> = sel_id
        .as_ref()
        .and_then(|id| debts_list.iter().find(|d| &d.id == id).cloned());

    // strategy target id (avalancheTarget / snowballTarget from stats).
    let target: Option<String> = match strategy().as_str() {
        "avalanche" => stats_v
            .as_ref()
            .and_then(|s| (!s.avalanche_target.is_empty()).then(|| s.avalanche_target.clone())),
        "snowball" => stats_v
            .as_ref()
            .and_then(|s| (!s.snowball_target.is_empty()).then(|| s.snowball_target.clone())),
        _ => None,
    };

    // sorted debts (faithful to the JSX useMemo).
    let sorted: Vec<DebtDto> = {
        let mut arr = debts_list.clone();
        match sort().as_str() {
            "apr" => arr.sort_by(|a, b| b.apr.total_cmp(&a.apr).then(b.balance.cmp(&a.balance))),
            "name" => arr.sort_by(|a, b| a.name.cmp(&b.name)),
            "payoff" => arr.sort_by_key(|d| d.months_to_payoff),
            _ => arr.sort_by(|a, b| b.balance.cmp(&a.balance)),
        }
        arr
    };

    // grouping (faithful to the JSX useMemo: groupLabel buckets in first-seen order).
    let groups: Vec<(Option<String>, Vec<DebtDto>)> = if group() {
        let mut order: Vec<String> = Vec::new();
        let mut buckets: std::collections::HashMap<String, Vec<DebtDto>> =
            std::collections::HashMap::new();
        for d in &sorted {
            let g = if d.group_label.is_empty() {
                match d.kind.as_str() {
                    "LEASE" | "LOAN" => "LEASES & LOANS",
                    "CARD" => "REVOLVING CREDIT",
                    _ => "OBLIGATIONS",
                }
                .to_string()
            } else {
                d.group_label.clone()
            };
            if !buckets.contains_key(&g) {
                order.push(g.clone());
            }
            buckets.entry(g).or_default().push(d.clone());
        }
        order
            .into_iter()
            .map(|g| {
                let items = buckets.remove(&g).unwrap_or_default();
                (Some(format!("{} · {}", g, items.len())), items)
            })
            .collect()
    } else {
        vec![(None, sorted.clone())]
    };

    let show_drawer = !dockable && drawer() && sel_debt.is_some();

    // ---- pre-computed display strings (the rsx format-segment parser is strict:
    // string-literal/closure/format! inside `{...}` is rejected — hoist them). ----
    const DASH: &str = "—";

    // top-bar summary line.
    let sum_count = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| s.count.to_string());
    let sum_owed = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        format!("CHF {}", chf(s.total_owed, 0))
    });
    let sum_monthly = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        format!("CHF {}", chf(s.total_monthly, 0))
    });
    let sum_debt_free = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        if s.debt_free_label.is_empty() {
            DASH.to_string()
        } else {
            s.debt_free_label.clone()
        }
    });

    // KPI band.
    let kpi_owed = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.total_owed, 0));
    let kpi_owed_sub = match &stats_v {
        Some(s) => {
            let pct = format!("{}%", (s.paid_off_total_pct * 100.0).round() as i64);
            format!("{} paid down of CHF {} borrowed", pct, chf(s.total_orig, 0))
        }
        None => format!("{DASH} paid down of {DASH} borrowed"),
    };
    let kpi_monthly = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.total_monthly, 0));
    let kpi_monthly_sub = match &stats_v {
        Some(s) => format!(
            "{} payments · {} auto-detected by GEMMA4",
            s.count, s.auto_count
        ),
        None => format!("{DASH} payments · {DASH} auto-detected by GEMMA4"),
    };
    let kpi_interest = stats_v
        .as_ref()
        .map_or(DASH.to_string(), |s| chf(s.total_interest_yr, 0));
    let avalanche_target_name = stats_v.as_ref().and_then(|s| {
        debts_list
            .iter()
            .find(|d| d.id == s.avalanche_target)
            .map(|d| d.name.clone())
    });
    let kpi_interest_sub_avg = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        format!("{:.1}%", s.weighted_apr * 100.0)
    });
    let kpi_debt_free = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        if s.debt_free_label.is_empty() {
            DASH.to_string()
        } else {
            s.debt_free_label.clone()
        }
    });
    let kpi_debt_free_sub = stats_v.as_ref().map_or(DASH.to_string(), |s| {
        format!("{} months at the current pace", s.horizon)
    });

    // open-balances section count.
    let bal_count = match &stats_v {
        Some(s) => s.count.to_string(),
        None => {
            if debts_ready {
                debts_list.len().to_string()
            } else {
                DASH.to_string()
            }
        }
    };
    let strategy_word = if strategy() == "snowball" {
        "SNOWBALL"
    } else {
        "AVALANCHE"
    };

    // IOU section count + column headers.
    let iou_total_count: Option<u32> = iou_stats_v.as_ref().map(|s| s.count_in + s.count_out);
    let iou_count_str = match iou_total_count {
        Some(n) => n.to_string(),
        None => {
            if ious_ready {
                ious_list.len().to_string()
            } else {
                DASH.to_string()
            }
        }
    };
    let iou_in: Vec<PersonalIouDto> = ious_list
        .iter()
        .filter(|p| p.dir == "in")
        .cloned()
        .collect();
    let iou_out: Vec<PersonalIouDto> = ious_list
        .iter()
        .filter(|p| p.dir == "out")
        .cloned()
        .collect();
    let iou_in_head = match &iou_stats_v {
        Some(s) => format!("{} · CHF {}", s.count_in, chf(s.owed_to_you, 0)),
        None => format!("{} · {DASH}", iou_in.len()),
    };
    let iou_out_head = match &iou_stats_v {
        Some(s) => format!("{} · CHF {}", s.count_out, chf(s.you_owe, 0)),
        None => format!("{} · {DASH}", iou_out.len()),
    };

    // top-bar date + cycle convenience.
    let topbar_date = if c.days == 0 {
        String::new()
    } else {
        format!("{} · DAY {}/{}", c.label, c.day, c.days)
    };

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            // Debts field — a DECAYING waveform: a SOLID coral principal mass
            // upper-right, a molten growth tail sweeping down-left, faint blue blobs.
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 137,
                shapes: r#"[
                    { char: "8", cx: .82, cy: .27, scale: .47, style: "solid", morph: "blob", live: false, fill: .82 },
                    { char: "3", cx: .29, cy: .74, scale: .54, style: "red", morph: "vein", live: true, fill: .5 },
                    { char: "e", cx: .57, cy: .45, scale: .22, style: "wire", morph: "vein", live: false, fill: .4 },
                    { char: "0", cx: .1, cy: .19, scale: .3, style: "faint", morph: "blob", live: false, fill: .5 },
                    { char: "5", cx: .94, cy: .9, scale: .2, style: "faint", morph: "blob", live: false, fill: .4 }
                ]"#.to_string(),
            }

            div { class: "app-shell swap",
                AiPanel { collapsed: ai_collapsed(), on_toggle: move |()| ai_collapsed.toggle() }

                div { class: "app-main",
                    TopBar { active: "DEBTS".to_string(), date_text: topbar_date }
                    div { class: "app-scroll", "data-screen-label": "DEBTS",
                        div { class: "debts-wrap",

                            // ===================== TOP BAR =====================
                            div { class: "debts-top",
                                div {
                                    div { class: "ttl", "Debts" }
                                    div { class: "sum",
                                        b { "{sum_count}" }
                                        " open balances · "
                                        span { class: "coral", "{sum_owed}" }
                                        " owed ·"
                                        b { " {sum_monthly}" }
                                        "/mo · debt-free {sum_debt_free}"
                                    }
                                }
                                div { class: "modes",
                                    button {
                                        class: if view() == "cards" { "m on" } else { "m" },
                                        onclick: move |_| view.set("cards".to_string()),
                                        "▦ CARDS"
                                    }
                                    button {
                                        class: if view() == "rows" { "m on" } else { "m" },
                                        onclick: move |_| view.set("rows".to_string()),
                                        "≡ ROWS"
                                    }
                                }
                            }

                            // ===================== KPI BAND =====================
                            div { class: "debts-kpis",
                                div { class: "debts-kpi accent",
                                    div { class: "lbl", span { "TOTAL OWED" } span { "OUTSTANDING" } }
                                    div { class: "big", span { class: "cur", "CHF" } "{kpi_owed}" }
                                    div { class: "ksub", "{kpi_owed_sub}" }
                                }
                                div { class: "debts-kpi blue",
                                    div { class: "lbl", span { "MONTHLY OUTFLOW" } span { "SCHEDULED" } }
                                    div { class: "big", span { class: "cur", "CHF" } "{kpi_monthly}" }
                                    div { class: "ksub", "{kpi_monthly_sub}" }
                                }
                                div { class: "debts-kpi",
                                    div { class: "lbl", span { "INTEREST" } span { "RUN-RATE / YR" } }
                                    div { class: "big", style: "color:var(--warn)", span { class: "cur", "CHF" } "{kpi_interest}" }
                                    div { class: "ksub",
                                        "Avg {kpi_interest_sub_avg}"
                                        if let Some(nm) = &avalanche_target_name {
                                            " · {nm} is the leak"
                                        }
                                    }
                                }
                                div { class: "debts-kpi",
                                    div { class: "lbl", span { "DEBT-FREE" } span { "PROJECTED" } }
                                    div { class: "big", style: "color:var(--ok)", "{kpi_debt_free}" }
                                    div { class: "ksub", "{kpi_debt_free_sub}" }
                                }
                            }

                            // ===================== PAYOFF TRAJECTORY (hero) =====================
                            PayoffTrajectory {
                                show_projection: projection(),
                                strategy: traj_strategy.clone(),
                                traj: traj_v.clone(),
                                stats: stats_v.clone(),
                                cycle: c.clone(),
                                loading: traj_loading,
                            }

                            // ===================== OPEN BALANCES =====================
                            div { class: "debts-sec",
                                span { class: "lbl", "∿ OPEN BALANCES" }
                                span { class: "ct", "{bal_count}" }
                                span { class: "rule" }
                                span { class: "meta",
                                    if target.is_some() {
                                        "◎ {strategy_word} TARGET MARKED · "
                                    }
                                    "▌ BAR = PAID OFF · CLICK TO INSPECT"
                                }
                                button {
                                    class: "gbtn dx-add",
                                    r#type: "button",
                                    onclick: move |_| debt_edit.write().open("", new_debt_draft()),
                                    "+ NEW DEBT"
                                }
                            }
                            DebtFormPanel { panel: debt_edit, on_saved: refresh_debts }

                            if debts_ready {
                                for (i , (label , items)) in groups.iter().enumerate() {
                                    {
                                        let label = label.clone();
                                        let items = items.clone();
                                        let key = label.clone().unwrap_or_else(|| i.to_string());
                                        let is_rows = view() == "rows";
                                        rsx! {
                                            div { key: "{key}",
                                                if let Some(l) = &label {
                                                    div { class: "debts-group", "{l}" span { class: "gr" } }
                                                }
                                                if is_rows {
                                                    div { class: "debt-rows",
                                                        for d in items.iter() {
                                                            div { key: "{d.id}", "data-src": "{d.src}",
                                                                DebtRow {
                                                                    d: d.clone(),
                                                                    active: sel_id.as_deref() == Some(d.id.as_str()),
                                                                    target: target.clone(),
                                                                    on_select: move |id: String| select_debt(id, &mut sel, dockable, &mut drawer),
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    div { class: "debt-grid",
                                                        for d in items.iter() {
                                                            div { key: "{d.id}", "data-src": "{d.src}", style: "display:contents",
                                                                DebtCard {
                                                                    d: d.clone(),
                                                                    active: sel_id.as_deref() == Some(d.id.as_str()),
                                                                    target: target.clone(),
                                                                    cycle: c.clone(),
                                                                    on_select: move |id: String| select_debt(id, &mut sel, dockable, &mut drawer),
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if debts_v.is_some() {
                                Awaiting { label: "OPEN BALANCES".to_string(), legend: "EMPTY".to_string(), message: "No debts. Add one with + NEW DEBT.".to_string() }
                            } else {
                                Awaiting { label: "OPEN BALANCES".to_string(), loading: debts_loading, tone: "coral".to_string() }
                            }

                            // ===================== PERSONAL · IOU =====================
                            if iou_show() {
                                div { class: "ious",
                                    div { class: "debts-sec ious-sec",
                                        span { class: "lbl", "⟷ PERSONAL · IOU" }
                                        span { class: "ct", "{iou_count_str}" }
                                        span { class: "rule" }
                                        span { class: "meta", "INFORMAL · NO INTEREST · KEPT OUT OF YOUR REAL DEBT" }
                                        button {
                                            class: "gbtn dx-add",
                                            r#type: "button",
                                            onclick: move |_| iou_edit.write().open("", new_iou_draft()),
                                            "+ NEW IOU"
                                        }
                                    }
                                    IouFormPanel { panel: iou_edit, on_saved: refresh_ious }

                                    if let Some(s) = &iou_stats_v {
                                        NetBeam { stats: s.clone(), count: iou_total_count }
                                    } else {
                                        div { class: "iou-beam osc-bkt",
                                            span { class: "osc-leg", "NET POSITION" }
                                            Awaiting { label: "NET POSITION".to_string(), loading: iou_stats_loading }
                                        }
                                    }

                                    if ious_ready {
                                        div { class: "iou-cols",
                                            div { class: "iou-col",
                                                div { class: "iou-colh in",
                                                    "← OWED TO YOU"
                                                    span { class: "n", "{iou_in_head}" }
                                                }
                                                for p in iou_in.iter() {
                                                    PersonCard {
                                                        key: "{p.id}",
                                                        p: p.clone(),
                                                        iou_pay,
                                                        iou_row,
                                                        iou_settle,
                                                        on_edit: move |p: PersonalIouDto| iou_edit.write().open(&p.id, iou_draft(&p)),
                                                        on_pay: pay_iou_cb,
                                                        on_settle: settle_iou_cb,
                                                        on_delete: delete_iou_cb,
                                                    }
                                                }
                                            }
                                            div { class: "iou-col",
                                                div { class: "iou-colh out",
                                                    "YOU OWE →"
                                                    span { class: "n", "{iou_out_head}" }
                                                }
                                                for p in iou_out.iter() {
                                                    PersonCard {
                                                        key: "{p.id}",
                                                        p: p.clone(),
                                                        iou_pay,
                                                        iou_row,
                                                        iou_settle,
                                                        on_edit: move |p: PersonalIouDto| iou_edit.write().open(&p.id, iou_draft(&p)),
                                                        on_pay: pay_iou_cb,
                                                        on_settle: settle_iou_cb,
                                                        on_delete: delete_iou_cb,
                                                    }
                                                }
                                            }
                                        }
                                    } else if ious_v.is_some() {
                                        Awaiting { label: "PERSONAL IOUS".to_string(), legend: "EMPTY".to_string(), message: "No IOUs. Add one with + NEW IOU.".to_string() }
                                    } else {
                                        Awaiting { label: "PERSONAL IOUS".to_string(), loading: ious_loading }
                                    }
                                }
                            }
                        }
                    }
                }

                // docked inspector (wide layouts)
                if dockable && sel_debt.is_some() {
                    DebtInspector {
                        d: sel_debt.clone(),
                        detail: detail_v.clone(),
                        detail_loading,
                        payments: payments_list.clone(),
                        payments_loading,
                        cycle: c.clone(),
                        variant: None,
                        on_close: move |()| sel.set(None),
                        on_edit: move |d: DebtDto| {
                            drawer.set(false);
                            debt_edit.write().open(&d.id, debt_draft(&d));
                        },
                        debt_pay,
                        debt_row,
                        on_pay: pay_debt_cb,
                        on_delete: delete_debt_cb,
                    }
                }
            }

            // drawer inspector (narrow layouts)
            if show_drawer {
                div { class: "sig-drawer-back", onclick: move |_| drawer.set(false),
                    div { class: "sig-drawer", onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        DebtInspector {
                            d: sel_debt.clone(),
                            detail: detail_v.clone(),
                            detail_loading,
                            payments: payments_list.clone(),
                            payments_loading,
                            cycle: c.clone(),
                            variant: Some("drawer".to_string()),
                            on_close: move |()| drawer.set(false),
                            on_edit: move |d: DebtDto| {
                                drawer.set(false);
                                debt_edit.write().open(&d.id, debt_draft(&d));
                            },
                            debt_pay,
                            debt_row,
                            on_pay: pay_debt_cb,
                            on_delete: delete_debt_cb,
                        }
                    }
                }
            }
        }
    }
}

/// Toggle selection + open the drawer when not dockable (was the JSX `selectDebt`).
fn select_debt(
    id: String,
    sel: &mut Signal<Option<String>>,
    dockable: bool,
    drawer: &mut Signal<bool>,
) {
    let was = sel();
    let same = was.as_deref() == Some(id.as_str());
    if same {
        sel.set(None);
    } else {
        sel.set(Some(id));
        if !dockable {
            drawer.set(true);
        }
    }
}

// ════════════════════════════ PAYOFF TRAJECTORY (hero) ══════════════════════

/// The combined-balance decay hero (`PayoffTrajectory` in the JSX). The SVG math
/// is kept verbatim; all geometry is `f64` CHF via `as_chf_f64`.
#[component]
fn PayoffTrajectory(
    show_projection: bool,
    strategy: String,
    traj: Option<TrajectoryDto>,
    stats: Option<DebtStatsDto>,
    cycle: CycleDto,
    loading: bool,
) -> Element {
    let points = traj.as_ref().map(|t| t.points.clone()).unwrap_or_default();
    if traj.is_none() || points.is_empty() {
        return rsx! {
            div { class: "traj osc-bkt blue",
                span { class: "osc-leg", "PAYOFF TRAJECTORY" }
                Awaiting { label: "COMBINED BALANCE DECAY".to_string(), loading }
            }
        };
    }
    let t = traj.as_ref().unwrap();
    let debt_free_label = if !t.debt_free_label.is_empty() {
        t.debt_free_label.clone()
    } else {
        stats.as_ref().map_or("—".to_string(), |s| {
            if s.debt_free_label.is_empty() {
                "—".to_string()
            } else {
                s.debt_free_label.clone()
            }
        })
    };

    let w = 1000.0_f64;
    let h = 196.0_f64;
    let pad_l = 54.0_f64;
    let pad_r = 64.0_f64;
    let pad_t = 26.0_f64;
    let pad_b = 34.0_f64;
    let base = h - pad_b;
    let m_min = f64::from(points[0].m);
    let m_max = f64::from(points[points.len() - 1].m);
    let span = if (m_max - m_min).abs() < f64::EPSILON {
        1.0
    } else {
        m_max - m_min
    };
    let max_y = points
        .iter()
        .map(|p| p.total.as_chf_f64())
        .fold(1.0_f64, f64::max)
        * 1.06;
    let x = |m: f64| pad_l + (m - m_min) / span * (w - pad_l - pad_r);
    let y = |v: f64| base - (v / max_y) * (base - pad_t);

    let to_pts = |arr: &[&TrajPointRef]| -> String {
        arr.iter()
            .map(|p| format!("{:.1},{:.1}", x(f64::from(p.m)), y(p.total)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // build f64 refs to keep the closure simple.
    struct TrajPointRef {
        m: i32,
        total: f64,
    }
    let pts: Vec<TrajPointRef> = points
        .iter()
        .map(|p| TrajPointRef {
            m: p.m,
            total: p.total.as_chf_f64(),
        })
        .collect();
    let hist_pts: Vec<&TrajPointRef> = pts.iter().filter(|p| p.m <= 0).collect();
    let proj_pts: Vec<&TrajPointRef> = pts.iter().filter(|p| p.m >= 0).collect();
    let hist_line = to_pts(&hist_pts);
    let proj_line = to_pts(&proj_pts);
    let hist_area = if let Some(first) = hist_pts.first() {
        format!(
            "{:.1},{:.1} {} {:.1},{:.1}",
            x(f64::from(first.m)),
            base,
            hist_line,
            x(0.0),
            base
        )
    } else {
        String::new()
    };
    let proj_area = if !proj_pts.is_empty() {
        format!(
            "{:.1},{:.1} {} {:.1},{:.1}",
            x(0.0),
            base,
            proj_line,
            x(m_max),
            base
        )
    } else {
        String::new()
    };

    // y gridlines at round franc levels.
    let step = if max_y > 30_000.0 { 10_000.0 } else { 5_000.0 };
    let mut lines: Vec<f64> = Vec::new();
    let mut v = step;
    while v < max_y {
        lines.push(v);
        v += step;
    }

    // x ticks: prefer backend xTicks, else today/+12/+24/debt-free.
    let ticks: Vec<i32> = if !t.x_ticks.is_empty() {
        t.x_ticks.clone()
    } else {
        let mut tk = vec![0];
        if m_max >= 12.0 {
            tk.push(12);
        }
        if m_max >= 24.0 {
            tk.push(24);
        }
        tk.push(m_max as i32);
        tk
    };

    let df_x = x(m_max);
    let df_y = y(0.0);
    let today_pt: Option<f64> = pts.iter().find(|p| p.m == 0).map(|p| p.total);

    // pre-computed strings.
    let hud_line = format!(
        "∿ COMBINED BALANCE DECAY · {} → {}",
        if cycle.label.is_empty() {
            "—".to_string()
        } else {
            cycle.label.clone()
        },
        debt_free_label
    );
    let today_label = format!("TODAY · {}", cycle.as_of);
    let foot_paid = match &stats {
        Some(s) => {
            let paid = Money::from_centimes(s.total_orig.centimes() - s.total_owed.centimes());
            format!("CHF {}", chf(paid, 0))
        }
        None => "CHF 0".to_string(),
    };
    let foot_of = stats.as_ref().map_or("of CHF 0".to_string(), |s| {
        format!("of {}", chf(s.total_orig, 0))
    });
    let foot_rate = stats.as_ref().map_or("—".to_string(), |s| {
        format!("{:.1}%", s.weighted_apr * 100.0)
    });
    let foot_horizon = stats
        .as_ref()
        .map_or("—".to_string(), |s| format!("{} MO", s.horizon));
    let snowball_target_name = stats
        .as_ref()
        .and_then(|s| (!s.snowball_target.is_empty()).then(|| s.snowball_target.clone()));
    let avalanche_target_name = stats
        .as_ref()
        .and_then(|s| (!s.avalanche_target.is_empty()).then(|| s.avalanche_target.clone()));
    let is_snowball = strategy == "snowball";

    rsx! {
        div { class: "traj osc-bkt blue",
            span { class: "osc-leg", "PAYOFF TRAJECTORY" }
            div { class: "traj-h",
                span { class: "hud", "{hud_line}" }
                div { class: "traj-key",
                    span { i { class: "k owed" } " OWED" }
                    span { i { class: "k proj" } " PROJECTED" }
                    span { i { class: "k free" } " DEBT-FREE" }
                }
            }

            svg {
                width: "100%",
                height: "{h}",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none",
                class: "traj-svg",
                defs {
                    linearGradient { id: "trajfill", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", stop_color: "rgba(255,94,77,.30)" }
                        stop { offset: "100%", stop_color: "rgba(255,94,77,0)" }
                    }
                    linearGradient { id: "trajproj", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", stop_color: "rgba(143,125,255,.20)" }
                        stop { offset: "100%", stop_color: "rgba(143,125,255,0)" }
                    }
                }

                // y gridlines
                for gv in lines.iter() {
                    {
                        let gy = y(*gv);
                        let k = format!("{}k", (*gv / 1000.0) as i64);
                        rsx! {
                            g { key: "{gv}",
                                line {
                                    x1: "{pad_l}", y1: "{gy}", x2: "{w - pad_r}", y2: "{gy}",
                                    stroke: "rgba(106,95,192,.16)", stroke_width: "1", stroke_dasharray: "2 4",
                                }
                                text {
                                    x: "{pad_l - 7.0}", y: "{gy + 3.0}", text_anchor: "end",
                                    fill: "var(--ink-3)", font_size: "8.5", font_family: "var(--font-body)", letter_spacing: ".04em",
                                    "{k}"
                                }
                            }
                        }
                    }
                }
                // ground line
                line {
                    x1: "{pad_l}", y1: "{base}", x2: "{w - pad_r}", y2: "{base}",
                    stroke: "rgba(106,95,192,.4)", stroke_width: "1",
                }

                // x ticks
                for m in ticks.iter() {
                    {
                        let tx = x(f64::from(*m));
                        let lbl = if *m == 0 { "NOW".to_string() } else { month_label(*m, &cycle) };
                        rsx! {
                            text {
                                key: "{m}",
                                x: "{tx}", y: "{base + 17.0}", text_anchor: "middle",
                                fill: "var(--ink-3)", font_size: "8.5", font_family: "var(--font-body)", letter_spacing: ".1em",
                                "{lbl}"
                            }
                        }
                    }
                }

                // projection area + line (under history)
                if show_projection && !proj_area.is_empty() {
                    polygon { points: "{proj_area}", fill: "url(#trajproj)" }
                }
                if show_projection && !proj_line.is_empty() {
                    polyline {
                        points: "{proj_line}", fill: "none", stroke: "var(--indigo-neon)",
                        stroke_width: "1.6", stroke_dasharray: "5 4", opacity: ".85",
                    }
                }

                // history area + line
                if !hist_area.is_empty() {
                    polygon { points: "{hist_area}", fill: "url(#trajfill)" }
                }
                if !hist_line.is_empty() {
                    polyline {
                        points: "{hist_line}", fill: "none", stroke: "var(--neon)",
                        stroke_width: "2.2", stroke_linejoin: "round",
                        style: "filter:drop-shadow(0 0 3px var(--neon))",
                    }
                }

                // today marker
                line {
                    x1: "{x(0.0)}", y1: "{pad_t - 8.0}", x2: "{x(0.0)}", y2: "{base + 6.0}",
                    stroke: "rgba(255,59,46,.5)", stroke_width: "1.2", stroke_dasharray: "3 3",
                }
                text {
                    x: "{x(0.0)}", y: "{pad_t - 12.0}", text_anchor: "middle",
                    fill: "var(--neon-dim)", font_size: "8.5", font_family: "var(--font-body)", letter_spacing: ".12em",
                    "{today_label}"
                }
                if let Some(tp) = today_pt {
                    circle {
                        cx: "{x(0.0)}", cy: "{y(tp)}", r: "3.5",
                        fill: "var(--neon-white)", style: "filter:drop-shadow(0 0 4px var(--neon))",
                    }
                }

                // debt-free endpoint
                if show_projection {
                    g {
                        circle {
                            cx: "{df_x}", cy: "{df_y}", r: "4",
                            fill: "var(--bg)", stroke: "var(--ok)", stroke_width: "1.8",
                            style: "filter:drop-shadow(0 0 5px var(--ok))",
                        }
                        text {
                            x: "{df_x}", y: "{df_y - 11.0}", text_anchor: "end",
                            fill: "var(--ok)", font_size: "9", font_family: "var(--font-display)", letter_spacing: ".04em",
                            "DEBT-FREE"
                        }
                        text {
                            x: "{df_x}", y: "{df_y - 1.0}", text_anchor: "end",
                            fill: "var(--ink-3)", font_size: "8", font_family: "var(--font-body)", letter_spacing: ".06em",
                            "{debt_free_label}"
                        }
                    }
                }
            }

            div { class: "traj-foot",
                span { class: "tf-stat", i { "PAID DOWN" } " " b { "{foot_paid}" } " " em { "{foot_of}" } }
                span { class: "tf-stat", i { "AVG RATE" } " " b { class: "warn", "{foot_rate}" } }
                span { class: "tf-stat", i { "DEBT-FREE IN" } " " b { "{foot_horizon}" } }
                span { class: "tf-note",
                    Dot { tone: "alert".to_string(), size: 6 }
                    " "
                    if is_snowball {
                        "SNOWBALL · smallest first"
                        if let Some(nm) = &snowball_target_name {
                            " → {nm}"
                        }
                    } else {
                        "AVALANCHE"
                        if let Some(nm) = &avalanche_target_name {
                            " · {nm} costs the most"
                        }
                        " — target it first"
                    }
                }
            }
        }
    }
}

// ════════════════════════════ PAYOFF METER ══════════════════════════════════

/// Payoff-progress meter (`PayoffMeter` in the JSX). Reads `paid_off_pct` (0–1).
#[component]
fn PayoffMeter(paid_off_pct: f64, tone: String) -> Element {
    let p = paid_off_pct.clamp(0.0, 1.0);
    let col = match tone.as_str() {
        "coral" => "var(--neon)",
        "warn" => "var(--warn)",
        _ => "var(--indigo-neon)",
    };
    let fill_w = (p * 100.0).max(2.0);
    let mk_left = p * 100.0;
    rsx! {
        div { class: "pay-meter",
            div { class: "pay-fill", style: "width:{fill_w}%;background:{col};box-shadow:0 0 6px {col}" }
            span { class: "pay-mk", style: "left:{mk_left}%" }
        }
    }
}

// ════════════════════════════ BALANCE DECAY LINE (inspector) ════════════════

/// Inspector balance-decay line (`DecayLine` in the JSX). SVG kept verbatim.
#[component]
fn DecayLine(
    decay: Option<DecaySeriesRef>,
    #[props(default = 300.0)] w: f64,
    #[props(default = 104.0)] h: f64,
) -> Element {
    let Some(decay) = decay else {
        return rsx! {
            div { class: "dim", style: "padding:20px 0;text-align:center;font-size:11px", "No decay series." }
        };
    };
    let series = decay.series.clone();
    if series.len() < 2 {
        return rsx! {
            div { class: "dim", style: "padding:20px 0;text-align:center;font-size:11px", "No decay series." }
        };
    }
    let today_idx = decay.today_index.min(series.len() - 1);
    let max_y = series.iter().copied().fold(1.0_f64, f64::max) * 1.08;
    let pad_b = 16.0_f64;
    let pad_t = 8.0_f64;
    let pad_l = 4.0_f64;
    let pad_r = 4.0_f64;
    let n = series.len();
    let x = |i: usize| pad_l + (i as f64) / (n as f64 - 1.0) * (w - pad_l - pad_r);
    let y = |v: f64| h - pad_b - (v / max_y) * (h - pad_t - pad_b);
    let hist_pts = series[..=today_idx]
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i), y(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let proj_pts = series[today_idx..]
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i + today_idx), y(*v)))
        .collect::<Vec<_>>()
        .join(" ");
    let rose = decay.hist_len > 1 && series[decay.hist_len - 1] > series[0];
    let hist_col = if rose {
        "var(--neon)"
    } else {
        "var(--indigo-neon)"
    };
    let drop = format!("filter:drop-shadow(0 0 3px {hist_col})");
    let today_x = x(today_idx);
    let today_y = y(series[today_idx]);
    rsx! {
        svg {
            width: "100%",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            preserve_aspect_ratio: "none",
            style: "display:block",
            line {
                x1: "{pad_l}", y1: "{h - pad_b}", x2: "{w - pad_r}", y2: "{h - pad_b}",
                stroke: "rgba(106,95,192,.3)", stroke_width: "1",
            }
            polyline {
                points: "{proj_pts}", fill: "none", stroke: "var(--indigo-neon)",
                stroke_width: "1.5", stroke_dasharray: "4 3", opacity: ".8",
            }
            polyline {
                points: "{hist_pts}", fill: "none", stroke: "{hist_col}",
                stroke_width: "2", stroke_linejoin: "round", style: "{drop}",
            }
            line {
                x1: "{today_x}", y1: "{pad_t - 4.0}", x2: "{today_x}", y2: "{h - pad_b}",
                stroke: "rgba(255,59,46,.4)", stroke_width: "1", stroke_dasharray: "2 3",
            }
            circle {
                cx: "{today_x}", cy: "{today_y}", r: "3",
                fill: "var(--neon-white)", style: "filter:drop-shadow(0 0 3px var(--neon))",
            }
        }
    }
}

/// Flattened decay series for [`DecayLine`] (the JSX merged `hist`+`forward`).
#[derive(Clone, PartialEq)]
struct DecaySeriesRef {
    /// `hist` then `forward`, as presentation CHF.
    series: Vec<f64>,
    /// Index of "today" within the merged series.
    today_index: usize,
    /// Length of the historical segment (for the rose/up colour test).
    hist_len: usize,
}

// ════════════════════════════ DEBT CARD ═════════════════════════════════════

/// One debt as a card (`DebtCard` in the JSX).
#[component]
fn DebtCard(
    d: DebtDto,
    active: bool,
    target: Option<String>,
    cycle: CycleDto,
    on_select: EventHandler<String>,
) -> Element {
    let (st_label, tone) = debt_status(&d.status);
    let pct = (d.paid_off_pct.clamp(0.0, 1.0) * 100.0).round() as i64;
    let months = d.months_to_payoff;
    let is_target = target.as_deref() == Some(d.id.as_str());
    let has_spark = d.hist.len() > 1;
    let months_label = if months >= 600 {
        "—".to_string()
    } else {
        format!("{months} MO")
    };
    let payoff_word = if d.kind == "CARD" {
        "REVOLVING".to_string()
    } else if months < 600 {
        format!("PAYOFF {}", month_label(months, &cycle))
    } else {
        "PAYOFF —".to_string()
    };

    let cls = {
        let mut s = format!("debt osc-bkt {tone}");
        if active {
            s.push_str(" on");
        }
        if is_target {
            s.push_str(" target");
        }
        s
    };
    let leg = if is_target {
        "◎ TARGET".to_string()
    } else if d.status_label.is_empty() {
        st_label.to_string()
    } else {
        d.status_label.clone()
    };
    let apr_str = format!("{:.1}% APR", d.apr * 100.0);
    let paid_of = format!(
        "CHF {} OF {}",
        chf(
            Money::from_centimes(d.orig.centimes() - d.balance.centimes()),
            0
        ),
        chf(d.orig, 0)
    );
    let pay_str = format!("CHF {}/MO", chf(d.monthly, 0));
    let spark_tone = if d.status == "high" { "neon" } else { "indigo" };
    let next_label = if d.next_label.is_empty() {
        "—".to_string()
    } else {
        d.next_label.clone()
    };

    let id_click = d.id.clone();
    let id_key = d.id.clone();

    rsx! {
        div {
            class: "{cls}",
            role: "button",
            tabindex: "0",
            onclick: move |_| on_select.call(id_click.clone()),
            onkeydown: move |e: Event<KeyboardData>| {
                let k = e.key();
                if k == Key::Enter || k == Key::Character(" ".to_string()) {
                    e.prevent_default();
                    on_select.call(id_key.clone());
                }
            },
            span { class: "osc-leg", "{leg}" }
            div { class: "debt-h",
                div { class: "debt-gl", b { "{d.glyph}" } }
                div { class: "debt-id",
                    span { class: "debt-nm", "{d.name}" }
                    span { class: "debt-len",
                        Dot { tone: if d.src == "llm" { "blue".to_string() } else { "ok".to_string() }, size: 5 }
                        "{d.lender}"
                    }
                }
                span { class: "debt-type", "{d.kind}" }
            }

            div { class: "debt-amt",
                span { class: "sp", span { class: "cur", "CHF" } "{chf(d.balance, 0)}" }
                span { class: "un", "OWED" }
                span { class: "apr", "{apr_str}" }
            }

            div { class: "debt-prog",
                PayoffMeter { paid_off_pct: d.paid_off_pct, tone: tone.to_string() }
                div { class: "debt-progmeta",
                    span { "{pct}% PAID OFF" }
                    span { "{paid_of}" }
                }
            }

            div { class: "debt-next",
                if d.status == "high" {
                    span { class: "nx alert", "⚠ COSTLIEST RATE YOU CARRY" }
                } else if d.status == "due" {
                    span { class: "nx warn", "⚠ DUE · {next_label}" }
                } else {
                    span { class: "nx", "NEXT · {next_label}" }
                }
                span { class: "pay", "{pay_str}" }
            }

            div { class: "debt-foot",
                span { class: "term", "{payoff_word} · {months_label}" }
                div { class: "decaytrack",
                    span { class: "pl", "BALANCE" }
                    if has_spark {
                        Spark { data: d.hist.clone(), w: 70.0, h: 20.0, tone: spark_tone.to_string() }
                    }
                }
            }
        }
    }
}

// ════════════════════════════ DEBT ROW (compact) ════════════════════════════

/// One debt as a compact row (`DebtRow` in the JSX).
#[component]
fn DebtRow(
    d: DebtDto,
    active: bool,
    target: Option<String>,
    on_select: EventHandler<String>,
) -> Element {
    let (st_label, tone) = debt_status(&d.status);
    let is_target = target.as_deref() == Some(d.id.as_str());
    let cls = {
        let mut s = "debtrow".to_string();
        if active {
            s.push_str(" on");
        }
        if is_target {
            s.push_str(" target");
        }
        s
    };
    let stat_cls = format!("dr-stat {tone}");
    let stat_label = if is_target {
        "◎ TARGET".to_string()
    } else if d.status_label.is_empty() {
        st_label.to_string()
    } else {
        d.status_label.clone()
    };
    let apr_str = format!("{:.1}%", d.apr * 100.0);
    let next_label = if d.next_label.is_empty() {
        "—".to_string()
    } else {
        d.next_label.clone()
    };
    let id_click = d.id.clone();
    rsx! {
        div { class: "{cls}", onclick: move |_| on_select.call(id_click.clone()),
            div { class: "dr-gl", b { "{d.glyph}" } }
            span { class: "dr-nm", "{d.name}" }
            span { class: "{stat_cls}", "{stat_label}" }
            div { class: "dr-meter", PayoffMeter { paid_off_pct: d.paid_off_pct, tone: tone.to_string() } }
            span { class: "dr-apr", "{apr_str}" }
            span { class: "dr-next", "{next_label}" }
            span { class: "dr-amt", "CHF " b { "{chf(d.balance, 0)}" } }
            span { class: "dr-pay", "CHF {chf(d.monthly, 0)}" i { "/mo" } }
        }
    }
}

// ════════════════════════════ INSPECTOR (right dock) ════════════════════════

/// Debt inspector (`DebtInspector` in the JSX). REFINANCE / ADJUST PLAN have
/// no UI for T17's `debt_plan` yet, so they render (faithful DOM) disabled.
#[component]
fn DebtInspector(
    d: Option<DebtDto>,
    detail: Option<DebtDetailDto>,
    detail_loading: bool,
    payments: Vec<DebtPaymentDto>,
    payments_loading: bool,
    cycle: CycleDto,
    variant: Option<String>,
    on_close: EventHandler<()>,
    on_edit: EventHandler<DebtDto>,
    debt_pay: Signal<Panel<PayDraft>>,
    debt_row: Signal<Panel<bool>>,
    on_pay: Callback<(String, PayDraft), ()>,
    on_delete: Callback<String>,
) -> Element {
    let panel_cls = match &variant {
        Some(v) => format!("sig-panel debt-insp {v}"),
        None => "sig-panel debt-insp".to_string(),
    };

    let Some(d) = d else {
        return rsx! {
            aside { class: "{panel_cls}",
                div { class: "sig-empty",
                    span { class: "mk", "∿" }
                    div { class: "tx",
                        "No debt selected."
                        br {}
                        "Click any "
                        b { "balance" }
                        " on the trajectory or a card to inspect its amortization, interest cost and AI payoff guidance."
                    }
                }
            }
        };
    };

    let months = d.months_to_payoff;
    let pct = (d.paid_off_pct.clamp(0.0, 1.0) * 100.0).round() as i64;
    let refinance = d.apr > 0.08;

    // decay series → merged presentation ref.
    let decay_ref: Option<DecaySeriesRef> = detail.as_ref().map(|dt| {
        let hist: Vec<f64> = dt
            .decay_series
            .hist
            .iter()
            .map(|m| m.as_chf_f64())
            .collect();
        let fwd: Vec<f64> = dt
            .decay_series
            .forward
            .iter()
            .map(|m| m.as_chf_f64())
            .collect();
        let hist_len = hist.len();
        let mut series = hist;
        series.extend(fwd);
        DecaySeriesRef {
            series,
            today_index: dt.decay_series.today_index,
            hist_len,
        }
    });
    let guidance_text = detail
        .as_ref()
        .map(|dt| dt.guidance.clone())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| d.note.clone());

    // pre-computed strings.
    let head_kls = format!("∿ DEBT · {} · {}", d.kind, d.lender);
    let delta_vs = format!("outstanding · {pct}% paid off · {:.1}% APR", d.apr * 100.0);
    let axis_right = if d.kind == "CARD" {
        "REVOLVING".to_string()
    } else {
        "PROJECTED →".to_string()
    };
    let interest_yr = format!("CHF {}", chf(d.annual_interest, 0));
    // interest_remaining: ≥600 months ⇒ revolving (∞ in the JSX).
    let interest_left = if months >= 600 {
        "∞".to_string()
    } else {
        format!("CHF {}", chf(d.interest_remaining, 0))
    };
    let payoff_label = if months >= 600 {
        "—".to_string()
    } else {
        month_label(months, &cycle)
    };
    let since_label = if d.since.is_empty() {
        "—".to_string()
    } else {
        d.since.clone()
    };
    let recent: Vec<DebtPaymentDto> = payments.iter().take(4).cloned().collect();
    let (pay_id, extra_id, edit_d) = (d.id.clone(), d.id.clone(), d.clone());
    let instalment = crate::data::budgets::cap_input_text(d.monthly);
    let pay_label = if debt_pay.read().draft.extra {
        "Extra payment · CHF".to_string()
    } else {
        "Instalment · CHF".to_string()
    };

    rsx! {
        aside { class: "{panel_cls}",
            div { class: "sig-head",
                div { class: "kls", "{head_kls}" }
                div { class: "nm", "{d.name}" }
                div { class: "ds", "{d.note}" }
                span { class: "x", title: "Close", onclick: move |_| on_close.call(()), "✕" }
            }

            div { class: "sig-delta",
                span { class: "big up",
                    span { style: "font-size:16px;color:var(--ink-3);margin-right:5px;vertical-align:4px", "CHF" }
                    "{chf(d.balance, 0)}"
                }
                span { class: "vs", "{delta_vs}" }
            }

            div { class: "sig-chart",
                if detail.is_none() {
                    Awaiting { label: "DECAY SERIES".to_string(), loading: detail_loading }
                } else {
                    DecayLine { decay: decay_ref }
                }
                div { class: "axis",
                    span { "6 MO BACK" }
                    span { "{axis_right}" }
                }
            }

            div { class: "sig-stats",
                div { class: "st", div { class: "k", "Outstanding" } div { class: "v coral", "CHF {chf(d.balance, 0)}" } }
                div { class: "st", div { class: "k", "Monthly" } div { class: "v", "CHF {chf(d.monthly, 0)}" } }
                div { class: "st", div { class: "k", "Interest / yr" } div { class: "v", "{interest_yr}" } }
                div { class: "st", div { class: "k", "Interest left" } div { class: "v", style: "font-size:16px", "{interest_left}" } }
                div { class: "st", div { class: "k", "Payoff" } div { class: "v", style: "font-size:16px", "{payoff_label}" } }
                div { class: "st", div { class: "k", "Since" } div { class: "v", style: "font-size:16px", "{since_label}" } }
            }

            div { class: "sig-recent",
                div { class: "h", "Recent payments" }
                if payments.is_empty() {
                    if payments_loading {
                        Awaiting { label: "PAYMENTS".to_string(), loading: true }
                    } else {
                        div { class: "dim", style: "font-size:11px;padding:6px 0", "No recent payments." }
                    }
                } else {
                    for r in recent.iter() {
                        div { class: "sig-occ", key: "{r.id}",
                            span { class: "dt", "{r.date}" }
                            span { class: "no", style: "flex:1", "{r.note}" }
                            span { class: "pr", "CHF {chf(r.balance, 0)}" }
                        }
                    }
                }
            }

            div { class: "insp-acts",
                if d.actions.pay {
                    button {
                        class: "gbtn p",
                        r#type: "button",
                        onclick: move |_| debt_pay.write().open(&pay_id, PayDraft { amount: instalment.clone(), extra: false }),
                        "PAY INSTALMENT"
                    }
                    button {
                        class: "gbtn",
                        r#type: "button",
                        onclick: move |_| debt_pay.write().open(&extra_id, PayDraft { amount: String::new(), extra: true }),
                        "PAY EXTRA"
                    }
                } else {
                    span { class: "dx-note", "PAID OFF · NO PAYMENT DUE" }
                }
                if refinance {
                    button { class: if d.status == "high" { "gbtn coral" } else { "gbtn" }, r#type: "button", disabled: true, "REFINANCE" }
                } else {
                    button { class: "gbtn", r#type: "button", disabled: true, "ADJUST PLAN" }
                }
            }
            PayField { panel: debt_pay, target: d.id.clone(), label: pay_label, on_pay }
            div { class: "insp-acts",
                button { class: "gbtn", r#type: "button", onclick: move |_| on_edit.call(edit_d.clone()), "EDIT" }
                DeleteConfirm { panel: debt_row, target: d.id.clone(), on_confirm: on_delete }
            }

            div { class: "sig-foot",
                div { class: "tx", "{guidance_text}" }
            }
        }
    }
}

// ════════════════════════════ PERSONAL · IOU LEDGER ═════════════════════════

/// Net-position beam (`NetBeam` in the JSX). SVG kept verbatim; geometry uses
/// `f64` CHF via `as_chf_f64`.
#[component]
fn NetBeam(stats: IouStatsDto, count: Option<u32>) -> Element {
    let owed_to_you = stats.owed_to_you.as_chf_f64();
    let you_owe = stats.you_owe.as_chf_f64();
    let net = stats.net.as_chf_f64();
    let w = 1000.0_f64;
    let h = 96.0_f64;
    let pad = 150.0_f64;
    let cx0 = w / 2.0;
    let axis_y = 52.0_f64;
    let half = w / 2.0 - pad;
    let max_total = owed_to_you.max(you_owe).max(1.0);
    let rx = cx0 + (owed_to_you / max_total) * half;
    let lx = cx0 - (you_owe / max_total) * half;
    let net_x = cx0 + (net / max_total) * half;
    let bh = 16.0_f64;
    let net_pos = stats.net.centimes() >= 0;

    let count_str = count.map_or("—".to_string(), |n| n.to_string());
    let hud = format!("⟷ PERSONAL · IOU LEDGER · {count_str} OPEN");
    let net_cls = if net_pos {
        "beam-net pos".to_string()
    } else {
        "beam-net neg".to_string()
    };
    let net_label = format!(
        "NET {}CHF {} {}",
        if net_pos { "+" } else { "−" },
        chf(money_abs(stats.net), 0),
        if net_pos {
            "IN YOUR FAVOUR"
        } else {
            "YOU'RE BEHIND"
        }
    );
    let needle_col = if net_pos {
        "var(--indigo-neon)"
    } else {
        "var(--neon)"
    };
    let needle_d = format!("M {net_x} {} l -5 -7 l 10 0 z", axis_y - bh / 2.0 - 11.0);
    let needle_style = format!("filter:drop-shadow(0 0 4px {needle_col})");
    let you_owe_str = chf(stats.you_owe, 0);
    let owed_str = chf(stats.owed_to_you, 0);

    rsx! {
        div { class: "iou-beam osc-bkt",
            span { class: "osc-leg", "NET POSITION" }
            div { class: "beam-h",
                span { class: "hud", "{hud}" }
                span { class: "{net_cls}", "{net_label}" }
            }
            svg {
                width: "100%",
                height: "{h}",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none",
                class: "beam-svg",
                // baseline
                line { x1: "{pad}", y1: "{axis_y}", x2: "{w - pad}", y2: "{axis_y}", stroke: "rgba(106,95,192,.35)", stroke_width: "1" }
                // you-owe bar (left, coral)
                rect { x: "{lx}", y: "{axis_y - bh / 2.0}", width: "{cx0 - lx}", height: "{bh}", fill: "rgba(255,94,77,.5)" }
                line {
                    x1: "{lx}", y1: "{axis_y - bh / 2.0 - 3.0}", x2: "{lx}", y2: "{axis_y + bh / 2.0 + 3.0}",
                    stroke: "var(--neon)", stroke_width: "1.5", style: "filter:drop-shadow(0 0 4px var(--neon))",
                }
                // owed-to-you bar (right, blue)
                rect { x: "{cx0}", y: "{axis_y - bh / 2.0}", width: "{rx - cx0}", height: "{bh}", fill: "rgba(143,125,255,.5)" }
                line {
                    x1: "{rx}", y1: "{axis_y - bh / 2.0 - 3.0}", x2: "{rx}", y2: "{axis_y + bh / 2.0 + 3.0}",
                    stroke: "var(--indigo-neon)", stroke_width: "1.5", style: "filter:drop-shadow(0 0 4px var(--indigo-neon))",
                }
                // zero tick
                line { x1: "{cx0}", y1: "{axis_y - bh / 2.0 - 9.0}", x2: "{cx0}", y2: "{axis_y + bh / 2.0 + 9.0}", stroke: "var(--neon-white)", stroke_width: "1.5" }
                text {
                    x: "{cx0}", y: "{axis_y + bh / 2.0 + 22.0}", text_anchor: "middle",
                    fill: "var(--ink-3)", font_size: "8", font_family: "var(--font-body)", letter_spacing: ".16em",
                    "EVEN"
                }
                // net needle
                path { d: "{needle_d}", fill: "{needle_col}", style: "{needle_style}" }
                // end labels
                text { x: "{lx - 10.0}", y: "{axis_y + 4.0}", text_anchor: "end", fill: "var(--neon-hot)", font_size: "14", font_family: "var(--font-display)", "{you_owe_str}" }
                text { x: "{lx - 10.0}", y: "{axis_y - 11.0}", text_anchor: "end", fill: "var(--ink-3)", font_size: "7.5", font_family: "var(--font-body)", letter_spacing: ".14em", "YOU OWE" }
                text { x: "{rx + 10.0}", y: "{axis_y + 4.0}", text_anchor: "start", fill: "var(--text-blue)", font_size: "14", font_family: "var(--font-display)", "{owed_str}" }
                text { x: "{rx + 10.0}", y: "{axis_y - 11.0}", text_anchor: "start", fill: "var(--ink-3)", font_size: "7.5", font_family: "var(--font-body)", letter_spacing: ".14em", "OWED TO YOU" }
            }
        }
    }
}

/// One personal-IOU card (`PersonCard` in the JSX). RECORD PAYMENT and MARK
/// SETTLED show only while the server offers them (`p.actions`); EDIT and
/// DELETE (with a confirm step) always.
#[component]
fn PersonCard(
    p: PersonalIouDto,
    iou_pay: Signal<Panel<PayDraft>>,
    iou_row: Signal<Panel<bool>>,
    iou_settle: Signal<Panel<bool>>,
    on_edit: EventHandler<PersonalIouDto>,
    on_pay: Callback<(String, PayDraft), ()>,
    on_settle: Callback<String>,
    on_delete: Callback<String>,
) -> Element {
    let inbound = p.dir == "in";
    // backend-derived repaid fraction (0–1); render only when > 0 (the JSX showed
    // it whenever repaidPct != null; seeded data always carries it, so we always
    // render the progress block, faithful to the seeded design).
    let pct = (p.repaid_pct.clamp(0.0, 1.0) * 100.0).round() as i64;
    let dir_cls = if inbound {
        "person in".to_string()
    } else {
        "person out".to_string()
    };
    let dir_lbl_cls = if inbound {
        "person-dir in".to_string()
    } else {
        "person-dir out".to_string()
    };
    let dir_lbl = if inbound {
        "← OWED TO YOU"
    } else {
        "YOU OWE →"
    };
    let repaid_of = format!(
        "CHF {} OF {}",
        chf(
            Money::from_centimes(p.of.centimes() - p.amount.centimes()),
            0
        ),
        chf(p.of, 0)
    );
    let (pay_id, settle_id, edit_p) = (p.id.clone(), p.id.clone(), p.clone());
    // One settle runs at a time; its pending line and error show on its card.
    let settle = iou_settle.read().clone();
    let settle_busy = settle.saving;
    let (settling_here, settle_error) = if settle.is_open_for(&p.id) {
        (settle.saving, settle.error)
    } else {
        (false, None)
    };

    rsx! {
        div { class: "{dir_cls}",
            div { class: "person-h",
                div { class: "person-av", b { "{p.initials}" } }
                div { class: "person-id",
                    span { class: "person-nm", "{p.person}" }
                    span { class: "{dir_lbl_cls}", "{dir_lbl}" }
                }
                div { class: "person-amt",
                    span { class: "cur", "CHF" }
                    "{chf(p.amount, 0)}"
                }
            }
            div { class: "person-reason", "{p.reason}" }
            div { class: "person-prog",
                div { class: "pp-bar", div { class: "pp-fill", style: "width:{pct}%" } }
                span { class: "pp-meta", "{pct}% REPAID · {repaid_of}" }
            }
            div { class: "person-foot",
                span { class: "since", "SINCE {p.since}" }
                div { class: "person-acts",
                    if p.actions.pay {
                        button {
                            class: "gbtn",
                            r#type: "button",
                            onclick: move |_| iou_pay.write().open(&pay_id, PayDraft::default()),
                            "RECORD PAYMENT"
                        }
                    }
                    if p.actions.settle {
                        button {
                            class: "gbtn p",
                            r#type: "button",
                            disabled: settle_busy,
                            onclick: move |_| on_settle.call(settle_id.clone()),
                            "MARK SETTLED"
                        }
                    }
                    InlineStatus { pending: settling_here, error: settle_error }
                }
            }
            PayField { panel: iou_pay, target: p.id.clone(), label: "Payment · CHF".to_string(), on_pay }
            div { class: "person-acts",
                button { class: "gbtn", r#type: "button", onclick: move |_| on_edit.call(edit_p.clone()), "EDIT" }
                DeleteConfirm { panel: iou_row, target: p.id.clone(), on_confirm: on_delete }
            }
        }
    }
}
