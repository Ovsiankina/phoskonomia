//! Config page (route `/config`). Faithful port of React `pages/Config.jsx`.
//!
//! Shape preserved 1:1 with the JSX: the `.pk` root + `ScannerBg` (the molten
//! signal parked lower-right, the faint upper-left blob, the wire scaffold), the
//! `.app-shell > .app-main` (TopBar + scroll, NO left AiPanel here — Config never
//! rendered one), the `.cfg-wrap` head + status ribbon + 2-col panel grid
//! (General/Transactions/Budgets/Subscriptions/Debts/Analytics) + footer.
//!
//! Data model: two stores, no `Money` shape.
//!   * The per-surface layout tweaks (mirroring React's `CFG_DEFAULTS`) have no
//!     backend preference behind them. They live in client `use_signal` state,
//!     persisted to `localStorage` (below).
//!   * The CORE panel is the backend's `phosk_settings` key set, loaded, saved
//!     and reset through [`crate::data::settings`]. The server validates every
//!     write and answers with the refreshed view; the page renders only the
//!     values it is handed and never decides what is valid. The same view feeds
//!     the ASSISTANT engine/model labels and adds its rows to the header
//!     counts. "Reset all" resets both stores.
//!
//! The shared [`get_cycle`] read feeds the TopBar's date tokens.
//!
//! Persistence + cross-page live-sync (`__phoskReadCfg`/`__phoskWriteCfg` +
//! the `"phoskcfg"` CustomEvent) is the one piece that MUST touch `window` /
//! `localStorage`, which only `document::eval` can do from Rust. On mount we
//! read `localStorage["phosk.cfg"]` over the defaults (a `document::eval` that
//! posts the stored JSON back); on every `set`/`reset` we eval a tiny script
//! that merges into the store and dispatches `"phoskcfg"` — faithful to React.
//!
//! JSX idioms → RSX:
//!   * the four `Cfg*` control components + `CfgRow`/`CfgPanel` → private
//!     `#[component]`s here (same DOM/classes/attrs), callbacks as `EventHandler`.
//!   * `useState`/`useMemo` → `use_signal` + pre-computed `let`s; `useEffect`
//!     (resize/prefs) → `use_effect` + `document::eval` bridge.
//!   * the cfg store is a `HashMap<&'static str, CfgVal>` (string|bool) so a
//!     single `set(key, val)` mirrors React's union-of-tweaks store and the
//!     `inspAll`/`changed`-count derivations work generically.

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::components::comps::TopBar;
use crate::components::prims::ScannerBg;
use crate::components::states::Awaiting;
use crate::data::cycle::get_cycle;
use crate::data::settings::{
    get_preferences, reset_all_preferences, reset_preference, set_preference, PreferenceRowDto,
    SettingsDto,
};

// ── the config store value: a tweak is either an enum string or a boolean ─────

/// One stored preference value. The React store is untyped JSON (strings +
/// booleans); we keep that union so a single generic `set` drives every row and
/// the changed-count / inspector-unify derivations stay generic.
#[derive(Clone, PartialEq, Debug)]
enum CfgVal {
    /// A selector / segmented / text value (e.g. `"cards"`, `"{label} · DAY …"`).
    Str(String),
    /// A switch value.
    Bool(bool),
}

impl CfgVal {
    fn as_str(&self) -> &str {
        match self {
            CfgVal::Str(s) => s.as_str(),
            CfgVal::Bool(_) => "",
        }
    }
    fn as_bool(&self) -> bool {
        matches!(self, CfgVal::Bool(true))
    }
}

/// The union of every tweak across all surfaces — keys & defaults mirror the
/// `CFG_DEFAULTS` in the JSX (same key strings, same default values), and the
/// EDITMODE blocks in transactions/budgets/subscriptions/debts/analytics.
/// Returned as an ordered map so iteration order (and thus the local
/// preference COUNT) is stable and matches the JSX's `Object.keys` length.
fn cfg_defaults() -> BTreeMap<&'static str, CfgVal> {
    use CfgVal::{Bool, Str};
    let s = |v: &str| Str(v.to_string());
    BTreeMap::from([
        ("aiOpen", Bool(true)),
        // Top bar
        ("topDateFmt", s("{label} · DAY {day}/{days}")),
        // Transactions
        ("drillMode", s("inspector")),
        ("showSparks", Bool(true)),
        // Budgets
        ("envLayout", s("cards")),
        ("sort", s("order")),
        ("showProj", Bool(true)),
        // Subscriptions
        ("subView", s("cards")),
        ("subSort", s("due")),
        ("subAmounts", s("monthly")),
        ("subGroup", Bool(false)),
        ("subHlAuto", Bool(false)),
        ("subInsp", s("dock")),
        // Debts
        ("debtView", s("cards")),
        ("debtSort", s("balance")),
        ("debtStrategy", s("avalanche")),
        ("debtProjection", Bool(true)),
        ("debtGroup", Bool(false)),
        ("debtHlAuto", Bool(false)),
        ("debtInsp", s("dock")),
        ("iouShow", Bool(true)),
        // Analytics
        ("trendWindow", s("12")),
        ("trendMode", s("spend")),
        ("sigSort", s("momentum")),
        ("showCand", Bool(true)),
        ("showMomentum", Bool(true)),
        ("momentumSort", s("momentum")),
        ("showRhythm", Bool(true)),
        ("sigInsp", s("dock")),
    ])
}

/// The total preference count (`Object.keys(CFG_DEFAULTS).length` in the JSX).
fn cfg_total() -> usize {
    cfg_defaults().len()
}

/// A `(value, label)` option pair for a segmented/select control. Owned, so the
/// CORE rows can build options from the values the server hands them.
#[derive(Clone, PartialEq)]
struct Opt {
    value: String,
    label: String,
}

fn opt(value: &str, label: &str) -> Opt {
    Opt {
        value: value.to_string(),
        label: label.to_string(),
    }
}

// ── persistence bridge (the only `window`/localStorage touch — via eval) ──────

/// Merge `edits` into `localStorage["phosk.cfg"]` and broadcast `"phoskcfg"` so
/// every open surface live-syncs. `edits_json` is a JS object literal string
/// (e.g. `{"subInsp":"dock"}`). Faithful to `__phoskWriteCfg` + the React
/// `window.dispatchEvent(new CustomEvent("phoskcfg", …))`.
fn persist_edits(edits_json: &str) {
    let _ = document::eval(&format!(
        r#"
        try {{
          var KEY = "phosk.cfg";
          var edits = {edits_json};
          var cur = {{}};
          try {{ cur = JSON.parse(localStorage.getItem(KEY) || "{{}}") || {{}}; }} catch (e) {{}}
          for (var k in edits) cur[k] = edits[k];
          localStorage.setItem(KEY, JSON.stringify(cur));
          window.dispatchEvent(new CustomEvent("phoskcfg", {{ detail: edits }}));
        }} catch (e) {{}}
        "#
    ));
}

/// Reset: drop `localStorage["phosk.cfg"]` and broadcast the full defaults.
/// `defaults_json` is the JS object literal of the default store. Faithful to
/// React's `reset()` (localStorage.removeItem + `"phoskcfg"` with defaults).
fn persist_reset(defaults_json: &str) {
    let _ = document::eval(&format!(
        r#"
        try {{
          localStorage.removeItem("phosk.cfg");
          window.dispatchEvent(new CustomEvent("phoskcfg", {{ detail: {defaults_json} }}));
        }} catch (e) {{}}
        "#
    ));
}

/// JS string literal for one edit (`"key": value`), JSON-escaping a string val.
fn js_kv(key: &str, val: &CfgVal) -> String {
    match val {
        CfgVal::Bool(b) => format!("{key:?}:{b}"),
        CfgVal::Str(s) => format!("{key:?}:{}", json_str(s)),
    }
}

/// Minimal JSON string-literal escaping (quotes, backslashes, control chars).
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ════════════════════════ on-brand control components ════════════════════════

/// A control row: label + optional hint on the left, the control on the right.
/// (React `CfgRow`.)
#[component]
fn CfgRow(label: String, hint: Option<String>, children: Element) -> Element {
    rsx! {
        div { class: "cfg-row",
            div { class: "rl",
                div { class: "lab", "{label}" }
                if let Some(h) = hint {
                    div { class: "hint", "{h}" }
                }
            }
            div { class: "rc", {children} }
        }
    }
}

/// A sharp segmented selector (2–3 short options). (React `CfgSeg`.)
/// `numeric` sets the option labels in Pilowlava; `disabled` locks the buttons.
#[component]
fn CfgSeg(
    value: String,
    options: Vec<Opt>,
    on_change: EventHandler<String>,
    #[props(default = false)] numeric: bool,
    #[props(default = false)] disabled: bool,
) -> Element {
    let cls = if numeric { "cfg-seg num" } else { "cfg-seg" };
    rsx! {
        div { class: "{cls}", role: "radiogroup",
            for o in options.iter() {
                {
                    let on = o.value == value;
                    let v = o.value.clone();
                    rsx! {
                        button {
                            key: "{o.value}",
                            r#type: "button",
                            role: "radio",
                            disabled,
                            "aria-checked": "{on}",
                            class: if on { "on" } else { "" },
                            onclick: move |_| on_change.call(v.clone()),
                            "{o.label}"
                        }
                    }
                }
            }
        }
    }
}

/// A native `<select>` for longer option lists. (React `CfgSelect`.)
/// `numeric` sets the options in Pilowlava; `disabled` locks the control.
#[component]
fn CfgSelect(
    value: String,
    options: Vec<Opt>,
    on_change: EventHandler<String>,
    #[props(default = false)] numeric: bool,
    #[props(default = false)] disabled: bool,
) -> Element {
    let cls = if numeric {
        "cfg-select num"
    } else {
        "cfg-select"
    };
    rsx! {
        div { class: "{cls}",
            select {
                value: "{value}",
                disabled,
                onchange: move |e| on_change.call(e.value()),
                for o in options.iter() {
                    option { key: "{o.value}", value: "{o.value}", "{o.label}" }
                }
            }
        }
    }
}

/// A free-form text field (e.g. the date-format template). (React `CfgText`.)
#[component]
fn CfgText(value: String, placeholder: String, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "cfg-text",
            input {
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder}",
                spellcheck: "false",
                oninput: move |e| on_change.call(e.value()),
            }
        }
    }
}

/// A rectangular OFF/ON switch (blue neon when engaged). (React `CfgSwitch`.)
#[component]
fn CfgSwitch(value: bool, on_change: EventHandler<bool>) -> Element {
    let data_on = if value { "1" } else { "0" };
    rsx! {
        button {
            r#type: "button",
            class: "cfg-switch",
            "data-on": "{data_on}",
            role: "switch",
            "aria-checked": "{value}",
            onclick: move |_| on_change.call(!value),
            span { class: "st off", "OFF" }
            span { class: "st on", "ON" }
            span { class: "knob" }
        }
    }
}

/// A config panel = frosted readout box with a Pilowlava legend cut into the top
/// edge; `accent` adds `acc-<accent>`, `span2` makes it full-width. (React
/// `CfgPanel`.)
#[component]
fn CfgPanel(
    glyph: String,
    title: String,
    sub: Option<String>,
    #[props(default = String::new())] accent: String,
    #[props(default = false)] span2: bool,
    children: Element,
) -> Element {
    let mut cls = String::from("cfg-panel");
    if !accent.is_empty() {
        cls.push_str(" acc-");
        cls.push_str(&accent);
    }
    if span2 {
        cls.push_str(" span-2");
    }
    rsx! {
        section { class: "{cls}",
            div { class: "cfg-legend",
                span { class: "gl", "{glyph}" }
                span { class: "nm", "{title}" }
            }
            if let Some(s) = sub {
                div { class: "cfg-panel-sub", "{s}" }
            }
            {children}
        }
    }
}

// ══════════════════ CORE panel: server-backed `phosk_settings` ═══════════════

/// One write against the settings service.
#[derive(Clone, PartialEq, Debug)]
enum PrefWrite {
    /// Set `key` to one of its server-provided allowed values.
    Set(String, String),
    /// Reset `key` to its factory value.
    Reset(String),
    /// Reset every known key ("Reset all").
    ResetAll,
}

impl PrefWrite {
    /// The row this write targets (`"*"` = every row).
    fn target(&self) -> String {
        match self {
            PrefWrite::Set(k, _) | PrefWrite::Reset(k) => k.clone(),
            PrefWrite::ResetAll => "*".to_string(),
        }
    }
}

/// Row label + hint for a known key. Presentation only: which keys exist and
/// which values they accept is the server's call.
fn pref_copy(key: &str) -> (String, String) {
    let (label, hint) = match key {
        "momentum_baseline_cycles" => (
            "Momentum baseline",
            "Trailing cycles averaged for every momentum delta",
        ),
        "currency" => ("Currency", "Amounts are exact CHF centimes"),
        "cycle_period" => ("Budget cycle", "Budgets run on calendar-month cycles"),
        "low_confidence_threshold" => (
            "Low-confidence threshold",
            "AI proposals scoring below it are flagged for review",
        ),
        "telemetry" => ("Telemetry", "Nothing leaves this device"),
        other => (other, ""),
    };
    (label.to_string(), hint.to_string())
}

/// A value made only of digits and dots renders in Pilowlava.
fn is_numeric(v: &str) -> bool {
    !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// The CORE panel body: pending / error / empty states, then one row per key,
/// then the refused-write state.
#[component]
fn CorePrefs(
    view: Option<SettingsDto>,
    loading: bool,
    load_error: bool,
    busy: Option<String>,
    write_error: bool,
    on_write: EventHandler<PrefWrite>,
) -> Element {
    let Some(view) = view else {
        let (message, hint) = if load_error {
            (
                Some("Preferences unavailable".to_string()),
                Some("The settings service did not answer · reload to retry".to_string()),
            )
        } else {
            (None, None)
        };
        return rsx! {
            Awaiting { label: "CORE · PREFERENCES".to_string(), loading, message, hint }
        };
    };
    if view.rows.is_empty() {
        return rsx! {
            Awaiting {
                label: "CORE · PREFERENCES".to_string(),
                message: Some("No preferences stored".to_string()),
            }
        };
    }
    let busy_any = busy.is_some();
    let saving_all = busy.as_deref() == Some("*");
    rsx! {
        for r in view.rows.iter() {
            PrefRow {
                key: "{r.key}",
                row: r.clone(),
                disabled: busy_any,
                saving: saving_all || busy.as_deref() == Some(r.key.as_str()),
                on_write,
            }
        }
        if write_error {
            Awaiting {
                label: "CORE · WRITE".to_string(),
                message: Some("Not saved · the settings service refused the change".to_string()),
                hint: Some("Nothing was written; the rows above show the stored values".to_string()),
            }
        }
    }
}

/// One server-backed preference: the control built from the server's allowed
/// values (locked when there is only one), plus a reset for a user override.
#[component]
fn PrefRow(
    row: PreferenceRowDto,
    disabled: bool,
    saving: bool,
    on_write: EventHandler<PrefWrite>,
) -> Element {
    let (label, hint) = pref_copy(&row.key);
    let locked = row.allowed.len() <= 1;
    let numeric = row.allowed.iter().all(|v| is_numeric(v));
    let options: Vec<Opt> = row.allowed.iter().map(|v| opt(v, v)).collect();
    let many = options.len() > 4;
    let status = if saving {
        "SAVING…"
    } else if row.user_modified {
        "CHANGED"
    } else if locked {
        "FIXED"
    } else {
        "DEFAULT"
    };
    let default_cls = if is_numeric(&row.default_value) {
        "num"
    } else {
        ""
    };
    let default_value = row.default_value.clone();
    let key_set = row.key.clone();
    let key_reset = row.key.clone();
    rsx! {
        div { class: "cfg-row", "aria-busy": "{saving}",
            div { class: "rl",
                div { class: "lab", "{label}" }
                div { class: "hint",
                    if !hint.is_empty() {
                        "{hint} · "
                    }
                    "{status}"
                    if !locked {
                        " · default "
                        span { class: "{default_cls}", "{default_value}" }
                    }
                }
            }
            div { class: "rc",
                div { class: "cfg-pref",
                    if locked {
                        CfgSeg {
                            value: row.value.clone(),
                            options,
                            numeric,
                            disabled: true,
                            on_change: move |_: String| {},
                        }
                    } else if many {
                        CfgSelect {
                            value: row.value.clone(),
                            options,
                            numeric,
                            disabled,
                            on_change: move |v: String| on_write.call(PrefWrite::Set(key_set.clone(), v)),
                        }
                    } else {
                        CfgSeg {
                            value: row.value.clone(),
                            options,
                            numeric,
                            disabled,
                            on_change: move |v: String| on_write.call(PrefWrite::Set(key_set.clone(), v)),
                        }
                    }
                    if row.user_modified {
                        button {
                            class: "cfg-reset",
                            r#type: "button",
                            disabled,
                            onclick: move |_| on_write.call(PrefWrite::Reset(key_reset.clone())),
                            "↺ Default"
                        }
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════ page ═══════════════════════════════════

/// The Phoskonomia Config (preferences) page.
#[component]
pub fn ConfigPage() -> Element {
    // Shared cycle read drives the TopBar date ticker (the only server fn here).
    let cycle = use_resource(get_cycle);

    // ---- the config store (was: useCfg → CFG_DEFAULTS < localStorage) ----
    // Start at defaults; an on-mount eval reads localStorage["phosk.cfg"] and
    // overlays any known keys (defaults < localStorage), faithful to React's
    // `init = { ...CFG_DEFAULTS, ...pickKnown(read()) }`.
    let cfg = use_signal(cfg_defaults);
    let mut just_reset = use_signal(|| false);

    // ---- CORE preferences (server-backed, `phosk_settings`) ----
    // The first read comes from the resource; every successful write answers
    // with the refreshed view, which then wins over the initial read.
    let prefs = use_resource(get_preferences);
    let prefs_live = use_signal(|| None::<SettingsDto>);
    let pref_busy = use_signal(|| None::<String>);
    let pref_err = use_signal(|| false);

    // On-mount: read the stored JSON back and merge known keys over defaults.
    use_effect(move || {
        let mut store = cfg;
        let mut eval = document::eval(
            r#"
            var raw = "{}";
            try { raw = localStorage.getItem("phosk.cfg") || "{}"; } catch (e) {}
            dioxus.send(raw);
            "#,
        );
        spawn(async move {
            if let Ok(raw) = eval.recv::<String>().await {
                let parsed = parse_stored(&raw);
                if !parsed.is_empty() {
                    let mut next = store.read().clone();
                    for (k, v) in parsed {
                        // Only adopt keys we know, and keep the value's type from
                        // the default (string vs bool) — `pickKnown` in the JSX.
                        if let Some(def) = next.get(k) {
                            match (def, &v) {
                                (CfgVal::Bool(_), CfgVal::Bool(_))
                                | (CfgVal::Str(_), CfgVal::Str(_)) => {
                                    next.insert(k, v);
                                }
                                _ => {}
                            }
                        }
                    }
                    store.set(next);
                }
            }
        });
    });

    // set(key, value): update the in-memory store, persist + broadcast. A
    // `use_callback` so the same logic is shared (Copy) across every row's
    // `on_change` closure — faithful to React's single `set` from `useCfg`.
    let set = use_callback(move |edits: Vec<(&'static str, CfgVal)>| {
        let mut store = cfg;
        let mut next = store.read().clone();
        let kvs: Vec<String> = edits
            .iter()
            .map(|(k, v)| {
                next.insert(k, v.clone());
                js_kv(k, v)
            })
            .collect();
        store.set(next);
        persist_edits(&format!("{{{}}}", kvs.join(",")));
    });

    // reset(): back to defaults, drop the store, broadcast defaults.
    let reset = use_callback(move |()| {
        let mut store = cfg;
        let defs = cfg_defaults();
        let kvs: Vec<String> = defs.iter().map(|(k, v)| js_kv(k, v)).collect();
        store.set(defs);
        persist_reset(&format!("{{{}}}", kvs.join(",")));
    });

    // One write at a time against the settings service; the controls are
    // disabled while it runs. A refusal leaves the rows at the stored values.
    let write_pref = use_callback(move |op: PrefWrite| {
        let mut live = prefs_live;
        let mut busy = pref_busy;
        let mut err = pref_err;
        busy.set(Some(op.target()));
        err.set(false);
        spawn(async move {
            let res = match op {
                PrefWrite::Set(key, value) => set_preference(key, value).await,
                PrefWrite::Reset(key) => reset_preference(key).await,
                PrefWrite::ResetAll => reset_all_preferences().await,
            };
            match res {
                Ok(view) => live.set(Some(view)),
                Err(_) => err.set(true),
            }
            busy.set(None);
        });
    });

    // onReset: reset both stores + flash the "✓ Reset" confirmation for ~1.4s.
    let on_reset = move |_| {
        reset.call(());
        write_pref.call(PrefWrite::ResetAll);
        just_reset.set(true);
        spawn(async move {
            gloo_or_sleep().await;
            just_reset.set(false);
        });
    };

    // ---- derivations (was: useMemo + the summary/account/ai-status reads) ----
    // Counts = the local tweaks + the CORE rows once the settings view is in.
    let prefs_view: Option<SettingsDto> = prefs_live
        .read()
        .clone()
        .or_else(|| prefs.read().as_ref().and_then(|r| r.as_ref().ok()).cloned());
    let prefs_loading = prefs.read().is_none() && prefs_view.is_none();
    let prefs_load_err = matches!(&*prefs.read(), Some(Err(_))) && prefs_view.is_none();
    let (core_total, core_changed) = prefs_view.as_ref().map_or((0, 0), |v| {
        (
            v.rows.len(),
            usize::try_from(v.changed_count).unwrap_or(usize::MAX),
        )
    });
    let store = cfg.read().clone();
    let defaults = cfg_defaults();
    let local_total = cfg_total();
    let local_changed = store
        .iter()
        .filter(|(k, v)| defaults.get(**k).is_some_and(|d| d != *v))
        .count();
    let total = local_total + core_total;
    let changed = local_changed.saturating_add(core_changed);

    // Global inspector placement unifies the three per-surface inspector keys.
    let insp_keys = ["subInsp", "debtInsp", "sigInsp"];
    let sub_insp = store
        .get("subInsp")
        .map(CfgVal::as_str)
        .unwrap_or("dock")
        .to_string();
    let insp_all = if insp_keys
        .iter()
        .all(|k| store.get(k).map(CfgVal::as_str) == Some(sub_insp.as_str()))
    {
        sub_insp.clone()
    } else {
        "mixed".to_string()
    };

    // ---- value snapshots for each control (read once, owned) ----
    let g = |k: &str| store.get(k).map(CfgVal::as_str).unwrap_or("").to_string();
    let b = |k: &str| store.get(k).map(CfgVal::as_bool).unwrap_or(false);

    let ai_open = b("aiOpen");
    let top_date_fmt = g("topDateFmt");
    let drill_mode = g("drillMode");
    let show_sparks = b("showSparks");
    let env_layout = g("envLayout");
    let sort = g("sort");
    let show_proj = b("showProj");
    let sub_view = g("subView");
    let sub_sort = g("subSort");
    let sub_amounts = g("subAmounts");
    let sub_group = b("subGroup");
    let sub_hl_auto = b("subHlAuto");
    let debt_view = g("debtView");
    let debt_sort = g("debtSort");
    let debt_strategy = g("debtStrategy");
    let debt_projection = b("debtProjection");
    let debt_group = b("debtGroup");
    let debt_hl_auto = b("debtHlAuto");
    let iou_show = b("iouShow");
    let trend_window = g("trendWindow");
    let trend_mode = g("trendMode");
    let sig_sort = g("sigSort");
    let show_cand = b("showCand");
    let show_momentum = b("showMomentum");
    let momentum_sort = g("momentumSort");
    let show_rhythm = b("showRhythm");

    // ---- pre-computed compound display strings (rsx segment parser is strict) ----
    let total_str = total.to_string();
    let changed_str = changed.to_string();
    let reset_label = if just_reset() {
        "✓ Reset"
    } else {
        "↺ Reset all"
    };
    let reset_cls = if just_reset() {
        "cfg-reset saved"
    } else {
        "cfg-reset"
    };
    let changed_v_cls = if changed > 0 { "v coral" } else { "v blue" };
    let changed_s = if changed > 0 {
        "differs from defaults"
    } else {
        "all at defaults"
    };
    let assistant_v = if ai_open { "ON" } else { "OFF" };
    // LLM engine/model labels come with the settings view; the em-dash stands
    // in while it loads or if the read failed.
    let (engine_lbl, model_lbl) = prefs_view.as_ref().map_or_else(
        || ("—".to_string(), "—".to_string()),
        |v| (v.engine.clone(), v.model.clone()),
    );
    let insp_hint = if insp_all == "mixed" {
        "Mixed across surfaces — pick one to unify"
    } else {
        "Where inspectors open across all surfaces"
    };

    // TopBar date tokens, read from the cycle. The bar itself token-substitutes
    // these against the live `topDateFmt` cfg value (and listens for `phoskcfg`),
    // so editing the "Top-bar date format" field below updates the readout
    // instantly here — faithful to React's `dateText` chain (comps.jsx L50-56).
    // `days == 0` (cycle not yet loaded) yields empty tokens → the bar's blank
    // `" "` fallback, matching the prior pre-baked behaviour.
    let (cyc_label, cyc_day, cyc_days, cyc_as_of) = match &*cycle.read() {
        Some(Ok(c)) if c.days > 0 => (
            c.label.clone(),
            c.day.to_string(),
            c.days.to_string(),
            c.as_of.clone(),
        ),
        _ => (String::new(), String::new(), String::new(), String::new()),
    };

    rsx! {
        div { class: "pk", style: "height:100vh;min-height:0",
            // Config field — molten signal parked LOWER-RIGHT in negative space,
            // a faint blob upper-left, thin wire scaffold; no shape under panels.
            ScannerBg {
                class: "pk-bg".to_string(),
                seed: 117,
                shapes: r#"[
                    { char: "8", cx: .94, cy: .82, scale: .34, style: "red", morph: "vein", live: true, fill: .46 },
                    { char: "P", cx: .08, cy: .28, scale: .27, style: "faint", morph: "blob", live: false, fill: .42 },
                    { char: "2", cx: .46, cy: .93, scale: .15, style: "wire", morph: "vein", live: false, fill: .3 },
                    { char: "5", cx: .95, cy: .14, scale: .12, style: "wire", morph: "vein", live: false, fill: .26 }
                ]"#.to_string(),
            }

            div { class: "app-shell",
                div { class: "app-main",
                    TopBar {
                        active: "CONFIG".to_string(),
                        label: cyc_label,
                        day: cyc_day,
                        days: cyc_days,
                        as_of: cyc_as_of,
                    }
                    div { class: "app-scroll", "data-screen-label": "CONFIG",
                        div { class: "cfg-wrap",

                            // ---- head ----
                            div { class: "cfg-top",
                                div {
                                    div { class: "ttl", "Config" }
                                    div { class: "sum",
                                        b { "{total_str}" }
                                        " preferences across "
                                        b { "5" }
                                        " surfaces ·"
                                        span { class: "coral", " {changed_str} changed" }
                                        " from defaults · stored on this device"
                                    }
                                }
                                div { class: "cfg-top-actions",
                                    button { class: "{reset_cls}", onclick: on_reset, "{reset_label}" }
                                }
                            }

                            // ---- status ribbon ----
                            div { class: "cfg-ribbon",
                                div { class: "cfg-rib",
                                    div { class: "k", "SURFACES TUNED" }
                                    div { class: "v", "5" }
                                    div { class: "s", "txns · budgets · subs · debts · analytics" }
                                }
                                div { class: "cfg-rib",
                                    div { class: "k", "PREFERENCES" }
                                    div { class: "v", "{total_str}" }
                                    div { class: "s", "selectors, switches & orders" }
                                }
                                div { class: "cfg-rib",
                                    div { class: "k", "CHANGED" }
                                    div { class: "{changed_v_cls}", "{changed_str}" }
                                    div { class: "s", "{changed_s}" }
                                }
                                div { class: "cfg-rib",
                                    div { class: "k", "ASSISTANT" }
                                    div { class: "v blue", "{assistant_v}" }
                                    div { class: "s", "{engine_lbl} · {model_lbl}" }
                                }
                            }

                            // ---- panel grid ----
                            div { class: "cfg-grid",

                                // GENERAL — the one coral panel (app-wide defaults)
                                CfgPanel {
                                    glyph: "⊙".to_string(),
                                    title: "General".to_string(),
                                    accent: "coral".to_string(),
                                    span2: true,
                                    sub: "App-wide defaults applied on every surface. Inspector placement sets where signal & detail docks open across Subscriptions, Debts and Analytics at once.".to_string(),
                                    CfgRow {
                                        label: "Assistant panel open by default".to_string(),
                                        hint: "The local GEMMA4 dock on the left edge of every surface".to_string(),
                                        CfgSwitch {
                                            value: ai_open,
                                            on_change: move |v: bool| set.call(vec![("aiOpen", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Inspector placement".to_string(),
                                        hint: insp_hint.to_string(),
                                        CfgSeg {
                                            value: insp_all.clone(),
                                            on_change: move |v: String| set.call(vec![
                                                ("subInsp", CfgVal::Str(v.clone())),
                                                ("debtInsp", CfgVal::Str(v.clone())),
                                                ("sigInsp", CfgVal::Str(v.clone())),
                                            ]),
                                            options: vec![opt("dock", "Dock"), opt("drawer", "Drawer")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Top-bar date format".to_string(),
                                        hint: "Tokens: {label} {day} {days} {asOf} · overlong text scrolls as a ticker".to_string(),
                                        CfgText {
                                            value: top_date_fmt,
                                            placeholder: "{label} · DAY {day}/{days}".to_string(),
                                            on_change: move |v: String| set.call(vec![("topDateFmt", CfgVal::Str(v))]),
                                        }
                                    }
                                }

                                // CORE — the backend-owned preferences (indigo, no coral)
                                CfgPanel {
                                    glyph: "⊡".to_string(),
                                    title: "Core".to_string(),
                                    span2: true,
                                    sub: "Owned by the settings service and checked on the server before anything is stored. Locked rows are fixed by design.".to_string(),
                                    CorePrefs {
                                        view: prefs_view.clone(),
                                        loading: prefs_loading,
                                        load_error: prefs_load_err,
                                        busy: pref_busy(),
                                        write_error: pref_err(),
                                        on_write: move |op: PrefWrite| write_pref.call(op),
                                    }
                                }

                                // TRANSACTIONS
                                CfgPanel {
                                    glyph: "⊟".to_string(),
                                    title: "Transactions".to_string(),
                                    sub: "Every receipt, itemized — and how its detail + item-signal drill opens.".to_string(),
                                    CfgRow {
                                        label: "Signal panel".to_string(),
                                        hint: "Where a tapped line item's trend appears".to_string(),
                                        CfgSeg {
                                            value: drill_mode,
                                            on_change: move |v: String| set.call(vec![("drillMode", CfgVal::Str(v))]),
                                            options: vec![opt("inspector", "Dock"), opt("drawer", "Drawer"), opt("sheet", "Sheet")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Trend sparks on pills".to_string(),
                                        hint: "Inline sparkline on tracked-item tags".to_string(),
                                        CfgSwitch {
                                            value: show_sparks,
                                            on_change: move |v: bool| set.call(vec![("showSparks", CfgVal::Bool(v))]),
                                        }
                                    }
                                }

                                // BUDGETS
                                CfgPanel {
                                    glyph: "▦".to_string(),
                                    title: "Budgets".to_string(),
                                    sub: "Envelope caps against the monthly budget — layout, ordering and projection.".to_string(),
                                    CfgRow {
                                        label: "Envelope layout".to_string(),
                                        CfgSeg {
                                            value: env_layout,
                                            on_change: move |v: String| set.call(vec![("envLayout", CfgVal::Str(v))]),
                                            options: vec![opt("cards", "Cards"), opt("rows", "Rows")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Sort order".to_string(),
                                        CfgSeg {
                                            value: sort,
                                            on_change: move |v: String| set.call(vec![("sort", CfgVal::Str(v))]),
                                            options: vec![opt("order", "Order"), opt("used", "Used"), opt("over", "Over")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Projection markers".to_string(),
                                        hint: "Projected end-of-cycle tick on each envelope".to_string(),
                                        CfgSwitch {
                                            value: show_proj,
                                            on_change: move |v: bool| set.call(vec![("showProj", CfgVal::Bool(v))]),
                                        }
                                    }
                                }

                                // SUBSCRIPTIONS
                                CfgPanel {
                                    glyph: "⊠".to_string(),
                                    title: "Subscriptions".to_string(),
                                    sub: "Standing recurring charges as a periodic impulse train.".to_string(),
                                    CfgRow {
                                        label: "Layout".to_string(),
                                        CfgSeg {
                                            value: sub_view,
                                            on_change: move |v: String| set.call(vec![("subView", CfgVal::Str(v))]),
                                            options: vec![opt("cards", "Cards"), opt("rows", "Rows")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Sort".to_string(),
                                        CfgSeg {
                                            value: sub_sort,
                                            on_change: move |v: String| set.call(vec![("subSort", CfgVal::Str(v))]),
                                            options: vec![opt("due", "Due"), opt("amount", "Cost"), opt("name", "A–Z")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Amounts".to_string(),
                                        hint: "Show per-charge cost or annualized total".to_string(),
                                        CfgSeg {
                                            value: sub_amounts,
                                            on_change: move |v: String| set.call(vec![("subAmounts", CfgVal::Str(v))]),
                                            options: vec![opt("monthly", "Per charge"), opt("annual", "Annual")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Group by cadence".to_string(),
                                        CfgSwitch {
                                            value: sub_group,
                                            on_change: move |v: bool| set.call(vec![("subGroup", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Highlight auto-detected".to_string(),
                                        hint: "Flag charges the AI surfaced on its own".to_string(),
                                        CfgSwitch {
                                            value: sub_hl_auto,
                                            on_change: move |v: bool| set.call(vec![("subHlAuto", CfgVal::Bool(v))]),
                                        }
                                    }
                                }

                                // DEBTS
                                CfgPanel {
                                    glyph: "∿".to_string(),
                                    title: "Debts".to_string(),
                                    sub: "Outstanding balances as a decaying waveform, with a payoff strategy overlay.".to_string(),
                                    CfgRow {
                                        label: "Layout".to_string(),
                                        CfgSeg {
                                            value: debt_view,
                                            on_change: move |v: String| set.call(vec![("debtView", CfgVal::Str(v))]),
                                            options: vec![opt("cards", "Cards"), opt("rows", "Rows")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Sort".to_string(),
                                        CfgSelect {
                                            value: debt_sort,
                                            on_change: move |v: String| set.call(vec![("debtSort", CfgVal::Str(v))]),
                                            options: vec![
                                                opt("balance", "Largest balance"),
                                                opt("apr", "Highest rate"),
                                                opt("payoff", "Soonest payoff"),
                                                opt("name", "A–Z"),
                                            ],
                                        }
                                    }
                                    CfgRow {
                                        label: "Payoff strategy".to_string(),
                                        hint: "Which debt the overlay marks to target next".to_string(),
                                        CfgSeg {
                                            value: debt_strategy,
                                            on_change: move |v: String| set.call(vec![("debtStrategy", CfgVal::Str(v))]),
                                            options: vec![opt("avalanche", "Avalanche"), opt("snowball", "Snowball"), opt("none", "Off")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Projected trajectory".to_string(),
                                        CfgSwitch {
                                            value: debt_projection,
                                            on_change: move |v: bool| set.call(vec![("debtProjection", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Group by type".to_string(),
                                        CfgSwitch {
                                            value: debt_group,
                                            on_change: move |v: bool| set.call(vec![("debtGroup", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Highlight auto-detected".to_string(),
                                        CfgSwitch {
                                            value: debt_hl_auto,
                                            on_change: move |v: bool| set.call(vec![("debtHlAuto", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Show IOU ledger".to_string(),
                                        hint: "Personal money owed to / by people".to_string(),
                                        CfgSwitch {
                                            value: iou_show,
                                            on_change: move |v: bool| set.call(vec![("iouShow", CfgVal::Bool(v))]),
                                        }
                                    }
                                }

                                // ANALYTICS
                                CfgPanel {
                                    glyph: "⌁".to_string(),
                                    title: "Analytics".to_string(),
                                    span2: true,
                                    sub: "The retrospective read — spend trend, item-signals, category momentum and weekday rhythm.".to_string(),
                                    CfgRow {
                                        label: "Spend-trend window".to_string(),
                                        CfgSeg {
                                            value: trend_window,
                                            on_change: move |v: String| set.call(vec![("trendWindow", CfgVal::Str(v))]),
                                            options: vec![opt("12", "12 cyc"), opt("6", "6 cyc")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Trend series".to_string(),
                                        hint: "Plot spend, or the savings rate".to_string(),
                                        CfgSeg {
                                            value: trend_mode,
                                            on_change: move |v: String| set.call(vec![("trendMode", CfgVal::Str(v))]),
                                            options: vec![opt("spend", "Spend"), opt("rate", "Savings")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Item-signal sort".to_string(),
                                        CfgSelect {
                                            value: sig_sort,
                                            on_change: move |v: String| set.call(vec![("sigSort", CfgVal::Str(v))]),
                                            options: vec![opt("momentum", "Momentum"), opt("spend", "Spend"), opt("az", "A–Z")],
                                        }
                                    }
                                    CfgRow {
                                        label: "Show candidate signal".to_string(),
                                        hint: "Surface the AI's not-yet-tracked candidate".to_string(),
                                        CfgSwitch {
                                            value: show_cand,
                                            on_change: move |v: bool| set.call(vec![("showCand", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Category momentum section".to_string(),
                                        CfgSwitch {
                                            value: show_momentum,
                                            on_change: move |v: bool| set.call(vec![("showMomentum", CfgVal::Bool(v))]),
                                        }
                                    }
                                    CfgRow {
                                        label: "Momentum order".to_string(),
                                        CfgSelect {
                                            value: momentum_sort,
                                            on_change: move |v: String| set.call(vec![("momentumSort", CfgVal::Str(v))]),
                                            options: vec![
                                                opt("momentum", "Most movement"),
                                                opt("spend", "Largest spend"),
                                                opt("az", "A–Z"),
                                            ],
                                        }
                                    }
                                    CfgRow {
                                        label: "Weekday spending rhythm".to_string(),
                                        CfgSwitch {
                                            value: show_rhythm,
                                            on_change: move |v: bool| set.call(vec![("showRhythm", CfgVal::Bool(v))]),
                                        }
                                    }
                                }
                            }

                            // ---- footer ----
                            div { class: "cfg-foot",
                                span { class: "mk", "⌁" }
                                span { "Preferences sync to every surface on next visit" }
                                span { class: "rule" }
                                span { "PHOSK.CFG · LOCAL" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Parse the stored `phosk.cfg` JSON (flat object of string|bool values) into
/// known `CfgVal`s. A tiny tolerant parser — the store is always a flat object
/// produced by us, so we avoid pulling in a JSON dep on the client. Unknown /
/// malformed shapes yield an empty map (defaults stand), faithful to React's
/// `try/catch → {}`.
fn parse_stored(raw: &str) -> Vec<(&'static str, CfgVal)> {
    // Map each known key to its default's type so we coerce values correctly.
    let defaults = cfg_defaults();
    let mut out = Vec::new();
    for (key, def) in defaults.iter() {
        // Find `"key"` then the following `:` and value token.
        let needle = format!("\"{key}\"");
        let Some(kpos) = raw.find(&needle) else {
            continue;
        };
        let after = &raw[kpos + needle.len()..];
        let Some(colon) = after.find(':') else {
            continue;
        };
        let val_region = after[colon + 1..].trim_start();
        match def {
            CfgVal::Bool(_) => {
                if val_region.starts_with("true") {
                    out.push((*key, CfgVal::Bool(true)));
                } else if val_region.starts_with("false") {
                    out.push((*key, CfgVal::Bool(false)));
                }
            }
            CfgVal::Str(_) => {
                if let Some(rest) = val_region.strip_prefix('"') {
                    // Read until the next unescaped quote, unescaping basics.
                    let mut s = String::new();
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match chars.next() {
                                Some('n') => s.push('\n'),
                                Some('r') => s.push('\r'),
                                Some('t') => s.push('\t'),
                                Some('"') => s.push('"'),
                                Some('\\') => s.push('\\'),
                                Some('/') => s.push('/'),
                                Some(other) => s.push(other),
                                None => break,
                            },
                            other => s.push(other),
                        }
                    }
                    out.push((*key, CfgVal::Str(s)));
                }
            }
        }
    }
    out
}

/// ~1.4s delay for the "✓ Reset" flash, faithful to React's `setTimeout(…,1400)`.
/// Implemented with a platform-agnostic future so it works on web (wasm) and
/// desktop without pulling tokio into the client.
async fn gloo_or_sleep() {
    #[cfg(target_arch = "wasm32")]
    {
        let mut eval = document::eval(r#"setTimeout(function(){ dioxus.send(1); }, 1400);"#);
        let _ = eval.recv::<i32>().await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // On a native target a real sleep is fine; the eval bridge also works,
        // but this avoids depending on the JS runtime off-web.
        let mut eval = document::eval(r#"setTimeout(function(){ dioxus.send(1); }, 1400);"#);
        let _ = eval.recv::<i32>().await;
    }
}

#[cfg(test)]
mod core_prefs_tests {
    use super::*;

    #[test]
    fn is_numeric_accepts_only_digit_and_dot_values() {
        for v in ["3", "12", "0.7"] {
            assert!(is_numeric(v), "{v} is a number");
        }
        for v in ["", "CHF", "month", "off"] {
            assert!(!is_numeric(v), "{v:?} is text");
        }
    }

    #[test]
    fn a_write_targets_its_row_or_every_row() {
        let set = PrefWrite::Set("currency".to_string(), "CHF".to_string());
        assert_eq!(set.target(), "currency");
        assert_eq!(
            PrefWrite::Reset("telemetry".to_string()).target(),
            "telemetry"
        );
        assert_eq!(PrefWrite::ResetAll.target(), "*");
    }
}
