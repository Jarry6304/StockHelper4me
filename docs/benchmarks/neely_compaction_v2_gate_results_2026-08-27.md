# Compaction v2 P0 Gate v3 Results — 2026-08-27

> 依 `m3Spec/neely_compaction_v2.md` §9(r4)驗收 tiling-round shadow 引擎
> (engine tag `tiling-round-g2.4`)。資料源 = 本機 production
> `structural_snapshots` 每檔最新 daily neely 列之
> `snapshot->'diagnostics'->'shadow_compaction'`,聚合腳本
> `scripts/verify_compaction_v2_gate.py`(退碼 0/1)。
> 四輪歷程與逐輪原始輸出:`docs/changelog/neely-compaction-v2.md`
> §Gate 第一~四輪。

## 摘要

| 項目 | 值 |
|---|---|
| 執行日期 | 2026-08-26(第一輪)~ 2026-08-27(第四輪全市場 + 三筆稀有 stage 驗屍) |
| universe | 2192 檔 daily(g2.4 = 2191;g2.1 殘 1 檔 = `_index_taiex_` 1900-01-01 殘列,既有 backlog) |
| 引擎 | `compaction/round_engine.rs` tiling-round(shadow 雙軌,serving forest 不受影響) |
| 召回門檻拍板 | **(A)**:允許類別 (a)–(h) + 硬門檻「未歸因缺口 = 0」(spec §9.3 r4 重寫);(B) 放寬 W2 回退 I5 否決 |
| 判定 | **硬性自動項全過**(依 r4 門檻);手動項(RSS / 抽驗 / 前端檢視)留切換 PR 前完成 |

---

## ✅ 硬性門檻(r4 §9.2 / §9.3)

| 項 | 門檻 | 全市場實測 | 判定 |
|---|---|---|---|
| 不變量 | I1–I6 violation = 0 | **0**(2192 檔) | ✅ |
| w1_violations | = 0 | **0** | ✅ |
| Terminal Impulse 存在(D-5) | Diagonal:* > 0 | **363 nodes** | ✅ |
| 召回驗屍 | **未歸因缺口 = 0** | 33,699 筆 miss、12 stage 類別 100% 落入允許類別 (a)–(h);`accepted_but_not_collected` = 0 | ✅ |
| runtime | shadow vs neely 全程占比 ≤ 2× | Σ=24.4s / Σ=28.3s = **86.3%**(p50 10.8ms/檔;run-all wall time 為 DB 議題附註,見 process-logs DB 維護節) | ✅ |
| forest_size(凍結側) | p99 ≤ 40 | shadow 收集全量 proxy p50=26 / p95=53 / p99=69(護欄前,觀測);**凍結側於切換 PR 以真 forest_size 判**(forest_max_size 200 / BeamSearchFallback 把關) | 留切換 PR |
| fusion | track1 讀新 `wave_count` 欄 | G2.4 前半落地,舊字串 parse 回歸不破(pytest 全綠) | ✅ |
| RSS ≤ 1.5× | 手動 | `peak_memory_mb` 未填值(backlog),工作管理員觀測留切換 PR 前 | 手動 |
| Level-1 Impulse 抽樣 100 例 | 抽驗 | 留切換 PR 前 | 手動 |
| 前端六檔巢狀 wave_tree | 檢視 | 留切換 PR 前 | 手動 |

## 📊 §9.3 召回驗屍 — 歸因分布(全市場 33,699 筆未召回)

召回率 5222/38921 = **13.42%**(觀測指標;r4 明訂 98% 對 label-blind
舊引擎結構上不可達)。第一拒絕階段分布,全類別對映 r4 允許類別:

| 階段 | 筆 | 佔缺口 | 允許類別 | 定性 |
|---|---|---|---|---|
| w2_label | 20,699 | 61.4% | (c) | I5 label 閉合:舊引擎候選生成 label-blind |
| w4 | 5,988 | 17.8% | (d) | S&B bars 基準(Q3 連動)+ round-1 適用語意差 |
| no_aligned_end | 4,856 | 14.4% | (e) | Neutral 橋接:舊視窗端點無對齊葉邊界 |
| w7 | 1,397 | 4.1% | (f) | Fib² 全相鄰對(舊引擎僅首尾邊界對) |
| tag_diff | 676 | 2.0% | (g) | A-9 變體細分後 tag 不同(視窗本身被接受) |
| w6 | 80 | 0.2% | (h) | D-5 分岔兩組條件皆不滿足 |
| len_mismatch | 2 | 0.0% | (e) | 內部葉數 ∉ {3,5,7,11}(見下稀有案例) |
| w5 | 1 | 0.003% | (h) | G2.2 `:5` 族硬閘 + synth 端點量測差(見下) |
| accepted_but_not_collected | **0** | — | 永不允許 | **收集正確性確證**(§7.1 I6) |

六檔第三輪分布(w2_label 58.8% / w4 15.8% / no_aligned_end 15.8% /
w7 5.3% / tag_diff 3.5% / w6 0.9%)與全市場全類別 ±3pp 一致 —
六檔代表性確認。

### 稀有 stage 案例逐筆定案(`recall_miss_examples`)

| 檔 | 案例鍵 | 定案 |
|---|---|---|
| 00892 | `len_mismatch:268-336:Triangle:Contracting` | 同質半導體 ETF ×2:起訖 bar 皆對齊 base 葉邊界,但舊 5-monowave Triangle 夾住葉數因 **Neutral 橋接內部合併**而 ∉ {3,5,7,11} → 歸 (e) 量測鍵(內部葉數) |
| 00893 | `len_mismatch:234-299:Triangle:Contracting` | 同上 |
| 6218 | `w5:183-197:Diagonal:Ending` | 15-bar Terminal:舊引擎於 `rules_passed_count` 恆 0 bug 時代以原始 monowave 量測接受;新階梯對 `synth_window` 合成端點跑量化 Ch5,邊界個案翻面。**非 W5 端點泛化回歸**(六檔 w5=0 成立),全市場唯一單例 → 歸 (h) |

## 🔍 觀測項

| 項 | 全市場 | 附註 |
|---|---|---|
| level_cap_hit | 2077/2192 = **94.8%** | A-8 `max_compaction_levels=4` 動態化議題的量測依據 |
| branch cap 命中 | 1299 檔;timed_out = 0 | 工程護欄(限深化探索不限收集) |
| shadow 耗時 | p50=10.8ms / p99=24.8ms / Σ=24.4s | |
| Level 分布 | 1: 43,158 / 2: 15,318 / 3: 511 / 4: 7 | Level-N(N≥2)分布供 architecture §13.3 延伸 |
| Complexity | 1: 58,210 / 2: 784;triplexity = 40 | §6.2 真算 |
| §6.1 邊界重評 | checked=2,537,461;Info 16.7% / Warn 11.0% | advisory,不拒絕 |
| A-10 anchors | union=5,336 vs overlap=23,743(高估 77.5%) | 現行近似語意收緊幅度 |
| Q3 殘差 | 2,374/7,545 = 31.5% | 端點 vs bars 分歧率(bars 已定案,留觀測) |

**Pattern 分布(top)**:Zigzag:Single 30,891;Flat 七變體全出值
(Elongated 5,761 / BFailure 3,692 / Common 3,040 / CFailure 2,546 /
DoubleFailure 2,164 / Irregular 1,757 / IrregularStrongB 957);
RunningCorrection 3,192;Triangle Contracting 1,849 / Expanding 415;
Impulse 1,716;Diagonal:Ending 363;Combination 細分出值(A-9)。

## 四輪歷程摘要

| 輪 | 範圍 | 關鍵結果 |
|---|---|---|
| 一(08-26) | 全市場 g2.4 前 | inv/w1 全 0;召回 10.68% → 揭露**收集限縮於最終 beam pool 違 §7.1** → 修正(materialize 時累積收集) |
| 二(08-26/27) | 六檔複測 | 收集修正生效(2330 degree-1 20→38)但召回持平 → `--diff` 排除 Neutral 錯位 / tag 差兩嫌疑,80–98% 屬 absent → 落召回驗屍儀表 |
| 三(08-27) | 六檔驗屍分布 | 114 筆缺口全數歸因(w2_label 58.8% 等);w5=0、accepted_but_not_collected=0 排除引擎缺陷假說 → 拍板題 (A)/(B) 成形 |
| 四(08-27) | 全市場 g2.4 | 分布與六檔 ±3pp 一致;3 筆稀有 stage 案例鍵定位並逐筆定案 → **(A) 拍板,未歸因缺口 = 0 閉合** |

## 結論

- ✅ tiling-round 引擎正確性:I1–I6 / w1 全市場零違反;Terminal 存在;
  收集正確性(`accepted_but_not_collected` = 0)確證。
- ✅ 召回缺口 100% 歸因於 spec 明訂修正(I5 / W4 bars / W7 / A-9 / D-5 /
  G2.2 硬閘)或量測鍵 — 是**修正的代價**,非回歸;(A) 拍板入 r4 §9.3。
- ✅ runtime 占比 86.3%(≤ 2×);shadow 絕對成本 p50 10.8ms/檔可忽略。
- ⏭ **切換刪舊 PR**(spec §3.3 / 附錄 B G2.4 後半):§7.1/§7.2 凍結流程、
  serving 改吃 tiling-round、刪 `exhaustive.rs` / `three_rounds.rs`、
  `beam_width` 移除、structure_label 新格式(Q6 起算)。凍結側
  forest_size p99 ≤ 40 與三項手動檢視於該 PR 驗。

---

## 切換後凍結側驗收(2026-08-28 補記)

G2.4 後半切換(neely_core 1.1.0,engine `tiling-round-v2`)後,DELETE
neely facts → 全市場 run-all → gate(凍結側,真 scenario_forest):

| 項 | 全市場實測(2191 檔) | 判定 |
|---|---|---|
| I1–I6 / w1 | 全 0 | ✅ |
| Terminal Impulse | 363 顆 | ✅ |
| runtime 引擎占比 | 91.4%(≤ 2×) | ✅ |
| **overflow(forest_max_size 200)** | **0/2191 觸發**;max = 131(00702) | ✅ |
| 凍結側 forest 分布 | p50=26 / p95=53 / **p99=69** — 與 shadow 期收集 proxy 完全一致(凍結 = 收集,零失真) | (A) 拍板後 ✅ |
| W5 RuleRejection(§4.1) | 3512 唯一視窗全數入 diagnostics.rejections | ✅ |
| wall time | **147.6 min**(vs 維護前夜 480 min — facts stats 修復在真實負載確認) | 附註 |
| facts 重生 | DELETE 舊引擎敘述 → v2 全量重生 84,532 筆 | 附註 |

**forest p99 門檻拍板 (A)(2026-08-28,user 拍板;spec r5 §9.2)**:
「p99 ≤ 40」係 Level-0 forest 形狀遺產(P0 Gate v2 p99=16 × 2.5),對
§7.1 全量收集(I6)不適用 — 改雙門檻「overflow 觸發率 = 0」+「凍結側
p99 ≤ 100(= cap 的一半)」;實測 0 觸發 / 69,兩項皆過。縮收集追 40
否決(違 I6 與召回拍板)。**至此 §9.2 自動門檻全數收案**;殘留三項手動
檢視(RSS ≤1.5× / Level-1 Impulse 抽樣 / 前端六檔巢狀 wave_tree,建議
併跑 verify_mcp_toolkit_v4_29.py 確認密集檔 payload budget)。

## §9.2 抽驗 → Overlap 閘修正複驗(2026-08-28 補記,neely_core 1.1.1)

Level-1 Impulse 抽驗(`scripts/sample_level1_impulse.py`,判準與 validator
同源)揭露:W5 族別閘門只閘 `overall_pass` 時,兩條 Overlap 規則互為排他
補集使 `both_overlaps_failed` 永不成立 — Trending row 的 Overlap_Trending
從未被強制,W4 小幅進 W2 區(R5 經 Ch9 容差豁免)的視窗凍結成 Impulse
(70/1232)。修正(Impulse kind 另要求 Overlap_Trending 未 fail)後
neely 單核重跑複驗:

| 項 | 修正前 | 修正後 | 判定 |
|---|---|---|---|
| I1–I6 / w1 | 全 0 | 全 0 | ✅ |
| Terminal Impulse | 363 | 360(tiling 連動) | ✅ |
| overflow / max forest | 0 / 131 | 0 / 131 | ✅ |
| 凍結側 p50/p95/p99 | 26/53/69 | 26/53/**68** | ✅ |
| runtime 占比 | 91.4% | 92.4% | ✅ |
| degree-1 Impulse | 1232 | **1162**(−70,精確吻合) | — |
| 抽驗 R7 / Overlap | 35 Ch9 豁免後仍 70 Overlap 不一致 | **1162/1162 雙全過** | ✅ |
| W5 拒絕唯一視窗 | 3512 | 3618(+106) | — |

三項手動檢視:RSS peak 218MB(≪1.5×)✅ / MCP payload PASS WITH
WARNINGS(indicators 105KB 既有)✅ / 前端六檔 API 全 200 ✅。
**§9.2 全數收案**(自動 + 抽驗 + 手動)。
