# Collector + Rust 完整 Schema 對照（資料對接用）

> **版本**：v2.0 / SCHEMA_VERSION=2.0（M1 close + review #3+#4 後快照）
> **整理日期**：2026-04-28（v1.6 SQLite 版）→ 2026-04-30（v2.0 PG 版）→ 2026-04-30（v1.7 review #3+#4 後）
> **分支**：`claude/review-collector-dependencies-n03rE`（PR #6 → `m1/postgres-migration`）
> **alembic head**：`a1b2c3d4e5f6`（baseline `0da6e52171b1` + `progress_status_check_expand`）
> **基準檔**：`alembic/versions/*` + `src/schema_pg.sql` + `config/collector.toml` field_rename + `rust_compute/src/main.rs`
> **盤點口徑**：27 張表（1 schema_metadata + 22 業務表 + 4 系統 / staging 表）。M3（多 core 計算）動工前 collector 端不再變動 schema，本檔即為**對接邊界**。

## v2.0 主要變動（相對 v1.6）

1. **DB 從 SQLite 切到 Postgres**（`postgresql://twstock:twstock@localhost:5432/twstock`）。
   - 文字型別 `TEXT` 多數 → `TEXT` / `VARCHAR` / `DATE`（Postgres 原生）
   - `detail TEXT JSON` 字串欄位 → `detail JSONB`（用 `->>` 取值）
   - 浮點欄位 `REAL` → `NUMERIC` / `DOUBLE PRECISION`
   - 整數欄位 `INTEGER` 可能升 `BIGINT`（看欄位）
   - `updated_at TEXT DEFAULT (datetime('now'))` → `updated_at TIMESTAMPTZ DEFAULT NOW()`
   - 範例：`json_extract(detail, '$.X')` → `detail->>'X'`（→ TEXT）或 `(detail->>'X')::numeric`
   - 確切型別請以 `alembic/versions/*.py` 與 `src/schema_pg.sql` 為準。
2. **trading_calendar 抽出為 Phase 0**（is_backer=true，必須在 Phase 5 institutional aggregator 之前建好）。
3. **exchange_rate 幣別由 19 收斂為 4**（USD / CNY / JPY / AUD）；TaiwanExchangeRate 必須帶 `data_id` 才會回完整時序，不帶就只會回每幣 3 個日期的縮水資料。
4. **Rust binary 升 v0.2.0**：sqlx + Postgres 取代 rusqlite + SQLite，CLI 由 `--db <path>` 改 `--database-url`，從 `DATABASE_URL` 環境變數讀。EXPECTED_SCHEMA_VERSION 升 `"2.0"`。
5. **Phase 1 收尾的真實筆數**（截至 2026-04-30，多股 backfill 自 2019-01-01）：見各表「預估筆數」（多數已對齊實際；少數因 FinMind 資料屬性差幾筆）。

## v1.7 變動（review #3 + #4，2026-04-30）

| 變動點 | 影響 |
|---|---|
| `api_sync_progress.chk_progress_status` CHECK 從 3 種擴成 5 種 | 可寫入 `empty`（API 回空陣列）/ `schema_mismatch`（FieldMapper 偵測欄位漂移）；之前 baseline 漏掉這兩種，會 violate constraint 卡死斷點續傳 |
| `stock_info.detail` JSONB 欄位開始實際使用 | 從 collector.toml field_rename 把 API `date` 改 pack 進 `detail.data_update_date`；`updated_at` 改交給 schema `DEFAULT NOW()` 與 upsert UPDATE 路徑強制 NOW()，語意統一為「同步時刻」 |
| `PostgresWriter._table_pks(table)` 動態查 information_schema | phase_executor / sync_tracker 不再硬編碼 PK 對照；schema 是 single source of truth |
| upsert UPDATE 路徑強制 `updated_at = NOW()` | 兩條 path 對齊（row dict 帶 / 不帶都套）；對沒 updated_at 欄位的表（fwd 三表、stock_sync_status 等）無影響 |
| Phase 4 mode 從 CLI runtime 傳給 `_run_phase4(mode)` | 不再從 `config.execution.mode` 讀；Rust 端 `process_stock(_mode)` 仍刻意忽略（業務理由：multiplier 倒推必須全量重算，partial 邏輯上錯誤） |

通用約定：
- 全表 `market TEXT NOT NULL`：固定 `'TW'`（macro phase 6 的 `market_index_us` 取決資料源）
- 全表 `source TEXT DEFAULT 'finmind'`
- `detail JSONB`：v2.0 為 JSONB（v1.6 為 TEXT JSON 字串）；存 toml 中以 `_` 開頭的次要欄位
- PK 欄位皆隱含 NOT NULL；其餘除非標示否則皆 nullable
- 預估筆數：以 backfill 起始 2019-01-01、單股回算（截至 2026-04-30 約 1776 個交易日）

### 文件 shorthand vs PG 實際型別對照

下面各表「型別」欄為 SQLite shorthand（沿用 v1.6 撰寫風格，便於閱讀），PG 實際型別在 `src/schema_pg.sql` 為準：

| Doc shorthand | PG 實際型別 |
|---|---|
| `TEXT` | `TEXT`（不限長度）|
| `TEXT`（日期）| `DATE` |
| `INTEGER`（小）| `INTEGER` 或 `SMALLINT` |
| `INTEGER`（大／股數／金額）| `BIGINT` |
| `REAL` | `NUMERIC(p,s)`（精度依語意而定，例：價格 `NUMERIC(15,4)`、比率 `NUMERIC(8,4)`）|
| `TEXT JSON` | `JSONB` |
| `TEXT`（updated_at）| `TIMESTAMPTZ DEFAULT NOW()` |

---

## Phase 1 — META（基礎資料）

### `stock_info` （PK: market, stock_id）
- 來源：`TaiwanStockInfo` + `TaiwanStockDelisting`（merge_strategy=`update_delist_date`）
- 更新頻率：每次 phase 1（建議每日）；is_backer=false
- 實測筆數（3052 → review #4 重跑後 4062，2026-04-30）
- 建議 index：PK 已涵蓋

| 欄位 | 型別 | NN | API 來源 / 備註 | 單位/值域 |
|---|---|---|---|---|
| stock_id | TEXT | ✓ | `stock_id` | 4 碼數字（部分指數/特殊代號例外）|
| stock_name | TEXT | | `stock_name` | UTF-8 |
| market_type | TEXT | | `type` rename | `twse` / `otc` / `emerging` |
| industry | TEXT | | `industry_category` rename | 中文產業名 |
| listing_date | DATE | | `listing_date` | `YYYY-MM-DD` |
| delist_date | DATE | | merge from `TaiwanStockDelisting` | 未下市為 NULL |
| par_value | NUMERIC(10,2) | | `par_value` | 元/股，常見 10.0 |
| detail | JSONB | | v1.7 起 pack `data_update_date`（FinMind API `date`，原「資料更新日」）| 例：`{"data_update_date":"2026-04-30"}` |
| updated_at | TIMESTAMPTZ | ✓ | v1.7 起改交給 `DEFAULT NOW()` + upsert UPDATE 強制 NOW() | 「**同步時刻**」（不是 FinMind 標日）|

### `trading_calendar` （PK: market, date）
- 來源：`TaiwanStockTradingDate`，**Phase 0**（必須最先跑）
- is_backer=true；Phase 5 institutional aggregator 依賴本表過濾 FinMind 週六鬼資料
- 實測筆數（1776，2026-04-30）
- 建議 index：PK 即可；用於 Rust 週 K 聚合與 institutional 過濾鬼資料

| 欄位 | 型別 | NN | 備註 |
|---|---|---|---|
| date | TEXT | ✓ | 僅交易日 |

### `market_index_tw` （PK: market, stock_id, date）
- 來源：`TaiwanStockTotalReturnIndex`，`fixed_ids=["TAIEX","TPEx"]`
- 實測筆數（3550，2026-04-30；兩檔 × ~1775）
- 建議 index：`(stock_id, date)`（快速取單一指數時序）

| 欄位 | 型別 | NN | 備註 | 單位 |
|---|---|---|---|---|
| stock_id | TEXT | ✓ | `TAIEX` / `TPEx` | |
| date | TEXT | ✓ | | |
| price | REAL | | 報酬指數點數 | |

---

## Phase 2 — EVENTS（除權息 / 減資 / 分割 / 面額變更 / 純現增）

### `price_adjustment_events` （PK: market, stock_id, date, event_type）
- 五種 event_type 共用一張表
- 更新頻率：phase 2（每日）；is_backer=true
- 預估筆數：每股每年 ~2~5 筆
- 建議 index：PK 已含 stock_id+date；如需事件類型聚合可加 `(event_type, date)`

| 欄位 | 型別 | NN | 備註 |
|---|---|---|---|
| event_type | TEXT | ✓ | `dividend` / `capital_reduction` / `split` / `par_value_change` / `capital_increase` |
| before_price | REAL | | 事件前收盤；`capital_increase` 由 Rust step 1.5 補 |
| after_price | REAL | | 事件後參考收盤；同上 |
| reference_price | REAL | | 事件後開盤參考價 |
| adjustment_factor | REAL DEFAULT 1.0 | | 後復權乘數；Rust Phase 4 直接讀此欄 |
| volume_factor | REAL DEFAULT 1.0 | | 量調整乘數 |
| cash_dividend | REAL | | 元/股（含現金股利+法定盈餘公積現金）|
| stock_dividend | REAL | | 元/股（StockEarnings+StockStatutory）/10 |
| detail | TEXT JSON | | 視 event_type 而定（見下）|

各 event_type 的 detail JSON 鍵：

| event_type | API 來源 | detail 鍵 |
|---|---|---|
| `dividend` | `TaiwanStockDividendResult` | `_event_subtype`, `_combined_dividend`, `_max_price`, `_min_price`, `_open_price` |
| `capital_reduction` | `TaiwanStockCapitalReductionReferencePrice` | `_reason`, `_limit_up`, `_limit_down`, `_exright_ref` |
| `split` | `TaiwanStockSplitPrice` | `_split_type`, `_max_price`, `_min_price` |
| `par_value_change` | `TaiwanStockParValueChange` | `_limit_up`, `_limit_down`, `_stock_name` |
| `capital_increase` | post_process（無原 API record） | `subscription_price`, `subscription_rate_raw`, `total_new_shares`, `total_participating_shares`, `source`, `status`（`pending_rust_phase4`，由 Rust 補算 AF）|

### `_dividend_policy_staging` （PK: market, stock_id, date）— internal/staging
- 來源：`TaiwanStockDividend`（純現增與權息混合事件由它供料）
- 預估筆數：每股每年 ~1 筆
- 建議 index：PK 已涵蓋；post_process 用 `detail->>'CashExDividendTradingDate'` 與 `detail->>'StockExDividendTradingDate'` 反查
- v1.7 review #4：post_process 4 處 SELECT 都已補 `market = 'TW'` filter

| 欄位 | 型別 | NN | 備註 |
|---|---|---|---|
| date | DATE | ✓ | API 「決議日期」 |
| detail | JSONB | | 21 個 PascalCase 欄位 |
| source | TEXT | | v1.6 加，DEFAULT `'finmind'` |

detail JSON 鍵（PascalCase 原樣，注意 FinMind 拼字錯誤保留）：
`StockEarningsDistribution`, `StockStatutorySurplus`, `StockExDividendTradingDate`, `TotalEmployeeStockDividend`, `TotalEmployeeStockDividendAmount`, `RatioOfEmployeeStockDividendOfTotal`, `RatioOfEmployeeStockDividend`, `CashEarningsDistribution`, `CashStatutorySurplus`, `CashExDividendTradingDate`, `CashDividendPaymentDate`, `TotalEmployeeCashDividend`, `TotalNumberOfCashCapitalIncrease`, `CashIncreaseSubscriptionRate`, `CashIncreaseSubscriptionpRrice`（sic）, `RemunerationOfDirectorsAndSupervisors`, `ParticipateDistributionOfTotalShares`, `AnnouncementDate`, `AnnouncementTime`, `_year`

---

## Phase 3 — RAW PRICE（原始日 K + 漲跌停）

### `price_daily` （PK: market, stock_id, date）
- 來源：`TaiwanStockPrice`；is_backer=true，segment_days=365
- 預估筆數：~1,772/股
- 建議 index：PK 已含 (stock_id, date)；如做跨股切片可加 `(date, stock_id)`

| 欄位 | 型別 | API 對應 | 單位 |
|---|---|---|---|
| open / high / low / close | REAL | `open` / `max` rename / `min` rename / `close` | 元 |
| volume | INTEGER | `Trading_Volume` rename | 股 |
| turnover | REAL | `Trading_turnover` rename | 元 |
| detail | TEXT JSON | `_trading_money`(Trading_money), `_spread`(spread) | |

> ⚠️ 這是**原始除權息斷點價**，做任何時序回測請改用 `price_daily_fwd`。

### `price_limit` （PK: market, stock_id, date）
- 來源：`TaiwanStockPriceLimit`
- 預估筆數：~1,772/股

| 欄位 | 型別 | API 對應 |
|---|---|---|
| limit_up | REAL | `limit_up`（元）|
| limit_down | REAL | `limit_down`（元）|
| detail | TEXT JSON | `_reference_price` |

---

## Phase 4 — Rust 計算產出（後復權 K 線）

執行體：`rust_compute/target/release/tw_stock_compute`（Windows: `.exe`）；schema_version=1.1。
讀 `price_daily` + `price_adjustment_events`，寫下三張表 + 更新 `stock_sync_status.fwd_adj_valid=1` + 補 `capital_increase` AF。
OHLC 一律 round 到小數第二位；volume 為 `raw_volume / multiplier` round 後存。

### `price_daily_fwd` （PK: market, stock_id, date）
- 預估筆數：與 `price_daily` 1:1
- 建議 index：PK 已涵蓋

| 欄位 | 型別 | 備註 |
|---|---|---|
| open / high / low / close | REAL | 後復權價 |
| volume | INTEGER | 後復權量（÷ multiplier）|

### `price_weekly_fwd` （PK: market, stock_id, year, week）
- 聚合：ISO week（`chrono::IsoWeek`），`year` 為 ISO year（跨年週可能落在 1 或 52/53）
- 預估筆數：~370/股
- OHLC 取週內 first/max/min/last；volume 加總

### `price_monthly_fwd` （PK: market, stock_id, year, month）
- 聚合：calendar `(year, month)`
- 預估筆數：~88/股
- 同週 K 聚合方式

---

## Phase 5 — CHIP / FUNDAMENTAL

### `institutional_daily` （PK: market, stock_id, date）
- 來源：`TaiwanStockInstitutionalInvestorsBuySell`，aggregator `pivot_institutional`
- 實測筆數（1775/股，2026-04-30）；每日 5 列 pivot 為 1 列；非交易日鬼資料由 `_filter_to_trading_days` 過濾
- 建議 index：PK 已涵蓋

5 類法人各自獨立欄位（皆 INTEGER，單位：股）：

| 欄位 | API name 值 |
|---|---|
| `foreign_buy` / `foreign_sell` | `Foreign_Investor`（外資不含外資自營商）|
| `foreign_dealer_self_buy` / `foreign_dealer_self_sell` | `Foreign_Dealer_Self` |
| `investment_trust_buy` / `investment_trust_sell` | `Investment_Trust` |
| `dealer_buy` / `dealer_sell` | `Dealer_self` |
| `dealer_hedging_buy` / `dealer_hedging_sell` | `Dealer_Hedging` |

`total` / `Total` / `合計` 列由 `INSTITUTIONAL_NAME_IGNORED` 靜默略過。

### `margin_daily` （PK: market, stock_id, date）
來源：`TaiwanStockMarginPurchaseShortSale`

| 欄位 | 型別 | API 對應 | 單位 |
|---|---|---|---|
| margin_purchase | INTEGER | `MarginPurchaseBuy` | 千股 |
| margin_sell | INTEGER | `MarginPurchaseSell` | 千股 |
| margin_balance | INTEGER | `MarginPurchaseTodayBalance` | 千股 |
| short_sale | INTEGER | `ShortSaleBuy` | 千股 |
| short_cover | INTEGER | `ShortSaleSell` | 千股 |
| short_balance | INTEGER | `ShortSaleTodayBalance` | 千股 |
| detail | TEXT JSON | `_margin_cash_repay`, `_margin_prev_balance`, `_margin_limit`, `_short_cash_repay`, `_short_prev_balance`, `_short_limit`, `_offset_loan_short`, `_note` | |

### `foreign_holding` （PK: market, stock_id, date）
來源：`TaiwanStockShareholding`

| 欄位 | 型別 | API 對應 |
|---|---|---|
| foreign_holding_shares | INTEGER | `ForeignInvestmentShares` |
| foreign_holding_ratio | REAL | `ForeignInvestmentSharesRatio` (0~100，%) |
| detail | TEXT JSON | `_remaining_shares`, `_remain_ratio`, `_upper_limit_ratio`, `_cn_upper_limit`, `_total_issued`, `_declare_date`, `_intl_code`, `_stock_name`, `_note` |

### `holding_shares_per` （PK: market, stock_id, date）
來源：`TaiwanStockHoldingSharesPer`，aggregator `pack_holding_shares`
頻率：週更（FinMind 約每週末更新）
實測筆數（~377/股，2026-04-30）
detail JSON 結構：

```json
{
  "<HoldingSharesLevel>": { "people": int, "percent": float, "unit": int },
  ...
}
```

### `valuation_daily` （PK: market, stock_id, date）
來源：`TaiwanStockPER`

| 欄位 | 型別 | API 對應 | 備註 |
|---|---|---|---|
| per | REAL | `PER` | 倍 |
| dividend_yield | REAL | `dividend_yield` | % |
| pbr | REAL | `PBR` | 倍 |

### `day_trading` （PK: market, stock_id, date）
來源：`TaiwanStockDayTrading`

| 欄位 | 型別 | API 對應 | 語意 |
|---|---|---|---|
| day_trading_buy | INTEGER | `BuyAmount` | **金額**（元），不是筆數 |
| day_trading_sell | INTEGER | `SellAmount` | **金額**（元），不是筆數 |
| detail | TEXT JSON | `_day_trading_flag`(BuyAfterSale), `_volume`(Volume) | flag=可否當沖 |

### `index_weight_daily` （PK: market, stock_id, date）
來源：`TaiwanStockMarketValueWeight`

| 欄位 | 型別 | API 對應 |
|---|---|---|
| weight | REAL | `weight_per`（%） |
| detail | TEXT JSON | `_rank`, `_stock_name`, `_index_type` |

### `monthly_revenue` （PK: market, stock_id, date）
來源：`TaiwanStockMonthRevenue`，segment_days=0（單一段全抓）
頻率：月更（每月 10 號前公告）
實測筆數（88/股，2026-04-30；88 個月，自 2019-01）

| 欄位 | 型別 | API 對應 | 單位 |
|---|---|---|---|
| revenue | REAL | `revenue` | 元 |
| revenue_mom | REAL | `revenue_month` rename | %（環比）|
| revenue_yoy | REAL | `revenue_year` rename | %（年比）|
| detail | TEXT JSON | `_country`, `_create_time` | |

### `financial_statement` （PK: market, stock_id, date, type）
- 三表共用，`type` 區分 `income` / `balance` / `cashflow`
- aggregator `pack_financial`，每日 N 個科目 → 1 列
- ⚠️ `date` = 會計期間結束日（季末），不是公告日；incremental 模式有較長 lookback
- 實測筆數（84/股，2026-04-30；28 季 × 3 表）

| 欄位 | 型別 | 備註 |
|---|---|---|
| type | TEXT | `income` / `balance` / `cashflow` |
| detail | TEXT JSON | `{ "<origin_name>": value, ... }`，key 優先英文 origin_name；fallback 中文科目名；value 為 REAL |

來源 dataset 對應：
- `TaiwanStockFinancialStatements` → `income`
- `TaiwanStockBalanceSheet` → `balance`
- `TaiwanStockCashFlowsStatement` → `cashflow`

---

## Phase 6 — MACRO（總體）

### `market_index_us` （PK: market, stock_id, date）
來源：`USStockPrice`，`fixed_ids=["SPY","^VIX"]`

| 欄位 | 型別 | API 對應 |
|---|---|---|
| open / high / low / close | REAL | `Open` / `High` / `Low` / `Close` |
| volume | INTEGER | `Volume` |
| detail | TEXT JSON | `_adj_close`(Adj_Close) |

### `exchange_rate` （PK: market, date, currency）
- 來源：`TaiwanExchangeRate`，必須帶 `data_id`(currency)
- v2.0 收斂為 4 幣別：USD CNY JPY AUD（v1.6 為 19 幣別；實際測試發現其餘幣別在 collector 內長期使用頻率低，且增加不必要的 API 起叫與 rate-limit 壓力）
- 實測筆數（7204，2026-04-30；4 幣 × ~1801，含非交易日）

| 欄位 | 型別 | API 對應 | 備註 |
|---|---|---|---|
| currency | TEXT | data_id 參數 | PK 一部分 |
| rate | REAL | `spot_buy` rename | 即期買匯 |
| detail | TEXT JSON | `_cash_buy`, `_cash_sell`, `_spot_sell` | |

### `institutional_market_daily` （PK: market, date）
- 來源：`TaiwanStockTotalInstitutionalInvestors`，aggregator `pivot_institutional_market`
- 欄位 = `institutional_daily` 砍掉 `stock_id` 後的 10 個 buy/sell（同樣 5 類法人）
- 實測筆數（1775，2026-04-30）

### `market_margin_maintenance` （PK: market, date）
來源：`TaiwanTotalExchangeMarginMaintenance`

| 欄位 | 型別 | API 對應 | 單位 |
|---|---|---|---|
| ratio | REAL | `TotalExchangeMarginMaintenance` | %（如 165.32）|

### `fear_greed_index` （PK: market, date）
來源：`CnnFearGreedIndex`

| 欄位 | 型別 | API 對應 |
|---|---|---|
| score | REAL | `fear_greed`（0–100）|
| label | TEXT | `fear_greed_emotion`（Fear / Greed / Neutral 等）|
| detail | TEXT JSON | 額外欄位 |

---

## 系統表（不對接外部）

### `schema_metadata` （PK: key）
v2.0 引入；存 `schema_version='2.0'` 等 metadata；Rust binary 啟動時 `assert_schema_version()` 讀此表 assert。

| 欄位 | 型別 | 備註 |
|---|---|---|
| key | TEXT | `schema_version` / `migrated_from` / `migrated_at` |
| value | TEXT | 對應字串值 |
| updated_at | TIMESTAMPTZ | DEFAULT NOW() |

### `stock_sync_status` （PK: market, stock_id）

| 欄位 | 型別 | 寫入者 | 用途 |
|---|---|---|---|
| last_full_sync | DATE | （保留未用，v1.6 砍 Python 端 dead helper）| - |
| last_incr_sync | DATE | 同上 | - |
| fwd_adj_valid | SMALLINT DEFAULT 0 | Rust Phase 4 (`mark_fwd_valid`) | `0`=待算 / `1`=已算；CHECK 限 0/1 |

> 🧭 **M3 候選欄位**：若做 Phase 4 dirty-detection 優化，可加 `last_phase4_at TIMESTAMPTZ` — Python 端比對 `price_adjustment_events.date > stock_sync_status.last_phase4_at` 決定是否要叫 Rust 重算（見 CLAUDE.md「待研究：Phase 4 真正的 incremental 優化」）。

### `api_sync_progress` （PK: api_name, stock_id, segment_start）
斷點續傳。`scripts/drop_table.py` drop 目標表時會連帶 `DELETE FROM api_sync_progress WHERE api_name IN (...)`。

| 欄位 | 型別 | 備註 |
|---|---|---|
| api_name | TEXT | toml `[[api]] name` |
| stock_id | TEXT | per-stock 模式才有意義；all_market 模式為 `__ALL__` sentinel；fixed_ids 模式為實際 id（如 `TAIEX` / `USD` / `SPY`）|
| segment_start / segment_end | DATE | segment 起訖日 |
| status | TEXT DEFAULT `'pending'` | **v1.7 起 5 種**：`pending` / `completed` / `failed` / `empty` / `schema_mismatch`（CHECK constraint 由 alembic `a1b2c3d4e5f6` 擴出）|
| record_count | INTEGER DEFAULT 0 | |
| error_message | TEXT | |
| updated_at | TIMESTAMPTZ DEFAULT NOW() | v1.7 起 upsert UPDATE 路徑強制 NOW() |

> ✅ **語意**：`empty` 表示「該 API 對該 stock_id × segment 真的回空陣列」（很常見：dividend_result 對沒發股利的股票、incremental 已無新資料）— **不是錯誤**，對 incremental 也視為「已處理」可跳過（`SKIP_STATUSES = {"completed", "empty"}`）。

---

## 對接快速索引

> ⚠️ v2.0 上同步換 PG 語法：
> - SQLite `json_extract(detail, '$.X')` → PG `detail->>'X'`（返 TEXT）或 `(detail->>'X')::numeric`（轉實數）。

| 用途 | 表 + 篩選條件 |
|---|---|
| 後復權日 K | `price_daily_fwd`（不是 `price_daily`）|
| 後復權週/月 K | `price_weekly_fwd` / `price_monthly_fwd`（ISO week vs calendar month）|
| 除權息事件清單 | `price_adjustment_events WHERE event_type='dividend'` |
| 純現增事件清單 | `price_adjustment_events WHERE event_type='capital_increase'` |
| 股利政策原文 | `_dividend_policy_staging.detail` JSONB；複雜查詢用 `detail->>'CashExDividendTradingDate'` |
| 財報科目值 | `SELECT (detail->>'<origin_name>')::numeric FROM financial_statement WHERE type IN ('income','balance','cashflow')` |
| 5 類法人 per-stock | `institutional_daily`（10 欄獨立）|
| 5 類法人全市場 | `institutional_market_daily`（同 10 欄）|
| 漲跌停參考價 | `price_limit.limit_up/_down` + `detail->>'_reference_price'` |
| 大盤指數 | `market_index_tw WHERE stock_id IN ('TAIEX','TPEx')` |
| 美股 / 恐慌指數 | `market_index_us WHERE stock_id IN ('SPY','^VIX')` |
| 外資 / 投信持股比 | `foreign_holding`（per-stock）|
| 月營收 / 年增 | `monthly_revenue`（PK 的 date 為當月 1 號或公告月初，依 FinMind）|
| 個股估值 | `valuation_daily`（per/pbr/yield 三合一）|

---

## M3（多 core 計算）對接邊界

> 本段是**給下一階段（NEO Pipeline / Rust workspace 多 core 計算）對接用的契約**。M1 / collector 端到此凍結，M3 在這 27 表之上加 derived / indicator 表，不回頭改 collector。

### 凍結邊界（contract）

下面這些**不會再改**，下游 M3 / Aggregation Layer 可放心依賴：

1. **27 張表名 + 各自 PK** — 不會增刪表、不會改 PK 欄位順序
2. **`market='TW'` 與 `source='finmind'`** — 兩個固定 const，多市場前不變
3. **`detail` 欄位的 `_` 前綴 → 去底線後成 JSON key** 的慣例（field_mapper step 1+2）
4. **`api_sync_progress.status` 5 種**（`pending/completed/failed/empty/schema_mismatch`）
5. **Phase 排程 0→6 線性**，Phase 4 走 Rust subprocess + stdout JSON Summary，schema_version assert
6. **Rust binary 永遠全量重算 fwd 三表**（`price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`）— 設計使然，不是未優化
7. **時區**：所有 `TIMESTAMPTZ` 用 server timezone（user 本機 +08）；所有 `DATE` 純日期不帶時區

### 推薦的 M3 讀取慣例

| 用途 | 該讀的表 | 理由 |
|---|---|---|
| 任何時序回測 / 技術分析 | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` | **後復權**，跨除權息日連續 |
| 量化籌碼面 | `institutional_daily` + `margin_daily` + `foreign_holding` + `day_trading` | 5 類法人各自獨立欄位、不要自己加總 |
| 基本面（財報） | `financial_statement WHERE type=...` + `monthly_revenue` | `detail->>'<origin_name>'` 取個別科目 |
| 估值 | `valuation_daily` | per/pbr/yield 三合一 |
| 全市場 / 宏觀 | `institutional_market_daily` + `market_index_tw` + `market_index_us` + `exchange_rate` + `fear_greed_index` + `market_margin_maintenance` | 都是每日 1 列（per 市場 / per 指數 / per 幣別）|
| 事件序列 | `price_adjustment_events` | event_type 5 種；`adjustment_factor` 欄位是後復權乘數的權威值 |
| 交易日曆 | `trading_calendar` | 過濾非交易日；Phase 0 必須先建好 |
| stock universe | `stock_info WHERE delist_date IS NULL` + filter `market_type` | 跟 `stock_resolver._query_from_db` 邏輯對齊 |

### M3 不應該動的東西

- ❌ **直接 INSERT / UPDATE 任何 collector 表** — collector 是 single writer。如果 M3 需要寫，請寫到「M3 自己的表」（建議 schema 命名前綴如 `m3_` 或 `agg_` / `indicator_`）
- ❌ **改 `api_sync_progress.status` 的 5 種值** — 這個 vocabulary 鎖死了
- ❌ **依賴 `stock_sync_status.last_full_sync` / `last_incr_sync`** — dead helper，欄位保留但不寫入；要做 dirty-detection 請新增欄位（見下方建議）
- ❌ **跨進程 INSERT `price_daily_fwd`** — 由 Rust binary 獨佔，M3 只能讀

### M3 可能會新增的東西（提示，不影響本邊界）

| 候選 | 落腳處建議 | 為何 collector 不負責 |
|---|---|---|
| 技術指標表（MA / MACD / RSI / 布林通道 等）| `agg_indicator_daily` 或 partition 過的 `indicator_values` | 計算密集，M3 用 Rust 多 core 算，不該擠進 collector phase |
| Wave Cores / Neely 波浪計算結果 | `m3_wave_state` / `m3_neely_*` | 業務模型層；用 `price_daily_fwd` 為輸入 |
| Cross-stock 統計（相關性、產業相對強度等）| `agg_cross_stock_*` | 多股 join，重算成本高，分層存 |
| Phase 4 dirty-detection 用的 `last_phase4_at` | 加進 `stock_sync_status` 的補欄位（不破壞既有 PK / 既有欄位）| 純 collector 內部優化，不歸 M3 |
| advisory lock 相關狀態表 | `_lock_*` | NEO Pipeline single-flight 用 |

### M3 動工前的最終驗證 checklist

```sql
-- 1. schema_metadata 對齊
SELECT * FROM schema_metadata;
-- 應有 schema_version='2.0'

-- 2. CHECK constraint 5 種 status
SELECT pg_get_constraintdef(oid) FROM pg_constraint
 WHERE conrelid = 'api_sync_progress'::regclass AND conname = 'chk_progress_status';
-- 應含 'empty', 'schema_mismatch'

-- 3. 27 張表都在
SELECT count(*) FROM information_schema.tables
 WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
-- 應 = 27

-- 4. 各 phase 至少有資料（user 本機 backfill 後）
SELECT 'price_daily' AS t, count(*) FROM price_daily
 UNION ALL SELECT 'price_daily_fwd', count(*) FROM price_daily_fwd
 UNION ALL SELECT 'institutional_daily', count(*) FROM institutional_daily
 UNION ALL SELECT 'financial_statement', count(*) FROM financial_statement;

-- 5. api_sync_progress 健康
SELECT status, count(*) FROM api_sync_progress GROUP BY status ORDER BY status;
-- 應只有 completed / empty（健康環境無 pending / failed / schema_mismatch）
```

通過後 M3 可開始動工，base branch 就是 PR #6 merge 後的 `m1/postgres-migration`（review #3+#4 + CLAUDE.md v1.7）。
