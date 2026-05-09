# API Pipeline Reference

**版本**:v3.2 r1(alembic head `u0v1w2x3y4z5` / 2026-05-09 v1.26 nice-to-haves merged)
**用途**:配合 `m2Spec/layered_schema_post_refactor.md`(spec)看 — spec 是 schema 規範,本檔是 **collector.toml entry × code path × 現行狀態**索引
**結構**:對齊 spec §三/§四/§六/§七/§八 + 補充 code 索引 / 加 entry 流程

> **Spec 為主、本檔為輔**:任何 schema/PK/欄位細節以 spec 為準。本檔重點:
> 1. 哪個 [[api]] entry 寫入哪張 Bronze 表
> 2. 對應 code 檔/函式入口
> 3. 現行 v1.26 狀態(R3 升格主名 / R4 entry _legacy 後綴 / Rust dirty queue self-pull / margin UNION)

---

## 目錄

1. [兩層架構總覽](#1-兩層架構總覽)
2. [Bronze 層 — entry × table × code 對映](#2-bronze-層--entry--table--code-對映)
3. [Silver 層 — builder × table × code 對映](#3-silver-層--builder--table--code-對映)
4. [系統表](#4-系統表)
5. [Bronze ingestion 通用流程](#5-bronze-ingestion-通用流程)
6. [Silver computation 通用流程](#6-silver-computation-通用流程)
7. [Dirty / Dependency 流向(spec §六)](#7-dirty--dependency-流向spec-六)
8. [退場狀態(spec §七)](#8-退場狀態spec-七)
9. [Cores 接點清單(spec §八,M3 reference)](#9-cores-接點清單spec-八m3-reference)
10. [Code 檔案索引](#10-code-檔案索引)
11. [Schema 統計](#11-schema-統計)
12. [命名 / 慣例](#12-命名--慣例)
13. [加 entry / 改 schema 流程](#13-加-entry--改-schema-流程)

---

## 1. 兩層架構總覽

### 1.1 兩層定位(對齊 spec §1.5)

| 層 | 責任 | 寫入時機 | 主要 module |
|---|---|---|---|
| **Bronze** | FinMind raw 直入 + 事件登錄(無計算) | collector backfill / incremental | `src/bronze/phase_executor.py` + `api_client.py` + `field_mapper.py` |
| **Silver(進階計算)** | 後復權 / 跨表 join / pivot / pack / 衍生欄(複雜計算) | orchestrator dirty queue pull(PR #20 trigger 觸發) | `src/silver/orchestrator.py` + `silver/builders/*.py` + Rust binary |
| (M3 / Cores) | 規則 / 算式套用(只讀 Silver,不算復權)| (未動工) | `m3Spec/`(預留) |

**強約束**(spec §1.5):「在 Silver 層判斷 Wave 結構、在 Cores 層做後復權」視為設計錯誤。

### 1.2 整體架構圖(spec §二)

```
   FinMind / 外部資料源
         │ (HTTP fetch)
         ▼
┌─── BRONZE ────────────────────────────────────────────────────────────────┐
│                                                                            │
│  B0_calendar    trading_date_ref                                           │
│  B1_meta        stock_info_ref · market_index_tw · market_ohlcv_tw         │
│  B2_events      price_adjustment_events · stock_suspension_events          │
│                 _dividend_policy_staging                                   │
│  B3_price_raw   price_daily · price_limit                                  │
│  B4_chip        institutional_investors_tw · margin_purchase_short_sale_tw │
│                 securities_lending_tw · short_sale_securities_lending_tw   │
│                 foreign_investor_share_tw · day_trading_tw                 │
│                 valuation_per_tw · holding_shares_per                      │
│                 government_bank_buy_sell_tw [DISABLED]                     │
│  B5_fundamental financial_statement · monthly_revenue                      │
│  B6_environment market_index_us · exchange_rate                            │
│                 institutional_market_daily · market_margin_maintenance     │
│                 total_margin_purchase_short_sale_tw                        │
│                 fear_greed_index · business_indicator_tw                   │
│                                                                            │
└─────────────┬──────────────────────────────────────────────────────────────┘
              │ (PG trigger / Rust binary)
              ▼
┌─── SILVER(進階計算)────────────────────────────────────────────────────┐
│                                                                            │
│  S1_adjustment        price_daily_fwd · price_weekly_fwd                   │
│  (Rust binary)        price_monthly_fwd · price_limit_merge_events         │
│                                                                            │
│  S4_derived_chip      institutional_daily_derived · margin_daily_derived   │
│  (SQL builders)       foreign_holding_derived · holding_shares_per_derived │
│                       day_trading_derived · valuation_daily_derived        │
│                                                                            │
│  S5_derived_fundamental  monthly_revenue_derived                           │
│  (SQL builders)          financial_statement_derived                       │
│                                                                            │
│  S6_derived_environment  taiex_index_derived · us_market_index_derived     │
│  (SQL builders)          exchange_rate_derived                             │
│                          market_margin_maintenance_derived                 │
│                          business_indicator_derived                        │
│                                                                            │
└─────────────┬──────────────────────────────────────────────────────────────┘
              │ (PyO3 → Rust 純計算,未動工)
              ▼
   M3 / CORES(chip / fundamental / environment / indicator / wave)
   (m3Spec/ 預留;接點見 §9)
```

---

## 2. Bronze 層 — entry × table × code 對映

對齊 spec §三 的 B0~B6 分類。每張表列:**寫入 entry / dataset / code path / 下游 trigger**。Schema 細節(PK / 欄位 / 索引)直接看 spec §三 對應段。

### 2.1 B0_calendar(spec §3.1,1 張)

| 表 | 寫入 entry | dataset | code path | dirty trigger |
|---|---|---|---|---|
| `trading_date_ref` | `trading_calendar` | TaiwanStockTradingDate | `phase_executor._run_api` → `field_mapper.transform`(無 rename)→ `db.upsert` | 無 |

**下游**:`aggregators._filter_to_trading_days`(週六鬼資料過濾)、`rust_compute/main.rs:load_trading_dates`、`silver/_common.get_trading_dates`。

### 2.2 B1_meta(spec §3.2,3 張)

| 表 | 寫入 entry | dataset | code path 特殊處理 | dirty trigger |
|---|---|---|---|---|
| `stock_info_ref` | `stock_info` + `stock_delisting` | TaiwanStockInfo + TaiwanStockDelisting | `stock_delisting` 走 `merge_strategy="update_delist_date"` 客製 UPDATE;`updated_at` schema DEFAULT NOW() + UPSERT 強制 NOW() | 無 |
| `market_index_tw` | `market_index_tw` | TaiwanStockTotalReturnIndex | `per_stock_fixed` × `["TAIEX","TPEx"]` | 無 |
| `market_ohlcv_tw` | `market_ohlcv_v3` | TaiwanStockPrice(把 TAIEX/TPEx 當 stock 抓) | `per_stock_fixed` × `["TAIEX","TPEx"]`;`max→high` / `min→low` / `Trading_Volume→volume`,Trading_money/turnover/spread → detail | → `taiex_index_derived`(generic `trg_mark_silver_dirty`) |

### 2.3 B2_events(spec §3.3,3 張)

| 表 | 寫入 entry(event_type) | code path | dirty trigger |
|---|---|---|---|
| `price_adjustment_events` | 4 entry 共寫:`dividend_result`(dividend) / `capital_reduction` / `split_price` / `par_value_change`,加 `dividend_policy` 經 post_process | `field_mapper.computed_fields = ["volume_factor", "cash_dividend", "stock_dividend"]`;`field_mapper._compute_dividend_fields()` 從 detail 拆 | → **4 fwd 表整檔 dirty**(`trg_mark_fwd_silver_dirty` UPDATE 全段歷史 SET is_dirty=TRUE) |
| `stock_suspension_events` | `stock_suspension` | TaiwanStockSuspended;`field_mapper`(`date → suspension_date`)| 無(M3 prev_trading_day 用,未動工) |
| `_dividend_policy_staging` | `dividend_policy` | 21 個 PascalCase 全 pack 進 detail JSONB → **`post_process="dividend_policy_merge"`** 觸發 `post_process.dividend_policy_merge(db, stock_id)`(`src/post_process.py`),拆權息事件入 `price_adjustment_events` + 偵測純現增 | 無(staging 後不留) |

### 2.4 B3_price_raw(spec §3.4,2 張)

| 表 | 寫入 entry | dataset | code path | dirty trigger |
|---|---|---|---|---|
| `price_daily` | `price_daily` | TaiwanStockPrice | `field_mapper`(`max→high` / `min→low` / `Trading_Volume→volume` / `Trading_money→_trading_money` 等) | 無(fwd 由 events 觸發) |
| `price_limit` | `price_limit` | TaiwanStockPriceLimit | `field_mapper`(`reference_price→_reference_price` 進 detail) | 無 |

> ⚠️ Cores **不應直接讀 `price_daily`**(改讀 `price_daily_fwd`)。`price_daily` 僅供 Rust binary 計算 fwd 時使用。

### 2.5 B4_chip(spec §3.5,9 張 = spec 7 + 2 PR #21-B 額外)

#### Spec §3.5 標準 7 張(per_stock 或 per_stock_pk_ext)

| 表 | 寫入 entry | dataset | code path 特殊處理 | dirty trigger |
|---|---|---|---|---|
| `institutional_investors_tw` | (PR #18 reverse-pivot 反推自 `institutional_daily` legacy) | — | `scripts/reverse_pivot_institutional.py`;**v3 dual-write entry 待加(post-R6)** | → `institutional_daily_derived` |
| `margin_purchase_short_sale_tw` | (PR #18 reverse-pivot 反推自 `margin_daily` legacy)| — | 同上 | → `margin_daily_derived` |
| `foreign_investor_share_tw` | (PR #18 reverse-pivot)| — | 同上 | → `foreign_holding_derived` |
| `day_trading_tw` | (PR #18 reverse-pivot)| — | 同上 | → `day_trading_derived` |
| `valuation_per_tw` | (PR #18 reverse-pivot)| — | 同上 | → `valuation_daily_derived` |
| `holding_shares_per`(R3 主名)| `holding_shares_per_v3` | TaiwanStockHoldingSharesPer | PR #18.5 raw,`field_rename = {"HoldingSharesLevel" = "holding_shares_level"}`,1 row/level | → `holding_shares_per_derived` |
| `securities_lending_tw` | `securities_lending` | TaiwanStockSecuritiesLending | v3.2 B-5;PK 5 欄(議借/競價 × fee_rate)| → `margin_daily_derived`(雙觸發) |

#### v3.2 額外(PR #21-B,2 張)

| 表 | 寫入 entry | dataset | 備註 | dirty trigger |
|---|---|---|---|---|
| `short_sale_securities_lending_tw` | `short_sale_securities_lending_v3` | TaiwanDailyShortSaleBalances | PR #21-B;`field_rename` 取 `SBLShortSales*` 3 欄(short_sales/returns/current_day_balance),其他 12 欄 PRAGMA drop | → `margin_daily_derived` |
| `government_bank_buy_sell_tw` | `government_bank_buy_sell_v3` **[DISABLED]** | TaiwanStockGovernmentBankBuySell | 需 FinMind sponsor tier(user 是 backer);schema + trigger 已落,待升 tier 切回 enabled | → `institutional_daily_derived`(LEFT JOIN,缺 row → gov_bank_net=NULL)|

### 2.6 B5_fundamental(spec §3.6,2 張)

| 表 | 寫入 entry | dataset | event_type | dirty trigger |
|---|---|---|---|---|
| `financial_statement`(R3 主名)| `financial_income_v3` / `financial_balance_v3` / `financial_cashflow_v3` | TaiwanStockFinancialStatements / BalanceSheet / CashFlowsStatement | income / balance / cashflow | → `financial_statement_derived`(special:Bronze.event_type ↔ Silver.type)|
| `monthly_revenue`(R3 主名)| `monthly_revenue_v3` | TaiwanStockMonthRevenue | — | → `monthly_revenue_derived` |

> **rename 慣例**:Bronze 保留 FinMind 原欄名(`revenue_year` / `revenue_month`),Silver builder 才 rename 為 `revenue_yoy` / `revenue_mom`。

### 2.7 B6_environment(spec §3.7,7 張 = spec 6 + 1 PR #21-B 額外)

#### Spec §3.7 標準 6 張

| 表 | 寫入 entry | dataset | code path 特殊處理 | dirty trigger |
|---|---|---|---|---|
| `market_index_us` | `market_index_us` | USStockPrice | `per_stock_fixed` × `["SPY","^VIX"]` | → `us_market_index_derived` |
| `exchange_rate` | `exchange_rate` | TaiwanExchangeRate | `per_stock_fixed` × 4 幣(USD/CNY/JPY/AUD);必須帶 data_id 才回完整時序;PK `(market, date, currency)` 不含 stock_id | → `exchange_rate_derived`(special:PK 含 currency)|
| `institutional_market_daily` | `institutional_market` | TaiwanStockTotalInstitutionalInvestors | `aggregation = pivot_institutional_market`(每日 3 列 → 1 列)| 無(目前無 Silver derived) |
| `market_margin_maintenance` | `market_margin` | TaiwanTotalExchangeMarginMaintenance | `field_rename = {"TotalExchangeMarginMaintenance" = "ratio"}` | → `market_margin_maintenance_derived`(special:2-col PK)|
| `fear_greed_index` | `fear_greed` | CnnFearGreedIndex | `field_rename = {"fear_greed" = "score", "fear_greed_emotion" = "label"}` | 無 |
| `business_indicator_tw` | `business_indicator` | TaiwanBusinessIndicator | v3.2 B-6;月頻;leading/coincident/lagging 加 `_indicator` 後綴避 PG 保留字 | → `business_indicator_derived`(special:注入 sentinel `_market_`)|

#### v3.2 額外(PR #21-B,1 張)

| 表 | 寫入 entry | dataset | 備註 | dirty trigger |
|---|---|---|---|---|
| `total_margin_purchase_short_sale_tw` | `total_margin_purchase_short_sale_v3` | TaiwanStockTotalMarginPurchaseShortSale | PR #21-B;FinMind 是 pivoted-by-row(`name ∈ {MarginPurchase, ShortSale}`),Bronze PK 加 `name`;builder 走 pivot | → `market_margin_maintenance_derived`(reuse `trg_mark_market_margin_dirty`)|

---

## 3. Silver 層 — builder × table × code 對映

對齊 spec §四 的 S1 / S4 / S5 / S6 分類。每張 Silver 表列:**Bronze 來源 / builder file / 衍生欄 / dirty 行為**。

### 3.1 S1_adjustment(spec §4.1,Rust binary,4 張)

> **唯一執行模型**:Rust binary,無 SQL builder。觸發 chain:`price_adjustment_events` 寫入 → `trg_mark_fwd_silver_dirty` → 4 fwd 表整檔 dirty → orchestrator 7c pull → Rust process_stock 全段重算。

| Silver 表 | Bronze 來源 | producer | 衍生欄(spec §4.1)|
|---|---|---|---|
| `price_daily_fwd` | `price_daily` + `price_adjustment_events` | Rust(`process_stock`) | OHLCV(已復權) + cumulative_adjustment_factor + cumulative_volume_factor + is_adjusted + adjustment_factor + is_dirty/dirty_at |
| `price_weekly_fwd` | `price_daily_fwd`(Rust 同 run)| Rust ISO week 聚合 | OHLCV + is_dirty/dirty_at |
| `price_monthly_fwd` | `price_daily_fwd`(Rust 同 run)| Rust 月聚合 | OHLCV + is_dirty/dirty_at |
| `price_limit_merge_events` | `price_limit` + `price_adjustment_events` | Rust(漲跌停隨 events 調整)| merge_type / detail / is_dirty/dirty_at |

**Rust binary**:`rust_compute/src/main.rs`
- `resolve_stock_ids`:`--stocks` 傳入優先;否則 `SELECT DISTINCT stock_id FROM price_daily_fwd WHERE is_dirty=TRUE`(v1.26 起)
- **multiplier 拆兩個**(v1.8):`price_multiplier`(從 AF) + `volume_multiplier`(從 vf)
- 永遠全段重算(後復權 multiplier 從尾端倒推,partial 邏輯上錯)

### 3.2 S4_derived_chip(spec §4.2,SQL builder,6 張)

> 執行模型:`silver/builders/*.py` 純 SQL(`fetch_bronze` → 業務邏輯 → `upsert_silver`)。觸發:Bronze 寫入 → trigger mark Silver dirty → orchestrator 7a pull → builder 跑 → 自動 reset is_dirty。

| Silver 表 | builder file | Bronze 來源 | 衍生欄 / 特殊處理 |
|---|---|---|---|
| `institutional_daily_derived` | `silver/builders/institutional.py` | `institutional_investors_tw` + `government_bank_buy_sell_tw`(LEFT JOIN)| 5 類法人 pivot(10 buy/sell);`gov_bank_net = buy - sell`(任一 NULL → NULL)|
| `margin_daily_derived` | `silver/builders/margin.py` | `margin_purchase_short_sale_tw` ∪ `short_sale_securities_lending_tw`(v1.26 UNION 主∪副 keys 消 stub) | 6 stored + detail JSONB(8 keys) + 3 alias `margin_short_sales_*` + 3 SBL `sbl_short_sales_*` |
| `foreign_holding_derived` | `silver/builders/foreign_holding.py` | `foreign_investor_share_tw` | 2 stored + detail JSONB pack(9 keys)|
| `holding_shares_per_derived` | `silver/builders/holding_shares_per.py` | `holding_shares_per`(R3 主名)| N rows/level → 1 row/(stock,date),detail JSONB pack 各 level |
| `day_trading_derived` | `silver/builders/day_trading.py` | `day_trading_tw` + `price_daily`(LEFT JOIN volume) | 2 stored + detail + 衍生欄 `day_trading_ratio = dt_volume / pd_volume × 100` |
| `valuation_daily_derived` | `silver/builders/valuation.py` | `valuation_per_tw` + `price_daily` + `foreign_investor_share_tw` | 3 stored + 衍生欄 `market_value_weight = (close × total_issued) / SUM_market_date` |

### 3.3 S5_derived_fundamental(spec §4.3,SQL builder,2 張)

| Silver 表 | builder file | Bronze 來源 | 備註 |
|---|---|---|---|
| `monthly_revenue_derived` | `silver/builders/monthly_revenue.py` | `monthly_revenue`(R3 主名) | rename `revenue_year → revenue_yoy` / `revenue_month → revenue_mom`;country/create_time → detail |
| `financial_statement_derived` | `silver/builders/financial_statement.py` | `financial_statement`(R3 主名) | event_type → type;origin_name → detail JSONB pack;**跨表依賴 monthly_revenue → 跑在 7b**(spec §6.4) |

### 3.4 S6_derived_environment(spec §4.4,SQL builder,5 張)

| Silver 表 | builder file | Bronze 來源 | 衍生欄 / 特殊處理 |
|---|---|---|---|
| `taiex_index_derived` | `silver/builders/taiex_index.py` | `market_ohlcv_tw` | OHLCV 1:1 |
| `us_market_index_derived` | `silver/builders/us_market_index.py` | `market_index_us` | OHLCV 1:1(SPY/^VIX)|
| `exchange_rate_derived` | `silver/builders/exchange_rate.py` | `exchange_rate` | PK 含 currency,rate + detail 1:1;`fetch_bronze(order_by="market, date, currency")` |
| `market_margin_maintenance_derived` | `silver/builders/market_margin.py` | `market_margin_maintenance` ∪ `total_margin_purchase_short_sale_tw`(v1.26 UNION) | ratio 1:1 + 衍生欄 `total_margin_purchase_balance` / `total_short_sale_balance`(pivot from副 Bronze name='MarginPurchase'/'ShortSale')|
| `business_indicator_derived` | `silver/builders/business_indicator.py` | `business_indicator_tw` | PK 注入 sentinel `stock_id='_market_'`(對齊 Silver 3-col PK convention)|

### 3.5 共用 helper(`silver/_common.py`)

| 函式 | 用途 |
|---|---|
| `fetch_bronze(db, table, stock_ids=, where=, order_by=)` | 統一 SELECT Bronze + stock filter |
| `upsert_silver(db, table, rows, pk_cols=)` | 批次 UPSERT 自帶 `is_dirty=FALSE / dirty_at=NULL` |
| `reset_dirty(db, table, pks, pk_cols)` | 顯式 reset(備用)|
| `get_trading_dates(db)` | 一次讀 `trading_date_ref` 過濾鬼資料 |

---

## 4. 系統表

> Cores 層**不應引用**這些表(spec §五)。

| 表 | PK | 用途 |
|---|---|---|
| `schema_metadata` | key | Rust binary 啟動時 assert `schema_version='3.2'` |
| `api_sync_progress` | (api_name, stock_id, segment_start) | collector 5-status 斷點續傳追蹤(`pending / completed / failed / empty / schema_mismatch`)|
| `stock_sync_status` | (market, stock_id) | `fwd_adj_valid` deprecated(已被 `price_daily_fwd.is_dirty` queue 取代,留作 belt-and-suspenders)|

---

## 5. Bronze ingestion 通用流程

```
CLI: python src/main.py {backfill,incremental} [--stocks ...]
  │
  └─→ src/main.py:_run_collector
        │
        └─→ src/bronze/phase_executor.PhaseExecutor.run(mode)
              │
              └─ for each enabled [[api]] entry:_run_api(api_config, mode)
                    │
                    ├─ _resolve_stock_ids(api_config) — 依 param_mode 取 stock 列表
                    │     ├─ all_market / all_market_no_id → ["__ALL__"](sentinel)
                    │     ├─ per_stock_fixed → fixed_ids(SPY/^VIX/TAIEX/USD/...)
                    │     └─ per_stock → fixed_ids 優先,否則 stock_resolver 動態清單
                    │
                    └─ for stock_id, segment in (stock_ids × segments):
                         ├─ sync_tracker.is_completed → 已完成跳過
                         ├─ api_client.fetch — aiohttp + rate_limiter(1600/h, 2250ms, 429 cooldown 120s)
                         ├─ field_mapper.transform(api_config, raw_records)
                         │    ├─ field_rename + detail_fields pack JSONB
                         │    ├─ computed_fields(volume_factor / cash_dividend / stock_dividend)
                         │    └─ schema_mismatch 偵測(novel fields warning)
                         ├─ aggregators.apply_aggregation(...) — 若 aggregation 有設
                         │    (pivot_institutional / pivot_institutional_market /
                         │     pack_holding_shares / pack_financial)
                         ├─ db.upsert(target_table, rows) — PRAGMA(information_schema)欄位過濾
                         └─ post_process — 若 post_process 有設(僅 dividend_policy_merge)

# Rust 後復權(無 [[api]] entry,bronze/phase_executor 直接派工)
  _run_phase4(mode)
    ├─ mode=="backfill" → 全市場 self._stock_list 派 Rust
    └─ mode=="incremental" → _fetch_dirty_fwd_stocks() 拉 dirty list,0 dirty → skip(v1.26)
  └─→ rust_bridge.run_phase4 → tw_stock_compute binary
```

**aggregators**(`src/aggregators.py`):

| function | 用途 | 對應 entry |
|---|---|---|
| `pivot_institutional` | 5 類法人 1 row × 10 col | `institutional_daily` |
| `pivot_institutional_market` | 全市場 pivot | `institutional_market` |
| `pack_holding_shares` | 多 level → detail JSONB pack | `holding_shares_per_legacy` |
| `pack_financial` | 中→英 origin_name → detail JSONB | `financial_*_legacy` 三 entry |
| `_filter_to_trading_days` | 過濾 FinMind 週六鬼資料 | `pivot_institutional` 內呼叫 |

---

## 6. Silver computation 通用流程

```
CLI: python src/main.py silver phase {7a,7b,7c} [--stocks ...] [--full-rebuild]
  │
  └─→ src/main.py:_run_silver
        │
        └─→ src/silver/orchestrator.SilverOrchestrator.run(phases, ...)
              │
              ├─ 7a:12 個獨立 builder 串列跑(PostgresWriter 單 conn,thread-safety 限制)
              │     └─→ silver/builders/{name}.run(db, stock_ids, full_rebuild)
              │           ├─ fetch_bronze(table, stock_ids=, where=, order_by=)
              │           ├─ pivot/pack/UNION 邏輯
              │           └─ upsert_silver(silver_table, rows, pk_cols=)
              │                └─ 自動帶 is_dirty=FALSE / dirty_at=NULL
              │
              ├─ 7b:financial_statement(跨表依賴 monthly_revenue 對齊)
              │
              └─ 7c:rust_bridge.run_phase4 — tw_market_core 系列(S1)
                    ├─ price_daily_fwd / price_weekly_fwd / price_monthly_fwd
                    ├─ price_limit_merge_events
                    └─ dirty queue(orchestrator):
                         None + full_rebuild=False → SELECT DISTINCT stock_id
                                                     FROM price_daily_fwd
                                                     WHERE is_dirty=TRUE
                         0 dirty → skip Rust dispatch
```

**Spec §6.5 整體 dirty 排程順序**(現行 orchestrator 對應):
1. **S1_adjustment**(Rust)→ orchestrator 7c
2. **S4 + S5_monthly_revenue + S6**(可平行)→ orchestrator 7a(目前串列,平行優化留 follow-up)
3. **S5_financial_statement**(依賴 monthly_revenue)→ orchestrator 7b
4. M3 / Cores(未動工)

⚠️ 現行 7a 對 day_trading_derived 用 `price_daily.volume`(Bronze)而非 `price_daily_fwd.volume`(Silver),與 spec §6.4 略有 deviation。後復權後 volume 跟 raw 不同(stock_dividend 切割),理論上 spec 對。修正排程或修 builder 用 fwd.volume 留 follow-up。

---

## 7. Dirty / Dependency 流向(spec §六)

### 7.1 主要 dirty 傳遞鏈(B2_events → S1_adjustment fwd 4 張)

```
price_adjustment_events  WRITE(收新除權息事件)
         │
         │ trigger: trg_mark_fwd_silver_dirty
         │          (整檔 stock 全段歷史 dirty,multiplier 倒推設計)
         ▼
price_daily_fwd / price_weekly_fwd /
price_monthly_fwd / price_limit_merge_events
         .is_dirty = TRUE  (該 stock 全段)
         │
         │ orchestrator 7c pull(silver/orchestrator._run_7c)
         │ OR bronze/phase_executor._run_phase4(incremental,v1.26)
         ▼
Rust binary process_stock(stock_id) — 全段重算
         │
         ▼
DELETE + INSERT price_daily_fwd → 新 row is_dirty=FALSE(自動 drain)
```

### 7.2 Bronze → Silver generic dirty(10 個 trigger)

`trg_mark_silver_dirty(silver_table)` 共用 function,Bronze 3-col PK 直接 pivot 進 Silver 同 PK:

| Bronze | Silver |
|---|---|
| `institutional_investors_tw` | `institutional_daily_derived` |
| `margin_purchase_short_sale_tw` | `margin_daily_derived` |
| `securities_lending_tw` | `margin_daily_derived`(雙觸發) |
| `foreign_investor_share_tw` | `foreign_holding_derived` |
| `holding_shares_per`(R3 主名) | `holding_shares_per_derived` |
| `day_trading_tw` | `day_trading_derived` |
| `valuation_per_tw` | `valuation_daily_derived` |
| `monthly_revenue`(R3 主名) | `monthly_revenue_derived` |
| `market_ohlcv_tw` | `taiex_index_derived` |
| `market_index_us` | `us_market_index_derived` |

### 7.3 Special dirty trigger(PK 形狀不同,5 個)

| Bronze | trigger function | Silver | 變體 |
|---|---|---|---|
| `financial_statement`(R3 主名) | `trg_mark_financial_stmt_dirty` | `financial_statement_derived` | event_type → type(4-col PK) |
| `exchange_rate` | `trg_mark_exchange_rate_dirty` | `exchange_rate_derived` | PK 含 currency 不含 stock_id |
| `market_margin_maintenance` | `trg_mark_market_margin_dirty` | `market_margin_maintenance_derived` | 2-col PK(無 stock_id)|
| `business_indicator_tw` | `trg_mark_business_indicator_dirty` | `business_indicator_derived` | Bronze 2-col → Silver 3-col + sentinel `_market_` |
| `price_adjustment_events` | `trg_mark_fwd_silver_dirty` | 4 fwd 表整檔 dirty | UPDATE 全段歷史 SET is_dirty=TRUE |

### 7.4 v3.2 副 Bronze trigger(PR #21-B,3 個)

| Bronze | Silver | trigger function |
|---|---|---|
| `government_bank_buy_sell_tw`(disabled)| `institutional_daily_derived` | `trg_mark_silver_dirty('institutional_daily_derived')` |
| `total_margin_purchase_short_sale_tw` | `market_margin_maintenance_derived` | `trg_mark_market_margin_dirty()`(reuse,函式 body 一致)|
| `short_sale_securities_lending_tw` | `margin_daily_derived` | `trg_mark_silver_dirty('margin_daily_derived')` |

**合計**:18 個 trigger(15 from PR #20 + 3 from PR #21-B)。

### 7.5 Silver 內部依賴

```
monthly_revenue_derived  (S5,7a)    ← 先跑
        │ 季度日期對齊
        ▼
financial_statement_derived  (S5,7b)  ← 後跑

price_daily_fwd  (S1,7c)             ← 先算
        │ volume 計算 ratio
        ▼
day_trading_derived  (S4,7a)         ← 後算(現行 builder 走 price_daily.volume,deviation)
```

---

## 8. 退場狀態(spec §七)

### 8.1 PR #R3 後 rename 觀察期(spec §7.1,**目前狀態**)

3 張 v2.0 舊表 rename 進入觀察期(R5 21~60 天):

| 表 | dual-write entry | 備註 |
|---|---|---|
| `holding_shares_per_legacy_v2` | `holding_shares_per_legacy`(R4 改名) | 對照 `holding_shares_per`(R3 主名)|
| `monthly_revenue_legacy_v2` | `monthly_revenue_legacy` | 對照 `monthly_revenue` |
| `financial_statement_legacy_v2` | `financial_income_legacy` / `financial_balance_legacy` / `financial_cashflow_legacy` | 對照 `financial_statement` |

R5 SLO(plan §7.2):Silver builder 12/12 OK / `api_sync_progress.status='failed'=0` / 3 表 row count 與主名表 ±1%。

### 8.2 PR #R6 後 DROP(spec §7.2,**未動工**)

R5 SLO 達標後永久 DROP 上述 3 張 `_legacy_v2` + 移除對應 collector.toml 5 個 `_legacy` entry。⚠️ 不可 rollback,需 backup。

### 8.3 v2.0 籌碼舊表退場候選(spec §7.3,**未動工**)

R6 後另起退場 PR(編號 #R7+)DROP 6 張舊表 + 對應 collector.toml entry:

| 退場候選 | 寫入 entry | 替代 Bronze | 估時 |
|---|---|---|---|
| `institutional_daily` | `institutional_daily`(`pivot_institutional`)| `institutional_investors_tw` | 1h |
| `margin_daily` | `margin_daily` | `margin_purchase_short_sale_tw` | 1h |
| `foreign_holding` | `foreign_holding` | `foreign_investor_share_tw` | 1h |
| `day_trading` | `day_trading` | `day_trading_tw` | 1h |
| `valuation_daily` | `valuation_daily` | `valuation_per_tw` | 1h |
| `index_weight_daily` | `index_weight` | (無下游使用,可直接 DROP)| 1h |

⚠️ 退場前需把上述 5 張 PR #18 reverse-pivot 改為 v3 dual-write entry(直接從 FinMind 抓 `*_tw`),否則 reverse-pivot 來源消失。

### 8.4 不退場但需重新評估(spec §7.4)

| 表 | 評估點 |
|---|---|
| `stock_sync_status` | `fwd_adj_valid` 已被 `price_daily_fwd.is_dirty` queue 取代(v1.26 Rust 已對齊),整張表可瘦身 |
| `market_index_tw` vs `market_ohlcv_tw` | 兩者並存職責重疊;長期應只保 `market_ohlcv_tw`(`taiex_index_derived` 已改讀後者)|

---

## 9. Cores 接點清單(spec §八,M3 reference)

> Cores 層設計時**只讀以下表**,不讀 Bronze 或 legacy 表。當作 `m3Spec/` 動工時的契約 reference。

### 9.1 chip_cores

| 讀取表(Silver)| 主要欄位 |
|---|---|
| `institutional_daily_derived` | foreign_buy/sell / dealer / investment_trust / **gov_bank_net** |
| `margin_daily_derived` | margin_balance / short_balance / **SBL 6 欄** |
| `foreign_holding_derived` | foreign_holding_shares / ratio |
| `holding_shares_per_derived` | detail(level taxonomy)|
| `day_trading_derived` | day_trading_buy/sell / **day_trading_ratio** |
| `valuation_daily_derived` | per / pbr / dividend_yield / **market_value_weight** |

### 9.2 fundamental_cores

| 讀取表(Silver)| 主要欄位 |
|---|---|
| `monthly_revenue_derived` | revenue / revenue_mom / revenue_yoy |
| `financial_statement_derived` | type(income/balance/cashflow) / detail(各會計科目) |

### 9.3 environment_cores

| 讀取表(Silver)| 主要欄位 |
|---|---|
| `taiex_index_derived` | OHLCV |
| `us_market_index_derived` | OHLCV(SPY / VIX) |
| `exchange_rate_derived` | rate(per currency) |
| `market_margin_maintenance_derived` | ratio / total_margin_purchase_balance / total_short_sale_balance |
| `business_indicator_derived` | leading / coincident / lagging / monitoring / monitoring_color |

### 9.4 indicator_cores

| 讀取表(Silver)| 主要欄位 |
|---|---|
| `price_daily_fwd` | OHLCV + cumulative_adjustment_factor + cumulative_volume_factor |
| `price_weekly_fwd` | OHLCV |
| `price_monthly_fwd` | OHLCV |
| `price_limit_merge_events` | merge_type / detail |

### 9.5 wave_cores

| 讀取表(Silver)| 主要欄位 |
|---|---|
| `price_daily_fwd` | OHLCV(Monowave Detection 主要來源)|
| `price_weekly_fwd` | OHLCV(週級轉折)|
| `price_monthly_fwd` | OHLCV(月級結構)|

### 9.6 跨 Cores 共用接點

| 讀取表 | 用途 |
|---|---|
| `price_daily_fwd` | **所有 stock-level Cores 的事實基底** |
| `trading_date_ref` | 交易日過濾 / prev_trading_day 計算 |
| `stock_info_ref` | 主檔過濾 / industry 分組(不查 delisting) |
| `stock_suspension_events` | 個股停牌過濾 |
| `price_adjustment_events` | 除權息事件查詢(僅事件,不算復權 — 已在 fwd) |

---

## 10. Code 檔案索引

### 10.1 Top-level

| 檔 | 用途 | 關鍵入口 |
|---|---|---|
| `src/main.py` | CLI(argparse + asyncio dispatch) | `_run_collector` / `_run_silver` |
| `src/bronze/phase_executor.py` | Bronze 排程 + Rust 派工 | `PhaseExecutor.run(mode)` / `_run_phase4(mode)` / `_fetch_dirty_fwd_stocks` |
| `src/silver/orchestrator.py` | Silver 排程(7a/7b/7c) | `SilverOrchestrator.run(phases, ...)` / `_run_7c` / `_fetch_dirty_fwd_stocks` |

### 10.2 Bronze 寫入 pipeline

| 檔 | 用途 | 關鍵函式 |
|---|---|---|
| `src/api_client.py` | aiohttp FinMind v4 HTTP client | `FinMindClient.fetch(api_config, stock_id, seg_start, seg_end)` |
| `src/rate_limiter.py` | token bucket | `RateLimiter.wait()` |
| `src/sync_tracker.py` | api_sync_progress 5-status 追蹤 | `is_completed / mark_progress / mark_failed / mark_schema_mismatch / mark_empty / get_last_sync` |
| `src/date_segmenter.py` | segment 計算 | `DateSegmenter.segments(api_config, mode, stock_id)` |
| `src/field_mapper.py` | API → schema 映射 + detail JSONB pack | `FieldMapper.transform(api_config, raw_records) → (rows, schema_mismatch)` |
| `src/aggregators.py` | pivot/pack 4 個 + filter | `apply_aggregation(name, rows, db, **opts)` |
| `src/db.py` | DBWriter + PostgresWriter | `upsert / upsert_with_strategy / query / query_one / table_pks` |
| `src/post_process.py` | dividend_policy → events 拆分 | `dividend_policy_merge(db, stock_id)` |
| `src/rust_bridge.py` | subprocess 派 Rust binary | `RustBridge.run_phase4(stock_ids, mode)` / `_check_binary_freshness`(v1.26 起 lazy) |

### 10.3 Silver builders(13 個 + helpers)

| 檔 | builder name | Silver 寫入 |
|---|---|---|
| `src/silver/builders/institutional.py` | `institutional` | `institutional_daily_derived` |
| `src/silver/builders/margin.py` | `margin` | `margin_daily_derived` |
| `src/silver/builders/foreign_holding.py` | `foreign_holding` | `foreign_holding_derived` |
| `src/silver/builders/holding_shares_per.py` | `holding_shares_per` | `holding_shares_per_derived` |
| `src/silver/builders/valuation.py` | `valuation` | `valuation_daily_derived` |
| `src/silver/builders/day_trading.py` | `day_trading` | `day_trading_derived` |
| `src/silver/builders/monthly_revenue.py` | `monthly_revenue` | `monthly_revenue_derived` |
| `src/silver/builders/taiex_index.py` | `taiex_index` | `taiex_index_derived` |
| `src/silver/builders/us_market_index.py` | `us_market_index` | `us_market_index_derived` |
| `src/silver/builders/exchange_rate.py` | `exchange_rate` | `exchange_rate_derived` |
| `src/silver/builders/market_margin.py` | `market_margin` | `market_margin_maintenance_derived` |
| `src/silver/builders/business_indicator.py` | `business_indicator` | `business_indicator_derived` |
| `src/silver/builders/financial_statement.py` | `financial_statement`(7b) | `financial_statement_derived` |
| `src/silver/_common.py` | shared helpers | `fetch_bronze / upsert_silver / reset_dirty / get_trading_dates` |
| `src/silver/builders/__init__.py` | builder 註冊表 | `BUILDERS = { name → module }` |

### 10.4 Rust binary

| 檔 | 用途 |
|---|---|
| `rust_compute/src/main.rs` | tw_market_core 後復權 + 週/月聚合 + price_limit_merge_events + dirty queue self-pull(v1.26) |
| `rust_compute/Cargo.toml` | dependencies(sqlx + chrono + serde) |

### 10.5 Verifier scripts

| 檔 | 驗證範圍 |
|---|---|
| `scripts/verify_pr18_bronze.py` | 5 張 PR #18 reverse-pivot round-trip |
| `scripts/verify_pr19b_silver.py` | 5 個 simple Silver builder 對 v2.0 legacy 等值 |
| `scripts/verify_pr19c_silver.py` | 5 個 market-level Silver builder 對 Bronze 1:1 |
| `scripts/verify_pr19c2_silver.py` | 3 個 PR #18.5 依賴 Silver builder 對 `_legacy_v2` 等值(R5 SLO ±1%) |
| `scripts/verify_pr20_triggers.py` | 18 個 Bronze→Silver dirty trigger 整合測試 |

---

## 11. Schema 統計

- **alembic head**:`u0v1w2x3y4z5_pr_r4_rename_v2_entry_names_legacy`(2026-05-09)
- **Bronze 表**(現行):
  - B0_calendar:1
  - B1_meta:3
  - B2_events:3(含 1 staging)
  - B3_price_raw:2
  - B4_chip:9(spec 7 + PR #21-B 2)
  - B5_fundamental:2(R3 升格主名)
  - B6_environment:7(spec 6 + PR #21-B 1)
  - 觀察期 legacy_v2:3(R6 後 DROP)
  - spec §7.3 退場候選:6
  - **小計**:36 張(觀察期 + 退場候選 全退後 → 27 張)
- **Silver 表**:15(S1: 4 / S4: 6 / S5: 2 / S6: 5 — 含 spec §六 全部)
- **System 表**:3(`schema_metadata` / `stock_sync_status` / `api_sync_progress`)
- **collector.toml entries**:39(38 enabled + 1 `government_bank_buy_sell_v3` disabled)
- **Silver builders**:13(12 in 7a + 1 in 7b)
- **Bronze→Silver triggers**:18(15 PR #20 + 3 PR #21-B)

---

## 12. 命名 / 慣例

對齊 CLAUDE.md「關鍵架構決策(不要改)」表。

| 慣例 | 說明 |
|---|---|
| `FieldMapper(db=db)` | 一定要帶 db,讓 schema 補欄位豁免名單 |
| `field_mapper.transform()` | 回 `(rows, schema_mismatch: bool)` tuple |
| `db.upsert()` | 自帶 PRAGMA 欄位過濾(API 新增欄位不炸) |
| `silver/_common.upsert_silver()` | 自帶 `is_dirty=FALSE` |
| `_table_pks` | 動態查 `information_schema`(schema 是 single source of truth)|
| `EXPECTED_SCHEMA_VERSION = "3.2"` | `rust_bridge.py` — schema 升版時 Rust + Python 兩端一起改 |
| PostgresWriter 單 connection | Silver 7a builder 串列跑(concurrent thread access 踩 psycopg thread-safety)|
| Phase 4 incremental 走 dirty queue(v1.26 起)| 0 dirty → skip Rust dispatch |
| Windows binary path 自動補 `.exe` | `rust_bridge.py` 內處理 |
| `cooldown_on_429_sec` 存 RateLimiter 實例 | api_client 從這讀,不 reread config |
| Rust 後復權拆兩個 multiplier | `price_multiplier`(從 AF) + `volume_multiplier`(從 vf);v1.8 切換 |
| **計算 / 規則分層**(spec §1.5)| Silver 做計算,Cores 套規則。違反者(在 Silver 判斷 Wave、在 Cores 做後復權)= 設計錯誤 |

---

## 13. 加 entry / 改 schema 流程

1. **加 collector.toml entry** — 確認 `param_mode / target_table / field_rename / aggregation / event_type / detail_fields / computed_fields` 是否齊全;對映 spec 哪個 Bronze 段(B0~B6)
2. **alembic migration**(若新 Bronze 表)— 新 CREATE TABLE + 對應 dirty trigger(若 Silver builder 接)+ schema_pg.sql 同步加 DDL(給 fresh DB)
3. **field_mapper / aggregators**(若新欄位 mapping 邏輯)— 加 `computed_fields` handler 或 `apply_aggregation` 分支
4. **db.upsert**(通常不用改)— PRAGMA 自動處理新欄
5. **Silver builder**(若新 Bronze 要餵 Silver)— 新檔 in `src/silver/builders/` + 註冊到 `BUILDERS` dict + 註冊到 `orchestrator.PHASE_GROUPS`(對映 spec S1/S4/S5/S6)
6. **CLAUDE.md** — alembic head + PR sequencing + v1.X 段落
7. **本檔** — 同步加 entry 到對應 spec 分段(§2.x B0~B6 / §3.x S1/S4/S5/S6)+ trigger 段(§7)+ Cores 接點(§9 若新 Silver 表)

---

_Generated 2026-05-09 / 對齊 m2Spec/layered_schema_post_refactor.md(v1.0,2026-05-06)結構;v1.26 nice-to-haves 後快照_
