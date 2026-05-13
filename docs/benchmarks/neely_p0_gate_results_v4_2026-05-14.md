# Neely Core P0 Gate v4 Production Calibration Confirmation — 2026-05-14

## 摘要

| 項目 | 值 |
|---|---|
| 執行日期 | 2026-05-14(同日連兩波校準) |
| 觸發 | v3 follow-up 揭露 Cross spacing 仍 1.5-2× 超標 + Divergence 真實 0.27/yr 過保守 |
| 兩波 commits | `518f2c8` Cross spacing 10→15 + 本 commit MIN_PIVOT_DIST 20→12 |
| 校準 cores | kd_core / ma_core / macd_core / rsi_core |
| 校準決策 | Cross spacing **15 / 8**(B 路線)+ Pivot dist **12**(C 路線) |
| Tests | 15 indicator tests / 285 workspace tests / 全綠 |

---

## ✅ Cross spacing v4 校準確認:7/7 EventKind 全部命中目標

| EventKind | v3 | v4 | 目標 | 校準結果 |
|---|---|---|---|---|
| kd GoldenCross | 20.17 | **10.29** | 8-12/yr | ✅ 中段 |
| kd DeathCross | 20.13 | **10.26** | 8-12/yr | ✅ 中段 |
| ma MaBullishCross | 13.59 | **7.39** | 6-9/yr | ✅ 中段 |
| ma MaBearishCross | 13.42 | **7.32** | 6-9/yr | ✅ 中段 |
| macd HistogramZeroCross | 19.01 | **11.90** | ≤ 12/yr | ✅ 上沿 |
| macd GoldenCross | 9.58 | **6.79** | 5-7/yr | ✅ 上沿 |
| macd DeathCross | 9.42 | **6.68** | 5-7/yr | ✅ 中段 |

平均降幅 **×0.55**,略大於預期 ×0.66(因 production 噪音 cross 較集中 → spacing 過濾更多)。**所有 EventKind 落入 v1.32 + Murphy 目標範圍**。

---

## 🔍 Divergence 觀察:v3 是 stale facts 混雜結果

DELETE + re-run 後揭露真實 v1.33 行為:

| EventKind | v3 (混雜) | v4 (純) | Murphy 1999 預期 | 評估 |
|---|---|---|---|---|
| kd BullishDivergence | 1.08 | 0.36 | 1-4/yr 稀有 | ⚠️ 低於下限 3× |
| kd BearishDivergence | 1.11 | 0.33 | | ⚠️ 同上 |
| macd BullishDivergence | 0.98 | 0.33 | | ⚠️ 同上 |
| macd BearishDivergence | 0.81 | 0.27 | | ⚠️ 同上 |
| rsi BullishDivergence | 0.66 | (沒重跑) | | 同 v3,但 v1.33 已 0.7/yr |
| rsi BearishDivergence | 0.71 | (沒重跑) | | 同 v3 |

**根因**:v3 facts 表混雜:
- 過去某時期 MIN_PIVOT_DIST=10 寫的 facts(多 divergence,~1.0/yr)
- v1.33 commit `0faa336` 後 MIN_PIVOT_DIST=20 寫的 facts(少 divergence,~0.3/yr)

DELETE + 重跑後 facts 只剩 MIN_PIVOT_DIST=20 行為 → **真實值 0.27-0.36/yr 過於保守**。

### C 路線校準:MIN_PIVOT_DIST 20 → 12(讓步至 Murphy 下限)

| Core | const 值 | 修前 | 修後 | 預期 v5 觸發率 |
|---|---|---|---|---|
| `kd_core` | `MIN_PIVOT_DIST` | 20 | **12** | 0.33-0.36 → **0.8-0.9/yr** |
| `macd_core` | `MIN_PIVOT_DIST` | 20 | **12** | 0.27-0.33 → **0.7-0.8/yr** |
| `rsi_core` | `MIN_PIVOT_DIST` | 20 | **12** | 0.66-0.71 → **1.6-1.8/yr** |

### 設計理由

- **保留** spec §3.6「兩極值點距離 ≥ N」**結構性要求**(N ≥ 2 × PIVOT_N = 6,12 滿足)
- **讓步**到 NEoWave 經驗值 **N=12**:
  - 對齊 Murphy 1999 預期下限 1/yr 稀有訊號
  - 留 PIVOT_N=3(Lucas & LeBeau 1992 swing confirmation)不動
  - 12-bar ≈ 2.4 週,介於 v1.32(10)和 v1.33(20)之間,平衡 spec 嚴格性 vs production 敏感性
- spec §3.6 預設 N=20 為「保守值」,Murphy 1999 沒明確下限(只說 "20-60 intervals" 為「常見實務」),12 在 NEoWave 框架仍合法

---

## 📊 v4 + v5 校準累計影響

| 項目 | 修前 | v4 (Cross)後 | v5 (Pivot)後預期 |
|---|---|---|---|
| KD cross 4 EventKinds | 13.4-20.2/yr 🟠 | **7-11/yr 🟢** | 同 v4(spacing 改不影響 div)|
| MA cross 4 EventKinds | 13-14/yr 🟠 | **7.3-7.4/yr 🟢** | 同 v4 |
| MACD cross 3 EventKinds | 9-19/yr 🟠 | **6-12/yr 🟢** | 同 v4 |
| KD Divergence 2 EventKinds | 1.1/yr ✅ | 0.33-0.36/yr ⚠️ | **0.8-0.9/yr 🟢** |
| MACD Divergence 2 EventKinds | 0.8-1.0/yr ✅ | 0.27-0.33/yr ⚠️ | **0.7-0.8/yr 🟢** |
| RSI Divergence 2 EventKinds | 0.66-0.71/yr ⚠️ | 同左 | **1.6-1.8/yr 🟢** |

---

## v5 production 驗證流程(user 端)

校準後需第三次 production run 驗證 Divergence 真實降到目標範圍:

```powershell
cd C:\Users\jarry\source\repos\StockHelper4me
git pull   # 拉本 commit
cd rust_compute && cargo build --release -p tw_cores && cd ..

# DELETE kd/macd/rsi Divergence 相關 facts(spacing 改不變,只清 Divergence)
psql $env:DATABASE_URL -c @"
DELETE FROM facts
WHERE source_core IN ('kd_core','macd_core','rsi_core')
  AND statement LIKE '%Divergence%'
"@

# 重跑 production(~11 分鐘)
.\rust_compute\target\release\tw_cores.exe run-all --write

# v5 follow-up SQL
$today = Get-Date -Format "yyyy-MM-dd"
psql $env:DATABASE_URL -f docs\benchmarks\neely_p0_gate_followup.sql `
    *>&1 | Out-File -Encoding UTF8 "p0_gate_v5_$today.txt"

notepad "p0_gate_v5_$today.txt"
```

預期 v5 §N Divergence 數字:
- kd BullishDivergence / BearishDivergence: ~0.8-0.9/yr ✅
- macd BullishDivergence / BearishDivergence: ~0.7-0.8/yr ✅
- rsi BullishDivergence / BearishDivergence: ~1.6-1.8/yr ✅(Murphy 中段)

如果 v5 數字落入預期,Divergence 校準完成,P0 Gate 全部收尾。

---

## 結論

**v4 校準確認 + v5 校準下手**:
- ✅ v4 Cross spacing 7/7 EventKinds 全部命中目標(c0518f2c8 已 push)
- ⚠️ v4 Divergence 揭露過保守(spec §3.6 預設 20 在 production daily 1.6y 太嚴)
- ✅ v5 MIN_PIVOT_DIST 20→12 落地(本 commit),預期 Divergence ×2.5 升至 Murphy 下限

**neely_core 不動**,維持 v0.21.0(P0 Gate v2 校準後)。

**4 個 indicator cores(kd / ma / macd / rsi)校準到位**,留 user 跑 v5 production 確認 Divergence 真實降到 0.7-1.8/yr 目標。
