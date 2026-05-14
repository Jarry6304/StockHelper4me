# Neely Core P0 Gate 六檔實測 Runbook

對齊 `m3Spec/neely_core_architecture.md §10.0`(P0 Gate 校準目標)+ §13.3(Degree Ceiling)。

P0 Gate 是 Neely Core 進入 P1 indicator core 之前的硬要求:六檔股票實際 production 資料跑出 forest 後,人工 visual review 校準關鍵常數,寫入 `docs/benchmarks/`。

---

## 1. 前提

- `claude/continue-previous-work-xdKrl` 已合進 user 本機 active branch
  (**Phase 14-19 完整 spec alignment**:PR #51 含 commit `9791b09`(Phase 13) →
  `4de97f3`(Phase 19),11 commits 累計)
- 本機 PG 17 + alembic head = `w2x3y4z5a6b7`(structural_snapshots / facts /
  indicator_values 三表已建)
- Silver `price_*_fwd` 後復權跑過所有六檔
- `rust_compute/target/release/tw_cores` binary 已 build with **neely_core v0.26.0**:
  ```bash
  cd rust_compute && cargo build --release -p tw_cores
  ```
- 預期 inventory:`tw_cores list-cores | grep neely_core` 應顯示 `0.26.0`

---

## 2. 六檔股票挑選依據(spec §10.0)

| 股票 | 代號 | 挑選原因 |
|---|---|---|
| 元大台灣50 | `0050` | ETF 走勢平滑,Power Rating 應 ≥ Bullish,候選 forest 應收斂(< 10) |
| 台積電 | `2330` | 龍頭股 + 上市 30+ 年(月線可達 Cycle 級 Degree Ceiling) |
| 微星 | `3363` | 中型股,有過明顯 5-wave Impulse 走勢 |
| 高端疫苗 | `6547` | 高波動 + 過去有 stock_dividend 事件 → 測 Hybrid OHLC 切割 |
| 大買家 | `1312` | 低成交量小型股 → 測 monowave Neutrality / ATR filter 邊界 |
| (自選 1 檔) | TBD | 加權指數 `_index_taiex_` 或近期上市新股 |

如果 user 要換組合,直接改 `neely_p0_gate_check.sql` 開頭的 `P0_GATE_STOCKS`。

---

## 3. 執行步驟

### 3.1 跑 tw_cores `run` 收集 snapshot

對每檔跑一次(預估每檔 1-3 秒):

```powershell
$env:DATABASE_URL = "postgresql://twstock:twstock@localhost:5432/twstock"
$stocks = @("0050", "2330", "3363", "6547", "1312")
foreach ($s in $stocks) {
    .\rust_compute\target\release\tw_cores.exe run --stock-id $s --write
}
```

(或一次 `tw_cores run-all --limit 6 --write` 全市場跑,然後篩這六檔。)

預期每檔 `INFO  structural_snapshots: 1 row UPSERT` + `facts: N rows`。

### 3.2 跑 SQL 收集校準資料

```powershell
psql $env:DATABASE_URL `
  -f docs/benchmarks/neely_p0_gate_check.sql `
  > p0_gate_results.txt
```

7 段輸出依序為:
- §0 Sanity check — 六檔是否齊全
- §1 Forest 規模 / candidate / monowave 計數
- §2 Validator pass/reject + 工程護欄
- §3 RuleRejection 拒絕原因 Top 10 per stock
- §4 Stage_elapsed_ms 各 stage 性能
- §5 P9-P12 新欄位觸發分布(missing_wave / emulation / reverse_logic / round3 / degree_ceiling)
- §6 Facts 產出量

### 3.3 Visual review 校準

對 `p0_gate_results.txt` 逐段檢視。

---

## 4. 校準目標(預設常數 → 預期值範圍)

| 常數 | 檔案位置 | 預設值 | 預期 P0 Gate 觀察 |
|---|---|---|---|
| `forest_max_size` | `config.rs:NeelyEngineConfig` | 1000 | §1 forest_size:六檔都 < 100 → 可降至 500;若任一檔接近 1000 → §2 overflow_triggered = true,需校準 |
| `compaction_timeout_ms` | `config.rs:NeelyEngineConfig` | 60000 | §1 elapsed_ms:六檔都 < 60s → 60s 寬鬆;若 §2 compaction_timeout = true → 增加 timeout |
| `beam_width` | `config.rs:NeelyEngineConfig` | 100 | §1 candidate_count 上限 = `beam_width × BEAM_CAP_MULTIPLIER` = 1000;若 forest 規模合理但 candidate 接近 1000 → 降 beam_width |
| `REVERSAL_ATR_MULTIPLIER` | `monowave/pure_close.rs` | 0.5 | §1 monowave_count:跨檔差距合理(0050 < 2330 < 3363 < 6547 < 1312);若 1312 比 0050 monowave 多 5× → multiplier 太小 |
| `STOCK_NEUTRAL_ATR_MULTIPLIER` | `monowave/neutrality.rs` | 1.0 | §3 拒絕原因中無大量 N/A → multiplier 合理;若 Neutral 比例 > 50% → multiplier 太大 |
| `BEAM_CAP_MULTIPLIER` | `candidates/generator.rs` | 10 | 同 `beam_width`,間接觀察 |
| `REVERSE_LOGIC_THRESHOLD` | `reverse_logic/mod.rs` | 2 | §5 reverse_logic_triggered:六檔的 scenario_count 分布 → 若 forest 多檔都 ≥ 2 → 閾值合理;若 0050 / 2330 ETF 平滑度 scenario_count 仍 ≥ 5 → 閾值偏寬 |
| `NEELY_FIB_RATIOS` / `FIB_TOLERANCE_PCT` | `fibonacci/ratios.rs` | 10 個 + 4% | spec 寫死不外部化,**不**校準 |
| `WATERFALL_TOLERANCE_PCT` | `fibonacci/ratios.rs` | 5% | spec 寫死,留 P11+ Reverse Logic 對接時啟用 |
| Degree Ceiling 閾值 | `degree/mod.rs:classify_degree()` | 1y/3y/10y/30y/100y | §5 degree_max:六檔 daily timeframe 應落在 SubMinuette ~ Minor;TAIEX monthly 才會到 Cycle |

---

## 5. 校準產出

校準完成後,寫一份 `docs/benchmarks/neely_p0_gate_results_<date>.md` 紀錄:

```markdown
# Neely Core P0 Gate 校準結果 — <YYYY-MM-DD>

執行時間:<timestamp>
neely_core 版本:0.19.0
資料範圍:<P0 Gate 跑日期>

## §1 Forest 規模摘要

| stock_id | monowave | candidate | forest | overflow | pass_pct |
|---|---|---|---|---|---|
| 0050 | ... | ... | ... | false | ... |
| ... | ... | ... | ... | ... | ... |

## §3 主要拒絕規則 Top 5(across 六檔)

1. Ch5_Essential(7) — W3 not shortest:N rejections (avg gap X%)
2. ...

## 校準決策

| 常數 | 修前 | 修後 | 原因 |
|---|---|---|---|
| forest_max_size | 1000 | (例)500 | 六檔最大 forest = 87 ≪ 1000 |
| ... | ... | ... | ... |

## 待 P1+ 校準的開放問題

1. ...
2. ...
```

---

## 6. 若觀察到的問題

### 6.1 `insufficient_data = true` 對全部六檔都成立

- 表示 Silver `price_daily_fwd` 資料不足 500 bar(預設 Daily warmup)
- 修法:確認 P0 Gate 股票都有至少 2 年 daily 資料(對齊 §13.1 warmup_periods)
- 或降 `warmup_periods` 預設(spec §13 留校準空間)

### 6.1.1 某檔股票 `snapshot_date = 1900-01-01` / `monowave_count = 0`

**症狀**:`tw_cores run --stock-id <X>` 跑完但 forest 全空。
**根因**:Silver `price_daily_fwd` 對該股票完全沒資料(circular bootstrap miss)。
Rust Phase 4 `resolve_stock_ids` 拉的是 `price_daily_fwd WHERE is_dirty=TRUE` —
但若該 stock 從未進過 `price_daily_fwd`,dirty queue 永遠不會選到 → Phase 4 never
runs → Silver 永遠空。

**Diagnostic**:

```powershell
$STOCK = "6547"   # 改成有問題的股票

# 1. Bronze 端是否有資料?
psql $env:DATABASE_URL -c @"
SELECT '$STOCK' AS stock_id,
       (SELECT COUNT(*) FROM price_daily WHERE stock_id = '$STOCK') AS bronze_rows,
       (SELECT MIN(date) FROM price_daily WHERE stock_id = '$STOCK') AS bronze_first,
       (SELECT MAX(date) FROM price_daily WHERE stock_id = '$STOCK') AS bronze_last,
       (SELECT COUNT(*) FROM price_daily_fwd WHERE stock_id = '$STOCK') AS silver_rows
"@
```

預期診斷:
- `bronze_rows > 0` AND `silver_rows = 0` → 確認 circular bootstrap miss
- `bronze_rows = 0` → Bronze 也沒料,需先補 Bronze backfill(`python src/main.py
  backfill --phases 3 --stocks $STOCK`)

**Bootstrap fix**(對 single stock 顯式跑 Phase 4):

```powershell
# 顯式對 6547 跑 Rust Phase 4 後復權(--stocks 旁路 dirty queue,直接讀 Bronze)
.\rust_compute\target\release\tw_stock_compute.exe --stocks $STOCK

# 確認 silver 有資料了
psql $env:DATABASE_URL -c "SELECT COUNT(*), MIN(date), MAX(date) FROM price_daily_fwd WHERE stock_id = '$STOCK'"

# 重跑 tw_cores 對該股寫 snapshot
.\rust_compute\target\release\tw_cores.exe run --stock-id $STOCK --write

# 再跑 P0 Gate §0-§5 query 確認 snapshot_date 不再是 1900-01-01
psql $env:DATABASE_URL -f docs\benchmarks\neely_p0_gate_check.sql > p0_gate_$STOCK.txt
```

**長期 fix**(留 P1):在 orchestrator `_fetch_dirty_fwd_stocks` 加 fallback ——
若 `price_daily` 有但 `price_daily_fwd` 沒的 stocks → 自動加進 dirty queue,
打破 circular bootstrap。對齊 PR #20 trigger 設計 spirit。

### 6.2 `overflow_triggered = true` 出現

- forest 爆量,觸發 BeamSearchFallback
- 校準:
  1. 先看 §1 forest_size 多大(若 ≪ 1000 → bug,不該觸發)
  2. 若 ~1000 → 真實爆量,需把 forest_max_size 升 1500 / 2000
  3. 若無法避免 → 升級 P8 Compaction Three Rounds 完整實作(目前 pass-through 簡化版)

### 6.3 §3 拒絕原因 90%+ 集中在某 RuleId

- 該規則太嚴 / 容差太小
- 校準步驟:
  1. 看 `avg_gap_pct` — 若 5-10% → 容差(±4% / ±10%)需放寬至 15%
  2. 若 > 50% → 規則邏輯有問題,需逐條對照 spec 章節

### 6.4 `stage_0_preconstructive` 耗時 > 5s(熱點)

- Ch3 ~200 branch if-else cascade 對 monowave 數量敏感
- 校準:
  1. 確認 `monowave_count` 合理(< 500 對 daily / 2 年)
  2. 若 monowave 過多 → 升 `REVERSAL_ATR_MULTIPLIER`(0.5 → 0.7)減少瑣碎反轉

---

## 7. P0 Gate 通過後

把校準後的常數寫死進 `neely_core/src/` 對應檔案,跑:

```bash
cargo test --release -p neely_core --no-fail-fast
cargo clippy --release -p neely_core --all-targets -- -D warnings
```

確認 0 regression,然後 bump neely_core version **0.26.0 → 1.0.0**(spec §10.0
P0 Gate 通過後才能 bump 主版本)。

之後可進 P1 indicator core(trendline_core / support_resistance_core /
divergence_core 等)。

---

## 8. Phase 14-17 metadata 驗證 SQL(2026-05-14 落地)

P0 Gate 通過前須驗證 Phase 14-17 各新 metadata field 真實寫入 production data。
本段 5 個 query 對齊 `neely_core/src/output.rs` Scenario 結構 + Phase 13/14/15/16/17
新欄位。queries 對 `structural_snapshots.snapshot->'scenario_forest'` JSONB array
逐 scenario 展開檢查。

### 8.1 Phase 13/14:max_retracement + post_pattern_behavior

```sql
SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->'max_retracement' IS NOT NULL
                       AND jsonb_typeof(s->'max_retracement') = 'number') AS with_max_retr,
    array_agg(DISTINCT s->>'max_retracement') AS max_retr_values,
    array_agg(DISTINCT CASE
        WHEN jsonb_typeof(s->'post_pattern_behavior') = 'string'
            THEN s->>'post_pattern_behavior'
        WHEN jsonb_typeof(s->'post_pattern_behavior') = 'object'
            THEN (SELECT key FROM jsonb_each(s->'post_pattern_behavior') LIMIT 1)
        ELSE NULL
    END) AS behavior_kinds
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core';
```

**預期(v5 全市場驗證已得)**:
- `with_max_retr` ~36%(只非 Neutral 場景填值,值集 `{0.65, 0.80, 0.90, NULL}` ✅)
- `behavior_kinds` ≥ 5 種 variants(Composite / FullRetracementRequired / MinRetracement /
  NextImpulseExceeds / NotFullyRetracedUnless / Unconstrained — 8 variant 中 6 種出現)

### 8.2 Phase 15:Scenario 群 2 fields

```sql
SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->>'round_state' IS NOT NULL) AS with_round_state,
    array_agg(DISTINCT s->>'round_state') AS round_states,
    COUNT(*) FILTER (WHERE jsonb_array_length(s->'monowave_structure_labels') > 0) AS with_mw_labels,
    COUNT(*) FILTER (WHERE jsonb_array_length(s->'pattern_isolation_anchors') > 0) AS with_anchors,
    COUNT(*) FILTER (WHERE (s->>'triplexity_detected')::bool = true) AS with_triplexity
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core';
```

**預期(v5 已驗)**:
- `with_round_state` 100% / `round_states = {Round2, Round3Pause}` ✅
- `with_mw_labels` 100% / `with_anchors` ~21% / `with_triplexity` ~0%(罕見)

### 8.3 Phase 16:FlatKind 7-variant + RunningCorrection 上提

```sql
-- 完整 pattern_type 分布
SELECT
    CASE jsonb_typeof(s->'pattern_type')
        WHEN 'string' THEN s->>'pattern_type'
        WHEN 'object' THEN (
            SELECT key || '(' || COALESCE(value->>'sub_kind',
                                          jsonb_path_query_first(value, '$.sub_kinds[0]')::text,
                                          '') || ')'
            FROM jsonb_each(s->'pattern_type') LIMIT 1
        )
        ELSE 'unknown'
    END AS pattern,
    COUNT(*)
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core'
GROUP BY 1 ORDER BY 2 DESC;
```

**預期(v5 已驗)**:11 種 pattern variants 出現:
- Zigzag(Single) 39% / Impulse 21% / Flat(Common) 15% /
  Flat(BFailure/CFailure/Irregular/DoubleFailure/Elongated) 共 23% /
  Diagonal(Leading/Ending) 2.3% / RunningCorrection 0.05% ✅

### 8.4 Phase 17:StructuralFacts 7 sub-fields 填充率

```sql
SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'fibonacci_alignment' IS NOT NULL
                       AND s->'structural_facts'->>'fibonacci_alignment' != 'null') AS fib,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'alternation' IS NOT NULL
                       AND s->'structural_facts'->>'alternation' != 'null') AS alt,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'channeling' IS NOT NULL
                       AND s->'structural_facts'->>'channeling' != 'null') AS chan,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'time_relationship' IS NOT NULL
                       AND s->'structural_facts'->>'time_relationship' != 'null') AS tr,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'volume_alignment' IS NOT NULL
                       AND s->'structural_facts'->>'volume_alignment' != 'null') AS vol,
    COUNT(*) FILTER (WHERE (s->'structural_facts'->>'gap_count')::int > 0) AS gaps_found,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'overlap_pattern' IS NOT NULL
                       AND s->'structural_facts'->>'overlap_pattern' != 'null') AS overlap
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core';
```

**預期(v5 已驗 7767 scenarios)**:
- fib 77.5% / alt 7.5%(3-wave NA → None;5-wave ~33% 填) / chan 100% /
  tr 100% / vol 100% / gaps 100%(1e-9 tol 嚴) / overlap 22.8%(5-wave 才填)

### 8.5 Smoke:單 scenario 完整 JSON(2330 第一個)

```sql
SELECT jsonb_pretty(snapshot->'scenario_forest'->0) AS first_scenario
FROM structural_snapshots
WHERE core_name = 'neely_core' AND stock_id = '2330'
LIMIT 1;
```

**預期**:完整 JSON 含 Phase 14-17 所有新欄位(`max_retracement` /
`post_pattern_behavior` / `round_state` / `monowave_structure_labels` /
`pattern_isolation_anchors` / `triplexity_detected` / `structural_facts` 7 sub-fields)。

---

## 9. v5 全市場 production verify(2026-05-14 已得)

User 已對 1263 stocks 跑 production verify(commit `4de97f3` 之前的 state),
結果歸檔於 `docs/benchmarks/neely_p0_gate_followup.sql` § §J-§N section。

| 指標 | 值 | spec 預期 | 評估 |
|---|---|---|---|
| neely_core 1263 stocks 全綠 | 0 error | 100% pass | ✅ |
| forest_size max | 31(1312A) | < forest_max_size=200 | ✅(15% utilization) |
| missing_wave_count avg/max | 5.16 / 16 | ≤ 20 spec | ✅ |
| reverse_logic triggered | 88.7% | — | ✅ |
| avg_filter_pct | 21.9% | ~20-30% | ✅ |
| Emulation DiagonalAsImpulse | 6 stocks(0.24%) | 罕見 | ✅ |
| FlatKind variants(6/7) | Common/BFailure/CFailure/Irregular/DoubleFailure/Elongated | ≥ 3 種 | ✅ |
| RunningCorrection top-level | 4 scenarios(0.05%) | 罕見(spec ±3) | ✅ |
| Phase 17 StructuralFacts fill rate | 7/7 sub-fields 填值正常 | — | ✅ |

→ **production v5 全綠**,scaffold P0 Gate 六檔實測即可推進 1.0.0。

---

## 10. Pre-1.0.0 Bump Checklist(2026-05-14)

從 v0.26.0 → 1.0.0 須完成:

| 項 | 狀態 |
|---|---|
| ✅ Phase 1-12 Stage 0-12 完整 pipeline | done |
| ✅ Phase 13 max_retracement Option<f64> | done (9791b09) |
| ✅ Phase 14 PostBehavior 8-variant + WaveNumber | done (4fd1c68) |
| ✅ Phase 15 Scenario 群 2 fields | done (65bef04) |
| ✅ Phase 16 FlatKind 7-variant + RunningCorrection 上提 | done (3685032) |
| ✅ Phase 17 StructuralFacts 7 sub-fields | done (6c3ea4d) |
| ✅ Phase 18 OBV Divergence oscillator | done (a222e7a) |
| ✅ Phase 19 RSI 接受現狀 + SQL CASE alignment | done (4de97f3) |
| ⏳ **六檔 visual review** 1312/0050/2330/3363/6547/(1 自選) | **TODO**(user 跑 §3 流程後肉眼校驗各檔 forest) |
| ⏳ 校準後 commit `docs/benchmarks/neely_p0_gate_results_<date>.md` | TODO |
| ⏳ bump version 0.26.0 → 1.0.0 + inventory description | TODO(校準後) |

**通過判定**:
- (a)六檔 §1 `forest_size` 在 spec 預期範圍(0050 ETF 收斂 < 10 / 2330 龍頭 < 30 / 1312 小型股 monowave 警惕)
- (b)六檔 §3 拒絕原因前 5 條 RuleId 都對應 spec § 章節(無「unknown」分類)
- (c)六檔 §5 P9-P12 metadata 觸發分布(missing_wave / emulation / reverse_logic /
  degree_max)落在 spec §10.0 + §13.3 預期範圍
- (d)§8 Phase 14-17 metadata 在六檔上的填充率對齊 v5 全市場驗(>= 7767 baseline 比例 ±5%)

通過後即可 bump 1.0.0,進 P1 indicator cores 階段。
