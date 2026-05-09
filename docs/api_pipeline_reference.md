# API Pipeline Reference

**版本**:v3.2 r1(alembic head `u0v1w2x3y4z5` / 2026-05-09 v1.26 nice-to-haves merged)
**範圍**:目前 production 跑的 39 個 `[[api]]` entry × 對應 Bronze schema × 處理流程 code 位置
**用途**:跨 session 銜接 + onboarding + 「這個 API 走哪條 path」一站式查找

> 本檔由 `config/collector.toml` + `src/schema_pg.sql` + `src/{api_client,field_mapper,aggregators,post_process}.py` + `src/bronze/phase_executor.py` + `src/silver/{orchestrator,builders/*}.py` 整合產出。改 schema / 加 entry 後請同步更新本檔。

---

## 0. 總覽

### 0.1 Medallion 架構(v3.2 r1)

| 層 | 內容 | 寫入時機 | 主要 module |
|---|---|---|---|
| Reference | `trading_date_ref`, `stock_info_ref` | Phase 0/1 | `phase_executor.py` |
| Bronze | 21+ raw 表(8 個 `*_tw` + 6 個 v3 主名 + 3 個 `*_legacy_v2` + 4 個 fwd + others) | Phase 1-6 collector + Phase 4 Rust 反推 | 同上 |
| Silver | 14 個 `*_derived`(含 `price_limit_merge_events`)+ 4 個 fwd | Phase 7a/7b/7c orchestrator | `silver/orchestrator.py` + `silver/builders/*.py` + Rust |
| M3 | indicator core / wave / strategy(spec only,未動工) | — | — |

### 0.2 Phase 1-6(Bronze 收集)流程

```
CLI: python src/main.py {backfill,incremental} [--phases ...] [--stocks ...]
   │
   └─→ src/main.py:_run_collector
         │
         └─→ src/bronze/phase_executor.PhaseExecutor.run(mode)
               │
               ├─ Phase 0: trading_date_ref(institutional aggregator 依賴)
               ├─ Phase 1: stock_info_ref / market_index_tw / market_ohlcv_tw
               │     └─→ _refresh_stock_list()(先雞後蛋:Phase 1 完才有 stock list)
               ├─ Phase 2: price_adjustment_events × 4 + stock_suspension + dividend_policy(post_process)
               ├─ Phase 3: price_daily / price_limit
               ├─ Phase 4: ── 特殊:_run_phase4(mode) 派 Rust binary ──
               │     └─→ rust_bridge.run_phase4(stock_ids=..., mode=...)
               │           └─→ rust_compute/target/release/tw_stock_compute
               │                 ├─ resolve_stock_ids:
               │                 │    --stocks 傳入 → 用該清單
               │                 │    否則 fallback price_daily_fwd.is_dirty=TRUE
               │                 │    (v1.26 起;v1.25 之前是 stock_sync_status.fwd_adj_valid=0)
               │                 └─ 對每 stock 全量重算 price_*_fwd × 3
               ├─ Phase 5: 19 entries(11 v2 legacy + 7 v3 dual-write + 1 SBL)
               └─ Phase 6: 7 entries(4 macro + 1 institutional_market + 1 market_margin + 1 total_margin_v3)

每個非 Phase 4 entry 走 _run_api(api_config, mode):
   │
   └─→ _resolve_stock_ids(api_config) — 依 param_mode 取得 stock 列表
         │
         ├─ all_market / all_market_no_id → ["__ALL__"]
         ├─ per_stock_fixed → fixed_ids(SPY/^VIX/TAIEX/TPEx/USD/CNY/JPY/AUD)
         └─ per_stock → fixed_ids 優先,否則動態 stock list

   └─→ for stock_id in stock_ids:
         segments = date_segmenter.segments(api_config, mode, stock_id)
         for (seg_start, seg_end) in segments:
           ├─ sync_tracker.is_completed → 已完成跳過
           ├─ api_client.fetch(api_config, stock_id, seg_start, seg_end)
           │    └─ rate_limiter.wait + aiohttp GET + 429 cooldown
           ├─ field_mapper.transform(api_config, raw_records)
           │    ├─ field_rename + detail_fields pack JSONB
           │    ├─ computed_fields(volume_factor / cash_dividend / stock_dividend)
           │    └─ schema_mismatch 偵測(novel fields warning)
           ├─ aggregators.apply_aggregation(...) — 若 api_config.aggregation 有設
           │    ├─ pivot_institutional / pivot_institutional_market
           │    ├─ pack_holding_shares
           │    └─ pack_financial
           ├─ db.upsert(target_table, rows) 或 db.upsert_with_strategy(...)
           │    └─ PRAGMA(information_schema.columns)欄位過濾
           └─ post_process(若 api_config.post_process 有設)
                └─ dividend_policy_merge — 拆權息事件入 price_adjustment_events
```

### 0.3 Phase 7(Silver 計算)流程

```
CLI: python src/main.py silver phase {7a,7b,7c} [--stocks ...] [--full-rebuild]
   │
   └─→ src/main.py:_run_silver
         │
         └─→ src/silver/orchestrator.SilverOrchestrator.run(phases, ...)
               │
               ├─ 7a: 12 個獨立 builder 串列跑
               │     └─→ src/silver/builders/{name}.run(db, stock_ids, full_rebuild)
               │           ├─ fetch_bronze(table, stock_ids=, where=, order_by=)
               │           ├─ pivot/pack 邏輯
               │           └─ upsert_silver(silver_table, rows, pk_cols=)
               │                └─ 自動帶 is_dirty=FALSE / dirty_at=NULL
               │
               ├─ 7b: financial_statement(跨表依賴 monthly_revenue)
               │
               └─ 7c: rust_bridge.run_phase4(...) — tw_market_core 4 表
                     ├─ price_daily_fwd / price_weekly_fwd / price_monthly_fwd
                     └─ price_limit_merge_events
                     dirty queue pull(v1.26):
                       stock_ids=None + full_rebuild=False → SELECT DISTINCT stock_id FROM
                                                              price_daily_fwd WHERE is_dirty=TRUE
                       0 dirty → skip Rust dispatch
```

---

## 1. Phase 0 — 交易日曆

| # | entry | dataset | target_table | 備註 |
|---|---|---|---|---|
| 0.1 | `trading_calendar` | `TaiwanStockTradingDate` | `trading_date_ref` | 獨立為 Phase 0;institutional aggregator 過濾鬼資料用 |

### 0.1 trading_calendar
- **collector.toml**:行 105–114
- **Bronze schema**(`schema_pg.sql:73–80`):
  ```sql
  CREATE TABLE trading_date_ref (
      market   TEXT NOT NULL,
      date     DATE NOT NULL,
      PRIMARY KEY (market, date)
  );
  ```
- **Code path**:`bronze/phase_executor._run_phase(0)` → `api_client.fetch` → `field_mapper.transform`(無 rename)→ `db.upsert("trading_date_ref", rows)`
- **下游**:
  - `aggregators._filter_to_trading_days()`(institutional pivot 前過濾)
  - `rust_compute/src/main.rs:load_trading_dates`(Rust Phase 4/7c)
  - `silver/_common.get_trading_dates()`(builder 共用)

---

## 2. Phase 1 — META

| # | entry | dataset | target_table | 備註 |
|---|---|---|---|---|
| 1.1 | `stock_info` | `TaiwanStockInfo` | `stock_info_ref` | `data_update_date` pack 進 detail |
| 1.2 | `stock_delisting` | `TaiwanStockDelisting` | `stock_info_ref` | `merge_strategy="update_delist_date"` |
| 1.3 | `market_index_tw` | `TaiwanStockTotalReturnIndex` | `market_index_tw` | TAIEX + TPEx |
| 1.4 | `market_ohlcv_v3` | `TaiwanStockPrice` | `market_ohlcv_tw` | PR #22:把 TAIEX/TPEx 當 stock 抓含 OHLCV |

### 1.1 stock_info
- **collector.toml**:行 79–89
- **Bronze schema**(`schema_pg.sql:49–71`):PK `(market, stock_id)`;欄位含 `name / type / industry_category / listing_date / delisting_date / par_value / detail JSONB / source / updated_at`
- **Code path**:`phase_executor._run_api → field_mapper.transform`(rename `date → _data_update_date` + pack 進 detail)→ `db.upsert("stock_info_ref", rows)`(走 schema `DEFAULT NOW()` + UPSERT 強制 `updated_at = NOW()`)
- **下游**:`stock_resolver.resolve` 取動態 stock list(per Phase 5/6 per_stock entries)

### 1.2 stock_delisting
- **collector.toml**:行 91–102
- **Bronze schema**:同上 1.1(共用 `stock_info_ref`)
- **Code path**:`field_mapper.transform → db.upsert_with_strategy("update_delist_date")` → `_merge_delist_date()`(SET delisting_date = ..., updated_at = NOW())。`backfill_start_override = "2022-01-01"`
- **特殊**:`merge_strategy = "update_delist_date"` → `db.py` 客製 UPDATE 路徑(不走 ON CONFLICT INSERT)

### 1.3 market_index_tw
- **collector.toml**:行 115–126
- **Bronze schema**(`schema_pg.sql:83–94`):PK `(market, stock_id, date)`;欄位 `price NUMERIC + detail JSONB`
- **Code path**:`per_stock_fixed` mode → fixed_ids `["TAIEX", "TPEx"]` → `_run_api` → `field_mapper.transform`(無 rename)→ `db.upsert`
- **下游**:Silver `taiex_index_derived` 改讀 `market_ohlcv_tw`(PR #22 後),本表保留作 Reference

### 1.4 market_ohlcv_v3 ← PR #22
- **collector.toml**:行 134–146
- **Bronze schema**(`schema_pg.sql:100–116`):PK `(market, stock_id, date)`;OHLCV + detail JSONB(含 trading_money / turnover / spread)
- **Code path**:`per_stock_fixed` mode → fixed_ids `["TAIEX", "TPEx"]` → 同 1.3 但用 `TaiwanStockPrice` dataset(同 `price_daily` source)
- **下游**:Silver builder `taiex_index`(`silver/builders/taiex_index.py` 讀 `market_ohlcv_tw`)→ `taiex_index_derived`

---

## 3. Phase 2 — EVENTS

| # | entry | dataset | target_table | event_type |
|---|---|---|---|---|
| 2.1 | `dividend_result` | `TaiwanStockDividendResult` | `price_adjustment_events` | `dividend` |
| 2.2 | `dividend_policy` | `TaiwanStockDividend` | `_dividend_policy_staging` | (post_process) |
| 2.3 | `capital_reduction` | `TaiwanStockCapitalReductionReferencePrice` | `price_adjustment_events` | `capital_reduction` |
| 2.4 | `split_price` | `TaiwanStockSplitPrice` | `price_adjustment_events` | `split` |
| 2.5 | `par_value_change` | `TaiwanStockParValueChange` | `price_adjustment_events` | `par_value_change` |
| 2.6 | `stock_suspension` | `TaiwanStockSuspended` | `stock_suspension_events` | (no event_type) |

### 共用 Bronze:price_adjustment_events
- **schema_pg.sql:128–148**:PK `(market, stock_id, date, event_type)`
- 欄位:`event_type`(CHECK in 5 種)/ `before_price` / `reference_price` / `cash_dividend` / `stock_dividend` / `volume_factor` / `detail JSONB`(after_price 等砍掉的欄改 pack JSONB)
- **CHECK**:`event_type IN ('dividend', 'split', 'par_value_change', 'capital_reduction', 'cash_increase')`
- **Trigger**(PR #20):`mark_fwd_dirty_on_event` → INSERT/UPDATE 觸發 `trg_mark_fwd_silver_dirty()` → UPDATE 4 fwd 表 SET is_dirty=TRUE(全段歷史 mark)

### 2.1 dividend_result
- **collector.toml**:行 152–166
- **Code path**:`field_mapper.transform`:`computed_fields = ["volume_factor", "cash_dividend", "stock_dividend"]` → `field_mapper._compute_dividend_fields()` 從 detail 拆。Bronze upsert 後 trigger 自動 mark fwd dirty
- **下游**:Phase 4 Rust 反推 `price_*_fwd`;`post_process._recompute_stock_dividend_vf` SQL 修 P1-17(stock_dividend vf 計算)

### 2.2 dividend_policy → post_process
- **collector.toml**:行 168–181
- **Bronze schema**(`schema_pg.sql:151–158`):staging table,PK `(market, stock_id, date)`,21 個 PascalCase 欄全 pack 進 detail JSONB + source 欄
- **Code path**:`field_mapper.transform`(21 個 underscore-prefixed rename → 全進 detail)→ `db.upsert("_dividend_policy_staging", rows)` → **`post_process="dividend_policy_merge"` 觸發** → `post_process.dividend_policy_merge(db, stock_id)`(`src/post_process.py:31-180`)
- **post_process 邏輯**:
  - 從 `_dividend_policy_staging.detail` 拆出 `_StockExDividendTradingDate` / `_CashExDividendTradingDate` / `_TotalNumberOfCashCapitalIncrease` 等
  - 衍生 `dividend` events 寫進 `price_adjustment_events`
  - 偵測「純現增」事件(dual subtype `cash_increase`)
- **下游**:Bronze `price_adjustment_events` 透過 trigger 觸發 fwd dirty

### 2.3–2.5 capital_reduction / split_price / par_value_change
- **collector.toml**:行 183–227
- **Code path**:同 2.1 pattern(`computed_fields = ["volume_factor"]`),寫進 `price_adjustment_events` 不同 event_type
- **欄位 rename**(每 entry 不同):
  - capital_reduction:`ClosingPriceonTheLastTradingDay → before_price`、`OpeningReferencePrice → reference_price`
  - split:`open_price → reference_price`
  - par_value:`before_close → before_price`、`after_ref_open → reference_price`

### 2.6 stock_suspension ← v3.2 B-4
- **collector.toml**:行 232–244
- **Bronze schema**(`schema_pg.sql:164–178`):PK `(market, stock_id, suspension_date, suspension_time)`;`reason` / `resumption_date` / `resumption_time` 欄
- **Code path**:`field_mapper.transform`(`date → suspension_date` rename)→ `db.upsert("stock_suspension_events", rows)`
- **下游**:供 `tw_market_core`(M3,未動工)做 prev_trading_day 個股級交易缺口識別

---

## 4. Phase 3 — RAW PRICE

| # | entry | dataset | target_table |
|---|---|---|---|
| 3.1 | `price_daily` | `TaiwanStockPrice` | `price_daily` |
| 3.2 | `price_limit` | `TaiwanStockPriceLimit` | `price_limit` |

### 3.1 price_daily
- **collector.toml**:行 249–260
- **Bronze schema**(`schema_pg.sql:224–238`):PK `(market, stock_id, date)`;`open / high / low / close / volume(BIGINT)/ turnover NUMERIC / detail JSONB`(`_trading_money` / `_spread`)
- **Code path**:`field_mapper.transform`(`max → high`、`min → low`、`Trading_Volume → volume`)→ `db.upsert("price_daily", rows)`
- **下游**:
  - Phase 4 Rust 讀(`rust_compute/src/main.rs:load_raw_prices`,SELECT open/high/low/close/volume)
  - Silver `valuation` builder LEFT JOIN(算 `market_value_weight`)
  - Silver `day_trading` builder LEFT JOIN(算 `day_trading_ratio = dt_volume / pd_volume × 100`)

### 3.2 price_limit
- **collector.toml**:行 262–273
- **Bronze schema**(`schema_pg.sql:241–251`):PK `(market, stock_id, date)`;`limit_up / limit_down + detail JSONB`(`_reference_price`)
- **Code path**:`field_mapper.transform`(`reference_price → _reference_price` 進 detail)→ `db.upsert`
- **下游**:Silver `price_limit_merge_events`(Rust Phase 7c 計算,合併連續漲跌停事件)

---

## 5. Phase 4 — RUST 後復權(無 [[api]] entry)

Phase 4 不在 collector.toml 註冊,由 `bronze/phase_executor._run_phase4(mode)` 直接派 Rust binary。

### 5.1 入口
- **`src/bronze/phase_executor.py:267-339`**:
  - `mode=="backfill"` → 全市場 `self._stock_list` 派 Rust
  - `mode=="incremental"` → `_fetch_dirty_fwd_stocks()` 拉 dirty list,0 dirty → skip(v1.26 起)
  - `_fetch_dirty_fwd_stocks` SQL:`SELECT DISTINCT stock_id FROM price_daily_fwd WHERE is_dirty = TRUE ORDER BY stock_id`
- **`src/rust_bridge.RustBridge.run_phase4(stock_ids=, mode=)`**(行 117–252):
  - 組 CLI:`tw_stock_compute --database-url ... --mode ... [--stocks ...]`
  - subprocess 啟動 + stderr/stdout capture
  - schema_version assertion(`EXPECTED_SCHEMA_VERSION = "3.2"`)
  - mtime freshness check(v1.26 起只在 `run_phase4` 才檢)

### 5.2 Rust binary
- **`rust_compute/src/main.rs`**:
  - `resolve_stock_ids(args)`(行 215+):`--stocks` 傳入優先;否則 SELECT distinct stock_id FROM price_daily_fwd WHERE is_dirty=TRUE(v1.26)
  - `process_stock(market, stock_id)`:
    - DELETE FROM price_daily_fwd → INSERT(後復權 reverse multiplier 倒推)
    - `compute_forward_adjusted` 拆 `price_multiplier`(從 AF) + `volume_multiplier`(從 vf)
    - DELETE FROM price_monthly_fwd → INSERT(月聚合)
    - INSERT price_weekly_fwd(週聚合)
    - UPSERT stock_sync_status(market, stock_id, fwd_adj_valid=1)— 對齊 deprecated flag
- **產出 Bronze**(`schema_pg.sql:260+`):`price_daily_fwd / price_weekly_fwd / price_monthly_fwd`,皆有 dirty 欄(PR #19a)+ partial index `WHERE is_dirty=TRUE`

---

## 6. Phase 5 — CHIP / FUNDAMENTAL(19 entries,gov_bank disabled)

### 6.1 v2.0 legacy entries(11 個)— 寫 v2 表 + R2 後 _legacy_v2

| # | entry | dataset | target_table | aggregation | 備註 |
|---|---|---|---|---|---|
| 5.1 | `institutional_daily` | `TaiwanStockInstitutionalInvestorsBuySell` | `institutional_daily` | `pivot_institutional` | 三大法人 5 類 pivot |
| 5.2 | `securities_lending` | `TaiwanStockSecuritiesLending` | `securities_lending_tw` | — | 借券成交明細(B-5) |
| 5.3 | `margin_daily` | `TaiwanStockMarginPurchaseShortSale` | `margin_daily` | — | 16 raw → 6 stored + 8 detail |
| 5.4 | `foreign_holding` | `TaiwanStockShareholding` | `foreign_holding` | — | 11 raw → 2 stored + 9 detail |
| 5.5 | `holding_shares_per_legacy` | `TaiwanStockHoldingSharesPer` | `holding_shares_per_legacy_v2` | `pack_holding_shares` | R4 entry name 加 _legacy |
| 5.6 | `valuation_daily` | `TaiwanStockPER` | `valuation_daily` | — | per/pbr/dividend_yield |
| 5.7 | `day_trading` | `TaiwanStockDayTrading` | `day_trading` | — | day_trading_buy/sell(金額) |
| 5.8 | `index_weight` | `TaiwanStockMarketValueWeight` | `index_weight_daily` | — | 指數成分權重 |
| 5.9 | `monthly_revenue_legacy` | `TaiwanStockMonthRevenue` | `monthly_revenue_legacy_v2` | — | R4 entry name 加 _legacy |
| 5.10 | `financial_income_legacy` | `TaiwanStockFinancialStatements` | `financial_statement_legacy_v2` | `pack_financial` | stmt_type=income,R4 加 _legacy |
| 5.11 | `financial_balance_legacy` | `TaiwanStockBalanceSheet` | `financial_statement_legacy_v2` | `pack_financial` | stmt_type=balance |
| 5.12 | `financial_cashflow_legacy` | `TaiwanStockCashFlowsStatement` | `financial_statement_legacy_v2` | `pack_financial` | stmt_type=cashflow |

### 6.2 v3 dual-write entries(6 個)— 寫 v3 主名(R3 升格)

| # | entry | dataset | target_table | event_type | 備註 |
|---|---|---|---|---|---|
| 5.13 | `holding_shares_per_v3` | `TaiwanStockHoldingSharesPer` | `holding_shares_per` | — | PR #18.5 raw,1 row/level |
| 5.14 | `financial_income_v3` | `TaiwanStockFinancialStatements` | `financial_statement` | `income` | PR #18.5 raw,1 row/origin_name |
| 5.15 | `financial_balance_v3` | `TaiwanStockBalanceSheet` | `financial_statement` | `balance` | 同上 |
| 5.16 | `financial_cashflow_v3` | `TaiwanStockCashFlowsStatement` | `financial_statement` | `cashflow` | 同上 |
| 5.17 | `monthly_revenue_v3` | `TaiwanStockMonthRevenue` | `monthly_revenue` | — | raw FinMind 欄名 |
| 5.18 | `short_sale_securities_lending_v3` | `TaiwanDailyShortSaleBalances` | `short_sale_securities_lending_tw` | — | PR #21-B,SBL 3 欄 |

### 6.3 disabled
- **5.19 `government_bank_buy_sell_v3`** → `government_bank_buy_sell_tw`:`enabled = false`,FinMind 需 sponsor tier(user 是 backer)。Bronze schema + trigger 已落,等升 tier 切回 true。

### 6.4 共用流程

`bronze/phase_executor._run_api(api_config, mode)`(行 150+)→ `api_client.fetch` → `field_mapper.transform`(rename + detail JSONB pack)→ **若有 `aggregation`**:`aggregators.apply_aggregation(name, rows, db)` → `db.upsert(target_table, rows)` → trigger 自動 mark Silver dirty。

### 6.5 aggregators(`src/aggregators.py`)

| function | 用途 | 對應 entry |
|---|---|---|
| `pivot_institutional(rows, db)` | 5 類法人 pivot 成 1 row × 10 col(buy/sell) | 5.1 institutional_daily |
| `pivot_institutional_market(rows, db)` | 全市場 pivot | 6.4 institutional_market |
| `pack_holding_shares(rows)` | 多 level → detail JSONB pack | 5.5 holding_shares_per_legacy |
| `pack_financial(rows, stmt_type)` | 中→英 origin_name → detail JSONB | 5.10/11/12 financial_*_legacy |
| `_filter_to_trading_days(rows, db)` | 過濾 FinMind 週六鬼資料 | pivot_institutional 內部呼叫 |

---

## 7. Phase 6 — MACRO

| # | entry | dataset | target_table | 備註 |
|---|---|---|---|---|
| 6.1 | `market_index_us` | `USStockPrice` | `market_index_us` | SPY + ^VIX |
| 6.2 | `exchange_rate` | `TaiwanExchangeRate` | `exchange_rate` | 4 幣(USD/CNY/JPY/AUD)per_stock_fixed |
| 6.3 | `institutional_market` | `TaiwanStockTotalInstitutionalInvestors` | `institutional_market_daily` | `pivot_institutional_market` |
| 6.4 | `market_margin` | `TaiwanTotalExchangeMarginMaintenance` | `market_margin_maintenance` | ratio (融資維持率) |
| 6.5 | `fear_greed` | `CnnFearGreedIndex` | `fear_greed_index` | 美股恐懼貪婪 |
| 6.6 | `business_indicator` | `TaiwanBusinessIndicator` | `business_indicator_tw` | leading/coincident/lagging _indicator(避 PG 保留字) |
| 6.7 | `total_margin_purchase_short_sale_v3` | `TaiwanStockTotalMarginPurchaseShortSale` | `total_margin_purchase_short_sale_tw` | PR #21-B,pivoted by name |

### 6.7 total_margin pivot 細節
- **collector.toml**:行 650–660
- **Bronze schema**(alembic q6r7s8t9u0v1):PK `(market, date, name)` — `name ∈ {'MarginPurchase', 'ShortSale', 'MarginPurchaseMoney}`
- **Silver builder pivot**(`silver/builders/market_margin._build_total_margin_lookup`):
  - `name='MarginPurchase'.today_balance` → Silver `total_margin_purchase_balance`
  - `name='ShortSale'.today_balance` → Silver `total_short_sale_balance`
  - `name='MarginPurchaseMoney'` 在 `KNOWN_SKIP_NAMES` silently skip

---

## 8. Phase 7 — Silver(orchestrator,不在 collector.toml)

`src/silver/orchestrator.SilverOrchestrator.run(phases, stock_ids, full_rebuild)`

### 8.1 7a — 12 個獨立 builder(串列;PostgresWriter 單 conn,thread-safety 限制)

| # | builder | Bronze 來源 | Silver 寫入 |
|---|---|---|---|
| 7a.1 | `institutional` | `institutional_investors_tw` + `government_bank_buy_sell_tw` | `institutional_daily_derived` |
| 7a.2 | `margin` | `margin_purchase_short_sale_tw` + `short_sale_securities_lending_tw` | `margin_daily_derived` |
| 7a.3 | `foreign_holding` | `foreign_investor_share_tw` | `foreign_holding_derived` |
| 7a.4 | `holding_shares_per` | `holding_shares_per`(R3 主名) | `holding_shares_per_derived` |
| 7a.5 | `valuation` | `valuation_per_tw` + `price_daily` + `foreign_investor_share_tw` | `valuation_daily_derived`(含 `market_value_weight`) |
| 7a.6 | `day_trading` | `day_trading_tw` + `price_daily` | `day_trading_derived`(含 `day_trading_ratio`) |
| 7a.7 | `monthly_revenue` | `monthly_revenue`(R3 主名) | `monthly_revenue_derived` |
| 7a.8 | `taiex_index` | `market_ohlcv_tw` | `taiex_index_derived` |
| 7a.9 | `us_market_index` | `market_index_us` | `us_market_index_derived` |
| 7a.10 | `exchange_rate` | `exchange_rate` | `exchange_rate_derived`(PK 含 currency) |
| 7a.11 | `market_margin` | `market_margin_maintenance` + `total_margin_purchase_short_sale_tw` | `market_margin_maintenance_derived` |
| 7a.12 | `business_indicator` | `business_indicator_tw` | `business_indicator_derived`(PK 含 sentinel `_market_`) |

### 8.2 7b — 跨表依賴

| # | builder | Bronze 來源 | Silver 寫入 |
|---|---|---|---|
| 7b.1 | `financial_statement` | `financial_statement`(R3 主名) | `financial_statement_derived` |

### 8.3 7c — Rust binary(派 `rust_bridge.run_phase4`)

- 產出 4 表:`price_daily_fwd / price_weekly_fwd / price_monthly_fwd / price_limit_merge_events`
- 走 dirty queue(orchestrator `_fetch_dirty_fwd_stocks` 拉 stock list)
- 0 dirty → skip Rust dispatch

### 8.4 Bronze→Silver dirty trigger(18 個:15 from PR #20 + 3 from PR #21-B)

| Bronze | Silver | trigger function |
|---|---|---|
| `institutional_investors_tw` | `institutional_daily_derived` | `trg_mark_silver_dirty('...')` |
| `margin_purchase_short_sale_tw` | `margin_daily_derived` | 同 |
| `securities_lending_tw` | `margin_daily_derived` | 同 |
| `foreign_investor_share_tw` | `foreign_holding_derived` | 同 |
| `holding_shares_per` | `holding_shares_per_derived` | 同(R3 後 ON 主名)|
| `day_trading_tw` | `day_trading_derived` | 同 |
| `valuation_per_tw` | `valuation_daily_derived` | 同 |
| `monthly_revenue` | `monthly_revenue_derived` | 同(R3 後 ON 主名)|
| `market_ohlcv_tw` | `taiex_index_derived` | 同 |
| `market_index_us` | `us_market_index_derived` | 同 |
| `financial_statement` | `financial_statement_derived` | `trg_mark_financial_stmt_dirty()`(event_type → type)|
| `exchange_rate` | `exchange_rate_derived` | `trg_mark_exchange_rate_dirty()`(currency PK)|
| `market_margin_maintenance` | `market_margin_maintenance_derived` | `trg_mark_market_margin_dirty()`(2-col PK)|
| `business_indicator_tw` | `business_indicator_derived` | `trg_mark_business_indicator_dirty()`(注入 sentinel)|
| `price_adjustment_events` | 4 fwd 表整檔 dirty | `trg_mark_fwd_silver_dirty()` UPDATE 4 fwd SET is_dirty=TRUE |
| `government_bank_buy_sell_tw`(PR #21-B)| `institutional_daily_derived` | `trg_mark_silver_dirty('institutional_daily_derived')` |
| `total_margin_purchase_short_sale_tw`(PR #21-B)| `market_margin_maintenance_derived` | `trg_mark_market_margin_dirty()`(reuse,函式 body 一致)|
| `short_sale_securities_lending_tw`(PR #21-B)| `margin_daily_derived` | `trg_mark_silver_dirty('margin_daily_derived')` |

---

## 9. Bronze 表 → 寫入 entry × Silver builder 對照

| Bronze table | 寫入 entries | Silver builder consumer |
|---|---|---|
| `trading_date_ref` | trading_calendar | (Reference,被 institutional aggregator 引用)|
| `stock_info_ref` | stock_info / stock_delisting | (Reference,被 stock_resolver 引用)|
| `market_index_tw` | market_index_tw | — |
| `market_ohlcv_tw` | market_ohlcv_v3 | `taiex_index` builder |
| `price_adjustment_events` | dividend_result / capital_reduction / split_price / par_value_change(+ post_process from dividend_policy) | trigger → 4 fwd 表 dirty |
| `_dividend_policy_staging` | dividend_policy(post_process 後不留)| — |
| `stock_suspension_events` | stock_suspension | (M3 prev_trading_day,未動工)|
| `price_daily` | price_daily | Phase 4 Rust + valuation/day_trading builders LEFT JOIN |
| `price_limit` | price_limit | Phase 7c Rust(price_limit_merge_events)|
| `institutional_daily`(v2 legacy)| institutional_daily | (PR #18 reverse-pivot 來源)|
| `securities_lending_tw` | securities_lending | trigger → margin_daily_derived dirty |
| `margin_daily`(v2 legacy)| margin_daily | (PR #18 reverse-pivot 來源)|
| `foreign_holding`(v2 legacy)| foreign_holding | (PR #18 reverse-pivot 來源)|
| `holding_shares_per_legacy_v2` | holding_shares_per_legacy | (R5 觀察期對照表)|
| `valuation_daily`(v2 legacy)| valuation_daily | (PR #18 reverse-pivot 來源)|
| `day_trading`(v2 legacy)| day_trading | (PR #18 reverse-pivot 來源)|
| `index_weight_daily`(v2 legacy)| index_weight | (無下游使用,spec §7.3 退場候選)|
| `monthly_revenue_legacy_v2` | monthly_revenue_legacy | (R5 觀察期對照表)|
| `financial_statement_legacy_v2` | financial_income/balance/cashflow_legacy | (R5 觀察期對照表)|
| `holding_shares_per`(R3 主名)| holding_shares_per_v3 | `holding_shares_per` builder |
| `financial_statement`(R3 主名)| financial_income/balance/cashflow_v3 | `financial_statement` builder |
| `monthly_revenue`(R3 主名)| monthly_revenue_v3 | `monthly_revenue` builder |
| `government_bank_buy_sell_tw` | government_bank_buy_sell_v3(disabled)| (institutional builder LEFT JOIN,缺 row → gov_bank_net=NULL)|
| `total_margin_purchase_short_sale_tw` | total_margin_purchase_short_sale_v3 | `market_margin` builder pivot |
| `short_sale_securities_lending_tw` | short_sale_securities_lending_v3 | `margin` builder UNION |
| `market_index_us` | market_index_us | `us_market_index` builder |
| `exchange_rate` | exchange_rate | `exchange_rate` builder |
| `institutional_market_daily` | institutional_market | (無 Silver builder,留作 reference)|
| `market_margin_maintenance` | market_margin | `market_margin` builder |
| `fear_greed_index` | fear_greed | (無 Silver builder)|
| `business_indicator_tw` | business_indicator | `business_indicator` builder |
| `price_daily_fwd / price_weekly_fwd / price_monthly_fwd` | (Rust Phase 4)| (Silver-equivalent;對齊 spec §2.3 Silver 14 張)|
| `price_limit_merge_events` | (Rust Phase 7c)| (同上)|

PR #18 reverse-pivot 5 張 _tw Bronze 由 `scripts/reverse_pivot_*.py` 從 v2 legacy 表反推填入(不在 collector.toml,non-recurring)。

---

## 10. Code 檔案索引

### 10.1 Top-level
| 檔 | 用途 | 關鍵入口 |
|---|---|---|
| `src/main.py` | CLI(argparse + asyncio dispatch) | `_run_collector` / `_run_silver` |
| `src/bronze/phase_executor.py` | Phase 0/1/2/3/5/6 排程 + Phase 4 Rust 派工 | `PhaseExecutor.run(mode)` |
| `src/silver/orchestrator.py` | Phase 7a/7b/7c 排程 | `SilverOrchestrator.run(phases, ...)` |

### 10.2 Per-entry pipeline
| 檔 | 用途 | 關鍵函式 |
|---|---|---|
| `src/api_client.py` | aiohttp FinMind v4 HTTP client | `FinMindClient.fetch(api_config, stock_id, seg_start, seg_end)` |
| `src/rate_limiter.py` | token bucket(1600/h、min_interval 2250ms、429 cooldown 120s)| `RateLimiter.wait()` |
| `src/sync_tracker.py` | api_sync_progress 5 種 status 追蹤 | `is_completed / mark_progress / mark_failed / mark_schema_mismatch / mark_empty / get_last_sync` |
| `src/date_segmenter.py` | segment 計算(per_stock 365d / financial 0d 全段抓 / no_end 邊界)| `DateSegmenter.segments(api_config, mode, stock_id)` |
| `src/field_mapper.py` | API → schema 映射 + detail JSONB pack + computed_fields | `FieldMapper.transform(api_config, raw_records) → (rows, schema_mismatch)` |
| `src/aggregators.py` | pivot/pack 4 個 + `_filter_to_trading_days` | `apply_aggregation(name, rows, db, **opts)` |
| `src/db.py` | DBWriter Protocol + PostgresWriter(prod)/ SqliteWriter(過渡)| `upsert / upsert_with_strategy / query / query_one / table_pks` |
| `src/post_process.py` | dividend_policy → events 拆分 + `_recompute_stock_dividend_vf` | `dividend_policy_merge(db, stock_id)` |
| `src/rust_bridge.py` | subprocess 派 Rust binary | `RustBridge.run_phase4(stock_ids, mode)` |

### 10.3 Silver builders(13 個)
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
| `src/silver/builders/financial_statement.py` | `financial_statement`(7b)| `financial_statement_derived` |
| `src/silver/_common.py` | shared helpers | `fetch_bronze / upsert_silver / reset_dirty / get_trading_dates` |
| `src/silver/builders/__init__.py` | builder 註冊表 | `BUILDERS = { name → module }` |

### 10.4 Rust binary
| 檔 | 用途 |
|---|---|
| `rust_compute/src/main.rs` | tw_market_core 後復權 + 週/月聚合 + price_limit_merge_events + dirty queue self-pull |
| `rust_compute/Cargo.toml` | dependencies(sqlx + chrono + serde) |

### 10.5 Verifier scripts
| 檔 | 驗證範圍 |
|---|---|
| `scripts/verify_pr18_bronze.py` | 5 張 PR #18 reverse-pivot 反推 round-trip(against legacy v2) |
| `scripts/verify_pr19b_silver.py` | 5 個 simple Silver builder 對 v2.0 legacy 等值 |
| `scripts/verify_pr19c_silver.py` | 5 個 market-level Silver builder 對 Bronze 1:1 |
| `scripts/verify_pr19c2_silver.py` | 3 個 PR #18.5 依賴 Silver builder 對 `_legacy_v2` 等值(R5 觀察期 SLO ±1%) |
| `scripts/verify_pr20_triggers.py` | 15 個 Bronze→Silver dirty trigger 整合測試 |

---

## 11. Schema 統計

- **alembic head**:`u0v1w2x3y4z5_pr_r4_rename_v2_entry_names_legacy`(2026-05-09)
- **Bronze tables**:21 個常駐 + 4 個 fwd + 1 個 staging
- **Silver tables**:14 個 `*_derived`(per spec §2.3)
- **Reference tables**:2(`trading_date_ref`、`stock_info_ref`)
- **System tables**:3(`schema_metadata`、`stock_sync_status`、`api_sync_progress`)
- **collector.toml entries**:39(38 enabled + 1 gov_bank disabled)
- **Silver builders**:13(12 個 in 7a + 1 個 in 7b + 0 個獨立 module 在 7c — 7c 走 rust_bridge)
- **Bronze→Silver triggers**:18(15 from PR #20 alembic n3o4p5q6r7s8 + 3 from PR #21-B alembic p5q6r7s8t9u0)

---

## 12. 命名 / 慣例(動工前必看)

| 慣例 | 說明 |
|---|---|
| `FieldMapper(db=db)` | 一定要帶 db,讓 schema 補欄位豁免名單 |
| `field_mapper.transform()` | 回 `(rows, schema_mismatch: bool)` tuple |
| `db.upsert()` | 自帶 PRAGMA 欄位過濾(API 新增欄位不炸) |
| `silver/_common.upsert_silver()` | 自帶 `is_dirty=FALSE` |
| `_table_pks` | 動態查 `information_schema`(schema 是 single source of truth) |
| `EXPECTED_SCHEMA_VERSION = "3.2"` | `rust_bridge.py:31` — schema 升版時 Rust + Python 兩端一起改 |
| PostgresWriter 單 connection | Phase 7a builder 串列跑(concurrent thread access 踩 psycopg thread-safety) |
| Phase 4 必須傳 `stock_ids`(backfill mode) | `stock_sync_status` 沒人寫入時 Rust 取不到清單 |
| Phase 4 incremental 走 dirty queue(v1.26 起) | 0 dirty → skip Rust dispatch |
| Windows binary path 自動補 `.exe` | `rust_bridge.py` 內處理(`asyncio.create_subprocess_exec` 不像 shell 自動補) |
| `cooldown_on_429_sec` 存 RateLimiter 實例 | api_client 從這讀,不 reread config |

---

## 13. 改 schema / 加 entry 的流程

1. **加 collector.toml entry** — 確認 `param_mode / target_table / field_rename / aggregation / event_type / detail_fields / computed_fields` 是否齊全
2. **alembic migration**(若新 Bronze 表)— 新 CREATE TABLE + 對應 dirty trigger(若 Silver builder 接)+ schema_pg.sql 同步加 DDL(給 fresh DB)
3. **field_mapper / aggregators**(若新欄位 mapping 邏輯)— 加 `computed_fields` handler 或 `apply_aggregation` 分支
4. **db.upsert**(通常不用改)— PRAGMA 自動處理新欄
5. **Silver builder**(若新 Bronze 要餵 Silver)— 新檔 in `src/silver/builders/` + 註冊到 `BUILDERS` dict + 註冊到 `orchestrator.PHASE_GROUPS`
6. **CLAUDE.md** — alembic head + PR sequencing + v1.X 段落
7. **本檔** — 同步加 entry + Bronze schema 描述 + code path

---

_Generated 2026-05-09 / v1.26 nice-to-haves merged(PR #30 commit `294840a`)。_
