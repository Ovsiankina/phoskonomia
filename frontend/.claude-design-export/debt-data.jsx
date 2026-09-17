/* Phoskonomia — Debts data. Outstanding balances read as a DECAYING WAVEFORM:
   each debt is a balance damping toward a zero baseline as it's paid down. The
   hero plots the combined balance envelope from history through today to the
   projected debt-free point. Extends window.PHOSK with `debts` + amortization
   helpers. Tuned to the June 2026 narrative: the Cornèrcard revolving balance is
   the high-interest leak GEMMA4 flags for the avalanche. */
(function () {
  const P = window.PHOSK;

  // hist = last 6 monthly balances, OLDEST → newest (last = current outstanding).
  // orig = the opening/peak balance (progress baseline). day = payment day.
  // apr is the nominal annual rate (decimal). monthly = the scheduled payment.
  const DEBTS = [
    { id: "leasing", name: "AUTO LEASING", lender: "AMAG · MULTILEASE", glyph: "⊙", type: "LEASE",
      orig: 32000, balance: 18600, apr: 0.039, monthly: 540, day: 1, term: 60, src: "user", status: "ok",
      since: "JUL 2023", hist: [21840, 21300, 20760, 20040, 19320, 18600],
      note: "60-month vehicle lease. Charged on the 1st, on schedule." },
    { id: "kredit", name: "PRIVATKREDIT", lender: "BANK CLER", glyph: "P", type: "LOAN",
      orig: 15000, balance: 8200, apr: 0.079, monthly: 410, day: 3, term: 48, src: "user", status: "ok",
      since: "MAR 2024", hist: [10100, 9740, 9360, 8970, 8580, 8200],
      note: "Personal loan, fixed 48-month term. Amortizing on schedule." },
    { id: "card", name: "CORNÈRCARD", lender: "REVOLVING CREDIT", glyph: "C", type: "CARD",
      orig: 6200, balance: 4350, apr: 0.149, monthly: 220, day: 25, term: null, src: "llm", status: "high",
      since: "2021", hist: [3980, 4120, 4010, 4280, 4180, 4350],
      note: "Revolving balance at 14.9% — by far the costliest franc you carry." },
    { id: "steuern", name: "STEUERN 2024", lender: "STEUERAMT ZÜRICH", glyph: "§", type: "TAX",
      orig: 7200, balance: 5400, apr: 0.04, monthly: 600, day: 28, term: 12, src: "user", status: "ok",
      since: "FEB 2026", hist: [7200, 6900, 6600, 6300, 5700, 5400],
      note: "Back-tax instalment plan with 4% Verzugszins. Five payments left." },
    { id: "klarna", name: "KLARNA", lender: "3 OPEN PLANS · BNPL", glyph: "K", type: "BNPL",
      orig: 980, balance: 540, apr: 0.0, monthly: 135, day: 15, term: null, src: "llm", status: "watch",
      since: "APR 2026", hist: [0, 0, 980, 845, 690, 540],
      note: "Three buy-now-pay-later plans stacking up. 0% — but easy to lose track of." },
    { id: "zahnarzt", name: "ZAHNARZT", lender: "DENTAL PAYMENT PLAN", glyph: "Z", type: "MEDICAL",
      orig: 2400, balance: 1280, apr: 0.0, monthly: 160, day: 8, term: 15, src: "user", status: "due",
      since: "NOV 2025", hist: [2080, 1920, 1760, 1600, 1440, 1280],
      note: "Interest-free dental plan. Next instalment due in a few days." },
  ];

  const monthlyRate = (d) => d.apr / 12;

  // Months until paid off given current balance + scheduled monthly payment.
  // Iterative amortization; capped so a barely-covering payment doesn't loop.
  const monthsToPayoff = (d) => {
    let b = d.balance, r = monthlyRate(d), n = 0;
    if (d.monthly <= b * r) return 600; // payment doesn't cover interest
    while (b > 0.5 && n < 600) { b = b + b * r - d.monthly; n++; }
    return n;
  };

  // Forward balance series, month by month, until zero (clamped at 0).
  const forwardSeries = (d, maxN) => {
    let b = d.balance, r = monthlyRate(d);
    const out = [b];
    for (let i = 0; i < maxN; i++) {
      if (b <= 0) { out.push(0); continue; }
      b = Math.max(0, b + b * r - d.monthly);
      out.push(b);
    }
    return out;
  };

  // Total remaining interest over the life of a debt (sum of interest charges).
  const interestRemaining = (d) => {
    let b = d.balance, r = monthlyRate(d), total = 0, n = 0;
    if (r === 0) return 0;
    if (d.monthly <= b * r) return Infinity;
    while (b > 0.5 && n < 600) { const int = b * r; total += int; b = b + int - d.monthly; n++; }
    return total;
  };

  const paidOffPct = (d) => Math.max(0, Math.min(1, 1 - d.balance / d.orig));
  const annualInterest = (d) => d.balance * d.apr; // run-rate interest cost / yr

  // month label N months ahead of the current cycle (JUN 2026 = month 0)
  const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
  const monthLabel = (offset) => {
    // current cycle is JUN 2026 → index 5
    let m = 5 + offset, y = 2026;
    while (m > 11) { m -= 12; y++; }
    while (m < 0) { m += 12; y--; }
    return MONTHS[m] + " " + (y % 100 < 10 ? "0" : "") + (y % 100);
  };

  // ---- roll-ups ----
  const totalOwed = DEBTS.reduce((t, d) => t + d.balance, 0);
  const totalOrig = DEBTS.reduce((t, d) => t + d.orig, 0);
  const totalMonthly = DEBTS.reduce((t, d) => t + d.monthly, 0);
  const totalInterestYr = DEBTS.reduce((t, d) => t + annualInterest(d), 0);
  const weightedApr = DEBTS.reduce((t, d) => t + d.apr * d.balance, 0) / totalOwed;
  const autoCount = DEBTS.filter((d) => d.src === "llm").length;
  const paidOffTotalPct = Math.max(0, Math.min(1, 1 - totalOwed / totalOrig));

  // global debt-free horizon = the longest single payoff
  const horizon = Math.max(...DEBTS.map(monthsToPayoff));
  const debtFreeLabel = monthLabel(horizon);

  // combined balance trajectory: 5 months history + today + projection to zero.
  // returns [{m, total}] where m is month-offset (negative = past).
  const trajectory = (() => {
    const out = [];
    // history (oldest 5 → today). hist has 6 entries; index 5 = current = offset 0.
    for (let h = 0; h <= 5; h++) {
      const off = h - 5; // -5 … 0
      const total = DEBTS.reduce((t, d) => t + (d.hist[h] ?? d.hist[d.hist.length - 1]), 0);
      out.push({ m: off, total });
    }
    // forward projection
    const fwd = DEBTS.map((d) => forwardSeries(d, horizon));
    for (let i = 1; i <= horizon; i++) {
      const total = DEBTS.reduce((t, d, k) => t + (fwd[k][i] ?? 0), 0);
      out.push({ m: i, total });
    }
    return out;
  })();

  // next due (forward) — soonest payment by day-of-month from cycle day
  const daysUntil = (d) => {
    const today = P.cycle.day, len = P.cycle.days;
    return d.day > today ? d.day - today : (len - today) + d.day;
  };
  const nextLabel = (d) => {
    const du = daysUntil(d);
    const nm = d.day > P.cycle.day ? "JUN" : "JUL";
    return d.day + " " + nm;
  };

  // attention flags (high interest / due soon / watch)
  const flagged = DEBTS.filter((d) => d.status !== "ok");
  // the avalanche target = highest-APR debt with a balance
  const avalancheTarget = [...DEBTS].sort((a, b) => b.apr - a.apr)[0];
  const snowballTarget = [...DEBTS].sort((a, b) => a.balance - b.balance)[0];

  Object.assign(P, {
    debts: DEBTS,
    debtMonthlyRate: monthlyRate,
    debtMonthsToPayoff: monthsToPayoff,
    debtForwardSeries: forwardSeries,
    debtInterestRemaining: interestRemaining,
    debtPaidOffPct: paidOffPct,
    debtAnnualInterest: annualInterest,
    debtDaysUntil: daysUntil,
    debtNextLabel: nextLabel,
    debtMonthLabel: monthLabel,
    debtTrajectory: trajectory,
    debtStats: {
      totalOwed, totalOrig, totalMonthly, totalInterestYr, weightedApr,
      count: DEBTS.length, autoCount, paidOffTotalPct,
      horizon, debtFreeLabel, flagged, avalancheTarget, snowballTarget,
    },
  });
  P.debtById = (id) => DEBTS.find((d) => d.id === id) || null;

  // status → tone/label used across the page
  P.debtStatus = (d) => {
    if (d.status === "high")  return { key: "high",  label: "HIGH INTEREST", tone: "coral" };
    if (d.status === "due")   return { key: "due",   label: "DUE SOON",      tone: "warn" };
    if (d.status === "watch") return { key: "watch", label: "REVIEW",        tone: "warn" };
    return { key: "ok", label: "ON TRACK", tone: "blue" };
  };

  /* ======================== PERSONAL · IOU LEDGER ==========================
     Money between you and people you know — NOT real debt. No interest, no
     amortization, no schedule. A two-sided informal ledger: `dir:"in"` = they
     owe you (a credit in your favour); `dir:"out"` = you owe them (a liability).
     `of` carries a partial-settlement original so a part-repaid IOU shows
     progress. Deliberately kept separate from the institutional DEBTS above. */
  const PERSONAL = [
    { id: "papa",   person: "PAPA",     initials: "Pa", dir: "in",  amount: 800, since: "MAR 2026", reason: "Bridge loan toward the flat deposit." },
    { id: "marco",  person: "MARCO B.", initials: "MB", dir: "in",  amount: 340, since: "MAY 2026", reason: "Laax ski cabin — fronted the whole weekend." },
    { id: "nadia",  person: "NADIA",    initials: "Na", dir: "in",  amount: 200, of: 500, since: "FEB 2026", reason: "Used camera — paying you back monthly." },
    { id: "jonas",  person: "JONAS",    initials: "Jo", dir: "in",  amount: 75,  since: "JUN 2026", reason: "Openair festival ticket." },
    { id: "lena",   person: "LENA K.",  initials: "LK", dir: "out", amount: 120, since: "JUN 2026", reason: "Hallenstadion concert — she booked both." },
    { id: "sophie", person: "SOPHIE",   initials: "So", dir: "out", amount: 45,  since: "JUN 2026", reason: "Dinner at Kreuz, split unevenly." },
  ];

  const owedToYou = PERSONAL.filter((p) => p.dir === "in").reduce((t, p) => t + p.amount, 0);
  const youOwe = PERSONAL.filter((p) => p.dir === "out").reduce((t, p) => t + p.amount, 0);

  Object.assign(P, {
    personal: PERSONAL,
    personalStats: {
      owedToYou, youOwe, net: owedToYou - youOwe,
      countIn: PERSONAL.filter((p) => p.dir === "in").length,
      countOut: PERSONAL.filter((p) => p.dir === "out").length,
      count: PERSONAL.length,
      maxSingle: Math.max(...PERSONAL.map((p) => p.amount)),
    },
  });
  P.personalById = (id) => PERSONAL.find((p) => p.id === id) || null;
})();
