# 重構後分層 Schema（Bronze + Silver）

> **版本**：v1.0（重構目標 schema，非當前狀態）
> **日期**：2026-05-06
> **配套文件**：`data_refactor_plan.md`（重構計畫）、`cores_overview.md`（Cores 層通用規範）、`adr/0001_tw_market_handling.md`
> **適用範圍**：tw-stock-collector / StockHelper4me — 所有 PR #R1~#R6 完成後的目標狀態
> **架構原則**：本文件遵循 README「架構原則：計算 / 規則分層」—— Silver 層承擔複雜計算，Cores 層只做規則 / 算式套用。詳見 §1.5。
> **目的**：
> 1. 提供 Cores 層設計時可直接引用的 Silver 表單一事實來源
> 2. 顯式呈現 Bronze → Silver 的 dirty 傳遞與依賴流向
> 3. 釐清重構後可移除 / rename 的 legacy 表

---

## 目錄

1. [文件約定](#一文件約定)
2. [整體架構總覽](#二整體架構總覽)
3. [Bronze 層](#三bronze-層)
4. [Silver 層](#四silver-層)
5. [系統表](#五系統表)
6. [Dirty / Dependency 流向圖](#六dirty--dependency-流向圖)
7. [退場表清單](#七退場表清單)
8. [給 Cores 層的接點清單](#八給-cores-層的接點清單)

---

## 一、文件約定

### 1.1 階段命名

對齊 `data_refactor_plan.md` §3.2~§3.3 的新階段命名：

- **Bronze**：`B0_calendar` / `B1_meta` / `B2_events` / `B3_price_raw` / `B4_chip` / `B5_fundamental` / `B6_environment`
- **Silver**：`S1_adjustment` / `S2_aggregation`（合併進 S1） / `S3_reverse_pivot` / `S4_derived_chip` / `S5_derived_fundamental` / `S6_derived_environment`

> 註：`S2_aggregation`（週/月 K 聚合）合併進 S1，因為 fwd 三表是同一支 Rust binary 一次產出，拆兩階段反而造成 orchestrator 複雜度增加。

### 1.2 表狀態標記

每張表前面的標記：

- 🟢 **新主表**：重構後保留，Cores 層可引用
- 🟡 **過渡表**：保留但即將退場 / 內容簡化
- 🔴 **退場表**：PR #R6 完成後 DROP，Cores 層**不可引用**
- ⚙️ **系統表**：collector 內部使用，Cores 層**不應引用**

### 1.3 維度標記

- **per_stock**：(market, stock_id, date) 三維度
- **per_stock_pk_ext**：三維度 + 額外 PK 欄（如 `event_type` / `currency` / `holding_shares_level`）
- **all_market**：(market, date) — 全市場單筆 / 日
- **calendar**：(market, date) — 交易日曆

### 1.4 來源 / 去處標記

- **API**：FinMind endpoint 名稱
- **dirty trigger**：寫入此表時觸發哪個 silver 表 dirty
- **upstream**：Silver 表的來源 Bronze 表
- **downstream（Cores）**：哪些 Cores 類別會讀此 Silver 表（前瞻指標）

### 1.5 與 Cores 層的職責邊界

Silver 層處理**複雜計算**（後復權、漲跌停合併、跨表 join、跨日狀態追溯），Cores 層處理**規則 / 算式套用**。詳細原則見 README「架構原則：計算 / 規則分層」與 `cores_overview.md` §1.1。

此邊界是強約束，違反者（例：在 Silver 層判斷 Wave 結構、在 Cores 層做後復權）視為設計錯誤。

歷史備註：v1.x / v2.0 r1 曾規劃 `TW-Market Core` 作為 Cores 層前置處理，r2 後此 Core 廢除，所有職責已歸 Silver 層 S1_adjustment。詳見 `adr/0001_tw_market_handling.md`。

---

## 二、整體架構總覽

```
                         ┌────────────────────────────────────────┐
                         │        FinMind / 外部資料源              │
                         └──────────────────┬─────────────────────┘
                                            │ (HTTP fetch)
                                            ▼
┌──────────────────────────────── BRONZE ──────────────────────────────────┐
│                                                                           │
│  B0_calendar         trading_date_ref                                    │
│  B1_meta             stock_info_ref · market_index_tw · market_ohlcv_tw  │
│  B2_events           price_adjustment_events · stock_suspension_events    │
│  B3_price_raw        price_daily · price_limit                            │
│  B4_chip             institutional_investors_tw · margin_purchase_*_tw   │
│                      foreign_investor_share_tw · day_trading_tw          │
│                      valuation_per_tw · holding_shares_per_tw            │
│                      securities_lending_tw                                │
│  B5_fundamental      financial_statement_tw · monthly_revenue_tw         │
│  B6_environment      market_index_us · exchange_rate                     │
│                      institutional_market_daily · market_margin_*        │
│                      fear_greed_index · business_indicator_tw            │
│                                                                           │
└─────────────────────────────────┬─────────────────────────────────────────┘
                                  │ (PG trigger / Rust binary)
                                  ▼
┌──────────────────────────────── SILVER ──────────────────────────────────┐
│                                                                           │
│  S1_adjustment       price_daily_fwd · price_weekly_fwd                  │
│  (Rust binary)       price_monthly_fwd · price_limit_merge_events        │
│                                                                           │
│  S4_derived_chip     institutional_daily_derived · margin_daily_derived  │
│  (SQL builders)      foreign_holding_derived · holding_shares_per_derived│
│                      day_trading_derived · valuation_daily_derived       │
│                                                                           │
│  S5_derived_fundamental  monthly_revenue_derived                          │
│  (SQL builders)          financial_statement_derived                      │
│                                                                           │
│  S6_derived_environment  taiex_index_derived · us_market_index_derived   │
│  (SQL builders)          exchange_rate_derived                            │
│                          market_margin_maintenance_derived                │
│                          business_indicator_derived                       │
│                                                                           │
└─────────────────────────────────┬─────────────────────────────────────────┘
                                  │ (PyO3 → Rust 純計算)
                                  ▼
┌──────────────────────────────── M3 / CORES ──────────────────────────────┐
│   chip_cores · fundamental_cores · environment_cores                     │
│   indicator_cores · wave_cores                                            │
│   （M3 schema 由 cores_refactor_plan.md 定義）                            │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## 三、Bronze 層

### 3.1 B0_calendar（交易日曆）

#### 🟢 `trading_date_ref` — 交易日曆

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |

- **PK**：`(market, date)`
- **API**：`trading_calendar`（FinMind）
- **維度**：calendar
- **設計**：row 存在 = 交易日，不存在 = 非交易日（無 `source` 欄）
- **dirty trigger**：無
- **DDL 範例**：

```sql
CREATE TABLE trading_date_ref (
    market  TEXT NOT NULL,
    date    DATE NOT NULL,
    PRIMARY KEY (market, date)
);
```

---

### 3.2 B1_meta（基礎參考）

#### 🟢 `stock_info_ref` — 股票主檔

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| stock_name | TEXT | |
| type | TEXT | twse / tpex / emerging |
| industry_category | TEXT | 主檔粗分類 |
| listing_date | DATE | |
| delisting_date | DATE | NULL = 仍上市 |
| par_value | NUMERIC(10, 2) | |
| detail | JSONB | data_update_date 等 |
| source | TEXT NN DEFAULT 'finmind' | |
| updated_at | TIMESTAMPTZ NN DEFAULT NOW() | ETL 內部 |

- **PK**：`(market, stock_id)`
- **API**：`stock_info` + `stock_delisting`（合併寫入）
- **維度**：per stock（單一檔）
- **dirty trigger**：無
- **Index**：
  - `idx_sir_active ON (market, stock_id) WHERE delisting_date IS NULL`
  - `idx_sir_industry ON (industry_category) WHERE industry_category IS NOT NULL`

#### 🟢 `market_index_tw` — 台股加權報酬指數

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK，TAIEX / TPEx |
| date | DATE NN | PK |
| price | NUMERIC(15, 4) | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`TaiwanStockTotalReturnIndex`
- **維度**：per_stock（指數視為特殊 stock_id）
- **dirty trigger**：無（補 close 用，taiex_index_derived 由 market_ohlcv_tw 觸發）

#### 🟢 `market_ohlcv_tw` — 台股大盤 OHLCV

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK，TAIEX / TPEx |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`TaiwanVariousIndicators5Seconds`（intraday 5-sec aggregate to daily OHLCV）
- **dirty trigger**：→ `taiex_index_derived`

> ⚠️ `market_index_tw` 與 `market_ohlcv_tw` 並存，前者僅 close、後者完整 OHLCV。重構後 environment_cores 應優先讀 `taiex_index_derived`，不應直接讀 Bronze。

---

### 3.3 B2_events（事件層）

#### 🟢 `price_adjustment_events` — 價格調整事件

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| event_type | TEXT NN | PK，5 種：dividend / capital_reduction / split / par_value_change / capital_increase |
| before_price | NUMERIC(15, 4) | |
| reference_price | NUMERIC(15, 4) | |
| volume_factor | NUMERIC(20, 10) NN DEFAULT 1.0 | P0-11 split 必要 |
| cash_dividend | NUMERIC(15, 6) | |
| stock_dividend | NUMERIC(15, 6) | |
| detail | JSONB | |

- **PK**：`(market, stock_id, date, event_type)` — per_stock_pk_ext
- **API**：5 個（`dividend_result` / `capital_reduction` / `split_price` / `par_value_change` / `dividend_policy`）共寫
- **dirty trigger**：→ **price_daily_fwd / price_weekly_fwd / price_monthly_fwd / price_limit_merge_events 全段歷史 dirty**（fwd 復權倒推設計，新 event 影響全段）
- **CHECK**：`event_type IN ('dividend', 'capital_reduction', 'split', 'par_value_change', 'capital_increase')`
- **Index**：`idx_price_adj_event_type_date ON (event_type, date DESC)`

#### 🟢 `stock_suspension_events` — 個股暫停交易

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| suspension_date | DATE NN | PK |
| suspension_time | TEXT | |
| resumption_date | DATE | 復牌日 |
| resumption_time | TEXT | |
| reason | TEXT | 暫停原因 |
| detail | JSONB | |

- **PK**：`(market, stock_id, suspension_date)`
- **API**：`stock_suspension`
- **用途**：`prev_trading_day(stock_id, date)` 模組 + 個股級交易缺口識別
- **dirty trigger**：無（事件查詢用）

#### 🟡 `_dividend_policy_staging` — 股利政策暫存表

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`dividend_policy`
- **用途**：post_process 後內容合併入 `price_adjustment_events`，本身不對接 Cores
- **下游**：collector 內部 `dividend_policy_merge` post-process 邏輯
- **Cores 不應直接讀**

---

### 3.4 B3_price_raw（原始價量）

#### 🟢 `price_daily` — 日 K 原始

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| turnover | NUMERIC(20, 2) | |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`TaiwanStockPrice`
- **dirty trigger**：無（fwd 由 events 觸發，價格本身不直接觸發）

> ⚠️ Cores 應**不讀** `price_daily`，改讀 `price_daily_fwd`（已復權）。`price_daily` 僅供 S1_adjustment 的 Rust binary 計算 fwd 時使用。

#### 🟢 `price_limit` — 漲跌停

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| limit_up / limit_down | NUMERIC(15, 4) | |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`TaiwanStockPriceLimit`
- **dirty trigger**：無

---

### 3.5 B4_chip（籌碼類）

> 重構後 PR #R6 完成，舊 v2.0 籌碼表（`institutional_daily` / `margin_daily` / `foreign_holding` / `day_trading` / `valuation_daily` / `index_weight_daily`）全部 DROP，只留 reverse-pivot 後的 `*_tw` 系列。

#### 🟢 `institutional_investors_tw` — 三大法人（每法人 1 row）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| investor_type | TEXT NN | PK，5 類：foreign / foreign_dealer_self / investment_trust / dealer / dealer_hedging |
| buy / sell | BIGINT | |
| name | TEXT | |

- **PK**：`(market, stock_id, date, investor_type)` — per_stock_pk_ext
- **API**：`InstitutionalInvestorsBuySell` v3
- **dirty trigger**：→ `institutional_daily_derived`

#### 🟢 `margin_purchase_short_sale_tw` — 融資融券（14 raw）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| margin_purchase / margin_sell / margin_balance | BIGINT | |
| short_sale / short_cover / short_balance | BIGINT | |
| margin_cash_repay / margin_prev_balance / margin_limit | BIGINT | |
| short_cash_repay / short_prev_balance / short_limit | BIGINT | |
| offset_loan_short | BIGINT | |
| note | TEXT | |

- **PK**：`(market, stock_id, date)`
- **API**：`MarginPurchaseShortSale` v3
- **dirty trigger**：→ `margin_daily_derived`（合併 `securities_lending_tw` 一起餵）

#### 🟢 `securities_lending_tw` — 借券成交明細（SBL）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| transaction_type | TEXT NN | PK，議借 / 競價 |
| volume | BIGINT | |
| fee_rate | NUMERIC(8, 4) NN | PK |
| close | NUMERIC(15, 4) | |
| original_return_date | DATE | |
| original_lending_period | INT | |
| detail | JSONB | |

- **PK**：`(market, stock_id, date, transaction_type, fee_rate)` — 5 維 per_stock_pk_ext
- **API**：`SecuritiesLending`
- **dirty trigger**：→ `margin_daily_derived`（SBL 6 欄）
- **設計**：同股同日「議借」+「競價」獨立，每 `fee_rate` 一筆

#### 🟢 `foreign_investor_share_tw` — 外資持股（11 raw）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| foreign_holding_shares | BIGINT | |
| foreign_holding_ratio | NUMERIC(8, 4) | |
| remaining_shares / remain_ratio | | |
| upper_limit_ratio / cn_upper_limit | NUMERIC(8, 4) | |
| total_issued | BIGINT | |
| declare_date | DATE | |
| intl_code / stock_name / note | TEXT | |

- **PK**：`(market, stock_id, date)`
- **API**：`ForeignInvestorShareholding` v3
- **dirty trigger**：→ `foreign_holding_derived`

#### 🟢 `day_trading_tw` — 當沖（4 raw）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| day_trading_buy / day_trading_sell | BIGINT | |
| day_trading_flag | TEXT | |
| volume | BIGINT | |

- **PK**：`(market, stock_id, date)`
- **API**：`DayTrading` v3
- **dirty trigger**：→ `day_trading_derived`

#### 🟢 `valuation_per_tw` — 估值

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| per | NUMERIC(10, 4) | |
| pbr | NUMERIC(10, 4) | |
| dividend_yield | NUMERIC(8, 4) | |

- **PK**：`(market, stock_id, date)`
- **API**：`PERPBRDividendYield` v3
- **dirty trigger**：→ `valuation_daily_derived`

#### 🟢 `holding_shares_per` — 股權分散（PR #R4 後升格，去 `_tw`）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| holding_shares_level | TEXT NN | PK，每 level 1 row |
| people | BIGINT | |
| percent | NUMERIC(8, 4) | |
| unit | BIGINT | |
| source | TEXT NN DEFAULT 'finmind' | **PR #R1 補回** |

- **PK**：`(market, stock_id, date, holding_shares_level)` — per_stock_pk_ext
- **API**：`StockHoldingSharesPer` v3
- **dirty trigger**：→ `holding_shares_per_derived`
- **遷移狀態**：PR #R4 從 `holding_shares_per_tw` rename 而來

---

### 3.6 B5_fundamental（基本面）

#### 🟢 `financial_statement` — 財報三表合一（PR #R4 後升格）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK，會計期間結束日 |
| event_type | TEXT NN | PK，income / balance / cashflow |
| type | TEXT | （Bronze 不必填，Silver 對齊用） |
| origin_name | TEXT NN | PK，會計科目英文名（中→英對應已落地） |
| value | NUMERIC(20, 4) | |

- **PK**：`(market, stock_id, date, event_type, origin_name)` — 5 維 per_stock_pk_ext
- **API**：`FinancialStatements` 三隻 v3（income / balance / cashflow）
- **dirty trigger**：→ `financial_statement_derived`（trigger function `trg_mark_financial_stmt_dirty`，Bronze.event_type ↔ Silver.type 對齊）
- **遷移狀態**：PR #R4 從 `financial_statement_tw` rename
- **CHECK**：`event_type IN ('income', 'balance', 'cashflow')`

#### 🟢 `monthly_revenue` — 月營收（PR #R4 後升格）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK，當月 1 號 |
| revenue | NUMERIC(20, 2) | |
| revenue_year | NUMERIC(10, 4) | FinMind 原欄（≈ yoy %），Silver builder rename |
| revenue_month | NUMERIC(10, 4) | FinMind 原欄（≈ mom %），Silver builder rename |
| country | TEXT | |
| create_time | TEXT | FinMind 對某些 row 回 `""` 不是 NULL，保 TEXT |

- **PK**：`(market, stock_id, date)`
- **API**：`MonthlyRevenue` v3
- **dirty trigger**：→ `monthly_revenue_derived`
- **遷移狀態**：PR #R4 從 `monthly_revenue_tw` rename

> 註：Bronze 不存 `revenue_mom` / `revenue_yoy`（衍生欄該在 Silver 算）。Silver builder rename `revenue_year → revenue_yoy`、`revenue_month → revenue_mom`。

---

### 3.7 B6_environment（環境因子 / 總經）

#### 🟢 `market_index_us` — 美股指數

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK，SPY / ^VIX |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, stock_id, date)`
- **API**：`USStockPrice`
- **dirty trigger**：→ `us_market_index_derived`

#### 🟢 `exchange_rate` — 匯率

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| currency | TEXT NN | PK，USD / EUR / JPY ... |
| rate | NUMERIC(15, 6) | spot_buy |
| detail | JSONB | cash_buy / cash_sell / spot_sell |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, date, currency)` — **不含 stock_id**
- **API**：`ExchangeRate`
- **dirty trigger**：→ `exchange_rate_derived`（special trigger function `trg_mark_exchange_rate_dirty`）

#### 🟢 `institutional_market_daily` — 大盤三大法人

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| foreign_buy / foreign_sell | BIGINT | |
| foreign_dealer_self_buy / foreign_dealer_self_sell | BIGINT | |
| investment_trust_buy / investment_trust_sell | BIGINT | |
| dealer_buy / dealer_sell | BIGINT | |
| dealer_hedging_buy / dealer_hedging_sell | BIGINT | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, date)` — all_market（無 stock_id）
- **API**：`TaiwanStockTotalInstitutionalInvestors`
- **dirty trigger**：無（目前無對應 Silver derived，cores 直接讀此 Bronze；未來若有需求可加）

#### 🟢 `market_margin_maintenance` — 大盤融資維持率

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| ratio | NUMERIC(8, 2) | 百分比 |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, date)` — all_market
- **API**：`TaiwanStockTotalMarginPurchaseShortSale` 類
- **dirty trigger**：→ `market_margin_maintenance_derived`（special function `trg_mark_market_margin_dirty`）

#### 🟢 `fear_greed_index` — CNN 恐懼貪婪指數

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| score | NUMERIC(6, 2) | 0–100 |
| label | TEXT | Fear / Greed / Neutral / Extreme Fear / Extreme Greed |
| detail | JSONB | |
| source | TEXT NN DEFAULT 'finmind' | |

- **PK**：`(market, date)` — all_market
- **API**：`USFearGreedIndex`
- **dirty trigger**：無（目前無 Silver derived）

#### 🟢 `business_indicator_tw` — 景氣指標（月頻）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN DEFAULT 'tw' | PK |
| date | DATE NN | PK，月初 |
| leading_indicator | NUMERIC(10, 4) | （PG 保留字 leading 加後綴） |
| coincident_indicator | NUMERIC(10, 4) | |
| lagging_indicator | NUMERIC(10, 4) | |
| monitoring | INT | 綜合分數 |
| monitoring_color | TEXT | R / YR / G / YB / B |
| detail | JSONB | |

- **PK**：`(market, date)` — all_market
- **API**：`TaiwanBusinessIndicator`
- **dirty trigger**：→ `business_indicator_derived`（special function `trg_mark_business_indicator_dirty`，Bronze 2-col PK → Silver 3-col PK 用 sentinel `_market_`）

---

## 四、Silver 層

### 4.1 S1_adjustment（後復權，Rust binary 計算）

> 此階段**唯一執行模型**：呼叫 Rust binary，無 SQL builder。
> 觸發來源：`price_adjustment_events` 寫入 → trigger `trg_mark_fwd_silver_dirty` 將 fwd 4 表整檔 dirty。

#### 🟢 `price_daily_fwd` — 後復權日 K

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | 已復權 |
| volume | BIGINT | 已復權（split 後） |
| cumulative_adjustment_factor | NUMERIC(20, 10) | 反推 raw price 用 |
| cumulative_volume_factor | NUMERIC(20, 10) | 反推 raw volume 用（P0-11 split 必要） |
| is_adjusted | BOOLEAN NN DEFAULT FALSE | 該日是否動過 |
| adjustment_factor | NUMERIC(20, 10) | 單日 AF，除錯用 |
| is_dirty | BOOLEAN NN DEFAULT FALSE | dirty queue |
| dirty_at | TIMESTAMPTZ | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`price_daily` + `price_adjustment_events`
- **producer**：Rust binary（`process_stock` 全段重算，multiplier 倒推）
- **Index**：
  - `idx_price_daily_fwd_id_date_desc ON (stock_id, date DESC)` — 主要查詢
  - `idx_price_daily_fwd_dirty ON (dirty_at) WHERE is_dirty = TRUE` — orchestrator pull queue
- **downstream（Cores）**：
  - **indicator_cores（全部）**：SMA / EMA / MACD / RSI / Bollinger / ATR / OBV / VWAP / Ichimoku / ADX
  - **wave_cores（全部）**：Traditional / Neely Monowave Detection
  - **chip_cores 部分**：需復權 close 計算 turnover ratio 等

> ⚠️ 這是 Cores 層**最核心的事實表**。所有需要價格資料的 Core 都應讀 fwd，不應讀 `price_daily`。

#### 🟢 `price_weekly_fwd` — 後復權週 K（ISO week）

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| year | INTEGER NN | PK |
| week | INTEGER NN | PK，1–53 |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| is_dirty | BOOLEAN NN DEFAULT FALSE | |
| dirty_at | TIMESTAMPTZ | |

- **PK**：`(market, stock_id, year, week)`
- **upstream**：`price_daily_fwd`（Rust 同次 run 一起算）
- **producer**：Rust binary
- **downstream（Cores）**：tw-stock-deep-analysis 週線 SMA / EMA / Ichimoku、wave_cores 週線轉折

#### 🟢 `price_monthly_fwd` — 後復權月 K

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| year | INTEGER NN | PK |
| month | INTEGER NN | PK，1–12 |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| is_dirty | BOOLEAN NN DEFAULT FALSE | |
| dirty_at | TIMESTAMPTZ | |

- **PK**：`(market, stock_id, year, month)`
- **upstream**：`price_daily_fwd`（Rust 同次 run）
- **producer**：Rust binary
- **downstream（Cores）**：wave_cores 大週期分析

#### 🟢 `price_limit_merge_events` — 漲跌停事件合併

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| merge_type | TEXT | |
| detail | JSONB | |
| is_dirty | BOOLEAN NN DEFAULT FALSE | |
| dirty_at | TIMESTAMPTZ | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`price_limit` + `price_adjustment_events`（漲跌停隨除權息調整）
- **producer**：Rust binary（同 fwd 一起算）
- **downstream（Cores）**：indicator_cores 漲跌停判斷、tw_market_core 漲跌停過濾

---

### 4.2 S4_derived_chip（籌碼衍生，SQL builder）

> 此階段執行模型：`silver/builders/*.py` 純 SQL（`fetch_bronze` → 業務邏輯 → `upsert_silver`）。
> 觸發：對應 Bronze 表寫入 → PG trigger 標 dirty → orchestrator 拉 dirty queue → 跑 builder。

#### 🟢 `institutional_daily_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| foreign_buy / foreign_sell | BIGINT | |
| foreign_dealer_self_buy / foreign_dealer_self_sell | BIGINT | |
| investment_trust_buy / investment_trust_sell | BIGINT | |
| dealer_buy / dealer_sell | BIGINT | |
| dealer_hedging_buy / dealer_hedging_sell | BIGINT | |
| **gov_bank_net** | BIGINT | 衍生欄（八大行庫淨額） |
| is_dirty | BOOLEAN NN DEFAULT FALSE | |
| dirty_at | TIMESTAMPTZ | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`institutional_investors_tw`（pivot 5 row → 1 row）
- **builder**：`silver/builders/institutional.py`
- **trigger function**：`trg_mark_silver_dirty('institutional_daily_derived')`
- **downstream（Cores）**：chip_cores 三大法人籌碼

#### 🟢 `margin_daily_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| margin_purchase / margin_sell / margin_balance | BIGINT | |
| short_sale / short_cover / short_balance | BIGINT | |
| detail | JSONB | |
| **margin_short_sales_short_sales** | BIGINT | SBL 衍生 |
| **margin_short_sales_short_covering** | BIGINT | |
| **margin_short_sales_current_day_balance** | BIGINT | |
| **sbl_short_sales_short_sales** | BIGINT | |
| **sbl_short_sales_returns** | BIGINT | |
| **sbl_short_sales_current_day_balance** | BIGINT | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`margin_purchase_short_sale_tw` + `securities_lending_tw`（雙來源）
- **builder**：`silver/builders/margin.py`
- **trigger function**：兩個 trigger（`mark_margin_derived_from_margin_dirty`、`mark_margin_derived_from_sbl_dirty`），都用 generic `trg_mark_silver_dirty('margin_daily_derived')`
- **downstream（Cores）**：chip_cores 融資融券、SBL 借券

#### 🟢 `foreign_holding_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| foreign_holding_shares | BIGINT | |
| foreign_holding_ratio | NUMERIC(8, 4) | |
| detail | JSONB | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`foreign_investor_share_tw`
- **builder**：`silver/builders/foreign_holding.py`
- **downstream（Cores）**：chip_cores 外資持股、industry-level 加總

#### 🟢 `holding_shares_per_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| detail | JSONB | level taxonomy 包進來 |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`holding_shares_per`（PR #R4 後）
- **builder**：`silver/builders/holding_shares_per.py`
- **downstream（Cores）**：chip_cores 大戶 / 散戶分布

#### 🟢 `day_trading_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| day_trading_buy / day_trading_sell | BIGINT | |
| **day_trading_ratio** | NUMERIC(10, 4) | (buy+sell) × 100 / volume，衍生欄 |
| detail | JSONB | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`day_trading_tw` + `price_daily_fwd.volume`（計算 ratio）
- **builder**：`silver/builders/day_trading.py`
- **downstream（Cores）**：chip_cores 當沖比

#### 🟢 `valuation_daily_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| per | NUMERIC(10, 4) | |
| pbr | NUMERIC(10, 4) | |
| dividend_yield | NUMERIC(8, 4) | |
| **market_value_weight** | NUMERIC(10, 6) | 市值權重，衍生欄 |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`valuation_per_tw` + 大盤總市值（market_value_weight 計算）
- **builder**：`silver/builders/valuation.py`
- **downstream（Cores）**：chip_cores 市值權重、fundamental_cores 估值

---

### 4.3 S5_derived_fundamental（基本面衍生）

#### 🟢 `monthly_revenue_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| revenue | NUMERIC(20, 2) | |
| revenue_mom | NUMERIC(10, 4) | rename 自 Bronze.revenue_month |
| revenue_yoy | NUMERIC(10, 4) | rename 自 Bronze.revenue_year |
| detail | JSONB | country / create_time |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`monthly_revenue`（PR #R4 後）
- **builder**：`silver/builders/monthly_revenue.py`
- **downstream（Cores）**：fundamental_cores 月營收動能

#### 🟢 `financial_statement_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK |
| date | DATE NN | PK |
| type | TEXT NN | PK，income / balance / cashflow |
| detail | JSONB | 各會計科目 |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date, type)` — 4 維 per_stock_pk_ext
- **upstream**：`financial_statement`（PR #R4 後）
- **builder**：`silver/builders/financial_statement.py`
- **trigger function**：`trg_mark_financial_stmt_dirty`（special，Bronze.event_type ↔ Silver.type 對齊）
- **依賴關係**：S5 內部跨表，`financial_statement_derived` 需要 `monthly_revenue_derived` 對齊財報季度日期映射 → 故 builder 跑在 7b（不能與 monthly_revenue 平行）
- **downstream（Cores）**：fundamental_cores ROE / EPS / 毛利率等

---

### 4.4 S6_derived_environment（環境衍生）

#### 🟢 `taiex_index_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK，TAIEX / TPEx |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| detail | JSONB | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`market_ohlcv_tw`
- **builder**：`silver/builders/taiex_index.py`
- **downstream（Cores）**：environment_cores 大盤、indicator_cores 大盤指標

#### 🟢 `us_market_index_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| stock_id | TEXT NN | PK，SPY / ^VIX |
| date | DATE NN | PK |
| open / high / low / close | NUMERIC(15, 4) | |
| volume | BIGINT | |
| detail | JSONB | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`market_index_us`
- **builder**：`silver/builders/us_market_index.py`
- **downstream（Cores）**：environment_cores 美股 / VIX 環境因子

#### 🟢 `exchange_rate_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| currency | TEXT NN | PK |
| rate | NUMERIC(15, 6) | |
| detail | JSONB | |
| is_dirty / dirty_at | | |

- **PK**：`(market, date, currency)` — **不含 stock_id**
- **upstream**：`exchange_rate`
- **builder**：`silver/builders/exchange_rate.py`
- **trigger function**：`trg_mark_exchange_rate_dirty`（special，PK 含 currency）
- **downstream（Cores）**：environment_cores 匯率因子

#### 🟢 `market_margin_maintenance_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN | PK |
| date | DATE NN | PK |
| ratio | NUMERIC(8, 2) | |
| **total_margin_purchase_balance** | BIGINT | 衍生欄 |
| **total_short_sale_balance** | BIGINT | 衍生欄 |
| is_dirty / dirty_at | | |

- **PK**：`(market, date)` — all_market
- **upstream**：`market_margin_maintenance`
- **builder**：`silver/builders/market_margin.py`
- **trigger function**：`trg_mark_market_margin_dirty`（special，2-col PK 無 stock_id）
- **downstream（Cores）**：environment_cores 大盤融資維持率

#### 🟢 `business_indicator_derived`

| 欄位 | 型別 | 說明 |
|---|---|---|
| market | TEXT NN DEFAULT 'tw' | PK |
| stock_id | TEXT NN DEFAULT '_market_' | PK，sentinel |
| date | DATE NN | PK |
| leading_indicator | NUMERIC(10, 4) | |
| coincident_indicator | NUMERIC(10, 4) | |
| lagging_indicator | NUMERIC(10, 4) | |
| monitoring | INT | |
| monitoring_color | TEXT | |
| is_dirty / dirty_at | | |

- **PK**：`(market, stock_id, date)`
- **upstream**：`business_indicator_tw`（Bronze 2-col PK → Silver 3-col PK 用 sentinel `_market_`）
- **builder**：`silver/builders/business_indicator.py`
- **trigger function**：`trg_mark_business_indicator_dirty`（special，Bronze 2-col → Silver 3-col + sentinel）
- **downstream（Cores）**：environment_cores 景氣循環

---

## 五、系統表

> Cores 層**不應引用**這些表。列在這裡是為了完整性。

#### ⚙️ `schema_metadata` — Schema 版本

- 欄位：自由
- 用途：Rust binary 啟動時 assert schema version

#### ⚙️ `api_sync_progress` — API 斷點續傳進度

- PK：`(api_name, stock_id, segment_start)`
- 欄位：`status` (pending / completed / failed / empty / schema_mismatch) / `record_count` / `error_message` / `updated_at`
- 用途：collector 斷點續傳；PR #R4 後 `api_name` 將從 `_v3` 收回主名

#### 🟡 ⚙️ `stock_sync_status` — 同步狀態（重構後可能淘汰）

- PK：`(market, stock_id)`
- 欄位：`last_full_sync` / `last_incr_sync` / `fwd_adj_valid`
- 重構考量：`fwd_adj_valid` 已被 `price_daily_fwd.is_dirty` queue 取代（PR #20），整張表可考慮在重構期間移除或瘦身

---

## 六、Dirty / Dependency 流向圖

### 6.1 主要 dirty 傳遞鏈

```
┌─────────── B2_events ────────────┐
│  price_adjustment_events  WRITE  │
└────────────────┬─────────────────┘
                 │
                 │ trigger: trg_mark_fwd_silver_dirty
                 │ (整檔 stock 全段歷史 dirty,multiplier 倒推)
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│  price_daily_fwd          .is_dirty = TRUE  (該 stock 全段) │
│  price_weekly_fwd         .is_dirty = TRUE  (該 stock 全段) │
│  price_monthly_fwd        .is_dirty = TRUE  (該 stock 全段) │
│  price_limit_merge_events .is_dirty = TRUE  (該 stock 全段) │
└──────────────────────┬───────────────────────────────────────┘
                       │ S1_adjustment orchestrator pull
                       ▼
                  Rust binary
                  process_stock(stock_id) — 全段重算
                       │
                       ▼
                  is_dirty = FALSE
                  (Cores 可消費)
```

### 6.2 Bronze → Silver 直接 dirty（generic `trg_mark_silver_dirty`）

| Bronze 寫入 | trigger | Silver 表 dirty |
|---|---|---|
| `institutional_investors_tw` | `mark_institutional_derived_dirty` | `institutional_daily_derived` |
| `margin_purchase_short_sale_tw` | `mark_margin_derived_from_margin_dirty` | `margin_daily_derived` |
| `securities_lending_tw` | `mark_margin_derived_from_sbl_dirty` | `margin_daily_derived`（雙觸發） |
| `foreign_investor_share_tw` | `mark_foreign_holding_derived_dirty` | `foreign_holding_derived` |
| `holding_shares_per` | `mark_holding_shares_per_derived_dirty` | `holding_shares_per_derived` |
| `day_trading_tw` | `mark_day_trading_derived_dirty` | `day_trading_derived` |
| `valuation_per_tw` | `mark_valuation_derived_dirty` | `valuation_daily_derived` |
| `monthly_revenue` | `mark_monthly_revenue_derived_dirty` | `monthly_revenue_derived` |
| `market_ohlcv_tw` | `mark_taiex_index_derived_dirty` | `taiex_index_derived` |
| `market_index_us` | `mark_us_market_index_derived_dirty` | `us_market_index_derived` |

### 6.3 特殊 dirty trigger（PK 形狀不同）

| Bronze 寫入 | trigger function | Silver 表 dirty | 原因 |
|---|---|---|---|
| `financial_statement` | `trg_mark_financial_stmt_dirty` | `financial_statement_derived` | Bronze.event_type ↔ Silver.type，4-col PK |
| `exchange_rate` | `trg_mark_exchange_rate_dirty` | `exchange_rate_derived` | PK 含 currency 不含 stock_id |
| `market_margin_maintenance` | `trg_mark_market_margin_dirty` | `market_margin_maintenance_derived` | 2-col PK，無 stock_id |
| `business_indicator_tw` | `trg_mark_business_indicator_dirty` | `business_indicator_derived` | Bronze 2-col → Silver 3-col + sentinel `_market_` |

### 6.4 Silver 內部依賴（跨 Silver builder）

```
┌─────────────────────────────────────┐
│  monthly_revenue_derived  (S5, 7a) │  ← 先跑
└────────────────┬────────────────────┘
                 │ 季度日期對齊
                 ▼
┌─────────────────────────────────────┐
│ financial_statement_derived (S5,7b)│  ← 後跑
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│  price_daily_fwd  (S1)             │  ← 先算
└────────────────┬────────────────────┘
                 │ volume 計算 ratio
                 ▼
┌─────────────────────────────────────┐
│  day_trading_derived  (S4, 7a-late)│  ← 後算（day_trading_ratio 衍生欄）
└─────────────────────────────────────┘
```

> 重構後 orchestrator 必須顯式宣告依賴，見 `data_refactor_plan.md` §4.3 Step 2 的 `Phase.depends_on`。

### 6.5 整體 Dirty 排程順序

orchestrator 應按以下順序處理 dirty queue：

```
1. S1_adjustment            (Rust binary)
   └─ 拉 price_daily_fwd.is_dirty → 跑 → reset dirty

2. S4_derived_chip + S5_monthly_revenue + S6_derived_environment   (並行)
   └─ 各自拉對應 derived.is_dirty → 跑 builder → reset dirty
   └─ ⚠️ 例外:day_trading_derived 必須等 S1 完成（需要 fwd.volume）

3. S5_financial_statement   (依賴 monthly_revenue_derived)
   └─ 拉 financial_statement_derived.is_dirty → 跑 → reset dirty

4. M3 / Cores 階段          (PyO3 → Rust 純計算,Cores 文件定義)
```

---

## 七、退場表清單

### 7.1 PR #R3 後 rename（保留觀察 21~60 天）

| 表 | rename 為 |
|---|---|
| `holding_shares_per` (舊 v2.0) | `holding_shares_per_legacy_v2` |
| `financial_statement` (舊 v2.0) | `financial_statement_legacy_v2` |
| `monthly_revenue` (舊 v2.0) | `monthly_revenue_legacy_v2` |

### 7.2 PR #R6 後 DROP（永久刪除）

| 表 | 替代品 |
|---|---|
| `holding_shares_per_legacy_v2` | `holding_shares_per`（PR #R4 後從 `_tw` 升格） |
| `financial_statement_legacy_v2` | `financial_statement`（同上） |
| `monthly_revenue_legacy_v2` | `monthly_revenue`（同上） |

### 7.3 v2.0 籌碼舊表（待後續評估退場）

> 以下舊表已被 reverse-pivot `*_tw` 系列取代，silver builder 早已切換來源，但本份重構 plan（PR #R1~#R6）尚未涵蓋其 DROP 步驟。建議在 PR #R6 後另起一輪退場 PR：

| 退場候選 | 替代品 |
|---|---|
| `institutional_daily` | `institutional_investors_tw` |
| `margin_daily` | `margin_purchase_short_sale_tw` |
| `foreign_holding` | `foreign_investor_share_tw` |
| `day_trading` | `day_trading_tw` |
| `valuation_daily` | `valuation_per_tw` |
| `index_weight_daily` | （無下游使用，可直接 DROP） |

### 7.4 不退場但需重新評估

| 表 | 評估點 |
|---|---|
| `stock_sync_status` | `fwd_adj_valid` 已由 dirty queue 取代，整張表可瘦身 |
| `market_index_tw` vs `market_ohlcv_tw` | 兩者並存但職責重疊；長期應只保 `market_ohlcv_tw` |

---

## 八、給 Cores 層的接點清單

> Cores 層設計時應**只讀以下表**，不讀 Bronze 或 legacy 表。

### 8.1 chip_cores 接點

| 讀取表（Silver） | 主要欄位 |
|---|---|
| `institutional_daily_derived` | foreign_buy / sell / dealer / investment_trust / gov_bank_net |
| `margin_daily_derived` | margin_balance / short_balance / SBL 6 欄 |
| `foreign_holding_derived` | foreign_holding_shares / ratio |
| `holding_shares_per_derived` | detail（level taxonomy） |
| `day_trading_derived` | day_trading_buy / sell / **day_trading_ratio** |
| `valuation_daily_derived` | per / pbr / dividend_yield / **market_value_weight** |

### 8.2 fundamental_cores 接點

| 讀取表（Silver） | 主要欄位 |
|---|---|
| `monthly_revenue_derived` | revenue / revenue_mom / revenue_yoy |
| `financial_statement_derived` | type (income/balance/cashflow) / detail（各會計科目） |

### 8.3 environment_cores 接點

| 讀取表（Silver） | 主要欄位 |
|---|---|
| `taiex_index_derived` | OHLCV |
| `us_market_index_derived` | OHLCV（SPY / VIX） |
| `exchange_rate_derived` | rate（per currency） |
| `market_margin_maintenance_derived` | ratio / total_margin_purchase_balance / total_short_sale_balance |
| `business_indicator_derived` | leading / coincident / lagging / monitoring / monitoring_color |

### 8.4 indicator_cores 接點

| 讀取表（Silver） | 主要欄位 |
|---|---|
| `price_daily_fwd` | OHLCV + cumulative_adjustment_factor + cumulative_volume_factor |
| `price_weekly_fwd` | OHLCV |
| `price_monthly_fwd` | OHLCV |
| `price_limit_merge_events` | merge_type / detail |

### 8.5 wave_cores 接點

| 讀取表（Silver） | 主要欄位 |
|---|---|
| `price_daily_fwd` | OHLCV（Monowave Detection 主要來源） |
| `price_weekly_fwd` | OHLCV（週級轉折） |
| `price_monthly_fwd` | OHLCV（月級結構） |

### 8.6 跨 Cores 共用接點

| 讀取表（Silver） | 用途 |
|---|---|
| `price_daily_fwd` | **所有 stock-level Cores 的事實基底** |
| `trading_date_ref` | 交易日過濾、prev_trading_day 計算 |
| `stock_info_ref` | 主檔過濾、industry 分組（不查 delisting） |
| `stock_suspension_events` | 個股停牌過濾 |
| `price_adjustment_events` | 除權息事件查詢（僅事件，不算復權 — 已在 fwd） |

---

## 附錄：表清單索引

### Bronze 層（共 21 張，重構後）

| 階段 | 表 | 用途 |
|---|---|---|
| B0 | trading_date_ref | 交易日曆 |
| B1 | stock_info_ref | 股票主檔 |
| B1 | market_index_tw | 加權指數（close） |
| B1 | market_ohlcv_tw | 加權指數（OHLCV） |
| B2 | price_adjustment_events | 5 種 event 共寫 |
| B2 | stock_suspension_events | 個股停牌 |
| B2 | _dividend_policy_staging | 暫存表 |
| B3 | price_daily | 原始日 K |
| B3 | price_limit | 漲跌停 |
| B4 | institutional_investors_tw | 三大法人 |
| B4 | margin_purchase_short_sale_tw | 融資融券 |
| B4 | securities_lending_tw | 借券 |
| B4 | foreign_investor_share_tw | 外資持股 |
| B4 | day_trading_tw | 當沖 |
| B4 | valuation_per_tw | 估值 |
| B4 | holding_shares_per | 股權分散（PR #R4 升格） |
| B5 | financial_statement | 財報三表（PR #R4 升格） |
| B5 | monthly_revenue | 月營收（PR #R4 升格） |
| B6 | market_index_us | 美股 |
| B6 | exchange_rate | 匯率 |
| B6 | institutional_market_daily | 大盤法人 |
| B6 | market_margin_maintenance | 大盤融資維持率 |
| B6 | fear_greed_index | 恐懼貪婪 |
| B6 | business_indicator_tw | 景氣指標 |

### Silver 層（共 15 張，重構後）

| 階段 | 表 | producer |
|---|---|---|
| S1 | price_daily_fwd | Rust binary |
| S1 | price_weekly_fwd | Rust binary |
| S1 | price_monthly_fwd | Rust binary |
| S1 | price_limit_merge_events | Rust binary |
| S4 | institutional_daily_derived | builder |
| S4 | margin_daily_derived | builder |
| S4 | foreign_holding_derived | builder |
| S4 | holding_shares_per_derived | builder |
| S4 | day_trading_derived | builder |
| S4 | valuation_daily_derived | builder |
| S5 | monthly_revenue_derived | builder |
| S5 | financial_statement_derived | builder |
| S6 | taiex_index_derived | builder |
| S6 | us_market_index_derived | builder |
| S6 | exchange_rate_derived | builder |
| S6 | market_margin_maintenance_derived | builder |
| S6 | business_indicator_derived | builder |

---

**文件結束。**
**下一份**：`cores_refactor_plan.md` — 接續本文件 §八「給 Cores 層的接點清單」，定義 M3 / Cores 五大類的 trait、Output schema、Fact 寫入策略、執行排程。
