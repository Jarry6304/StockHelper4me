# API Pipeline Reference

**版本**:v3.2 r1(alembic head `u0v1w2x3y4z5` / 2026-05-09 v1.26 nice-to-haves merged)
**範圍**:39 個 `[[api]]` entry × Bronze schema × Silver builder × code 位置
**結構**:依資料屬性分層(**Bronze 原始 + Silver 進階計算**),不照 Phase 排序

> 本檔由 `config/collector.toml` + `src/schema_pg.sql` + `src/{api_client,field_mapper,aggregators,post_process}.py` + `src/bronze/phase_executor.py` + `src/silver/{orchestrator,builders/*}.py` 整合產出。改 schema / 加 entry 後請同步更新本檔。

---

## 0. 兩層定位

| 層 | 內容 | 寫入時機 | 主要 module |
|---|---|---|---|
| **Bronze**(原始) | FinMind raw 直入 + 反推 fwd K 線 + 事件表 | collector backfill / incremental | `src/bronze/phase_executor.py` |
| **Silver**(進階計算) | 12 個 `*_derived`(Python builder)+ 4 個 fwd / merge events(Rust) | orchestrator dirty queue pull(PR #20 trigger 觸發) | `src/silver/orchestrator.py` + `silver/builders/*.py` |

### 0.1 Bronze 寫入通用流程

```
CLI: python src/main.py {backfill,incremental} [--stocks ...]
   │
   └─→ src/bronze/phase_executor.PhaseExecutor.run(mode)
         │
         └─ 對每個 enabled 的 [[api]] entry:_run_api(api_config, mode)
               │
               ├─ _resolve_stock_ids(api_config) — 依 param_mode 取 stock 列表
               │     ├─ all_market / all_market_no_id → ["__ALL__"](sentinel)
               │     ├─ per_stock_fixed → fixed_ids(SPY/^VIX/TAIEX/USD/...)
               │     └─ per_stock → fixed_ids 優先,否則動態 stock_resolver 清單
               │
               └─ for stock_id in stock_ids:
                    segments = date_segmenter.segments(api_config, mode, stock_id)
                    for (seg_start, seg_end) in segments:
                      ├─ sync_tracker.is_completed → 已完成跳過
                      ├─ api_client.fetch — aiohttp + rate limit 1600/h + 429 cooldown
                      ├─ field_mapper.transform(api_config, raw_records)
                      │    ├─ field_rename + detail_fields pack JSONB
                      │    ├─ computed_fields(volume_factor / cash_dividend / stock_dividend)
                      │    └─ schema_mismatch 偵測(novel fields warning)
                      ├─ aggregators.apply_aggregation(...) — 若有設 aggregation
                      │    (pivot_institutional / pack_holding_shares / pack_financial 等)
                      ├─ db.upsert(target_table, rows) — PRAGMA(information_schema)欄位過濾
                      └─ post_process — 若 api_config.post_process 有設(僅 dividend_policy_merge)

# 後復權 K 線(Rust binary)— 與上方流程獨立,無 [[api]] entry
   bronze/phase_executor._run_phase4(mode)
     ├─ mode=="backfill" → 全市場 self._stock_list 派 Rust
     └─ mode=="incremental" → _fetch_dirty_fwd_stocks() 拉 dirty list,0 dirty → skip(v1.26)
   └─→ rust_bridge.run_phase4 → tw_stock_compute binary
         ├─ 讀 price_daily + price_adjustment_events
         └─ 寫 price_daily_fwd / price_weekly_fwd / price_monthly_fwd
```

### 0.2 Silver 進階計算流程

```
CLI: python src/main.py silver phase {7a,7b,7c} [--stocks ...] [--full-rebuild]
   │
   └─→ src/silver/orchestrator.SilverOrchestrator.run(phases, ...)
         │
         ├─ 7a:12 個獨立 builder 串列跑(PostgresWriter 單 conn,thread-safety 限制)
         │     └─→ src/silver/builders/{name}.run(db, stock_ids, full_rebuild)
         │           ├─ fetch_bronze(table, stock_ids=, where=, order_by=)
         │           ├─ pivot/pack/UNION 邏輯
         │           └─ upsert_silver(silver_table, rows, pk_cols=)
         │                └─ 自動帶 is_dirty=FALSE / dirty_at=NULL
         │
         ├─ 7b:financial_statement(跨表依賴 monthly_revenue 對齊)
         │
         └─ 7c:rust_bridge.run_phase4 — tw_market_core 系列
               ├─ price_daily_fwd / price_weekly_fwd / price_monthly_fwd
               ├─ price_limit_merge_events
               └─ dirty queue:None + full_rebuild=False → SELECT DISTINCT stock_id
                                                          FROM price_daily_fwd
                                                          WHERE is_dirty=TRUE
                                                          0 → skip
```

---

## 1. Bronze 層 — 依資料屬性分層

39 個 entry 寫入 ~28 個 Bronze table(部分 entry 共用同一表 — e.g. financial 三表 → `financial_statement`)。

### 1.1 Reference(2 張)

| 表 | PK | 寫入 entry | dataset | code path |
|---|---|---|---|---|
| `trading_date_ref` | (market, date) | `trading_calendar` | TaiwanStockTradingDate | phase_executor → field_mapper(無 rename)→ db.upsert |
| `stock_info_ref` | (market, stock_id) | `stock_info` / `stock_delisting` | TaiwanStockInfo / TaiwanStockDelisting | stock_delisting 走 `merge_strategy="update_delist_date"` 客製 UPDATE |

### 1.2 價格類(5 張)

| 表 | PK | 寫入 entry | dataset | 關鍵欄位 |
|---|---|---|---|---|
| `price_daily` | (market, stock_id, date) | `price_daily` | TaiwanStockPrice | open/high/low/close/volume/turnover + detail |
| `price_limit` | (market, stock_id, date) | `price_limit` | TaiwanStockPriceLimit | limit_up/limit_down + detail(_reference_price)|
| `market_index_tw` | (market, stock_id, date) | `market_index_tw` | TaiwanStockTotalReturnIndex | price(報酬指數,只 close)|
| `market_ohlcv_tw` | (market, stock_id, date) | `market_ohlcv_v3` | TaiwanStockPrice(TAIEX/TPEx 當 stock 抓) | OHLCV + detail |
| `market_index_us` | (market, stock_id, date) | `market_index_us` | USStockPrice | OHLCV(SPY/^VIX) + detail(_adj_close) |

**code 細節**(`field_mapper.transform`):
- `price_daily`:`max → high` / `min → low` / `Trading_Volume → volume`(v1.2/v1.3 漏 rename 會被 PRAGMA 過濾)
- `market_ohlcv_v3`:同 price_daily pattern,但 `Trading_money` / `Trading_turnover` / `spread` 走 detail
- `market_index_us`:`Open/Close/High/Low/Volume → 小寫`,`Adj_Close → _adj_close`

### 1.3 事件類(3 張 + 1 staging)

| 表 | PK | 寫入 entry(event_type)| dataset | 備註 |
|---|---|---|---|---|
| `price_adjustment_events` | (market, stock_id, date, event_type) | `dividend_result` (dividend) / `capital_reduction` (capital_reduction) / `split_price` (split) / `par_value_change` / dividend_policy 經 post_process | 4 dataset + 1 衍生 | 5 種 event_type 共用;`computed_fields = ["volume_factor"]` |
| `_dividend_policy_staging` | (market, stock_id, date) | `dividend_policy` | TaiwanStockDividend | post_process 後不留;21 個 PascalCase 全 pack 進 detail |
| `stock_suspension_events` | (market, stock_id, suspension_date, suspension_time) | `stock_suspension` | TaiwanStockSuspended | v3.2 B-4;reason / resumption_date / resumption_time |

**post_process 邏輯**(`src/post_process.py:dividend_policy_merge`):從 staging detail 拆 `_StockExDividendTradingDate / _CashExDividendTradingDate / _TotalNumberOfCashCapitalIncrease` 等,衍生 `dividend` events 寫進 `price_adjustment_events`,並偵測純現增(`cash_increase` event_type)。

**Trigger**(PR #20):`mark_fwd_dirty_on_event` ON `price_adjustment_events` → `trg_mark_fwd_silver_dirty()` → UPDATE 4 fwd 表 SET is_dirty=TRUE(全段歷史 mark)。

### 1.4 個股籌碼類(9 張)

| 表 | PK | 寫入 entry | dataset | 備註 |
|---|---|---|---|---|
| `institutional_investors_tw` | (market, stock_id, date, investor_type) | (PR #18 reverse-pivot 反推自 institutional_daily) | — | 5 類法人;非 collector.toml entry,由 `scripts/reverse_pivot_institutional.py` 從 v2.0 legacy 反推填入 |
| `margin_purchase_short_sale_tw` | (market, stock_id, date) | (PR #18 reverse-pivot 反推自 margin_daily) | — | 同上;14 raw fields |
| `securities_lending_tw` | (market, stock_id, date, transaction_type, fee_rate) | `securities_lending` | TaiwanStockSecuritiesLending | v3.2 B-5;借券成交明細 trade-level |
| `short_sale_securities_lending_tw` | (market, stock_id, date) | `short_sale_securities_lending_v3` | TaiwanDailyShortSaleBalances | PR #21-B;借券賣出 daily aggregate;餵 margin builder SBL 3 欄 |
| `foreign_investor_share_tw` | (market, stock_id, date) | (PR #18 reverse-pivot 反推) | — | 11 raw fields |
| `day_trading_tw` | (market, stock_id, date) | (PR #18 reverse-pivot 反推) | — | 4 raw fields |
| `valuation_per_tw` | (market, stock_id, date) | (PR #18 reverse-pivot 反推) | — | per/pbr/dividend_yield |
| `holding_shares_per`(R3 主名) | (market, stock_id, date, holding_shares_level) | `holding_shares_per_v3` | TaiwanStockHoldingSharesPer | PR #18.5 raw,1 row/level(無 aggregation) |
| `government_bank_buy_sell_tw` | (market, stock_id, date) | `government_bank_buy_sell_v3` [DISABLED] | TaiwanStockGovernmentBankBuySell | 需 FinMind sponsor tier;Bronze schema + trigger 已落,等升 tier |

### 1.5 市場級籌碼類(3 張,無 stock_id)

| 表 | PK | 寫入 entry | dataset | 備註 |
|---|---|---|---|---|
| `institutional_market_daily` | (market, date) | `institutional_market` | TaiwanStockTotalInstitutionalInvestors | `aggregation = pivot_institutional_market`(每日 3 列 → 1 列)|
| `market_margin_maintenance` | (market, date) | `market_margin` | TaiwanTotalExchangeMarginMaintenance | ratio(融資維持率) |
| `total_margin_purchase_short_sale_tw` | (market, date, name) | `total_margin_purchase_short_sale_v3` | TaiwanStockTotalMarginPurchaseShortSale | PR #21-B;FinMind 是 pivoted-by-row(name ∈ {MarginPurchase, ShortSale}),Silver builder 走 pivot |

### 1.6 財報類(2 張)

| 表 | PK | 寫入 entry(stmt_type/event_type)| dataset | 備註 |
|---|---|---|---|---|
| `financial_statement`(R3 主名) | (market, stock_id, date, event_type, origin_name) | `financial_income_v3` (income) / `financial_balance_v3` (balance) / `financial_cashflow_v3` (cashflow) | TaiwanStockFinancialStatements / BalanceSheet / CashFlowsStatement | PR #18.5 raw,1 row/origin_name(無 aggregation)|
| `monthly_revenue`(R3 主名) | (market, stock_id, date) | `monthly_revenue_v3` | TaiwanStockMonthRevenue | PR #18.5 raw,FinMind 原欄名(revenue_year/month;Silver builder 才 rename 為 yoy/mom)|

### 1.7 總體類(3 張)

| 表 | PK | 寫入 entry | dataset | 備註 |
|---|---|---|---|---|
| `exchange_rate` | (market, date, currency) | `exchange_rate` | TaiwanExchangeRate | per_stock_fixed × 4 幣(USD/CNY/JPY/AUD);必須帶 data_id 才回完整時序 |
| `fear_greed_index` | (market, date) | `fear_greed` | CnnFearGreedIndex | score / label;美股恐懼貪婪 |
| `business_indicator_tw` | (market, date) | `business_indicator` | TaiwanBusinessIndicator | v3.2 B-6;月頻;leading/coincident/lagging 加 `_indicator` 後綴避 PG 保留字 |

### 1.8 v2.0 legacy 觀察期(R5 21~60 天 → R6 後 DROP)

3 張表透過 `*_legacy` entry dual-write,等 SLO 確認後 R6 永久 DROP。

| 表 | 寫入 entry | 對應 R3 升格主名 |
|---|---|---|
| `holding_shares_per_legacy_v2` | `holding_shares_per_legacy`(R4 改名) | `holding_shares_per` |
| `monthly_revenue_legacy_v2` | `monthly_revenue_legacy` | `monthly_revenue` |
| `financial_statement_legacy_v2` | `financial_income_legacy` / `financial_balance_legacy` / `financial_cashflow_legacy` | `financial_statement` |

### 1.9 v2.0 legacy spec §7.3 退場候選(無觀察期)

R6 之後另起退場 PR(編號 #R7+),DROP 6 張舊表 + 對應 collector.toml entry。

| 表 | 寫入 entry | 替代來源 | 退場 PR |
|---|---|---|---|
| `institutional_daily` | `institutional_daily`(`pivot_institutional`) | `institutional_investors_tw`(reverse-pivot) | 退場候選 |
| `margin_daily` | `margin_daily` | `margin_purchase_short_sale_tw` | 退場候選 |
| `foreign_holding` | `foreign_holding` | `foreign_investor_share_tw` | 退場候選 |
| `day_trading` | `day_trading` | `day_trading_tw` | 退場候選 |
| `valuation_daily` | `valuation_daily` | `valuation_per_tw` | 退場候選 |
| `index_weight_daily` | `index_weight` | (無下游使用)| 直接 DROP |

### 1.10 Rust 反推產物(4 張,Bronze 但由 Silver 7c 產出)

| 表 | PK | 來源 | 計算邏輯 |
|---|---|---|---|
| `price_daily_fwd` | (market, stock_id, date) | `price_daily` + `price_adjustment_events` | Rust 倒推 multiplier,multiplier 拆 `price_multiplier`(從 AF) + `volume_multiplier`(從 vf) |
| `price_weekly_fwd` | (market, stock_id, year, week) | `price_daily_fwd` | Rust ISO week 聚合 |
| `price_monthly_fwd` | (market, stock_id, year, month) | `price_daily_fwd` | Rust 月聚合 |
| `price_limit_merge_events` | TBD per spec §2.6.6 | `price_limit` | Rust 7c 合併連續漲跌停事件 |

(Spec 把這 4 張歸 Silver 14 張清單;命名仍是 Bronze-like,由 Rust 計算,因此放這裡參照。)

---

## 2. Silver 層(進階資料計算)

### 2.1 12 個獨立 derived(Python builder × `silver/orchestrator.PHASE_7A_BUILDERS`)

每個 builder 走相同 pattern:`fetch_bronze → pivot/pack/UNION → upsert_silver`(`silver/_common.py` 共用)。

| Silver | builder file | Bronze 來源 | 備註 |
|---|---|---|---|
| `institutional_daily_derived` | `institutional.py` | `institutional_investors_tw` + `government_bank_buy_sell_tw`(LEFT JOIN) | 5 類法人 pivot;`gov_bank_net = buy - sell`(任一 NULL → NULL) |
| `margin_daily_derived` | `margin.py` | `margin_purchase_short_sale_tw` ∪ `short_sale_securities_lending_tw` | v1.26 後 UNION 主∪副 keys 消 stub row;6 stored + detail + 3 alias + 3 SBL |
| `foreign_holding_derived` | `foreign_holding.py` | `foreign_investor_share_tw` | 2 stored + detail JSONB pack(9 keys) |
| `holding_shares_per_derived` | `holding_shares_per.py` | `holding_shares_per`(R3 主名) | N rows/level → 1 row + detail JSONB pack |
| `valuation_daily_derived` | `valuation.py` | `valuation_per_tw` + `price_daily` + `foreign_investor_share_tw` | 3 stored 1:1 + 衍生欄 `market_value_weight = (close × total_issued) / SUM_market` |
| `day_trading_derived` | `day_trading.py` | `day_trading_tw` + `price_daily` (LEFT JOIN) | 衍生欄 `day_trading_ratio = dt_volume / pd_volume × 100` |
| `monthly_revenue_derived` | `monthly_revenue.py` | `monthly_revenue`(R3 主名) | rename revenue_year → revenue_yoy / revenue_month → revenue_mom |
| `taiex_index_derived` | `taiex_index.py` | `market_ohlcv_tw` | OHLCV 1:1 |
| `us_market_index_derived` | `us_market_index.py` | `market_index_us` | OHLCV 1:1(SPY/^VIX)|
| `exchange_rate_derived` | `exchange_rate.py` | `exchange_rate` | PK 含 currency,rate + detail 1:1 |
| `market_margin_maintenance_derived` | `market_margin.py` | `market_margin_maintenance` ∪ `total_margin_purchase_short_sale_tw` | v1.26 後 UNION;ratio 1:1 + 2 衍生欄(total_margin pivot) |
| `business_indicator_derived` | `business_indicator.py` | `business_indicator_tw` | PK 注入 sentinel `stock_id='_market_'`(對齊 Silver 3-col PK convention)|

### 2.2 1 個跨表依賴 derived(7b)

| Silver | builder file | Bronze 來源 | 備註 |
|---|---|---|---|
| `financial_statement_derived` | `financial_statement.py` | `financial_statement`(R3 主名) | event_type → type;origin_name → detail JSONB |

### 2.3 4 個 Rust 計算產物(7c)

走 `rust_bridge.run_phase4`,不在 `silver/builders/` 下:

| Silver | 來源 | 計算 |
|---|---|---|
| `price_daily_fwd` | price_daily + pae | 後復權倒推(price/volume multiplier 拆兩個) |
| `price_weekly_fwd` | price_daily_fwd | ISO week 聚合 |
| `price_monthly_fwd` | price_daily_fwd | 月聚合 |
| `price_limit_merge_events` | price_limit | 漲跌停事件合併 |

### 2.4 Silver 共用 helper(`src/silver/_common.py`)

| 函式 | 用途 |
|---|---|
| `fetch_bronze(db, table, stock_ids=, where=, order_by=)` | 統一 SELECT Bronze + stock filter |
| `upsert_silver(db, table, rows, pk_cols=)` | 批次 UPSERT 自帶 `is_dirty=FALSE / dirty_at=NULL` |
| `reset_dirty(db, table, pks, pk_cols)` | 顯式 reset(備用,trigger path 走) |
| `get_trading_dates(db)` | 一次讀 trading_date_ref(過濾鬼資料用) |

---

## 3. Bronze→Silver dirty trigger(18 個)

PR #20(15)+ PR #21-B(3)落地。Bronze upsert 觸發 trigger 自動 mark Silver row dirty。

### 3.1 通用 3-col PK(10)

`trg_mark_silver_dirty(silver_table)` 共用 function,對 Bronze (market, stock_id, date)pivot 進 Silver 同 PK。

| Bronze | Silver |
|---|---|
| `institutional_investors_tw` | `institutional_daily_derived` |
| `margin_purchase_short_sale_tw` | `margin_daily_derived` |
| `securities_lending_tw` | `margin_daily_derived` |
| `foreign_investor_share_tw` | `foreign_holding_derived` |
| `holding_shares_per`(R3 主名)| `holding_shares_per_derived` |
| `day_trading_tw` | `day_trading_derived` |
| `valuation_per_tw` | `valuation_daily_derived` |
| `monthly_revenue`(R3 主名)| `monthly_revenue_derived` |
| `market_ohlcv_tw` | `taiex_index_derived` |
| `market_index_us` | `us_market_index_derived` |

### 3.2 PK 變體 special(5)

| Bronze | Silver | trigger function | 變體 |
|---|---|---|---|
| `financial_statement`(R3 主名)| `financial_statement_derived` | `trg_mark_financial_stmt_dirty()` | event_type → type(4-col PK)|
| `exchange_rate` | `exchange_rate_derived` | `trg_mark_exchange_rate_dirty()` | PK 含 currency 不含 stock_id |
| `market_margin_maintenance` | `market_margin_maintenance_derived` | `trg_mark_market_margin_dirty()` | 2-col PK |
| `business_indicator_tw` | `business_indicator_derived` | `trg_mark_business_indicator_dirty()` | 注入 sentinel `_market_` |
| `price_adjustment_events` | 4 fwd 表整檔 dirty | `trg_mark_fwd_silver_dirty()` | UPDATE 4 fwd 全段歷史 SET is_dirty=TRUE |

### 3.3 PR #21-B 副 Bronze trigger(3)

| Bronze | Silver | trigger function |
|---|---|---|
| `government_bank_buy_sell_tw` | `institutional_daily_derived` | `trg_mark_silver_dirty('institutional_daily_derived')` |
| `total_margin_purchase_short_sale_tw` | `market_margin_maintenance_derived` | `trg_mark_market_margin_dirty()`(reuse,函式 body 一致)|
| `short_sale_securities_lending_tw` | `margin_daily_derived` | `trg_mark_silver_dirty('margin_daily_derived')` |

---

## 4. Bronze ↔ Silver 對照(反向查找)

| Bronze | Silver consumer | 備註 |
|---|---|---|
| `trading_date_ref` | (Reference,被 institutional aggregator 引用)| — |
| `stock_info_ref` | (Reference,被 stock_resolver 引用)| — |
| `price_daily` | Phase 4/7c Rust + valuation/day_trading builders LEFT JOIN | 多消費者 |
| `price_limit` | `price_limit_merge_events`(Rust 7c)| — |
| `market_index_tw` | (無 Silver consumer;市場指數已由 market_ohlcv_tw 替代)| 保留 reference |
| `market_ohlcv_tw` | `taiex_index_derived` | 1:1 |
| `market_index_us` | `us_market_index_derived` | 1:1 |
| `price_adjustment_events` | trigger → 4 fwd 表 dirty(Rust 7c) | 全段 mark |
| `_dividend_policy_staging` | (post_process 後不留)| — |
| `stock_suspension_events` | (M3 prev_trading_day,未動工)| — |
| `institutional_investors_tw` | `institutional_daily_derived` | + gov_bank LEFT JOIN |
| `margin_purchase_short_sale_tw` | `margin_daily_derived` | + SBL UNION |
| `securities_lending_tw` | trigger → margin_daily_derived dirty(stub)| 不在 builder 主路徑,trigger only |
| `short_sale_securities_lending_tw` | `margin_daily_derived` | UNION 副 Bronze |
| `foreign_investor_share_tw` | `foreign_holding_derived` | 1:1 |
| `day_trading_tw` | `day_trading_derived` | + price_daily LEFT JOIN |
| `valuation_per_tw` | `valuation_daily_derived` | + price_daily / foreign_investor_share LEFT JOIN |
| `holding_shares_per`(R3) | `holding_shares_per_derived` | levels pack |
| `government_bank_buy_sell_tw`(disabled)| `institutional_daily_derived`(LEFT JOIN,缺 row → gov_bank_net=NULL)| — |
| `total_margin_purchase_short_sale_tw` | `market_margin_maintenance_derived` | UNION 副 Bronze pivot |
| `monthly_revenue`(R3) | `monthly_revenue_derived` | rename |
| `financial_statement`(R3) | `financial_statement_derived` | origin_name pack |
| `institutional_market_daily` | (無 Silver builder)| reference 用 |
| `market_margin_maintenance` | `market_margin_maintenance_derived` | + total_margin UNION |
| `exchange_rate` | `exchange_rate_derived` | 1:1 |
| `fear_greed_index` | (無 Silver builder)| reference 用 |
| `business_indicator_tw` | `business_indicator_derived` | 注入 sentinel |
| `*_legacy_v2`(3 張) | (R5 觀察期對照表)| R6 後 DROP |
| `institutional_daily` / `margin_daily` / `foreign_holding` / `day_trading` / `valuation_daily` / `index_weight_daily` | (PR #18 reverse-pivot 反向來源)| spec §7.3 退場候選 |

---

## 5. Code 檔案索引

### 5.1 Top-level

| 檔 | 用途 | 關鍵入口 |
|---|---|---|
| `src/main.py` | CLI(argparse + asyncio dispatch) | `_run_collector` / `_run_silver` |
| `src/bronze/phase_executor.py` | Bronze 排程 + Rust 派工 | `PhaseExecutor.run(mode)` |
| `src/silver/orchestrator.py` | Silver 排程(7a/7b/7c)| `SilverOrchestrator.run(phases, ...)` |

### 5.2 Bronze 寫入 pipeline

| 檔 | 用途 | 關鍵函式 |
|---|---|---|
| `src/api_client.py` | aiohttp FinMind v4 HTTP client | `FinMindClient.fetch(api_config, stock_id, seg_start, seg_end)` |
| `src/rate_limiter.py` | token bucket(1600/h、min_interval 2250ms、429 cooldown 120s)| `RateLimiter.wait()` |
| `src/sync_tracker.py` | api_sync_progress 5 種 status | `is_completed / mark_progress / mark_failed / mark_schema_mismatch / mark_empty / get_last_sync` |
| `src/date_segmenter.py` | segment 計算(per_stock 365d / financial 0d 全段抓)| `DateSegmenter.segments(api_config, mode, stock_id)` |
| `src/field_mapper.py` | API → schema 映射 + detail JSONB pack + computed_fields | `FieldMapper.transform(api_config, raw_records) → (rows, schema_mismatch)` |
| `src/aggregators.py` | pivot/pack 4 個 + `_filter_to_trading_days` | `apply_aggregation(name, rows, db, **opts)` |
| `src/db.py` | DBWriter Protocol + PostgresWriter | `upsert / upsert_with_strategy / query / query_one / table_pks` |
| `src/post_process.py` | dividend_policy → events 拆分 + `_recompute_stock_dividend_vf` | `dividend_policy_merge(db, stock_id)` |
| `src/rust_bridge.py` | subprocess 派 Rust binary | `RustBridge.run_phase4(stock_ids, mode)` |

### 5.3 Silver builders(13 個 + helpers)

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

### 5.4 Rust binary

| 檔 | 用途 |
|---|---|
| `rust_compute/src/main.rs` | tw_market_core 後復權 + 週/月聚合 + price_limit_merge_events + dirty queue self-pull(v1.26) |
| `rust_compute/Cargo.toml` | dependencies(sqlx + chrono + serde) |

### 5.5 aggregators(`src/aggregators.py`)

| function | 用途 | 對應 entry |
|---|---|---|
| `pivot_institutional(rows, db)` | 5 類法人 pivot 成 1 row × 10 col | `institutional_daily` |
| `pivot_institutional_market(rows, db)` | 全市場 pivot | `institutional_market` |
| `pack_holding_shares(rows)` | 多 level → detail JSONB pack | `holding_shares_per_legacy` |
| `pack_financial(rows, stmt_type)` | 中→英 origin_name → detail JSONB | `financial_*_legacy` 三 entry |
| `_filter_to_trading_days(rows, db)` | 過濾 FinMind 週六鬼資料 | pivot_institutional 內部呼叫 |

### 5.6 Verifier scripts

| 檔 | 驗證範圍 |
|---|---|
| `scripts/verify_pr18_bronze.py` | 5 張 PR #18 reverse-pivot 反推 round-trip(against legacy v2)|
| `scripts/verify_pr19b_silver.py` | 5 個 simple Silver builder 對 v2.0 legacy 等值 |
| `scripts/verify_pr19c_silver.py` | 5 個 market-level Silver builder 對 Bronze 1:1 |
| `scripts/verify_pr19c2_silver.py` | 3 個 PR #18.5 依賴 Silver builder 對 `_legacy_v2` 等值(R5 觀察期 SLO ±1%)|
| `scripts/verify_pr20_triggers.py` | 18 個 Bronze→Silver dirty trigger 整合測試 |

---

## 6. Schema 統計

- **alembic head**:`u0v1w2x3y4z5_pr_r4_rename_v2_entry_names_legacy`(2026-05-09)
- **Reference 表**:2(`trading_date_ref` / `stock_info_ref`)
- **Bronze 表**:~28 個常駐(各類加總)+ 1 staging(`_dividend_policy_staging`)
- **Silver 表**:14 個(12 個 7a + 1 個 7b + 4 個 7c Rust;`price_limit_merge_events` 含內)
- **System 表**:3(`schema_metadata` / `stock_sync_status` / `api_sync_progress`)
- **collector.toml entries**:39(38 enabled + 1 `government_bank_buy_sell_v3` disabled — sponsor tier)
- **Silver builders**:13(12 in 7a + 1 in 7b)
- **Bronze→Silver triggers**:18(15 from PR #20 alembic n3o4p5q6r7s8 + 3 from PR #21-B alembic p5q6r7s8t9u0)

---

## 7. 命名 / 慣例(動工前必看)

對齊 CLAUDE.md「關鍵架構決策(不要改)」表。

| 慣例 | 說明 |
|---|---|
| `FieldMapper(db=db)` | 一定要帶 db,讓 schema 補欄位豁免名單 |
| `field_mapper.transform()` | 回 `(rows, schema_mismatch: bool)` tuple |
| `db.upsert()` | 自帶 PRAGMA 欄位過濾(API 新增欄位不炸) |
| `silver/_common.upsert_silver()` | 自帶 `is_dirty=FALSE` |
| `_table_pks` | 動態查 `information_schema`(schema 是 single source of truth)|
| `EXPECTED_SCHEMA_VERSION = "3.2"` | `rust_bridge.py:31` — schema 升版時 Rust + Python 兩端一起改 |
| PostgresWriter 單 connection | Silver 7a builder 串列跑(concurrent thread access 踩 psycopg thread-safety)|
| Phase 4 incremental 走 dirty queue(v1.26 起)| 0 dirty → skip Rust dispatch |
| Windows binary path 自動補 `.exe` | `rust_bridge.py` 內處理 |
| `cooldown_on_429_sec` 存 RateLimiter 實例 | api_client 從這讀,不 reread config |
| Rust 後復權拆兩個 multiplier | `price_multiplier`(從 AF) + `volume_multiplier`(從 vf);v1.8 切換 |

---

## 8. 加 entry / 改 schema 流程

1. **加 collector.toml entry** — 確認 `param_mode / target_table / field_rename / aggregation / event_type / detail_fields / computed_fields` 是否齊全
2. **alembic migration**(若新 Bronze 表)— 新 CREATE TABLE + 對應 dirty trigger(若 Silver builder 接)+ schema_pg.sql 同步加 DDL(給 fresh DB)
3. **field_mapper / aggregators**(若新欄位 mapping 邏輯)— 加 `computed_fields` handler 或 `apply_aggregation` 分支
4. **db.upsert**(通常不用改)— PRAGMA 自動處理新欄
5. **Silver builder**(若新 Bronze 要餵 Silver)— 新檔 in `src/silver/builders/` + 註冊到 `BUILDERS` dict + 註冊到 `orchestrator.PHASE_GROUPS`
6. **CLAUDE.md** — alembic head + PR sequencing + v1.X 段落
7. **本檔** — 同步加 entry 到對應分類段(1.1~1.10) + Silver builder 段(2.x)+ trigger 段(3.x)+ 對照表(4.)

---

_Generated 2026-05-09 / 結構從 Phase-based 改成 Bronze/Silver 屬性分層(v1.26 後)_
