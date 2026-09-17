# Phoskonomia — Frontend API Contract (`phosk_api`)

**Real, frontend-only HTTP API.** Base URL `http://127.0.0.1:3819/api/v1`
(override with `PHOSK_API_ADDR`). JSON in/out. Single-tenant, local-first,
**loopback-bound** (ADR-007). Run: `cargo run -p phosk_api`.

**Status:** every route below is wired for real; handlers return
`501 {"error":"not_implemented","todo":"<context>: <feature>"}` until the
backend behind them is built. Live so far: `/health`, plus the three dashboard
read-model endpoints `/cycle/current` `/cycle/current/totals`
`/cycle/current/spend-series` `/cycle/current/top-shops` (served from the
`phosk_insights` gateway over the in-memory Swiss seed; money as CHF numbers).
`/insights/dashboard` stays `501` until the LLM spine lands. Backend work to
turn each remaining `501` green is tracked in
`backend/documentation/backend-features-todo.md`.

**Conventions**
- References are **human terms**, never UUIDs: category NAME (`GROCERIES`),
  shop NAME (`MIGROS`), stable slug ids (`coffee`, `card`, `spotify`, `t0`).
- Common query params on read endpoints: `period=day|week|month|quarter|year|custom`
  (+ `from`/`to` for custom), `category`, `shop`, `sort`, `order`, `limit`, `offset`.
- CORS is permissive in dev (frontend is a separate origin, e.g. `localhost:3717`);
  TODO: restrict to the frontend origin + add the local auth secret.

---

## Health
| Method | Path | Notes |
|---|---|---|
| GET | `/health` | **Live.** `{status, service}`. |

## Dashboard / cycle
| Method | Path | Purpose |
|---|---|---|
| GET | `/cycle/current` | Active cycle: `label, day, days, asOf, startDate, endDate, daysLeft`. |
| GET | `/cycle/current/totals` | KPIs: `budget, spent, remaining, savingsTarget, saved, savingsProjected, savingsRate, lastCycleSpent, spentPct, vsLastCyclePct, perDayToStayOnBudget`. |
| GET | `/cycle/current/spend-series` | `daily[], cumulative[], pace[]`, `compare=lastCycle` → `lastCycleCumulative[]`, `todayIndex`. |
| GET | `/cycle/current/top-shops` | `shops[]{shop, total, share}`, `maxTotal`. |
| GET | `/insights/dashboard` | GEMMA4 narrative: `{text, model, engine, category, estimatedSavings}`. |

## Transactions
| Method | Path | Purpose |
|---|---|---|
| GET | `/transactions` | List `{id,date,shop,category,amount,item_count,fixed,flag,signal_ids[],low_conf_count}` + `summary` + `available_shops/categories`. Filters: `period,from,to,shop,category,signal,flagged,q,sort,order,limit,offset`. |
| POST | `/transactions` | Create (manual or `photo` multipart → OCR+LLM pipeline). |
| GET | `/transactions/{id}` | Detail + `lines[]{name,qty,unit_price,line_total,category,signal_id,confidence}`, `avg_confidence`, `source{type,pipeline,ocr_engine}`, `ocr_regions[]`. |
| PATCH | `/transactions/{id}` | Edit (`category,shop,fixed,date,amount` — any subset). |
| DELETE | `/transactions/{id}` | Delete. |
| GET | `/transactions/{id}/lines` | Line items + `sigs[]`, `lowConf`. |
| PATCH | `/transactions/{id}/lines/{lineIndex}` | Review/correct a line (`name,qty,unit_price,category,signal_id,confirmed`). |
| POST | `/transactions/{id}/reprocess` | AI re-read of low-confidence lines (`line_indices[]` optional). |

## Categories / budget envelopes
| Method | Path | Purpose |
|---|---|---|
| GET | `/categories` | `{name,budget,spent,items,fixed,usedPct,remaining,proj,hist[],status,spark[]}`. |
| POST | `/categories` | Create envelope (`name,cap,fixed,note`). |
| GET | `/categories/{name}` | Detail (`overCapAmount,projectedSpend,guidance,histAvg`). |
| PATCH | `/categories/{name}` | Set/raise/lower cap (`budget` absolute or `delta`). |
| GET | `/categories/{name}/transactions` | Transactions in a category. |
| GET | `/budget/totals` | `budget, allocated, spent, projected, remaining, overAllocated, unallocated, envelopeCount`. |
| GET | `/budget/allocation` | `segments[]{name,cap,fixed,share}`, `aiAdvice{model,text,suggestedTrimCategory,suggestedTrimAmount}`. |

## Shops
| GET | `/shops` | `{name,txn_count,total_amount}`. |

## Alerts
| Method | Path | Purpose |
|---|---|---|
| GET | `/alerts` | `{id,tone(alert/warn/info/llm),tag,head,body,actions[],kind,source(rule/llm),relatedCategory,relatedRecurring}`. Filters: `tone,tag,status,source`. |
| POST | `/alerts/{id}/apply` | Apply suggestion (`new_budget, move_to_savings`). |
| POST | `/alerts/{id}/dismiss` | Dismiss. |
| POST | `/alerts/{id}/snooze` | Snooze (`until` or `duration`). |
| GET | `/alerts/{id}/target` | Resolve deep-link filter `{category, period}`. |

## Recurring (dashboard summary)
| GET | `/recurring` | `{id,name,amount,cycle,next,status(ok/soon/due),src(user/llm),daysUntil}`, `monthlyTotal`. |
| POST | `/recurring/{name}/mark-paid` | Mark paid. |
| POST | `/recurring/{name}/confirm` | Confirm AI-detected. |

## Subscriptions (full page)
| Method | Path | Purpose |
|---|---|---|
| GET | `/subscriptions` | `{id,name,glyph,category,cadence,amount,monthlyEquiv,annual,day,month,source,status,statusLabel,since,note,hist[],daysUntil,nextLabel,priceRose}`. Filters: `sort,group,cadence,category,source,status,amounts`. |
| POST | `/subscriptions` | Create. |
| GET | `/subscriptions/stats` | `{count,monthly,annual,autoCount,chargedThisCycle,upcomingThisCycle,next30{...},flagged{...}}`. |
| GET | `/subscriptions/billing-sweep` | Impulse-train timeline `{cycle,impulses[],footer}`. |
| POST | `/subscriptions/detect` | AI detect recurring (`lookbackMonths`). |
| GET | `/subscriptions/{id}` | Detail + `guidance{text,severity}`, `histAxisLabel`. |
| PATCH | `/subscriptions/{id}` | Edit. |
| POST | `/subscriptions/{id}/pause` · `/resume` · `/cancel` | Lifecycle. |
| POST | `/subscriptions/{id}/mark-paid` | Mark a charge paid. |
| GET·POST | `/subscriptions/{id}/charges` | List / record charges. |
| POST | `/subscriptions/{id}/confirm` · `/dismiss` · `/snooze` | AI-suggestion + attention actions. |

## Debts (institutional)
| Method | Path | Purpose |
|---|---|---|
| GET | `/debts` | `{id,name,lender,glyph,type,orig,balance,apr,monthly,day,term,src,status,since,note,hist[]}` + derived `{paidOffPct,monthsToPayoff,annualInterest,interestRemaining,nextLabel,statusLabel,forwardSeries[],groupLabel}`. |
| POST | `/debts` | Create. |
| GET | `/debts/stats` | `{count,totalOwed,totalOrig,totalMonthly,totalInterestYr,weightedApr,autoCount,horizon,debtFreeLabel,flagged[],avalancheTarget,snowballTarget}`. |
| GET | `/debts/trajectory` | `points[]{m,total}`, `xTicks[]`, `debtFreeLabel` (`strategy=avalanche|snowball`). |
| PUT | `/debts/strategy` | Set strategy. |
| GET | `/debts/{id}` | Detail + `decaySeries{hist[],forward[],todayIndex}`, `stats`, `guidance`. |
| PATCH·DELETE | `/debts/{id}` | Edit / delete. |
| GET·POST | `/debts/{id}/payments` | History / record extra payment. |
| PATCH | `/debts/{id}/plan` | Adjust plan (`monthly,day,term`). |
| POST | `/debts/{id}/refinance` | Refinance. |

## Personal IOUs
| Method | Path | Purpose |
|---|---|---|
| GET | `/personal-ious` | `{id,person,initials,dir(in/out),amount,of,since,reason,repaidPct}`. |
| POST | `/personal-ious` | Create. |
| GET | `/personal-ious/stats` | `{owedToYou,youOwe,net,countIn,countOut,maxSingle}`. |
| PATCH·DELETE | `/personal-ious/{id}` | Edit / delete. |
| POST | `/personal-ious/{id}/payments` · `/settle` · `/settle-up` · `/remind` | Lifecycle. |

## Item-signals (AI micro-categories)
| Method | Path | Purpose |
|---|---|---|
| GET | `/signals` | `{id,label,unit,parent,desc,glyph,cycleQty,lastQty,cycleSpend,avgUnit,deltaPct,conf,since,txns,series[12],candidate}`. |
| POST | `/signals` | Track (from `candidateId` or `label`). |
| GET | `/signals/candidates` | AI-proposed signals. |
| GET | `/signals/movers` | `{riser,faller,all[]}` by `deltaPct`. |
| GET | `/signals/{id}` | Detail + `recent[]{date,shop,note,qty,price}`. |
| DELETE | `/signals/{id}` | Untrack / pause. |
| POST | `/signals/{id}/cap` | Soft cap (`amount,period,nudgeAtPct`). |
| POST | `/signals/candidates/{id}/track` · `/dismiss` | Approve / reject candidate. |

## Analytics
| Method | Path | Purpose |
|---|---|---|
| GET | `/analytics/spend-history` | 12-cycle `points[]{m,yr,spend,budget,saved,rate,projected,over}`. |
| GET | `/analytics/spend-history/stats` | `{income,cur,prev,avg,avgRate,peak,low,curVsAvgPct,curVsPrevPct,totalSaved}`. |
| GET | `/analytics/category-momentum` | `{name,now,budget,series[12],deltaPct,priorAvg}`. |
| GET | `/analytics/rhythm/weekday` | `weekday[]{d,v}`, `stats{peak,weekendShare}`. |
| GET | `/analytics/insights/movers` | GEMMA4 read + `suggestedCap{signalId,amount,projectedSavings}`. |

## AI assistant
| Method | Path | Purpose |
|---|---|---|
| GET | `/ai/feed` | `{id,kind(categorize/reprocess/suggest/detect),text,conf,sig,state,actions[],time}`. |
| POST | `/ai/feed/{id}/dismiss` | Dismiss feed item. |
| GET·POST | `/ai/chat` | History / send (local GEMMA4; reply may carry `track`/`cap` intents). |
| GET | `/ai/status` | `{model,engine,location,online,watchedSignalLabels[]}`. |
| POST | `/ai/reprocess` | Reprocess low-confidence (global scope). |

## Settings / account / shell
| Method | Path | Purpose |
|---|---|---|
| GET·PATCH·DELETE | `/settings/preferences` | Get / update / reset ~30 UI prefs. |
| GET | `/settings/preferences/defaults` | Defaults. |
| GET | `/settings/summary` | `{totalPreferences,changedCount,engine,model,storedOnDevice}`. |
| GET·PATCH | `/account` | Profile / edit (`holder,iban`). |
| GET | `/account/ai/engines` | Available local engines/models. |
| PUT | `/account/ai/engine` | Select engine/model. |
| GET | `/nav/pages` | Navigation pages. |
| GET·PATCH | `/config` | UI config subset (`topDateFmt`, …). |

## Exports (CSV)
| GET | `/exports/transactions.csv` · `/exports/budget.csv` · `/exports/subscriptions.csv` | CSV (mirror list filters). Paths chosen to avoid `/{id}` collision. |
