/* Phoskonomia — mock data. Swiss CHF, Swiss-German shops. June 2026 cycle.
   Numbers are heroes (Oscillocore). Money formatted Swiss-style: CHF 1'234.50 */

const PHOSK = (function () {
  // ---- Swiss currency format: apostrophe thousands, dot decimal ----
  function chf(n, dp = 2) {
    const neg = n < 0;
    const s = Math.abs(n).toFixed(dp);
    const [int, dec] = s.split(".");
    const grouped = int.replace(/\B(?=(\d{3})+(?!\d))/g, "\u2019"); // ’
    return (neg ? "\u2212" : "") + grouped + (dec ? "." + dec : "");
  }

  const cycle = { label: "JUN 2026", day: 19, days: 30, asOf: "19 JUN" };

  const totals = {
    budget: 4200,
    spent: 2614.4,
    get remaining() { return this.budget - this.spent; },
    savingsTarget: 900,
    saved: 560,
    savingsProjected: 740,
    savingsRate: 0.18,
    lastCycleSpent: 3980.2,
  };

  // 19 days of daily spend (CHF). Big spikes = rent+insurance on the 1st.
  const daily = [
    1998.00, 64.50, 28.40, 41.20, 132.95, 12.30, 88.25, 47.10, 9.80, 64.50,
    18.40, 129.90, 33.10, 79.00, 7.20, 64.50, 12.40, 53.85, 28.10,
  ];
  // cumulative
  const cumulative = (() => { let a = 0; return daily.map(d => (a += d)); })();
  // straight "budget pace" target across the month
  const pace = Array.from({ length: cycle.days }, (_, i) => (totals.budget / (cycle.days - 1)) * i);

  const categories = [
    { name: "GROCERIES",        budget: 800, spent: 612.30, items: 41 },
    { name: "DINING & CAFÉS",   budget: 350, spent: 410.20, items: 18 },
    { name: "HOUSING",          budget: 1680, spent: 1680.00, items: 1, fixed: true },
    { name: "HEALTH",           budget: 520, spent: 346.40, items: 6 },
    { name: "TRANSPORT",        budget: 280, spent: 168.00, items: 12 },
    { name: "SUBSCRIPTIONS",    budget: 190, spent: 188.82, items: 7 },
    { name: "UTILITIES",        budget: 240, spent: 158.00, items: 3 },
    { name: "HOUSEHOLD",        budget: 200, spent: 226.30, items: 5 },
    { name: "LEISURE",          budget: 300, spent: 144.50, items: 9 },
    { name: "CLOTHING",         budget: 150, spent: 0.00, items: 0 },
  ];

  const transactions = [
    { date: "19 JUN", shop: "MIGROS",          amount: 53.85,  cat: "GROCERIES",     items: 14 },
    { date: "19 JUN", shop: "SBB",             amount: 6.80,   cat: "TRANSPORT",     items: 1 },
    { date: "18 JUN", shop: "COOP PRONTO",     amount: 12.40,  cat: "GROCERIES",     items: 3, flag: true },
    { date: "18 JUN", shop: "STARBUCKS",       amount: 7.20,   cat: "DINING & CAFÉS",items: 1 },
    { date: "17 JUN", shop: "SWISSCOM",        amount: 79.00,  cat: "SUBSCRIPTIONS", items: 1, fixed: true },
    { date: "17 JUN", shop: "DENNER",          amount: 33.10,  cat: "GROCERIES",     items: 9 },
    { date: "16 JUN", shop: "GALAXUS",         amount: 129.90, cat: "HOUSEHOLD",     items: 2 },
    { date: "16 JUN", shop: "RESTAURANT KREUZ",amount: 64.50,  cat: "DINING & CAFÉS",items: 3 },
    { date: "15 JUN", shop: "APOTHEKE",        amount: 28.40,  cat: "HEALTH",        items: 2, flag: true },
    { date: "14 JUN", shop: "COOP",            amount: 88.25,  cat: "GROCERIES",     items: 22 },
    { date: "13 JUN", shop: "AVEC",            amount: 9.80,   cat: "GROCERIES",     items: 2 },
    { date: "12 JUN", shop: "MANOR",           amount: 41.20,  cat: "CLOTHING",      items: 1 },
    { date: "11 JUN", shop: "VOLG",            amount: 18.40,  cat: "GROCERIES",     items: 6 },
  ];

  // Recurring — confirmed clean, with next-due tracking.
  const recurring = [
    { name: "MIETE",          amount: 1680.00, cycle: "Monthly · 1st",  next: "1 JUL",  status: "ok",   src: "user" },
    { name: "KRANKENKASSE",   amount: 318.00,  cycle: "Monthly · 1st",  next: "1 JUL",  status: "ok",   src: "user" },
    { name: "SWISSCOM",       amount: 79.00,   cycle: "Monthly · 17th", next: "17 JUL", status: "ok",   src: "llm" },
    { name: "ACTIV FITNESS",  amount: 89.00,   cycle: "Monthly · 5th",  next: "—",      status: "due",  src: "llm" },
    { name: "NETFLIX",        amount: 24.90,   cycle: "Monthly · 8th",  next: "8 JUL",  status: "ok",   src: "llm" },
    { name: "SPOTIFY",        amount: 12.95,   cycle: "Monthly · 5th",  next: "5 JUL",  status: "ok",   src: "llm" },
    { name: "SERAFE TV",      amount: 27.92,   cycle: "Yearly · Mar",   next: "MAR 27", status: "ok",   src: "user" },
    { name: "SUNRISE",        amount: 45.00,   cycle: "Monthly · 22nd", next: "22 JUN", status: "soon", src: "llm" },
  ];

  // Alerts — finance language, NEVER "anomaly". tone: alert|warn|info|llm
  const alerts = [
    { tone: "alert", tag: "DINING & CAFÉS", head: "17% over budget",
      body: "CHF 60.20 over the CHF 350 cap with 11 days left in the cycle.",
      actions: ["RAISE CAP", "DISMISS"] },
    { tone: "warn", tag: "GROCERIES", head: "On pace to exceed",
      body: "Projected CHF 870 by 30 Jun — about CHF 70 above budget at the current rate.",
      actions: ["VIEW", "DISMISS"] },
    { tone: "llm", tag: "ACTIV FITNESS", head: "Recurring charge not seen",
      body: "Usually charged CHF 89.00 on the 5th. Not recorded this cycle — paused, or a missing receipt?",
      actions: ["MARK PAID", "SNOOZE"] },
    { tone: "llm", tag: "TRANSPORT", head: "Suggested budget cut",
      body: "Under budget 3 cycles running. Lower the cap from CHF 280 to CHF 220 and move CHF 60 to savings?",
      actions: ["APPLY", "DISMISS"] },
    { tone: "warn", tag: "SUBSCRIPTIONS", head: "99% used",
      body: "CHF 188.82 of CHF 190. Next charge (Sunrise, CHF 45) will push this over.",
      actions: ["RAISE CAP", "DISMISS"] },
  ];

  const account = { holder: "L. OVSIANKINA", iban: "CH93 ⋯ 8412", model: "GEMMA4", engine: "OLLAMA" };

  return { chf, cycle, totals, daily, cumulative, pace, categories, transactions, recurring, alerts, account };
})();

window.PHOSK = PHOSK;
