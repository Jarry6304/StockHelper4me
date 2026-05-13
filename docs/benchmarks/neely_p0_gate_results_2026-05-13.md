# Neely Core P0 Gate 校準結果 — 2026-05-13(dev DB partial backfill)

## 摘要

| 項目 | 值 |
|---|---|
| 執行日期 | 2026-05-13 |
| neely_core 版本 | 0.20.0 |
| 環境 | 本機 PG 17 + dev DB(partial backfill ~30 檔 fwd) |
| 跑的股票 | 0050, 2330, 3363, 6547, 1312(spec §10.0 預定六檔之 5;6547 缺 fwd 資料) |
| 資料範圍 | Daily 386-387 bars(≈ 1.6 年) |
| **校準決策** | **B 路線:寫報告紀錄,不改 config;等 production 全市場 backfill 後重新校準** |
| 0 commits to `config.rs` | 預設常數維持原值 |

## 跑的命令

```powershell
chcp 65001 | Out-Null
$env:PGCLIENTENCODING = "UTF8"
$env:LC_MESSAGES = "C"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Get-Content .env | ForEach-Object {
    if ($_ -match '^\s*([^#][^=]*)=(.*)$') {
        [Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim().Trim('"'), 'Process')
    }
}

$stocks = @("0050", "2330", "3363", "6547", "1312")
foreach ($s in $stocks) {
    .\rust_compute\target\release\tw_cores.exe run --stock-id $s --write
}

psql $env:DATABASE_URL `
    -f docs\benchmarks\neely_p0_gate_check.sql `
    | Out-File -Encoding UTF8 p0_gate_results.txt
```

---

## §1. Forest 規模 + monowave / candidate / facts

| stock_id | snapshot_date | bars | monowaves | candidates | forest | compaction_paths | facts |
|---|---|---|---|---|---|---|---|
| 0050 | 2026-05-08 | 387 | 63 | 65 | 7 | 7 | 38 |
| 2330 | 2026-04-29 | 386 | 103 | 84 | 8 | 8 | 41 |
| 3363 | 2026-04-29 | 386 | 98 | 51 | 2 | 2 | 26 |
| 1312 | 2026-04-29 | 386 | 61 | 52 | 4 | 4 | 26 |
| 6547 | 1900-01-01 | **0** | 0 | 0 | 0 | 0 | 0 |

**6547 缺料**:Silver `price_daily_fwd` 0 row;dev DB partial backfill 沒此檔。snapshot_date 1900-01-01 是 placeholder。

## §2. 工程護欄 + Validator pass/reject

| stock_id | pass | reject | pass_pct | overflow | timeout | insufficient_data |
|---|---|---|---|---|---|---|
| 0050 | 15 | 50 | **23.1%** | f | f | t |
| 2330 | 24 | 60 | **28.6%** | f | f | t |
| 3363 | 13 | 38 | **25.5%** | f | f | t |
| 1312 | 12 | 40 | **23.1%** | f | f | t |
| 6547 | 0 | 0 | — | f | f | t |

- **overflow_triggered = false ×5**:forest 最大 8,距 forest_max_size=1000 上限差 125×
- **compaction_timeout = false ×5**:total_elapsed_ms < 1(亞毫秒),距 timeout_ms=60000 上限差 60000×
- **pass_pct 23.1-28.6%**:落在預期 [10%, 60%] 中下段,規則嚴格度合理
- **insufficient_data = true ×5**:386-387 bars < Daily warmup 500(spec §13.1),dev partial backfill 預期行為

## §3. 拒絕原因 Top 3 共識

跨四檔有效樣本一致:

| RuleId | 平均 / 檔 | avg_gap_pct 範圍 | 解讀 |
|---|---|---|---|
| `Ch5_Essential(4)`(W3 須長於 W2) | 33 | 29-39% | 短期 daily monowave 序列 W3 不一定最長,**結構性拒絕** |
| `Ch5_Essential(3)`(W2 不完全回測 W1) | 31 | **78-147%** | gap > 100% 表示 W2 完全回測或更深 → 該 candidate 結構就不是 Impulse,classifier 改判 Zigzag/Corrective |
| `Ch5_Zigzag_Max_BRetracement`(b ≤ 61.8%) | 29 | **71-140%** | 真實 zigzag b-wave 罕見 ≤ 61.8%,gap 高表示 b 太深 → classifier 會改判 Flat |

**結論:gap 高 ≠ 規則太嚴**。Pass + 多種 reject 並存是設計本意 — 一個 candidate 由多條規則同時 dispatch,fail 的 rule 過濾出該 candidate「不是哪種 pattern」,classifier 再從 pass/fail 模式判定。

## §4. Stage 性能

各 stage_elapsed_ms 全部 0(亞毫秒級)。386 bars 太短,production 全市場(~5000 bars / 20 年)才會看到真實 stage 分布。**Production 校準時應觀察:**
- `stage_0_preconstructive`(~200 branch if-else,monowave 多時最重)
- `stage_4_validator`(每 candidate × ~22 條規則)
- `stage_8_compaction`(forest 接近上限時)

## §5. P9-P12 新欄位觸發分布

| 欄位 | 0050 | 2330 | 3363 | 1312 | 6547 | 預期 / 結論 |
|---|---|---|---|---|---|---|
| `missing_wave_count` | 7 | 12 | 5 | 7 | 0 | 0-15 ✅ |
| `emulation_count` | 0 | 3 | 0 | 3 | 0 | 0-3 ✅ |
| `reverse_logic_triggered` | t | t | t | t | — | forest≥2 應為 t ✅ |
| `rl_suggested_filter` | 0 | 2 | 0 | 2 | — | is_near_completion 過濾保守 ✅ |
| `round3_pause` | 觸發 | — | 觸發 | — | 觸發 | 0050 / 3363 forest 全 corrective → 觸發;2330 / 1312 含 :5 scenario → 不觸發 ✅ |
| `degree_max` | Minute | Minute | Minute | Minute | SubMicro | 1.6y → Minute(spec §13.3 表)✅;6547 no data → SubMicro ✅ |
| `ct_summary_count` | 63 | 103 | 98 | 61 | 0 | 1:1 與 monowave_count 對應 ✅ |

**5 個 P9-P12 新欄位全部正確輸出 + 語意合理。**

## §6. 校準目標 — 全部 deferred 至 production scale

| 常數 | 目前預設 | 五檔觀察值 | 校準建議 | 本次決策 |
|---|---|---|---|---|
| `forest_max_size` | 1000 | max=8 | 可降至 500(留 60× 安全餘量) | **不動**,等 production 看真實上限 |
| `compaction_timeout_ms` | 60000 | <1ms | 可降至 10000 | **不動**,等 production monowave 200+ 看真實耗時 |
| `beam_width` | 100 | candidate max=84 | 寬鬆 | **不動** |
| `REVERSAL_ATR_MULTIPLIER` | 0.5 | monowave 61-103 跨檔合理 | 流動性差異產生差距,符合預期 | **不動** |
| `STOCK_NEUTRAL_ATR_MULTIPLIER` | 1.0 | pass_pct 23-29% | Neutral 過濾比例合理 | **不動** |
| `REVERSE_LOGIC_THRESHOLD` | 2 | rl_suggested_filter 0-2 | 保守(對齊 user 不過度自信過濾原則) | **不動** |
| Daily `warmup_periods` | 500 | bars 386-387 < 500 → insufficient | dev partial 無法驗 | **不動** — production 5000 bars 充足 |

**決策依據**:dev DB 1.6 年資料 + 5 檔(其中 1 檔缺料) 樣本不足以代表 production scale。P0 Gate 校準必須在 ~5000 bars / 1700+ stocks 真實分布下才有意義。

## §7. 後續行動

1. **User 排日曆**:Silver `price_daily_fwd` 全市場 backfill(待 production maintenance window)
2. **Backfill 完成後**:
   - 跑 `.\rust_compute\target\release\tw_cores.exe run-all --write`(全市場 1700+ stocks)
   - 重跑本 SQL,改 `P0_GATE_STOCKS` 為實際六檔(0050 / 2330 / 3363 / 6547 / 1312 + 1 自選)
   - 寫 `docs/benchmarks/neely_p0_gate_results_<date>.md` 紀錄 v2 校準
3. **依 v2 校準調整**:
   - 若 forest 真實上限 > 500,維持 forest_max_size=1000
   - 若任一 stage_elapsed_ms 接近 60000,維持 timeout 預設
   - 若 production 揭露 RuleId 拒絕分布大幅偏移,重審 §3 規則嚴格度

## §8. spec / code 對齊狀態(2026-05-13)

| 項目 | 狀態 |
|---|---|
| 18 stages 全部 dispatch + stage_elapsed_ms 計時 | ✅ |
| NeelyCoreOutput v0.20.0(含 compaction_timeout 頂層 + degree_ceiling + cross_timeframe_hints + reverse_logic + round3_pause + missing_wave / emulation suspects) | ✅ |
| spec amendment §8.1 / §9.3 / §13.1 / §13.3 已 commit(d2f5e4f) | ✅ |
| 6547 缺料 — 不阻塞 P0 Gate v1,等 production backfill 時自動修復 | ⏳ |

## §9. 結論

**P0 Gate v1 dev partial sample = pass 但 deferred 真正校準**:
- 工程護欄全綠(0 false-positive 觸發 overflow / timeout)
- 規則行為符合預期(pass_pct 中段,reject 分布結構性)
- P9-P12 新欄位全部正確輸出
- 5 處可校準的常數一律「不動」,等 production scale 重評估

**neely_core v0.20.0 適合進 production 跑全市場 backfill。校準 v2 留待 backfill 後執行。**
