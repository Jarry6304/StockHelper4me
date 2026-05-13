# Neely Core P0 Gate 六檔實測 Runbook

對齊 `m3Spec/neely_core_architecture.md §10.0`(P0 Gate 校準目標)+ §13.3(Degree Ceiling)。

P0 Gate 是 Neely Core 進入 P1 indicator core 之前的硬要求:六檔股票實際 production 資料跑出 forest 後,人工 visual review 校準關鍵常數,寫入 `docs/benchmarks/`。

---

## 1. 前提

- `claude/continue-previous-work-xdKrl` 已合進 user 本機 active branch(P12 commit `e9804f4`)
- 本機 PG 17 + alembic head = `w2x3y4z5a6b7`(structural_snapshots / facts / indicator_values 三表已建)
- Silver `price_*_fwd` 後復權跑過所有六檔
- `rust_compute/target/release/tw_cores` binary 已 build:
  ```bash
  cd rust_compute && cargo build --release -p tw_cores
  ```

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

確認 0 regression,然後 bump neely_core version 0.19.0 → 1.0.0(spec §10.0 P0 Gate 通過後才能 bump 主版本)。

之後可進 P1 indicator core(trendline_core / support_resistance_core / divergence_core 等)。
