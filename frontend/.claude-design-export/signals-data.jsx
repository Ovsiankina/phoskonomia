/* Phoskonomia — item-level data: receipt line items, tracked ITEM-SIGNALS
   (micro-categories the AI maintains, e.g. Coffee / Pain au chocolat / Beer),
   and a loud AI activity feed. Extends window.PHOSK in place. */
(function () {
  const P = window.PHOSK;

  /* ---- ITEM-SIGNALS — distinct micro-categories tracked across all time ----
     These are NOT budget categories. Each is one product the AI isolates from
     receipts and follows on its own axis. 12-month series = units per month. */
  const signals = {
    coffee: {
      id: "coffee", label: "COFFEE", unit: "servings", parent: "DINING & CAFÉS",
      desc: "espresso · cappuccino · beans · to-go", glyph: "⌁",
      cycleQty: 23, lastQty: 18, cycleSpend: 86.40, lastSpend: 71.20, avgUnit: 3.76,
      deltaPct: +28, conf: 0.94, since: "OCT 2025", txns: 11,
      series: [9, 11, 8, 12, 10, 13, 12, 14, 15, 17, 19, 23],
      recent: [
        { date: "19 JUN", shop: "MIGROS", note: "Espresso Bohnen 500g", qty: 1, price: 8.90 },
        { date: "18 JUN", shop: "COOP PRONTO", note: "Kaffee to-go", qty: 1, price: 3.50 },
        { date: "18 JUN", shop: "STARBUCKS", note: "Cappuccino Grande", qty: 1, price: 7.20 },
        { date: "13 JUN", shop: "AVEC", note: "Kaffee to-go", qty: 1, price: 3.50 },
        { date: "14 JUN", shop: "COOP", note: "Café crème ×2", qty: 2, price: 4.20 },
      ],
    },
    pain: {
      id: "pain", label: "PAIN AU CHOCOLAT", unit: "pieces", parent: "GROCERIES",
      desc: "viennoiserie · boulangerie", glyph: "⌁",
      cycleQty: 14, lastQty: 12, cycleSpend: 17.60, lastSpend: 14.40, avgUnit: 1.26,
      deltaPct: +20, conf: 0.91, since: "NOV 2025", txns: 6,
      series: [6, 7, 5, 8, 7, 9, 8, 10, 9, 11, 12, 14],
      recent: [
        { date: "19 JUN", shop: "MIGROS", note: "Pain au chocolat ×4", qty: 4, price: 1.20 },
        { date: "18 JUN", shop: "COOP PRONTO", note: "Pain au chocolat ×2", qty: 2, price: 1.40 },
        { date: "11 JUN", shop: "VOLG", note: "Pain au chocolat ×2", qty: 2, price: 1.30 },
        { date: "14 JUN", shop: "COOP", note: "Pain au chocolat ×3", qty: 3, price: 1.25 },
      ],
    },
    beer: {
      id: "beer", label: "BEER", unit: "units", parent: "GROCERIES",
      desc: "lager · IPA · on-tap", glyph: "⌁",
      cycleQty: 19, lastQty: 22, cycleSpend: 58.10, lastSpend: 66.00, avgUnit: 3.06,
      deltaPct: -14, conf: 0.88, since: "SEP 2025", txns: 7,
      series: [24, 21, 26, 22, 25, 20, 23, 21, 24, 22, 22, 19],
      recent: [
        { date: "19 JUN", shop: "MIGROS", note: "Feldschlösschen 6×33cl", qty: 6, price: 1.60 },
        { date: "16 JUN", shop: "REST. KREUZ", note: "Stange ×2", qty: 2, price: 5.50 },
        { date: "17 JUN", shop: "DENNER", note: "Quöllfrisch 4×50cl", qty: 4, price: 1.95 },
      ],
    },
  };

  // a signal the AI is *proposing* to start tracking (loud AI, inline suggestion)
  const signalCandidate = {
    id: "gruyere", label: "GRUYÈRE AOP", unit: "portions", parent: "GROCERIES",
    desc: "seen 6× this cycle across 3 shops", glyph: "⌁",
    cycleQty: 6, conf: 0.83, deltaPct: null, candidate: true,
    series: [0, 0, 1, 2, 1, 3, 2, 4, 3, 5, 4, 6],
  };

  /* ---- line items per transaction (attached by index) ----
     each line: name, qty, unit price, category, signal id|null, confidence */
  const L = (n, q, p, cat, sig, conf) => ({ n, q, p, cat, sig: sig || null, conf: conf == null ? 0.97 : conf });
  const lines = {
    MIGROS: [
      L("Vollmilch 1L", 2, 1.60, "GROCERIES"),
      L("Pain au chocolat", 4, 1.20, "GROCERIES", "pain", 0.91),
      L("Gruyère AOP 220g", 1, 5.85, "GROCERIES"),
      L("Bananen 1.1kg", 1, 3.30, "GROCERIES"),
      L("Espresso Bohnen 500g", 1, 8.90, "GROCERIES", "coffee", 0.95),
      L("Feldschlösschen 6×33cl", 1, 9.60, "GROCERIES", "beer", 0.93),
      L("Rüebli 1kg", 1, 2.40, "GROCERIES"),
      L("Joghurt nature", 4, 0.95, "GROCERIES"),
      L("Eier 6er Bio", 1, 4.20, "GROCERIES"),
      L("Bas|er Läcker|i", 1, 4.50, "GROCERIES", null, 0.58),
      L("Spaghetti 500g", 1, 1.80, "GROCERIES"),
      L("Tomaten 400g", 1, 1.50, "GROCERIES"),
    ],
    "COOP PRONTO": [
      L("Pain au chocolat", 2, 1.40, "GROCERIES", "pain", 0.74),
      L("Kaffee to-go", 1, 3.50, "DINING & CAFÉS", "coffee", 0.66),
      L("Sandwich Poulet", 1, 3.90, "DINING & CAFÉS", null, 0.61),
      L("R3d Bu|| 25cl", 1, 2.20, "GROCERIES", null, 0.49),
    ],
    STARBUCKS: [
      L("Cappuccino Grande", 1, 7.20, "DINING & CAFÉS", "coffee", 0.96),
    ],
    "RESTAURANT KREUZ": [
      L("Cordon bleu", 1, 29.00, "DINING & CAFÉS"),
      L("Rösti", 1, 24.50, "DINING & CAFÉS"),
      L("Stange (Bier)", 2, 5.50, "DINING & CAFÉS", "beer", 0.9),
    ],
    DENNER: [
      L("Quöllfrisch 4×50cl", 1, 7.80, "GROCERIES", "beer", 0.92),
      L("Pouletbrust 500g", 1, 9.20, "GROCERIES"),
      L("Reis 1kg", 1, 3.40, "GROCERIES"),
      L("Zwiebeln 1kg", 1, 2.20, "GROCERIES"),
      L("Apfelsaft 1L", 1, 1.95, "GROCERIES"),
      L("Schokolade 100g", 3, 1.50, "GROCERIES"),
      L("Pa|n au choco|at", 2, 1.25, "GROCERIES", "pain", 0.55),
    ],
    COOP: [
      L("Café crème", 2, 4.20, "DINING & CAFÉS", "coffee", 0.9),
      L("Pain au chocolat", 3, 1.25, "GROCERIES", "pain", 0.93),
      L("Rindshackfleisch 400g", 1, 8.80, "GROCERIES"),
      L("Mozzarella ×2", 2, 1.95, "GROCERIES"),
      L("Salatgurke", 1, 1.40, "GROCERIES"),
      L("Brot Ruchmehl", 1, 2.60, "GROCERIES"),
      L("Butter 200g", 1, 2.90, "GROCERIES"),
      L("Orangen 2kg", 1, 4.95, "GROCERIES"),
      L("Quöllfrisch 6×33cl", 1, 9.60, "GROCERIES", "beer", 0.89),
      L("Waschmittel", 1, 12.90, "HOUSEHOLD"),
    ],
    AVEC: [
      L("Kaffee to-go", 1, 3.50, "DINING & CAFÉS", "coffee", 0.84),
      L("Gipfeli", 1, 2.30, "GROCERIES"),
      L("Mineralwasser 50cl", 1, 1.50, "GROCERIES"),
    ],
    VOLG: [
      L("Pain au chocolat", 2, 1.30, "GROCERIES", "pain", 0.88),
      L("Vollmilch 1L", 1, 1.65, "GROCERIES"),
      L("Käse Stück", 1, 4.80, "GROCERIES"),
      L("Tomaten 500g", 1, 2.10, "GROCERIES"),
      L("Brot", 1, 2.40, "GROCERIES"),
      L("Bananen", 1, 2.95, "GROCERIES"),
    ],
    SBB: [L("Billett · Zone 10", 1, 6.80, "TRANSPORT")],
    SWISSCOM: [L("Mobile Abo · inOne", 1, 79.00, "SUBSCRIPTIONS", null, 0.99)],
    GALAXUS: [
      L("Tischlampe LED", 1, 89.00, "HOUSEHOLD"),
      L("Mehrfachstecker", 1, 40.90, "HOUSEHOLD"),
    ],
    APOTHEKE: [
      L("Da|acin Ge|", 1, 18.40, "HEALTH", null, 0.52),
      L("Vitamin D3", 1, 10.00, "HEALTH", null, 0.63),
    ],
    MANOR: [L("Hemd Slim Fit", 1, 41.20, "CLOTHING")],
  };

  // attach lines to each transaction by shop name; give every txn a stable id
  P.transactions.forEach((t, i) => {
    t.id = "t" + i;
    t.lines = lines[t.shop] || [L(t.shop + " · total", 1, t.amount, t.cat)];
    // does this receipt touch a tracked signal?
    t.sigs = [...new Set(t.lines.filter(l => l.sig).map(l => l.sig))];
    t.lowConf = t.lines.filter(l => l.conf < 0.7).length;
  });

  /* ---- loud AI activity feed — reprocessing, auto-categorize, suggestions ---- */
  const aiFeed = [
    { kind: "categorize", text: "Auto-filed “Pain au chocolat ×4” → signal PAIN AU CHOCOLAT", conf: 0.91, sig: "pain", time: "now" },
    { kind: "reprocess", text: "Re-reading 2 low-confidence items on COOP PRONTO", state: "running", time: "now" },
    { kind: "suggest", text: "New item-signal candidate: GRUYÈRE AOP — seen 6× this cycle. Track it?", actions: ["TRACK", "DISMISS"], cand: true, time: "2m" },
    { kind: "detect", text: "BEER down 14% vs last cycle — 19 vs 22 units", conf: 0.88, sig: "beer", time: "6m" },
    { kind: "categorize", text: "Linked “Café crème ×2” → signal COFFEE (COOP, 14 Jun)", conf: 0.90, sig: "coffee", time: "1h" },
    { kind: "detect", text: "COFFEE up 28% — pace to beat your 6-month high", conf: 0.94, sig: "coffee", time: "3h" },
  ];

  Object.assign(P, { signals, signalCandidate, aiFeed });
  P.signalById = (id) => P.signals[id] || (P.signalCandidate.id === id ? P.signalCandidate : null);
  P.trackedSignals = () => Object.values(P.signals);
})();
