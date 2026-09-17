/* Phoskonomia — Subscriptions data. Standing/recurring charges read as a
   periodic IMPULSE TRAIN on the scope: each subscription fires on a fixed day
   of the cycle. Extends window.PHOSK with `subscriptions` + recurrence helpers.
   Tuned to the rest of the app's June 2026 narrative (Activ Fitness not seen,
   Sunrise due soon, subscriptions envelope at 99%). */
(function () {
  const P = window.PHOSK;

  // hist = last 6 monthly charges, OLDEST → newest (price creep shows as steps).
  // day = charge day of month (impulse position). cat ties to a budget envelope.
  const SUBS = [
    { id: "zvv",     name: "ZVV ABO",        glyph: "Z", cat: "TRANSPORT",     cadence: "monthly", amount: 85.00, day: 1,  src: "user", status: "ok",
      since: "JAN 2022", hist: [85, 85, 85, 85, 85, 85], note: "Zürich transit pass. Charged on the 1st." },
    { id: "nzz",     name: "NZZ DIGITAL",    glyph: "N", cat: "LEISURE",       cadence: "monthly", amount: 39.00, day: 1,  src: "user", status: "ok",
      since: "SEP 2023", hist: [29, 29, 39, 39, 39, 39], note: "News subscription. Intro rate ended in Apr." },
    { id: "icloud",  name: "ICLOUD+",        glyph: "i", cat: "SUBSCRIPTIONS", cadence: "monthly", amount: 2.95,  day: 3,  src: "llm",  status: "ok",
      since: "2019",     hist: [2.95, 2.95, 2.95, 2.95, 2.95, 2.95], note: "200 GB storage tier." },
    { id: "spotify", name: "SPOTIFY",        glyph: "S", cat: "SUBSCRIPTIONS", cadence: "monthly", amount: 12.95, day: 5,  src: "llm",  status: "ok",
      since: "MAR 2018", hist: [11.95, 11.95, 12.95, 12.95, 12.95, 12.95], note: "Premium · price rose CHF 1 in Apr." },
    { id: "activ",   name: "ACTIV FITNESS",  glyph: "A", cat: "HEALTH",        cadence: "monthly", amount: 89.00, day: 5,  src: "llm",  status: "due",
      since: "FEB 2024", hist: [89, 89, 89, 89, 89, 89], note: "Gym. Usual CHF 89 on the 5th — NOT seen this cycle." },
    { id: "netflix", name: "NETFLIX",        glyph: "Nf",cat: "SUBSCRIPTIONS", cadence: "monthly", amount: 24.90, day: 8,  src: "llm",  status: "ok",
      since: "2016",     hist: [21.90, 21.90, 21.90, 24.90, 24.90, 24.90], note: "Standard plan · raised CHF 3 in Mar." },
    { id: "disney",  name: "DISNEY+",        glyph: "D", cat: "SUBSCRIPTIONS", cadence: "monthly", amount: 12.90, day: 12, src: "llm",  status: "watch",
      since: "NOV 2021", hist: [9.90, 9.90, 12.90, 12.90, 12.90, 12.90], note: "Streaming · 0 watch hours logged in 38 days." },
    { id: "swisscom",name: "SWISSCOM",       glyph: "Sw",cat: "UTILITIES",     cadence: "monthly", amount: 79.00, day: 17, src: "llm",  status: "ok",
      since: "2015",     hist: [79, 79, 79, 79, 79, 79], note: "Internet + blue TV. Charged on the 17th." },
    { id: "sunrise", name: "SUNRISE",        glyph: "Su",cat: "UTILITIES",     cadence: "monthly", amount: 45.00, day: 22, src: "llm",  status: "soon",
      since: "2015",     hist: [45, 45, 45, 45, 45, 45], note: "Mobile plan. Next charge in 3 days." },
    { id: "linkedin",name: "LINKEDIN",       glyph: "L", cat: "LEISURE",       cadence: "monthly", amount: 39.90, day: 28, src: "llm",  status: "watch",
      since: "APR 2025", hist: [39.90, 39.90, 39.90, 39.90, 39.90, 39.90], note: "Career Premium · unused 61 days. Cancel?" },
    { id: "serafe",  name: "SERAFE",         glyph: "R", cat: "UTILITIES",     cadence: "yearly",  amount: 335.00, day: null, month: "MAR", src: "user", status: "ok",
      since: "2019",     hist: [320, 320, 335], note: "TV/radio licence. Billed once a year in March." },
  ];

  const monthlyEquiv = (s) => s.cadence === "yearly" ? s.amount / 12 : s.amount;
  const annual = (s) => s.cadence === "yearly" ? s.amount : s.amount * 12;

  // days until the NEXT charge from today (cycle day P.cycle.day), monthly only.
  const daysUntil = (s) => {
    if (s.cadence !== "monthly") return null;
    const today = P.cycle.day, len = P.cycle.days;
    return s.day > today ? s.day - today : (len - today) + s.day;
  };
  // has this monthly sub already fired in the current cycle?
  const firedThisCycle = (s) => s.cadence === "monthly" && s.day <= P.cycle.day;

  // next charge label
  const nextLabel = (s) => {
    if (s.cadence === "yearly") return s.month + " 27";
    if (s.status === "due") return "—";
    const d = daysUntil(s);
    const nextMonth = s.day > P.cycle.day ? "JUN" : "JUL";
    return s.day + " " + nextMonth;
  };

  // recent charges (newest first) derived from hist on the charge day.
  const MONTHS = ["JUN", "MAY", "APR", "MAR", "FEB", "JAN"];
  const recentCharges = (s) => {
    if (s.cadence === "yearly") {
      return [{ date: "MAR " + s.month, amount: s.hist[s.hist.length - 1] },
              { date: "MAR 2025", amount: s.hist[s.hist.length - 2] },
              { date: "MAR 2024", amount: s.hist[s.hist.length - 3] }].filter(r => r.amount != null);
    }
    const hv = [...s.hist].reverse(); // newest-first
    const out = [];
    let mi = 0;
    // skip JUN if it hasn't fired yet (or was missed/due)
    if (!firedThisCycle(s) || s.status === "due") mi = 1;
    for (let k = 0; mi < MONTHS.length && k < 4; mi++, k++) {
      out.push({ date: s.day + " " + MONTHS[mi], amount: hv[mi] ?? hv[hv.length - 1] });
    }
    return out;
  };

  // ---- roll-ups ----
  const active = SUBS;
  const subsMonthly = active.reduce((t, s) => t + monthlyEquiv(s), 0);
  const subsAnnual = active.reduce((t, s) => t + annual(s), 0);
  const autoCount = active.filter(s => s.src === "llm").length;

  // charged so far this cycle vs still upcoming this cycle (monthly only)
  const chargedThisCycle = active.filter(s => firedThisCycle(s) && s.status !== "due")
    .reduce((t, s) => t + s.amount, 0);
  const upcomingThisCycle = active.filter(s => s.cadence === "monthly" && s.day > P.cycle.day)
    .reduce((t, s) => t + s.amount, 0);

  // next 30 days of charges (forward from today), monthly only — sorted by daysUntil
  const next30 = active.filter(s => s.cadence === "monthly")
    .map(s => ({ s, d: daysUntil(s) }))
    .filter(x => x.d <= 30)
    .sort((a, b) => a.d - b.d);
  const next30Total = next30.reduce((t, x) => t + x.s.amount, 0);

  // attention flags (due / soon / watch)
  const flagged = active.filter(s => s.status === "due" || s.status === "watch");

  Object.assign(P, {
    subscriptions: SUBS,
    subMonthlyEquiv: monthlyEquiv,
    subAnnual: annual,
    subDaysUntil: daysUntil,
    subFired: firedThisCycle,
    subNextLabel: nextLabel,
    subRecent: recentCharges,
    subStats: {
      monthly: subsMonthly, annual: subsAnnual, count: active.length, autoCount,
      chargedThisCycle, upcomingThisCycle, next30, next30Total, flagged,
    },
  });
  P.subById = (id) => SUBS.find(s => s.id === id) || null;

  // status → tone/label used across the page
  P.subStatus = (s) => {
    if (s.status === "due")   return { key: "due",   label: "NOT SEEN", tone: "coral" };
    if (s.status === "soon")  return { key: "soon",  label: "DUE SOON", tone: "warn" };
    if (s.status === "watch") return { key: "watch", label: "REVIEW",   tone: "warn" };
    return { key: "ok", label: "ACTIVE", tone: "blue" };
  };
})();
