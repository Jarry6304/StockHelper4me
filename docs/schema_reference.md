# DB Schema 速查手冊(v3.2 r1)

> **版本**:v3.2 r1
> **alembic head**:`x3y4z5a6b7c8`(2026-05-11,Hotfix A1 — financial_statement PK origin_name → type)
> **schema_version**:`3.2`(寫在 `schema_metadata.value WHERE key='schema_version'`)
> **盤點時間**:2026-05-14
> **總表數**:~56(Bronze 26 + Silver 18 + M3 三表 + Reference/System 3 + Legacy 3 + staging 1)

---

## 0. 本檔定位

本檔是「**by-table 速查手冊**」— 給定一個表名 → 快速看到 layer / PK / 上游 / 對應 spec 章節。**不**抄欄位明細 / DDL / SQL — 那些在 spec / `schema_pg.sql` / `alembic/versions/`。

**找不到要找的東西?** 看 `docs/schema_master.md` 的決策樹。

### 與 v2.0(舊版)的關係

舊 v2.0(SCHEMA_VERSION=2.0 / 27 張表 / M1 era)已完全被本版取代。原本以「Phase 1-6」為主軸組織的章節 retired,改以「Bronze 層 / Silver 層 / M3 Cores 三表 / Reference / System / Legacy」分類。歷史值想看 git log `docs/schema_reference.md` 前一版。

---

## 1. 文件定位與分工

| 文件 | 主軸 | 收什麼 | 不收什麼 |
|---|---|---|---|
| **本檔**(`schema_reference.md`)| **by table 速查** | 56 張表 stub + PR 歷史 | 欄位明細 / SQL / Cores 邏輯 |
| `m2Spec/layered_schema_post_refactor.md` | **by layer 規範** | Bronze + Silver 欄位 / dirty trigger / 來源語意 | M3 Cores 三表 |
| `m3Spec/cores_overview.md` §3 / §7 | **by core 契約** | IndicatorCore trait / 三表寫入規約 / params_hash | 個別 Core spec |
| `m3Spec/{indicator,chip,...}_cores.md` | **by core deep-dive** | 各 core 的 Params / Output / EventKind | DB schema |
| `docs/cores_schema_map.md` | **by core 反查** | 22 cores ↔ tables 關係 | 個別 core 內部邏輯 |
| `docs/api_pipeline_reference.md` | **by collector entry 索引** | collector.toml entry × code path × Bronze 表 | Silver 後流程 |
| `src/schema_pg.sql` | **fresh DB DDL** | 一鍵建表的 SQL | 規範 / 語意 |
| `alembic/versions/*.py` | **增量 migration** | 每次 schema 變更的 op + revision id | 規範 / 語意 |

權威順序見 `docs/schema_master.md` §2。

---

## 2. 全表清單(~56 張)

> **欄位說明**:`layer` 用 `B0-B6 / S1-S6 / M3 / Ref / Sys / Legacy`(對齊 layered_schema §1.1);`狀態` 用 `active / staging / legacy`;`spec` 用相對路徑 + 錨點。

### Bronze 層(26 張 active + 3 legacy)

| 表名 | layer | PK | 狀態 | spec 錨點 | 引入版本 |
|---|---|---|---|---|---|
| `trading_date_ref` | B0 | (market, date) | active | layered §3.1 | baseline |
| `stock_info_ref` | B1 | (market, stock_id) | active | layered §3.2 | baseline |
| `market_index_tw` | B1 | (market, stock_id, date) | active | layered §3.2 | baseline |
| `market_ohlcv_tw` | B1 | (market, stock_id, date) | active | layered §3.2 | PR #11 / B-1 |
| `price_adjustment_events` | B2 | (market, stock_id, date, event_type) | active | layered §3.3 | baseline |
| `stock_suspension_events` | B2 | (market, stock_id, suspension_date) | active | layered §3.3 | baseline |
| `securities_lending_tw` | B2 | (market, stock_id, date, transaction_type, fee_rate) | active | layered §3.3 | baseline |
| `business_indicator_tw` | B2 | (market, date) | active | layered §3.3 | PR #M3-batch r3 |
| `_dividend_policy_staging` | B2 | (market, stock_id, date) | staging | layered §3.3 | baseline |
| `price_daily` | B3 | (market, stock_id, date) | active | layered §3.4 | baseline |
| `price_limit` | B3 | (market, stock_id, date) | active | layered §3.4 | baseline |
| `institutional_investors_tw` | B4 | (market, stock_id, date, investor_type) | active | layered §3.5 | PR #18(reverse-pivot) |
| `margin_purchase_short_sale_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #18 |
| `foreign_investor_share_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #18 |
| `day_trading_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #18 |
| `valuation_per_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #18 |
| `holding_shares_per` | B4 | (market, stock_id, date, holding_shares_level) | active(R3 升格主名) | layered §3.5 | PR #R3 |
| `government_bank_buy_sell_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #21-B |
| `short_sale_securities_lending_tw` | B4 | (market, stock_id, date) | active | layered §3.5 | PR #21-B |
| `financial_statement` | B5 | (market, stock_id, date, event_type, type) | active(R3 升格 / A1 PK fix) | layered §3.6 | PR #R3 + Hotfix A1 |
| `monthly_revenue` | B5 | (market, stock_id, date) | active(R3 升格主名) | layered §3.6 | PR #R3 |
| `market_index_us` | B6 | (market, stock_id, date) | active | layered §3.7 | baseline |
| `exchange_rate` | B6 | (market, date, currency) | active | layered §3.7 | baseline |
| `institutional_market_daily` | B6 | (market, date) | active | layered §3.7 | baseline |
| `market_margin_maintenance` | B6 | (market, date) | active | layered §3.7 | baseline |
| `total_margin_purchase_short_sale_tw` | B6 | (market, date, name) | active | layered §3.7 | PR #21-B |
| `fear_greed_index` | B6 | (market, date) | active | layered §3.7 | baseline |

### Silver 層(18 張)

| 表名 | layer | PK | 狀態 | spec 錨點 | 引入版本 |
|---|---|---|---|---|---|
| `price_daily_fwd` | S1 | (market, stock_id, date) | active | layered §4.1 | baseline |
| `price_weekly_fwd` | S1 | (market, stock_id, year, week) | active | layered §4.1 | baseline |
| `price_monthly_fwd` | S1 | (market, stock_id, year, month) | active | layered §4.1 | baseline |
| `price_limit_merge_events` | S1 | (market, stock_id, date) | active | layered §4.1 | PR #20 |
| `institutional_daily_derived` | S4 | (market, stock_id, date) | active | layered §4.2 | PR #19a |
| `margin_daily_derived` | S4 | (market, stock_id, date) | active | layered §4.2 | PR #19a |
| `foreign_holding_derived` | S4 | (market, stock_id, date) | active | layered §4.2 | PR #19a |
| `day_trading_derived` | S4 | (market, stock_id, date) | active | layered §4.2 | PR #19a |
| `holding_shares_per_derived` | S4 | (market, stock_id, date) | active | layered §4.2 | PR #19a |
| `valuation_daily_derived` | S5 | (market, stock_id, date) | active | layered §4.3 | PR #19a |
| `financial_statement_derived` | S5 | (market, stock_id, date, type) | active | layered §4.3 | PR #19a |
| `monthly_revenue_derived` | S5 | (market, stock_id, date) | active | layered §4.3 | PR #19a |
| `taiex_index_derived` | S6 | (market, stock_id, date) | active | layered §4.4 | PR #19a |
| `us_market_index_derived` | S6 | (market, stock_id, date) | active | layered §4.4 | PR #19a |
| `exchange_rate_derived` | S6 | (market, date, currency) | active | layered §4.4 | PR #19a |
| `market_margin_maintenance_derived` | S6 | (market, date) | active | layered §4.4 | PR #19a |
| `business_indicator_derived` | S6 | (market, stock_id='_market_', date) | active | layered §4.4 | PR #M3-batch r3 |
| `fear_greed_index` *(直讀 Bronze,例外)* | S6 | (market, date) | active | layered §4.4 | baseline |

### M3 Cores 三表(2026-05-09 PR-7 落地)

| 表名 | PK / Unique constraint | 用途 | spec 錨點 |
|---|---|---|---|
| `indicator_values` | (stock_id, value_date, timeframe, source_core, params_hash) | 時序輸出(每 core 多 row);MACD/RSI/KD/MA 等指標序列 | cores_overview §7.1 |
| `structural_snapshots` | (stock_id, snapshot_date, timeframe, core_name, params_hash) | 結構快照(每 stock 少 row);Wave Forest / SR / Trendline | cores_overview §7.1 |
| `facts` | UNIQUE(stock_id, fact_date, timeframe, source_core, COALESCE(params_hash,''), md5(statement)) | Append-only 事件 Fact;golden_cross / breakout / divergence 等 | cores_overview §六 + §7.4 |

### Reference / System 表(3 張)

| 表名 | PK | 用途 |
|---|---|---|
| `schema_metadata` | key | 版本控制(`schema_version` / `migrated_from` / `migrated_at`);Rust binary 啟動時 assert |
| `stock_sync_status` | (market, stock_id) | 同步進度(`last_full_sync` / `last_incr_sync` / `fwd_adj_valid`);v1.26 後 Rust binary 改讀 `price_daily_fwd.is_dirty` |
| `api_sync_progress` | (api_name, stock_id, segment_start) | API 斷點續傳;`status` ∈ `pending / completed / failed / empty / schema_mismatch`(由 alembic `a1b2c3d4e5f6` CHECK 落下) |

### Legacy 退場表(3 張,PR #R6 待 DROP)

| 表名 | 對應主名 | rename 來源 | 預計 DROP |
|---|---|---|---|
| `holding_shares_per_legacy_v2` | `holding_shares_per` | PR #R2(2026-05-09) | PR #R6(觀察期 21-60 天後) |
| `financial_statement_legacy_v2` | `financial_statement` | PR #R2 | PR #R6 |
| `monthly_revenue_legacy_v2` | `monthly_revenue` | PR #R2 | PR #R6 |

---

## 3. Bronze 速查(26 張)

> 每表 PK + 一行語意 + 連結 layered_schema。**不抄欄位明細**(那在 layered_schema §3.x)。

### B0 — Calendar

- **`trading_date_ref`** PK(market, date)— 交易日曆,Phase 0 預載入,所有 Phase 5/6 aggregator 對齊用。詳見 layered §3.1。

### B1 — Meta / Reference

- **`stock_info_ref`** PK(market, stock_id)— 股票主檔,含 `delist_date`(NULL=未下市)/ `market_type` / `industry`。upsert UPDATE 強制 `updated_at=NOW()`(v1.7 P0-P1 路徑同步)。layered §3.2。
- **`market_index_tw`** PK(market, stock_id, date)— 加權指數 / 櫃買指數(TAIEX / TPEx);收盤價 + 漲跌 + 成交額。詳見 layered §3.2。
- **`market_ohlcv_tw`** PK(market, stock_id, date)— TAIEX / TPEx 完整 OHLCV(來源 `TaiwanStockPrice` with data_id=TAIEX/TPEx;PR #22 解 5Seconds aggregate 失敗後改走此 dataset)。layered §3.2。

### B2 — Events / Staging

- **`price_adjustment_events`** PK(market, stock_id, date, event_type)— 5 種 event:`dividend / capital_reduction / split / par_value_change / capital_increase`;含 `before_price` / `reference_price` / `adjustment_factor` / `volume_factor`(v1.8 拆兩 multiplier)。Rust S1 後復權的權威輸入。layered §3.3。
- **`stock_suspension_events`** PK(market, stock_id, suspension_date)— 暫停交易事件。layered §3.3。
- **`securities_lending_tw`** PK(market, stock_id, date, transaction_type, fee_rate)— 借券成交明細(trade-level);與 `short_sale_securities_lending_tw`(SBL daily)是不同 dataset。layered §3.3。
- **`business_indicator_tw`** PK(market, date)— 月頻景氣指標(國發會;領先/同時/落後/燈號);PR #M3-batch r3 引入。layered §3.3。
- **`_dividend_policy_staging`** PK(market, stock_id, date)— 暫存表,post_process 拆解 dividend_policy 到 events 後清空。

### B3 — Raw Price

- **`price_daily`** PK(market, stock_id, date)— 原始日 K(未復權);Rust S1 從這推 fwd。layered §3.4。
- **`price_limit`** PK(market, stock_id, date)— 漲跌停參考(`limit_up` / `limit_down` / `detail` 含 `_reference_price`)。layered §3.4。

### B4 — Chip(籌碼)

> v3.2 r1 一律 raw(展開 / 多 row),Silver `*_derived` 做 pivot / aggregation。PR #18 系列 reverse-pivot 落地。

- **`institutional_investors_tw`** PK(market, stock_id, date, investor_type)— 5 類法人各一 row。layered §3.5。
- **`margin_purchase_short_sale_tw`** PK(market, stock_id, date)— 融資融券原始(14 欄展開)。layered §3.5。
- **`foreign_investor_share_tw`** PK(market, stock_id, date)— 外資持股(11 欄展開)。layered §3.5。
- **`day_trading_tw`** PK(market, stock_id, date)— 當沖原始。layered §3.5。
- **`valuation_per_tw`** PK(market, stock_id, date)— PER / PBR / yield 原始三合一。layered §3.5。
- **`holding_shares_per`** PK(market, stock_id, date, holding_shares_level)— 股權分散(17 levels 一 row 一級);PR #R3 升格主名(舊名 `_tw` 後綴)。layered §3.5。
- **`government_bank_buy_sell_tw`** PK(market, stock_id, date)— 八大行庫買賣超(PR #21-B 引入);需 FinMind sponsor tier,backer 跑出 0 events。layered §3.5。
- **`short_sale_securities_lending_tw`** PK(market, stock_id, date)— SBL 日次合計(short_sales / returns / current_day_balance);與 `securities_lending_tw` 是不同 dataset。layered §3.5。

### B5 — Fundamental

- **`financial_statement`** PK(market, stock_id, date, event_type, type)— 財報三表(income/balance/cashflow);PR #R3 升格 + Hotfix A1(2026-05-11)PK origin_name → type。layered §3.6。
- **`monthly_revenue`** PK(market, stock_id, date)— 月營收;PR #R3 升格主名。layered §3.6。

### B6 — Environment / Macro

- **`market_index_us`** PK(market, stock_id, date)— SPY / ^VIX 美股指數。layered §3.7。
- **`exchange_rate`** PK(market, date, currency)— USD/EUR/JPY/AUD 等匯率(19 幣);FinMind 必須帶 data_id 才回完整時序。layered §3.7。
- **`institutional_market_daily`** PK(market, date)— 大盤三大法人(同 institutional_investors_tw 結構,市場級)。layered §3.7。
- **`market_margin_maintenance`** PK(market, date)— 大盤融資維持率。layered §3.7。
- **`total_margin_purchase_short_sale_tw`** PK(market, date, name)— 大盤融資融券 pivot(name ∈ MarginPurchase / ShortSale / MarginPurchaseMoney);PR #21-B + hotfix `q6r7s8t9u0v1`(2026-05-08)。layered §3.7。
- **`fear_greed_index`** PK(market, date)— CNN 恐慌貪婪指數;Silver 端尚無 derived(架構例外,fear_greed_core 直讀 Bronze)。layered §3.7 + cores_overview §6.2。

---

## 4. Silver 速查(18 張)

> Silver 層全為 `*_derived` 或 `price_*_fwd`,均承載 `is_dirty / dirty_at` 欄位(PR #19a + PR #20 dirty queue)。

### S1 — Adjustment(Rust binary 計算)

- **`price_daily_fwd`** / **`price_weekly_fwd`** / **`price_monthly_fwd`** — 後復權 K 線;Rust `tw_stock_compute` 從 `price_daily` + `price_adjustment_events` 算出。詳見 layered §4.1。Rust 永遠**全量重算**(multiplier 倒推設計使然)。
- **`price_limit_merge_events`** PK(market, stock_id, date)— PR #20 Rust 計算合併事件。

### S4 — Derived Chip(SQL builder)

> Silver builder 走 `src/silver/builders/{name}.py` + `src/silver/_common.py`;PR #20 上線後 Bronze → Silver dirty trigger 接管 §5.6 短期補丁。

- **`institutional_daily_derived`** PK(market, stock_id, date)— 5 法人 pivot(10 buy/sell 欄)+ `gov_bank_net = gov_bank.buy - gov_bank.sell`(LEFT JOIN government_bank_buy_sell_tw,目前 backer tier 為 NULL)。layered §4.2。
- **`margin_daily_derived`** PK(market, stock_id, date)— 6 stored + detail JSONB 8 keys + `sbl_short_sales_*` 3 衍生欄(LEFT JOIN short_sale_securities_lending_tw,fill rate 99.21%)。layered §4.2。
- **`foreign_holding_derived`** PK(market, stock_id, date)— 2 stored + detail JSONB 9 keys。layered §4.2。
- **`day_trading_derived`** PK(market, stock_id, date)— 2 stored + `day_trading_ratio = day_trading.volume × 100 / price_daily_fwd.volume`(v1.27 對齊 chip_cores §7.2 / m3Spec)。layered §4.2。
- **`holding_shares_per_derived`** PK(market, stock_id, date)— pivot 17 levels → detail JSONB(對齊 v2.0 aggregate_holding_shares)。layered §4.2。

### S5 — Derived Fundamental

- **`valuation_daily_derived`** PK(market, stock_id, date)— 3 stored + `market_value_weight`(cross-stock SUM 算市值權重 [0, 1])。layered §4.3。
- **`financial_statement_derived`** PK(market, stock_id, date, type)— pivot 多 origin_name → detail JSONB(中文 key,v1.29 Round 1 修;balance type 是 % common-size 處理)。layered §4.3。
- **`monthly_revenue_derived`** PK(market, stock_id, date)— raw rename(`revenue_year` → `revenue_yoy` / `revenue_month` → `revenue_mom`)+ country / create_time 進 detail。layered §4.3。

### S6 — Derived Environment(market-level)

- **`taiex_index_derived`** PK(market, stock_id ∈ TAIEX/TPEx, date)— Bronze `market_ohlcv_tw` 1:1 + detail。layered §4.4。
- **`us_market_index_derived`** PK(market, stock_id ∈ SPY/^VIX, date)— Bronze `market_index_us` 1:1。layered §4.4。
- **`exchange_rate_derived`** PK(market, date, currency)— 不含 stock_id;Bronze `exchange_rate` 1:1。layered §4.4。
- **`market_margin_maintenance_derived`** PK(market, date)— 不含 stock_id;ratio 1:1 + `total_margin_purchase_balance` / `total_short_sale_balance`(LEFT JOIN total_margin_purchase_short_sale_tw)。layered §4.4。
- **`business_indicator_derived`** PK(market, stock_id='`_market_`' sentinel, date)— Bronze 2-col → Silver 3-col 升維;Cores 端 Fact 改寫 `_index_business_`(loader 轉換)。layered §4.4 + cores_overview §6.2.1。

---

## 5. M3 Cores 三表

> spec 在 `m3Spec/cores_overview.md` §3 / §7;by-core 反查在 `docs/cores_schema_map.md`。

### `indicator_values`

- **PK / Unique**:(`stock_id`, `value_date`, `timeframe`, `source_core`, `params_hash`)
- **用途**:時序型輸出(MACD line / RSI value / KD K&D / MA series / Bollinger bands / OBV cumulative ...)
- **每 row 包含**:`value_json` JSONB(完整 Output struct 序列化)+ `source_version` + `created_at`
- **寫入規約**:cores_overview §7.1(三類分流);params_hash 由 `fact_schema::params_hash()`(blake3 + canonical JSON,§7.4)算
- **特殊**:`ma_core` 用 `series_by_spec`(多均線);`taiex_core` 用 `series_by_index`(TAIEX/TPEx 雙序列);兩者在 `tw_cores/src/main.rs:extract_indicator_meta` 有 nested fallback

### `structural_snapshots`

- **PK / Unique**:(`stock_id`, `snapshot_date`, `timeframe`, `core_name`, `params_hash`)
- **用途**:結構快照(Wave `neely_core` Scenario Forest / 未來 SR / Trendline 等)
- **每 row 包含**:`snapshot_json` JSONB(完整 Forest / SR list)+ `derived_from_core`(若有衍生關係)
- **目前寫入者**:`neely_core` only(P0 階段)
- **與 indicator_values 區別**:時序 vs 快照;快照通常 per stock per timeframe 一 row,時序通常多 row

### `facts`

- **Unique constraint**:(`stock_id`, `fact_date`, `timeframe`, `source_core`, `COALESCE(params_hash, '')`, `md5(statement)`) — 確保同核心同事件不重複
- **用途**:append-only 事件 Fact(golden_cross / breakout / divergence / 籌碼異動 / 估值 zone transition / 等)
- **每 row 包含**:`statement` TEXT(人類可讀)+ `metadata` JSONB(可機器處理)+ `source_core` / `source_version`
- **寫入路徑**:每 core 的 `produce_facts(output) -> Vec<Fact>`;`tw_cores/src/main.rs:write_facts` 用 UNNEST array batch INSERT(PR-9c)
- **詳見**:cores_overview §六(事實邊界規範)+ §7.4(params_hash)

---

## 6. Reference / System 表

### `schema_metadata`

- PK:`key`
- 已知 key:`schema_version` = `'3.2'` / `migrated_from` / `migrated_at`
- Rust binary 啟動時 assert `schema_version == "3.2"`(`rust_bridge.py:EXPECTED_SCHEMA_VERSION`)
- schema 升版時 Rust + Python 兩端必須一起改

### `stock_sync_status`

- PK:(market, stock_id)
- 欄位:`last_full_sync` / `last_incr_sync` / `fwd_adj_valid`
- v1.26 起 Rust binary `resolve_stock_ids` fallback **改讀 `price_daily_fwd.is_dirty=TRUE`**(對齊 PR #20 dirty queue);`fwd_adj_valid` 保留欄位但 deprecated 不再寫入

### `api_sync_progress`

- PK:(api_name, stock_id, segment_start)
- 欄位:`status` ∈ `pending / completed / failed / empty / schema_mismatch`
- CHECK constraint `chk_progress_status` 由 alembic `a1b2c3d4e5f6` 落下(v1.7 review #1 補 `empty` + `schema_mismatch` 兩種,避免 baseline 卡死斷點續傳)

---

## 7. Legacy 退場表

> v2.0 籌碼舊表 PR #R2(2026-05-09)rename `_legacy_v2`,進入 observation period 21-60 天;PR #R6 後永久 DROP。觀察期最早 2026-05-30 結束。

| 表名 | 對應主名 | 觀察 SLO |
|---|---|---|
| `holding_shares_per_legacy_v2` | `holding_shares_per` | Silver builder 持續每日 12/12 OK;`api_sync_progress.status='failed'`=0;legacy row count 與主名表 ±1% |
| `financial_statement_legacy_v2` | `financial_statement` | 同上 |
| `monthly_revenue_legacy_v2` | `monthly_revenue` | 同上 |

PR #R6 落地前可 rollback:downgrade 反向 rename。後續不再支援 rollback。

---

## 8. PR #R 系列遷移歷史

> 為何 schema 長現在這樣?從這段時序看。

| PR | alembic revision | 日期 | 動作 | 影響面 |
|---|---|---|---|---|
| **#R1** | `r7s8t9u0v1w2` | 2026-05-09 | ADD COLUMN `source TEXT NOT NULL DEFAULT 'finmind'` 至 3 張 `_tw` Bronze(holding_shares_per_tw / financial_statement_tw / monthly_revenue_tw) | additive only,0 collector / builder 改動 |
| **#R2** | `s8t9u0v1w2x3` | 2026-05-09 | 3 張 v2.0 表 RENAME `_legacy_v2`(holding_shares_per / financial_statement / monthly_revenue → `*_legacy_v2`)+ idx rename | collector.toml v2.0 entries `target_table` 同步改 `*_legacy_v2`(dual-write 維持) |
| **#R3** | `t9u0v1w2x3y4` | 2026-05-09 | 3 張 `_tw` Bronze 去 `_tw` 升格為主名(holding_shares_per_tw → holding_shares_per 等)+ idx rename | collector.toml v3 entries `target_table` 同步改主名;PG trigger 自動跟著 RENAME(無需 DROP+重建) |
| **#R4** | `u0v1w2x3y4z5` | 2026-05-09 | collector.toml v2.0 entry name 加 `_legacy` 後綴(plan §6.3 簡化選項);api_sync_progress.api_name 同步 UPDATE | 既有 backfill 進度紀錄無痕跟到新 entry name |
| **#R6**(未落地) | TBD | 觀察期 21-60 天後 | DROP 3 張 `_legacy_v2` + 5 個 v2.0 `_legacy` entry 從 collector.toml 移除 | 不可 rollback,需 backup |

### 其他關鍵 PR(非 R 系列)

| PR | alembic | 日期 | 動作 |
|---|---|---|---|
| **#21-B** | `p5q6r7s8t9u0` | 2026-05-05 | 3 張新 Bronze:`government_bank_buy_sell_tw` / `total_margin_purchase_short_sale_tw` / `short_sale_securities_lending_tw` |
| **#21-B hotfix** | `q6r7s8t9u0v1` | 2026-05-08 | `total_margin_purchase_short_sale_tw` PK 加 `name`(pivot-by-row);6 欄重建 |
| **#22 fwd dedup** | `v1w2x3y4z5a6` | 2026-05-09 | `price_adjustment_events` 加 trigger `trg_pae_dedup_par_value_split` 防 par_value + split 同日 dup |
| **#M3-PR-7** | `w2x3y4z5a6b7` | 2026-05-09 | M3 三表落地(indicator_values / structural_snapshots / facts) |
| **Hotfix A1** | `x3y4z5a6b7c8` | 2026-05-11 | financial_statement PK origin_name → type(3 stocks 受影響:2330/2357/2836) |

**目前 head:`x3y4z5a6b7c8`**

---

## 9. 變更政策(改 schema 時必看)

任何 schema 改動(ADD / ALTER / DROP / RENAME)必須**同步更新**以下文件:

1. **`alembic/versions/<new_revision>.py`** — 寫 idempotent migration(`IF EXISTS` / `IF NOT EXISTS`),`down_revision` 指向當前 head
2. **`src/schema_pg.sql`** — fresh DB 一鍵建表;確保跑 schema_pg.sql 後與 alembic upgrade head 結果一致
3. **`m2Spec/layered_schema_post_refactor.md`** §3.x / §4.x — Bronze / Silver 規範
4. **`docs/schema_reference.md`**(本檔)— §2 全表清單 + §3-§7 速查 + §8 PR 歷史時序版加新列
5. **`docs/cores_schema_map.md`**(若改的表是 Silver 輸入給 Cores)— 反向索引同步
6. **`schema_metadata.schema_version`**(若是 major version bump)— bump `'3.2'` → `'3.3'`;Rust `rust_bridge.py:EXPECTED_SCHEMA_VERSION` 同步

詳細 checklist 見 `docs/schema_master.md` §6。

---

## 10. 驗證

```sql
-- alembic head 對齊
SELECT version_num FROM alembic_version;
-- 應 = 'x3y4z5a6b7c8'

-- schema_version
SELECT value FROM schema_metadata WHERE key = 'schema_version';
-- 應 = '3.2'

-- 全表 count
SELECT count(*) FROM information_schema.tables
WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
-- 應 ~56

-- api_sync_progress.status CHECK
SELECT pg_get_constraintdef(oid) FROM pg_constraint
 WHERE conrelid = 'api_sync_progress'::regclass AND conname = 'chk_progress_status';
-- 應含 5 種 status: pending / completed / failed / empty / schema_mismatch
```

如想看欄位明細 / SQL DDL:`src/schema_pg.sql`;如想看 schema 演進細節:`alembic/versions/*.py`。
