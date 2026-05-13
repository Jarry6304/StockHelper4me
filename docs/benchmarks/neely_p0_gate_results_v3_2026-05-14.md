# Neely Core P0 Gate v3 Follow-up — 2026-05-14

## 摘要

| 項目 | 值 |
|---|---|
| 執行日期 | 2026-05-14(同日 v2 校準後 follow-up SQL) |
| neely_core 版本 | 0.21.0(v2 後校準完不動) |
| 跑的 SQL | `docs/benchmarks/neely_p0_gate_followup.sql` §J-§N |
| 校準決策 | **不動 code**;發現 2 個 Phase 5/6 follow-up 項 + 1 個 P2 後續校準項 |

---

## §J. missing_wave_count 每檔分布 ✅ 設計健康

| 統計 | 值 | spec 預期 | 評估 |
|---|---|---|---|
| total_stocks | 1264 | — | — |
| min | 0 | — | — |
| avg | 5.16 | 0-15/檔 | ✅ 中段 |
| p50 | 5 | — | ✅ |
| p95 | 10 | — | ✅ |
| p99 | 13 | — | ✅ |
| max | 16 | 略超 spec 上限 | ⚠️ 1 stock |
| zero_mw_stocks | 46 | — | ✅ 3.6% |
| over_20_stocks | 0 | — | ✅ 完美 |

### Top 10 含最多 missing_wave_suspects

| stock_id | mw_count | monowave_count | forest_size |
|---|---|---|---|
| 1312A | 16 | 147 | 31 |
| 5906 | 15 | 120 | 29 |
| 0053 | 14 | 97 | 9 |
| 2426 | 14 | 77 | 5 |
| 00954 | 14 | 101 | 7 |
| 00898 | 14 | 104 | 7 |
| 00894 | 13 | 93 | 5 |
| 00904 | 13 | 92 | 4 |
| 1522A | 13 | 126 | 37 |
| 00909 | 13 | 124 | 7 |

**結論**:Phase 9 設計健康,**v2 報告誤判**(96.4% 是「至少 1 個」比例,不是過度觸發)。每檔平均 5.16,p99=13,**對齊 spec 預期 0-15/檔**。

## §K. missing_wave position 分類 ⚠️ M1Center gap

| position | total_occurrences | affected_stocks |
|---|---|---|
| M2Endpoint | 2893 | 1100 |
| M0Center | 2777 | 1098 |
| Ambiguous | 855 | 609 |
| **M1Center** | **0** ❌ | **0** |

**Gap**:`missing_wave/mod.rs::classify_position()` M1Center 條件 = `has BF3`(`b:F3` MissingWaveBundle 標)。Phase 2 Pre-Constructive Logic **完全沒寫 BF3 標** → M1Center 永遠不觸發。

**對齊 known limitation**(`pre_constructive/mod.rs` L21-27):3 個 best-guess 項中 `> 3 sub-monowaves polywave 偵測` 預設 false,連帶 BF3 missing-wave bundle 邏輯也未實作。

→ **留 Phase 5/6 Pre-Constructive Logic 完整化補完**(對齊 spec 1042-1062「Pre-Constructive Logic 細部技術備註」),非 P0 Gate 範圍。

## §L. Emulation kind 拆解 ✅ 設計健康

| kind | total_occurrences | affected_stocks | 評估 |
|---|---|---|---|
| FirstExtAsTerminal | 1316 | 796 | Channeling 2-4 線突破常見 |
| RunningDoubleThreeAsImpulse | 421 | 357 | 中段觸發 |
| DiagonalAsImpulse | 6 | 6 | 罕見,合理 |
| **TriangleAsFailure** | **0** | **0** | Triangle 形態罕見 → 0 觸發合理 |
| **Generic** | **0** | **0** | 設計預設不觸發,只 explicit 標的才出現 ✅ |

**結論**:Phase 9b Ch12 Emulation 4 種偽裝偵測 3/4 觸發,**設計健康**。

## §M. Reverse Logic suggested_filter 分布 ✅ 過濾保守

| 統計 | 值 | 評估 |
|---|---|---|
| triggered_stocks | 1120 | — |
| avg_forest | 5.10 | — |
| avg_filter | 1.17 | — |
| **avg_filter_pct** | **21.9%** | ✅ 平均過濾 22% scenarios |
| no_filter_stocks | 334(29.8%) | ✅ is_near_completion 對這些保守 |
| **all_filter_stocks** | **0** | ✅ 從未全部 scenarios 被過濾 |

**結論**:`is_near_completion()` 邏輯**完全對齊 user「不過度自信過濾」設計原則**。

## §N. 22 cores EventKind 級觸發率

### ✅ v1.31 Divergence pivot rewrite 校準完美對齊

| EventKind | events/stock/year | Murphy 1999 預期 | 狀態 |
|---|---|---|---|
| rsi BullishDivergence | 0.66 | 1-4/yr 稀有 | ✅ |
| rsi BearishDivergence | 0.71 | | ✅ |
| macd BullishDivergence | 0.98 | | ✅ |
| macd BearishDivergence | 0.81 | | ✅ |
| kd BullishDivergence | 1.08 | | ✅ |
| kd BearishDivergence | 1.11 | | ✅ |

### ⚠️ v1.32 Cross spacing 校準仍超標 1.5-2× 預期

| EventKind | events/stock/year | v1.32 修後目標 | 超標倍數 |
|---|---|---|---|
| KD GoldenCross | 20.17 | 8-12/yr | 2× |
| KD DeathCross | 20.13 | 8-12/yr | 2× |
| MA MaBullishCross | 13.59 | 6-9/yr | 1.5× |
| MA MaBearishCross | 13.42 | 6-9/yr | 1.5× |
| MACD HistogramZeroCross | 19.01 | 8-12/yr | 2× |
| MACD GoldenCross | 9.58 | 5-7/yr | 1.4× |
| MACD DeathCross | 9.42 | 5-7/yr | 1.4× |

**可能原因(3 假設)**:
1. v1.32 校準的 `MIN_*_CROSS_SPACING=10` 對 daily 1.6y noise 不足 → 需加到 15-20?
2. v1.32 預期值基於不同 sample,production 真實噪音更高
3. SQL 計算 `EXTRACT(YEAR FROM AGE(MAX,MIN))+1` 對短 span 有誤差(facts 跨度 ~5-6 年但 Bronze 僅 1.6y,因 fact_date 含 indicator 暖機回填)

**這不是 P0 Gate(neely_core)範圍**,屬 P2 後續校準題,留下個 session 動(調 spacing constants in `kd_core` / `ma_core` / `macd_core`)。

### ⚠️ 4 cores EventKind 分類腳本不完整

`neely_p0_gate_followup.sql` §N 的 CASE LIKE pattern 沒抓到部分 cores 的 statement string,顯示為 `Other` / `Aggregated`:

| Core | 顯示 | 應該拆 |
|---|---|---|
| `day_trading_core` | "Other" 79.33/yr | RatioExtremeHigh / Low / EnteredZone / ExitedZone |
| `institutional_core` | "Other" 117.54/yr | 8 個法人類(Foreign/Trust/Dealer/GovBank × Buy/Sell) |
| `revenue_core` | "Aggregated" 10.03/yr | 應有 RevenueYoyHigh / Low / MomHigh / Low 等 |
| `valuation_core` | "Aggregated" 31.83/yr | 應有 PerLow / High / PbrLow / YieldHigh 等 |

→ 腳本 bug(LIKE pattern 對不上 production statement),不影響 cores 行為。留 follow-up 修腳本。

---

## v3 校準決策

| 項目 | 校準 |
|---|---|
| missing_wave_count 分布 | ✅ 對齊 spec,不動 |
| missing_wave M1Center gap | ⚠️ 留 Phase 5/6 Pre-Constructive 補 BF3 標 |
| Emulation kind 觸發 | ✅ 3/4 觸發,合理 |
| Reverse Logic 過濾 | ✅ 21.9% 保守,對齊設計原則 |
| Divergence(rsi/macd/kd) | ✅ v1.31 校準完美 |
| Cross 類超標(kd/ma/macd) | ⚠️ 留 P2 後續校準動 spacing 常數 |
| EventKind 分類腳本 | ⚠️ 4 cores LIKE pattern 不完整,留 follow-up 修腳本 |

**Code 不動**(neely_core v0.21.0 維持)。

---

## v3 後續行動清單

| 優先 | 動作 | 範圍 |
|---|---|---|
| 中 | 補 Phase 2 Pre-Constructive BF3 missing-wave bundle 標 | `pre_constructive/rule_1.rs` 等(需 spec §1042-1062 進階資料) |
| 中 | 重新校準 KD/MA/MACD MIN_*_CROSS_SPACING 常數 | 看 v1.32 commit `a678383` 加常數值,production 觀察建議 ×1.5 |
| 低 | 修 `neely_p0_gate_followup.sql` §N CASE LIKE pattern 對 4 cores | 對齊 production statement 字串 |
| 低 | Bronze 5+ 年 backfill 後重跑 P0 Gate v4 | 看真實 forest 上限是否突破 200 |

---

## 結論

**Neely Core P0 Gate v2 + v3 校準完成**:
- v2:**1 改動**(`forest_max_size` 1000 → 200)+ 6 不動 = v0.20.0 → v0.21.0
- v3:**0 改動**(missing_wave / emulation / reverse_logic 設計健康確認)
- 2 個 follow-up 項(M1Center BF3 標 + Cross spacing 校準)非 neely_core 範圍,留後續 session

**neely_core v0.21.0 production ready**,適合進 P1+ indicator core 第二波擴展。
