# dossier 欄位怎麼讀(m3Spec/wave_judgment_loop.md §4)

> dossier = `neely_forecast(stock_id, date)` 回應(MCP)或
> `GET /stocks/{id}/waves` 的 `dossier` 段(web)。無 primary、無分數 —
> 排序只反映結構(degree desc → end desc → start asc)。

## 頂層

| 欄 | 讀法 |
|---|---|
| `engine.neely` / `engine.traditional` | 引擎版本;判讀落庫時 PIT 錨定用(驗證器自動填) |
| `engine.assumption_hash` | E1 假設集指紋;null = 1.1.1 舊 snapshot(J2 對舊判讀只能判 engine_changed) |
| `assumptions[]` | 8 個工程/詮釋常數 — **步 0 必讀**:REVERSAL_ATR 0.5 決定 monowave 切割顆粒,±4%/±10% 決定規則容差;判讀受哪些影響寫進 notes |
| `cross_timeframe.direction_conflict` | true = daily 與 weekly/monthly 候選方向互斥 → degree_read 要保守 |
| `cross_timeframe.notes` | 「資料窗不足」= 該 timeframe degree 脈絡不可用(§11),不是衝突 |
| `active_judgment.{daily,weekly,monthly}` | 前次判讀(protocol 步 4 最小修改基準);null = 首判 |
| `quality_caveat.is_usable` | false = 引擎對現況無有效候選(合法結果)→ 傾向 no_fit,不要硬改引擎 |

## 每 timeframe 段

| 欄 | 讀法 |
|---|---|
| `snapshot_ref.snapshot_date` | 判讀 `as_of` 必須 ≤ 此日(驗證硬約束) |
| `monowave_count` / `last_bar` | 資料量脈絡;Missing Wave 表換算用 |
| `live_edge.ambiguity` | E4:`count` = 互斥候選數(同 end 同型不重計)、`kinds`、`degree_level`;null = 舊 snapshot(未知,非 0)。count 多 → Reverse Logic:市場在中段 |
| `candidates[]` | **判讀唯一合法選擇集**;`historical.count` 僅脈絡,完整 forest 走 `/stocks/{id}/neely/forest` |
| `truncated` | true = live-edge 候選超過 cap,被砍的是低 degree / 較舊 end 尾端 — 需要全量時走 /neely/forest |
| `traditional.candidates` / `concordance` | 傳統派並排(不整合);`shared="endpoints"` = 兩引擎同端點,佐證權重可提 |

## 候選三區

| 區 | 欄 | 讀法 |
|---|---|---|
| 身分 | `anchor_key` | 判讀輸出引用這把鍵(日期樹,跨 run 穩定);`wave_tree` 深樹有 `children_omitted` 收斂,完整樹走 /neely/forest |
| 證據 | `ch6_status` | `Confirmed`(市場已確認)>`Pending`(接受待驗)>`Deferred`(live edge 無 post-pattern,或舊 snapshot 缺欄 — 未知) |
| 證據 | `robust` | false = 端點依附 0.5 噪音門檻(0.3/0.7 組看不到)→ 不可當 single;**null = 未知(舊 snapshot),非 false** |
| 證據 | `advisory_findings` | Ch9/邊界重評諮詢;Strong 級(如 Trendline Touchpoints ≥5)= 強反證 |
| 前瞻 | `invalidation_triggers` | 判讀 `invalidation` 必須一致或更嚴;`PriceBreakBelow/Above` 的價位直接引用 |
| 前瞻 | `expected_fib_zones` / `post_pattern_behavior` / `max_retracement` | 方向與投影脈絡(S3 track1 消費) |
| 機械 | `is_invalidated` | 現價已破失效價(機械判定)— true 的候選不要選 preferred |

## 常見情境

- **candidates = []**:只能 `no_fit`(forest 空或全歷史);`no_fit_reason` 寫「無 live-edge 候選」+ 缺口
- **全部 Deferred + robust null**(舊 snapshot):判讀可做,但 notes 標明證據等級低;建議先重跑 run-all
- **ambiguity.count = 1 且 robust = true**:`single` 的理想情境
- **daily 候選 degree > weekly 可容納**:degree_read 下修,或 no_fit(引擎跨 tf 不協同,判讀者補)
