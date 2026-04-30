# Collectors 操作參考（Collector / API 為單位）

> **版本**：v2.0（PG 遷移後 / Phase 1 收尾）
> **整理日期**：2026-04-30
> **基準檔**：`config/collector.toml` v1.7 + `src/phase_executor.py` + `src/api_client.py`
> **配套文件**：`docs/schema_reference.md`（以表為單位的 schema 對照）

本文件以 **collector（API）** 為單位組織，回答這些問題：

- 這個 collector 怎麼設定？
- 跑 backfill 跟 incremental 各自的行為？
- 它依賴什麼前置 collector？
- 出錯（schema_mismatch、429、空結果）怎麼處理？
- 新增/移除一個 collector 要動哪些東西？

如果你想知道「某張表長什麼樣 / 哪些欄位 / 對應 API 來源」，請看 `schema_reference.md`。

---

## 1. 整體架構

### 1.1 Phase 排程

Collector 工作分 **7 個 phase（0–6）**，serial 執行，前一 phase 完成才會跑下一個：

| Phase | 名稱 | API 數 | 內容 | 備註 |
|---|---|---|---|---|
| 0 | TRADING_CALENDAR | 1 | 交易日曆 | v2.0 新增；is_backer=true，必須最先跑 |
| 1 | META | 3 | 上市櫃清單、下市清單、加權報酬指數 | Phase 1 完成後重新解析股票清單（先雞後蛋）|
| 2 | EVENTS | 5 | 除權息、減資、分割、面額變更、股利政策（暫存） | dividend_policy 走 post_process 拆權息 + 偵測純現增 |
| 3 | RAW PRICE | 2 | 日 K、漲跌停 | Phase 4 計算後復權的輸入 |
| 4 | RUST 計算 | — | Rust binary 計算後復權 + 週/月 K | 不是 [[api]]，由 `rust_bridge` 呼叫 |
| 5 | CHIP / FUNDAMENTAL | 11 | 法人籌碼、融資融券、外資持股、估值、月營收、財報三表等 | 多數依賴 trading_calendar 過濾鬼資料 |
| 6 | MACRO | 5 | 美股指數、匯率、全市場法人、市場融資維持率、Fear & Greed | 全市場資料，不分股 |

**全部 27 個 [[api]] 條目**（toml 中 `enabled=true` 才會跑）。

### 1.2 共通行為

**斷點續傳**（`src/sync_tracker.py`）：
- 以 `(api_name, stock_id, segment_start)` 為主鍵記錄 `api_sync_progress`
- `status='completed'` 或 `'empty'` 視為已完成，下次跑會 skip
- `status='failed'` 帶錯誤訊息；下次跑會重試
- `status='schema_mismatch'` 入庫但需人工確認

**Rate limit**（`src/rate_limiter.py`，全域共用 token bucket）：
- 1600 calls/hour、burst 5、最小間隔 2250ms
- 收到 429 → cooldown 120s 並清空 token

**Retry**（`src/api_client.py`）：
- 預設 3 次；指數退避（base 5s, max 60s）
- 遇 429 / 500 / 502 / 503 / 504 才重試

**Date segment 切分**（`src/date_segmenter.py`）：
- `segment_days=0` → 單段 `[backfill_start, today]`（適用月營收、財報、stock_info 等不分段的 API）
- `segment_days=N` → 按 N 天切段（如 365 表示每次拉一年）
- 財報三表（`pack_financial`）incremental 模式有額外 lookback：因為 API 回的 date 是「會計期間結束日」而非公告日，純看 last_sync+1 會永遠抓不到新公告的舊期報表

### 1.3 設定檔欄位速查

| 欄位 | 必填 | 用途 |
|---|---|---|
| `name` | ✓ | collector 唯一識別（也是 `api_sync_progress.api_name`）|
| `dataset` | ✓ | FinMind dataset 名稱（如 `TaiwanStockPrice`）|
| `param_mode` | ✓ | `all_market` / `all_market_no_id` / `per_stock` / `per_stock_no_end` / `per_stock_fixed` |
| `target_table` | ✓ | DB 寫入目標表 |
| `phase` | ✓ | 0–6 |
| `enabled` | ✓ | false 時跳過 |
| `is_backer` | ✓ | 是否為「不可缺」的關鍵 collector（影響 backfill 失敗時的重要性判斷）|
| `segment_days` | ✓ | 日期分段，見 1.2 |
| `fixed_ids` | | per_stock_fixed 模式的固定 id 清單（如 `["TAIEX","TPEx"]`）|
| `field_rename` | | API 原欄位 → DB 欄位的 rename mapping |
| `detail_fields` | | 需要進 `detail` JSONB 的欄位（一般以 `_` 開頭的 rename 結果）|
| `aggregation` | | aggregator 名稱（pivot_institutional / pack_holding_shares 等）|
| `event_type` | | Phase 2 events 表共用，用此區分 |
| `merge_strategy` | | 特殊合併策略（目前只有 `update_delist_date`）|
| `post_process` | | 後處理 hook（目前只有 `dividend_policy_merge`）|
| `stmt_type` | | 財報三表的 type 區分（income/balance/cashflow）|
| `backfill_start_override` | | 個別覆寫起始日（如 `stock_delisting` 從 2022-01-01 開始夠用）|

---

## 2. Collector 詳細表

每個 collector 一段，依 phase 排序。**「依賴」表示前置 phase 必須先跑完才能跑這個**。

### Phase 0：TRADING_CALENDAR

#### `trading_calendar`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockTradingDate` |
| param_mode | `all_market` |
| target_table | `trading_calendar` |
| segment_days | 0（單段全抓）|
| is_backer | **true** |

**用途**：建立 2019-01-01 起的台股交易日曆（單欄 `date`）。

**為什麼是 Phase 0**：Phase 5 的 `pivot_institutional` aggregator 會用 `trading_calendar` 過濾 FinMind 偶爾在週六回的鬼資料（已知 FinMind 行為）。如果這張表是空的，aggregator 會跳過過濾並 log warning。

**依賴**：無。

**頻率**：每日跑（incremental 模式僅補上次同步後的新交易日）。

**錯誤處理**：API 返空結果代表這段時間沒新交易日，正常；status 標 `empty` 即可。

---

### Phase 1：META

#### `stock_info`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockInfo` |
| param_mode | `all_market`（不需 data_id）|
| target_table | `stock_info` |
| segment_days | 0 |
| is_backer | false |
| field_rename | `type→market_type`、`industry_category→industry`、`date→updated_at` |

**用途**：上市櫃所有股票的基本資料（代號、名稱、產業、面額、上市日）。

**API 細節**：API 回的 `date` 欄位是「資料更新日」，被 rename 成 `updated_at`。

**依賴**：無。

**Phase 1 完成後**：`phase_executor` 會重新呼叫 `stock_resolver.resolve()` 從本表撈出待處理股票清單，供 Phase 2 起使用（解決首次執行時 stock_info 為空的先雞後蛋問題）。

---

#### `stock_delisting`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockDelisting` |
| param_mode | `all_market` |
| target_table | `stock_info`（合併寫入）|
| segment_days | 0 |
| backfill_start_override | `2022-01-01` |
| merge_strategy | **`update_delist_date`** |

**用途**：抓下市櫃清單，**合併寫入** `stock_info.delist_date` 欄位（不是新建一列）。

**特殊**：`merge_strategy=update_delist_date` 觸發 `phase_executor._merge_delist_date`，對每筆資料跑 `UPDATE stock_info SET delist_date=...`，而非 upsert。

**依賴**：建議在 `stock_info` 之後跑（同 Phase）。

---

#### `market_index_tw`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockTotalReturnIndex` |
| param_mode | `per_stock_fixed` |
| target_table | `market_index_tw` |
| fixed_ids | `["TAIEX", "TPEx"]` |
| segment_days | 365 |

**用途**：加權報酬指數 + 櫃買報酬指數。

**注意**：API 回 `(date, stock_id, price)` 三欄；DB schema PK 改為 `(market, stock_id, date)` 適配。

---

### Phase 2：EVENTS

#### `dividend_result`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockDividendResult` |
| param_mode | `per_stock` |
| target_table | `price_adjustment_events` |
| event_type | `dividend` |
| backfill_start_override | `2022-01-01` |

**用途**：除權息結果。直接寫進 `price_adjustment_events` with `event_type='dividend'`。

**邊角**：API 偶爾回「權息」混合事件（cash + stock 一起），field_mapper 此時會把 `cash_dividend / stock_dividend` 設為 NULL，**等 `dividend_policy_merge` 後處理補齊**。

---

#### `dividend_policy`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockDividend` |
| param_mode | `per_stock_no_end`（沒有 end_date 參數） |
| target_table | `_dividend_policy_staging`（暫存） |
| post_process | **`dividend_policy_merge`** |

**用途**：抓股利政策原始資料（21 個 PascalCase 欄位）進暫存表，phase 結束後跑 `dividend_policy_merge`：

1. **Step 1**：找 `price_adjustment_events` 中 `cash_dividend IS NULL AND stock_dividend IS NULL` 的權息混合事件，從暫存表查對應日期的明細，補齊兩個欄位。
2. **Step 2**：找暫存表中有現金增資但 `price_adjustment_events` 無對應日期的純現增事件，新增 `event_type='capital_increase'`，AF 暫填 1.0 等 Rust Phase 4 補算。

**依賴**：必須在 `dividend_result` 之後跑（dividend_result 的權息事件要 dividend_policy 的 staging 才能拆）。

---

#### `capital_reduction`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockCapitalReductionReferencePrice` |
| param_mode | `per_stock` |
| event_type | `capital_reduction` |
| backfill_start_override | `2020-01-01` |

**用途**：減資參考價（before_price / after_price / reference_price）。

---

#### `split_price`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockSplitPrice` |
| param_mode | `all_market_no_id`（無 data_id）|
| event_type | `split` |
| segment_days | 365 |

**用途**：股票分割參考價。**全市場一次抓，不分股**。

---

#### `par_value_change`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockParValueChange` |
| param_mode | `all_market_no_id` |
| event_type | `par_value_change` |
| segment_days | 365 |

**用途**：面額變更。全市場一次抓。

---

### Phase 3：RAW PRICE

#### `price_daily`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockPrice` |
| param_mode | `per_stock` |
| target_table | `price_daily` |
| segment_days | 365 |
| field_rename | `max→high`、`min→low`、`Trading_Volume→volume`、`Trading_turnover→turnover` |

**用途**：日 K 原始價格。**這是除權息斷點價，做時序回測請改用 `price_daily_fwd`**（Rust Phase 4 算出）。

**踩過的坑**：v1.2 / v1.3 缺 field_rename，導致 `max` / `min` / `Trading_Volume` 被 schema 過濾後靜默丟掉。v1.4 補回。

---

#### `price_limit`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockPriceLimit` |
| param_mode | `per_stock` |
| target_table | `price_limit` |

**用途**：每日漲跌停價。

---

### Phase 4：RUST 計算

不是 [[api]] 條目。由 `phase_executor._run_phase4` → `rust_bridge.run_phase4` 呼叫 Rust binary `rust_compute/target/release/tw_stock_compute`。

**Rust binary 做的事**：

1. 補算 `price_adjustment_events.event_type='capital_increase'` 的 `adjustment_factor`
2. 後復權計算：`price_daily` + AF → `price_daily_fwd`
3. 週 K 聚合：`price_daily_fwd` → `price_weekly_fwd`（ISO week）
4. 月 K 聚合：`price_daily_fwd` → `price_monthly_fwd`（calendar month）
5. 更新 `stock_sync_status.fwd_adj_valid=1`

**CLI**（v2.0）：`tw_stock_compute --database-url <pg_url> --mode <backfill|incremental> [--stocks <comma_list>]`

**EXPECTED_SCHEMA_VERSION = "2.0"**（在 `src/rust_bridge.py` 與 `rust_compute/src/main.rs` 都要對齊；不對齊會 log warning 並提示重建）。

**依賴**：Phase 3 必須完成（沒 raw price 就沒輸入）；Phase 2 的 `price_adjustment_events` 也要齊。

**踩過的坑**：v1 → v2 切換時 release binary 沒重編，導致跑舊 SQLite 版執行檔，`processed=N` 但 `price_daily_fwd` 為空。修法是 commit `0ba3b5f`。**將來改 Rust 邏輯記得 `cargo build --release`**。

---

### Phase 5：CHIP / FUNDAMENTAL（11 個 collector）

> 多數 per-stock，segment_days=365；非交易日鬼資料由 `trading_calendar`（Phase 0）過濾。

#### `institutional_daily`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockInstitutionalInvestorsBuySell` |
| aggregation | **`pivot_institutional`** |

**用途**：三大法人買賣超 per-stock。API 每日 5 列（外資 / 外資自營 / 投信 / 自營 / 自營避險）→ pivot 成 1 列 10 個欄位。

**依賴 Phase 0**：aggregator 用 `trading_calendar` 過濾 FinMind 在週六回的非交易日鬼資料（`['2019-08-24', '2019-10-26']` 等已知）。Phase 0 沒跑會 log warning「trading_dates 為空」並跳過過濾。

---

#### `margin_daily`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockMarginPurchaseShortSale` |
| field_rename | 16 個欄位 → 6 個直存 + 8 個入 detail JSON |

**用途**：融資融券。

---

#### `foreign_holding`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockShareholding` |

**用途**：外資持股。`foreign_holding_ratio` 單位是 % (0–100)。

---

#### `holding_shares_per`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockHoldingSharesPer` |
| aggregation | **`pack_holding_shares`** |

**用途**：股權分散表。每日多筆級距 pack 成 1 筆 detail JSON。

**頻率**：API 約每週末更新一次；實測單股 ~377 筆/年（接近每週一次）。

---

#### `valuation_daily`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockPER` |

**用途**：本益比 / 殖利率 / 淨值比。

---

#### `day_trading`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockDayTrading` |

**用途**：當沖。

**踩過的坑**：v1.2 把 `BuyAfterSale → day_trading_buy`（語意錯：BuyAfterSale 是 string「可否當沖旗標」），v1.4 改成 `BuyAmount → day_trading_buy`（金額元）。**`day_trading_buy/sell` 是金額不是筆數**。

---

#### `index_weight`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockMarketValueWeight` |

**用途**：指數成分權重。

**特性**：FinMind 對較舊年份回空，本檔在 Phase 1 收尾測試發現 2019–2022 多為 empty，2023 起才有資料。屬資料屬性，非 bug。

---

#### `monthly_revenue`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockMonthRevenue` |
| segment_days | 0（單段全抓）|

**用途**：月營收 + 月增 (`revenue_mom`) + 年增 (`revenue_yoy`)。

**頻率**：月更，每月 10 號前公告。

---

#### `financial_income` / `financial_balance` / `financial_cashflow`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockFinancialStatements` / `TaiwanStockBalanceSheet` / `TaiwanStockCashFlowsStatement` |
| aggregation | **`pack_financial`** |
| stmt_type | `income` / `balance` / `cashflow` |
| segment_days | 0（單段全抓）|

**用途**：財報三表。三個 collector 共用 `financial_statement` 表，靠 `type` 欄位區分。

**邊角**：date 是「會計期間結束日」（季末），不是公告日。incremental 模式有 lookback 補抓上一季尚未公告完的舊期報表。

**Schema validation**：API 每次都回兩個 novel field（`origin_name`、`value`），這是因為 aggregator 用 pack 把多列合一，原始 row 級的這兩個欄位不會直存。本 session 的去重快取確保警告只印一次。

---

### Phase 6：MACRO（5 個 collector）

#### `market_index_us`

| 設定 | 值 |
|---|---|
| dataset | `USStockPrice` |
| param_mode | `per_stock_fixed` |
| fixed_ids | `["SPY", "^VIX"]` |

**用途**：S&P 500 ETF + VIX 指數。注意 `^VIX` 是大寫。

---

#### `exchange_rate`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanExchangeRate` |
| param_mode | `per_stock_fixed` |
| fixed_ids | **`["USD", "CNY", "JPY", "AUD"]`**（v2.0 由 19 收斂為 4） |
| segment_days | 365 |

**用途**：台幣匯率。

**為什麼必須帶 data_id**：TaiwanExchangeRate 不帶 data_id 就只會回每幣 3 個日期的縮水資料；帶了才會回完整時序。

**v2.0 變動**：v1.6 是 19 幣別（AUD CAD CHF CNY EUR GBP HKD IDR JPY KRW MYR NZD PHP SEK SGD THB USD VND ZAR），實測長期看 90%+ 用量集中在 USD/CNY/JPY/AUD，其餘幣別帶來的 API 起叫與 rate-limit 壓力 vs 實用性不成比例，故收斂。需要其他幣別時加回 fixed_ids 並重跑此 collector 即可。

---

#### `institutional_market`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanStockTotalInstitutionalInvestors` |
| param_mode | `all_market` |
| aggregation | **`pivot_institutional_market`** |

**用途**：全市場三大法人。同 institutional_daily 但無 stock_id。

---

#### `market_margin`

| 設定 | 值 |
|---|---|
| dataset | `TaiwanTotalExchangeMarginMaintenance` |
| param_mode | `all_market` |

**用途**：整體市場融資維持率（%，如 165.32）。

---

#### `fear_greed`

| 設定 | 值 |
|---|---|
| dataset | `CnnFearGreedIndex` |
| param_mode | `all_market` |

**用途**：CNN 恐懼貪婪指數（0–100 + label）。

---

## 3. 常見運維任務

### 3.1 新增一個 collector

1. 在 `config/collector.toml` 加一個 `[[api]]` 條目，至少填齊必填欄位（見 1.3）。
2. 確認 DB 已有對應 `target_table`（沒有的話開 alembic migration）。
3. 確認 `field_rename` 涵蓋 API 回傳的所有有意義欄位；schema validation 會在執行時檢查。
4. 跑單股 dry-run 驗證：`python src/main.py --verbose phase 5 --stocks 2330 --dry-run`。
5. 真跑：`python src/main.py phase 5 --stocks 2330`。
6. 用 `python scripts/check_all_tables.py` 確認筆數合理。

### 3.2 重跑某個 collector

如果你發現某個 collector 結果有問題，要它重新拉：

```powershell
# 方法 1：刪 api_sync_progress 對應記錄，下次跑會重做
psql -d twstock -c "DELETE FROM api_sync_progress WHERE api_name='institutional_daily' AND stock_id='2330'"

# 方法 2：刪整張表（慎用）+ progress
python scripts/drop_table.py institutional_daily   # 會連帶刪 api_sync_progress
```

### 3.3 處理失敗的 segment

```powershell
# 看哪些 segment 失敗
python src/main.py status

# 直接重跑會自動重試 status='failed' 的 segment
python src/main.py backfill --stocks 2330
```

### 3.4 處理 schema_mismatch

代表 API 回的欄位跟 `field_rename` 定義不符。可能是 FinMind 改 API。

1. 看 log 找 novel fields：`[SchemaValidation] <api>: novel fields detected: {...}`
2. 評估那些欄位是否需要：
   - 不需要 → 加進 `detail_fields` 或忽略（會自動進 detail JSONB 或被 schema 過濾掉）
   - 需要 → 補 `field_rename` 並考慮 alembic migration 加新欄位

### 3.5 incremental 模式

```powershell
# 跑增量同步（從 api_sync_progress 的最後 segment_end 起算）
python src/main.py incremental
```

財報三表會自動帶 lookback；其他 API 從 last_sync+1 起算。

### 3.6 切換 SQLite debug 模式（罕用）

```powershell
$env:TWSTOCK_USE_SQLITE="1"
$env:SQLITE_PATH="data/tw_stock_dev.db"
python src/main.py backfill --stocks 2330
```

注意：SQLite fallback 不維護新功能，僅供 CI / 離線 debug。Production 走 Postgres。

---

## 4. 已知 FinMind 行為

整理測試過程中遇到的 FinMind API 邊角：

1. **TaiwanExchangeRate 不帶 data_id 只回每幣 3 個日期**：見 `exchange_rate` 章節。
2. **TaiwanStockInstitutionalInvestorsBuySell 在週六回鬼資料**：實測 `2019-08-24`、`2019-10-26` 等週六日有資料，由 `pivot_institutional` aggregator 用 trading_calendar 過濾。
3. **TaiwanStockDividendResult 的「權息」混合事件**：cash + stock 同時除權息時，需 `dividend_policy_merge` 從股利政策原文補齊兩個欄位。
4. **TaiwanStockMarketValueWeight 較舊年份多為空**：實測 2019–2022 個股 weight 多為 empty，2023 起穩定有資料。
5. **TaiwanStockFinancialStatements 三表的 date 是會計期間結束日**：incremental 模式必須有 lookback。
6. **TaiwanStockDividend 有 21 個欄位且含 PascalCase typo**：例如 `CashIncreaseSubscriptionpRrice`（sic），原樣保留作為 detail JSON key。

---

## 5. 相關檔案速覽

| 檔案 | 角色 |
|---|---|
| `config/collector.toml` | API 設定中央表 |
| `src/main.py` | CLI 進入點（backfill / incremental / phase / status / validate）|
| `src/phase_executor.py` | Phase 排程引擎 |
| `src/api_client.py` | FinMind HTTP client + retry |
| `src/rate_limiter.py` | 全域 token bucket |
| `src/date_segmenter.py` | segment 切分邏輯 |
| `src/field_mapper.py` | rename / detail / computed_fields / schema validation |
| `src/sync_tracker.py` | api_sync_progress 斷點續傳追蹤 |
| `src/post_process.py` | dividend_policy_merge 後處理 |
| `src/db.py` | DBWriter（Postgres + SQLite fallback）|
| `src/rust_bridge.py` | Phase 4 Rust binary 橋接 |
| `rust_compute/src/main.rs` | Rust 後復權 + 週/月 K 聚合 |
| `scripts/check_all_tables.py` | 22 張表筆數體檢 |
| `scripts/drop_table.py` | 刪表 + 連帶刪 api_sync_progress |
| `docs/schema_reference.md` | 以表為單位的 schema 對照（搭配本文件閱讀）|

---

## 6. 變更歷史

- **v2.0（2026-04-30）** — 本次首版。配合 SQLite → PG 遷移、Phase 0 抽出、exchange_rate 收斂為 4 幣別、Rust binary 升 v0.2.0 後整理。
- v1.x 的 SQLite 階段沒有獨立的 collectors.md，相關內容散在 `schema_reference.md` 與 collector.toml 註解內。
