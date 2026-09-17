/* Phoskonomia — Analytics page data. Retrospective / cross-cycle series that the
   live dashboard doesn't carry: a 12-cycle spend history, per-category momentum
   over time, a weekday spending rhythm, and signal-mover rankings. Extends
   window.PHOSK in place. Item-signals themselves come from signals-data.jsx. */
(function () {
  const P = window.PHOSK;

  /* ---- 12-cycle history (oldest → current). Current cycle = JUN, projected. ----
     income is the household's monthly take-home; saved = income − spend
     (cashflow saving, distinct from the goal jar on the dashboard). */
  const income = 5100;
  const hist = [
    { m: "JUL", yr: "'25", spend: 3820 },
    { m: "AUG", yr: "'25", spend: 4015 },
    { m: "SEP", yr: "'25", spend: 3650 },
    { m: "OCT", yr: "'25", spend: 3990 },
    { m: "NOV", yr: "'25", spend: 4280 },
    { m: "DEC", yr: "'25", spend: 4610 },   // holiday peak — over budget
    { m: "JAN", yr: "'26", spend: 3470 },
    { m: "FEB", yr: "'26", spend: 3580 },
    { m: "MAR", yr: "'26", spend: 3910 },
    { m: "APR", yr: "'26", spend: 4050 },
    { m: "MAY", yr: "'26", spend: 3980 },
    { m: "JUN", yr: "'26", spend: 4120, projected: true },  // current, run-rate
  ];
  const budgetLine = 4200;
  hist.forEach((h) => {
    h.budget = budgetLine;
    h.saved = income - h.spend;
    h.rate = h.saved / income;
    h.over = h.spend > h.budget;
  });

  const spendHistory = hist;
  const histStats = (() => {
    const closed = hist.filter((h) => !h.projected);
    const avg = closed.reduce((s, h) => s + h.spend, 0) / closed.length;
    const peak = hist.reduce((a, h) => (h.spend > a.spend ? h : a), hist[0]);
    const low = closed.reduce((a, h) => (h.spend < a.spend ? h : a), closed[0]);
    const cur = hist[hist.length - 1];
    const prev = hist[hist.length - 2];
    const avgRate = closed.reduce((s, h) => s + h.rate, 0) / closed.length;
    const totalSaved = hist.reduce((s, h) => s + h.saved, 0);
    return {
      avg, peak, low, cur, prev, avgRate, income, totalSaved,
      curVsAvgPct: Math.round(((cur.spend - avg) / avg) * 100),
      curVsPrevPct: Math.round(((cur.spend - prev.spend) / prev.spend) * 100),
      months: hist.length,
    };
  })();

  /* ---- per-category momentum: a 12-cycle series anchored to the live spend,
     with a deterministic trend so some categories visibly rise and some fall.
     trend > 1 → growing into now; < 1 → cooling. ---- */
  const hash = (s) => { let h = 0; for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0; return h; };
  const TREND = {
    "GROCERIES": 1.04, "DINING & CAFÉS": 1.34, "HOUSING": 1.0, "HEALTH": 0.82,
    "TRANSPORT": 0.74, "SUBSCRIPTIONS": 1.12, "UTILITIES": 0.97, "HOUSEHOLD": 1.18,
    "LEISURE": 0.88, "CLOTHING": 0.42,
  };
  const catTrends = P.categories.map((c) => {
    const now = c.spent;
    const t = TREND[c.name] != null ? TREND[c.name] : 1.0;
    const h = hash(c.name);
    const n = 12;
    const series = [];
    for (let i = 0; i < n; i++) {
      const f = i / (n - 1);                    // 0 → 1 (now)
      // base ramps from now/t (12 mo ago) up to now, plus deterministic wobble
      const baseStart = t > 0 ? now / t : now;
      const lin = baseStart + (now - baseStart) * f;
      const wob = Math.sin(h * 0.013 + i * 1.7) * 0.10 + Math.cos(h * 0.007 + i) * 0.06;
      series.push(Math.max(0, lin * (1 + wob)));
    }
    series[n - 1] = now;                          // pin the last point to live data
    const prior = series.slice(n - 4, n - 1);
    const priorAvg = prior.reduce((s, v) => s + v, 0) / prior.length || 1;
    const deltaPct = Math.round(((now - priorAvg) / priorAvg) * 100);
    return { name: c.name, fixed: !!c.fixed, now, budget: c.budget, items: c.items, series, deltaPct };
  });

  /* ---- spending rhythm: average DISCRETIONARY spend by weekday (rent/insurance
     on the 1st excluded so the shape reads). The scanner reads your week. ---- */
  const weekday = [
    { d: "MON", v: 64 }, { d: "TUE", v: 48 }, { d: "WED", v: 72 },
    { d: "THU", v: 58 }, { d: "FRI", v: 118 }, { d: "SAT", v: 142 }, { d: "SUN", v: 39 },
  ];
  const weekdayStats = (() => {
    const peak = weekday.reduce((a, x) => (x.v > a.v ? x : a), weekday[0]);
    const max = Math.max(...weekday.map((x) => x.v));
    const total = weekday.reduce((s, x) => s + x.v, 0);
    const weekendShare = Math.round(((weekday[4].v + weekday[5].v + weekday[6].v) / total) * 100);
    return { peak, max, total, avg: total / 7, weekendShare };
  })();

  /* ---- signal movers: rank tracked item-signals by momentum ---- */
  P.signalMovers = () => {
    const arr = P.trackedSignals();
    const up = [...arr].sort((a, b) => (b.deltaPct || 0) - (a.deltaPct || 0));
    return { riser: up[0], faller: up[up.length - 1], all: up };
  };

  Object.assign(P, { spendHistory, histStats, catTrends, weekday, weekdayStats, analyticsIncome: income });
})();
