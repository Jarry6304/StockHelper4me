# A-V3 Spot-Check 操作說明

> **任務**:r2-1 動工 — 解 P0-2 (A-V3) + 連動 P0-8 (C1) volume 合併與後復權順序
> **執行者**:user(本機 PG 17,sandbox 連不到)
> **配套檔案**:`scripts/av3_spot_check.sql`

---

## 背景:為什麼需要這個 spot-check

`indicator_cores_volume.md §2.4` 假定 `price_daily_fwd.volume` 已隨除權息調整。但實際上 collector 端有**兩個矛盾邏輯**:

| 來源 | 邏輯 |
|---|---|
| `src/field_mapper.py:194-203` | **純現金 dividend** 時 `volume_factor = 1.0`(volume **不動**)|
| `rust_compute/src/main.rs:447` | **所有事件**統一 `fwd_volume = raw_volume / multiplier` |

`field_mapper` 寫 `volume_factor` 進 `price_adjustment_events.volume_factor` 欄位,但 **Rust 完全沒讀** 這欄,只用 `adjustment_factor`。所以實際 DB 行為由 Rust 決定。

A-V3 要驗證的是**實際 DB 結果到底走哪派**:

| 派系 | close 後復權 | volume 後復權 | dollar_vol 守恆 | 反映實際流動性 |
|---|---|---|---|---|
| (a) 學術派 / Rust 派 | × AF | / AF | ✓ | ✗ |
| (b) 實務派 / field_mapper 派 | × AF | 不動(現金 dividend)/ AF(股本變動)| ✗(現金日)| ✓ |
| (c) 純粹派 | × AF | 完全不動 | ✗ | ✓ |

---

## 執行步驟(PowerShell)

```powershell
cd C:\Users\jarry\source\repos\StockHelper4me
psql $env:DATABASE_URL -f scripts\av3_spot_check.sql > av3_result.txt 2>&1
```

或互動執行(看 `\echo` 標題):

```powershell
psql $env:DATABASE_URL -f scripts\av3_spot_check.sql
```

執行時間:< 1 秒(全是 indexed 查詢)。

完成後請把 `av3_result.txt` 全文貼回對話。

---

## 判讀對照表(我會看到結果再給結論,但你可以先自己看)

### Test 1 — 2330 黃金日

對 2019-01-02 / 2022-03-15 / 2022-03-16 三天(都有 close_ratio ≠ 1.0):

| vol_ratio 結果 | 派系 | 對應動作 |
|---|---|---|
| ≈ 1.0(差 < 0.5%)| 實務/純粹派 | blueprint §4.4 ALTER **必走**,加 `volume_adjusted` 欄位讓 Rust 算另一版 |
| ≈ 1/close_ratio(dollar_vol_preserved ≈ 1.0)| Rust 派 | blueprint §4.4 ALTER **不需** volume_adjusted,但需加 `cumulative_adjustment_factor` 給 Wave Cores 用 |
| 介於兩者間 / 不一致 | WEIRD | 先解 Test 6 sanity,再 deeper investigation |

對 2026-04-24(最新日),vol_ratio 一定要 = 1.0 (Test 6 也驗一次)。

### Test 2 — 2330 全部除權息事件

每一列 verdict 欄會直接告訴你「派系判定」。

關鍵看點:
- **若 2330 大部分是現金 dividend(stock_dividend = 0),且 vol_ratio < 1.0**:
  確認 **Rust 派**(field_mapper 的 volume_factor 邏輯被 Rust 蓋過)
- **若 vol_ratio = 1.0 而 close_ratio > 1**:
  確認 **實務派**(現金 dividend volume 不動)

### Test 3 — 全市場 stock_dividend > 0 事件

純股票股利的事件,**兩派都同意 volume 該調**。
- 若 vol_ratio < 1.0 ✓:正常
- 若 vol_ratio = 1.0 ✗:嚴重 bug,連股本變動都沒動 volume

預期會看到大概 vol_ratio ≈ 1/(1 + stock_dividend/10) — 例如 stock_dividend = 0.5 元 → vol_ratio ≈ 0.95。

### Test 4 — split / capital_reduction / capital_increase 事件

跟 Test 3 同理,股本變動事件 volume 一定該變。
- 若有 capital_increase 行 vol_ratio = 1.0:**Rust patch_capital_increase_af 可能 bug**
- 對減資(capital_reduction):AF < 1,vol_ratio > 1(volume 變多,反映減資後股數變少回推等值股數)

### Test 5 — adjustment_factor vs volume_factor 一致性

Output 是 group by event_type 的計數:
- 若所有 event_type 的 `af_diff_vf_count = 0`:兩欄永遠相等 → `volume_factor` 欄完全冗餘,可砍
- 若 `event_type='dividend'` 行 `af_diff_vf_count > 0`:
  - field_mapper 對現金 dividend 寫 vf=1.0、AF=fwd/raw 比;確認 collector 有區分但 Rust 沒讀
  - 這是設計斷裂的硬證據,P0-8/C1 的 spec 修正必須提

### Test 6 — Sanity

最新日 vol_ratio + close_ratio 必須都 ≈ 1.0,否則 Rust 一定有 bug。

---

## 我會根據結果決定的事(等你貼結果)

1. **A-V3 verdict 定案**(學術派 / 實務派 / 純粹派 / WEIRD)
2. **blueprint `m2Spec/collector_rust_restructure_blueprint_v3_2.md` §四 4.4 條件 ALTER**:
   - 若 Rust 派:刪掉「ALTER price_daily_fwd 加 volume_adjusted」這條,改加 `cumulative_adjustment_factor`
   - 若實務派:保留 ALTER 但改成「加 volume_adjusted_strict」(Rust 派的 volume),原 volume 欄位保留(實務派的 volume)
   - 若 WEIRD:暫停,先解 Rust bug
3. **r3 §三 修正 P0-2 動作**更新為實機驗證後的具體連動
4. **r3 §三 修正 P0-8(C1)**更新 tw_market_core §五 5.1 的「合併與後復權順序」具體分支寫法
5. **collector 改動清單**:
   - 是否砍 `volume_factor` 欄位?(若 Test 5 顯示永遠 = AF,可砍)
   - 是否改 Rust 用 volume_factor 而非 AF?(若決議走實務派)
   - 是否更新 `field_mapper.py:194-203`?(若決議走 Rust 派,刪這段)

---

## 邊界 / 已知限制

1. 若你的本機 PG 17 沒跑過 Phase 4(price_daily_fwd 為空):Test 1-4 會 join 不到結果。先確認 `SELECT COUNT(*) FROM price_daily_fwd WHERE stock_id='2330';` > 0
2. 若你的 2330 資料只到 2024 年(沒到 2026-04-24):Test 1 第 4 列會空。把 2026-04-24 改成 `(SELECT MAX(date) FROM price_daily WHERE stock_id='2330')` 即可,或忽略該列
3. 若 PG 沒安裝 `psql` CLI:可改用 Python `psycopg.connect` + `cursor.execute(open(...).read())` 跑;但一般 PG 17 server 都有 client tool
4. 整數 round 累積誤差可能讓某些舊日子 verdict 邊界不清(close_ratio 跟 1/vol_ratio 差 0.1%)— 容忍度 SQL 已設 0.5%
