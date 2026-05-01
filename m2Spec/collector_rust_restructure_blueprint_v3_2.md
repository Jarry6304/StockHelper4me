# Collector + Rust 架構重構藍圖（對齊 v3.2 定稿）

> **版本**：blueprint r1
> **基準**：`m2Spec/collector_schema_consolidated_spec_v3_2.md`（動工版）
> **目標**：把現行 v2.0 collector（Python）+ rust_compute（Rust）重新切分到 v3.2 的 **Bronze / Reference / Silver / M3** 四層，並列出**動工順序、模組變更、Schema 異動、不變項**。
> **狀態**：盤點 + 規劃，尚未動工。動工前須通過 v3.2 §8.1 K-1 + §8.5 G-ATR-1 / A-V3 / W-1 / F-V1 阻塞驗證。

---

## 〇、TL;DR

| 動作 | 數量 | 內容 |
|---|---|---|
| 🟢 **保留不動**（API 抓取邏輯 + Rust 後復權核心） | ~70% | api_client / rate_limiter / sync_tracker / date_segmenter / Rust `process_stock` 後復權算法 |
| 🟡 **改名 + 精簡欄位**（既有表） | 7 張 | `trading_calendar→trading_date_ref`、`stock_info→stock_info_ref`、`price_adjustment_events`（砍 3 欄）、6 張 Bronze 改寫進 `*_derived` Silver |
| 🔴 **拆 Bronze / Silver**（目前混在一起） | 13 張 Silver | `institutional_daily`（已 pivot）→ raw `institutional_investors_tw` + `institutional_daily_derived`；其餘 12 張同樣切 |
| 🆕 **Bronze 新增** | 4 張 | `market_ohlcv_tw`、`stock_suspension_events`、`securities_lending_tw`、`business_indicator_tw` |
| 🆕 **Silver 新增**（M3 預計算載體） | 14 張 + 3 view | 全套 `*_derived` + dirty 欄位 + 3 個 SQL view |
| 🆕 **M3 三表** | 3 張 | `structural_snapshots` / `indicator_values` / `core_dependency_graph` |
| 🔧 **Collector code 結構** | 4 處 | phase_executor 拆 Bronze/Silver 兩段、aggregators 從「Bronze 寫前 pivot」改成「Silver 計算」、新增 `dirty_marker.py`、Rust 拆出 `silver_compute` 模組 |

---

## 一、現況 v2.0 vs v3.2 完整對照

### 1.1 表級別 mapping

| v2.0 既有 | v3.2 對應 | 處置 |
|---|---|---|
| `stock_info` | `stock_info_ref` | **改名**；砍 `listing_date / delisting_reason / is_active / last_updated`；保留 `industry_category / type / delisting_date` |
| `trading_calendar` | `trading_date_ref` | **改名**；砍 `is_trading_day`（row 存在 = 交易日） |
| `market_index_tw` | 拆兩用：raw 仍寫 `market_index_tw`（指數價）+ 新 `market_ohlcv_tw` (TAIEX OHLCV 由 TotalReturnIndex+VariousIndicators5Seconds 補) | **保留 + 新增** |
| `price_adjustment_events`（v2.0） | `price_adjustment_events`（v3.2 精簡版） | **砍 3 欄**：`adjustment_factor`（移 Silver 算）、`source_dataset`（用 event_type 推）、`after_price`（Beta 不抓） |
| `_dividend_policy_staging` | （仍保留為 staging）| 不變；`dividend_policy_merge` post-process 邏輯保留 |
| `price_daily` | （Bronze）`price_daily` | **不動**；仍由 Phase 3 抓 |
| `price_limit` | （Bronze）`price_limit` | **不動**；併入 `tw_market_core` 還原來源之一 |
| `price_daily_fwd / price_weekly_fwd / price_monthly_fwd` | （Silver）同名 | **不動**；Rust Phase 4 計算路徑保留 |
| `institutional_daily`（已 pivot 5 類法人 10 欄） | 拆：Bronze raw `institutional_investors_tw`（每筆 1 法人 1 row）+ Silver `institutional_daily_derived`（pivot 後 + `gov_bank_net`） | **拆兩張**；目前 v2.0 直接寫 pivot 後 → 違反 Medallion |
| `margin_daily` | 同上拆：Bronze raw `margin_purchase_short_sale_tw` + Silver `margin_daily_derived`（+SBL 6 欄） | **拆兩張**；新增 `securities_lending_tw` Bronze + 進 Silver |
| `foreign_holding` | 拆：Bronze `foreign_investor_share_tw` + Silver `foreign_holding_derived` | **拆兩張** |
| `holding_shares_per`（已 pack JSONB） | 拆：Bronze `holding_shares_per_tw` + Silver `holding_shares_per_derived` | **拆兩張**（Bronze 維持 raw row，Silver 才 pack） |
| `valuation_daily` | 拆 Bronze + Silver `valuation_daily_derived`（+`market_value_weight`；`market_value` 走 view） | **拆兩張** + 新增 view |
| `day_trading` | 拆 Bronze + Silver `day_trading_derived` | **拆兩張** |
| `monthly_revenue` | 拆 Bronze + Silver `monthly_revenue_derived` | **拆兩張** |
| `financial_statement`（已 pack JSONB） | 拆 Bronze + Silver `financial_statement_derived` | **拆兩張** |
| `market_index_us` | 拆 Bronze + Silver `us_market_index_derived` | **拆兩張** |
| `exchange_rate` | 拆 Bronze + Silver `exchange_rate_derived` | **拆兩張** |
| `institutional_market_daily`（已 pivot 全市場） | 改 view `total_institutional_view` 取代 | **降 view**；底層 Bronze 留 |
| `market_margin_maintenance` | 拆 Bronze + Silver `market_margin_maintenance_derived`（+市場融資融券 2 欄） | **拆兩張** |
| `fear_greed_index` | （依 B 主檔；外部源） | 不動 |
| `api_sync_progress` | （不變） | 5 種 status 已對齊 |
| `stock_sync_status` | （不變） | Rust Phase 4 寫 `fwd_adj_valid` 路徑保留；Silver dirty-detection 改用 `is_dirty/dirty_at` |
| `schema_metadata` | （不變） | `schema_version` bump 到 `3.2` |
| ❌ `industry_chain_ref`（v3.1 提案） | 不存在 | v3.2 Q1 移出 Collector |
| ❌ `business_indicator_core`（v3.1 提案） | 降 reference | v3.2 Q2，仍要 Bronze + Silver 但不算 Core |
| 🆕 `stock_suspension_events` | Bronze 新增 | TaiwanStockSuspended |

### 1.2 Phase ↔ v3.2 動工項對照

| v2.0 Phase | v3.2 對應 | 變更 |
|---|---|---|
| Phase 0 trading_calendar | **R-1** trading_date_ref | 改名 + 砍欄；改寫 `phase_executor._run_phase0` |
| Phase 1 stock_info / market_index_tw | **R-2** stock_info_ref + **B-1/B-2** market_ohlcv_tw + market_index_tw 並存 | 改名 + 砍欄；新增 `market_ohlcv_tw` 抓取；TAIEX 報酬 vs 不還原指數並存 |
| Phase 2 dividend / split / par_value / capital_reduction | **B-3** price_adjustment_events（精簡版）+ **B-4** stock_suspension_events | `price_adjustment_events` schema 砍 3 欄；新增 `stock_suspension_events` 抓取 |
| Phase 3 price_daily / price_limit | （不變） | 直接寫 Bronze |
| Phase 4 Rust 後復權 + 週月K | （不變 + Silver 化）| Rust 內 `patch_capital_increase_af` 改寫進 Silver 自身 AF 欄位，**不再 UPDATE Bronze price_adjustment_events**；增加 `price_limit_merge_events` 計算 |
| Phase 5 法人 / 融資 / 借券 / 持股 / 估值 / 當沖 | **Bronze 抓 + Silver 計算 雙段** | aggregators 從「寫前 pivot」改「寫後 Silver 化」；新增 SBL 抓取（B-5）|
| Phase 6 macro（exchange_rate / market_index_us 等）+ **B-6 business_indicator_tw** | 同上拆 Bronze/Silver | exchange_rate/us 都拆兩張；新增 business_indicator_tw |
| 🆕 Phase 7 — Silver 計算（dirty 驅動）| **v3.2 §8.7 Phase 7a/7b/7c/7d** | 全新階段；目前 v2.0 沒有對應 |

---

## 二、四層架構定義（落地版）

```
                ┌────────────────────────────────────────┐
                │             FinMind / 外部資料源         │
                └───────────────────┬────────────────────┘
                                    │ (api_client + rate_limiter)
                                    ▼
┌───────────────────────────────────────────────────────────────────┐
│                       BRONZE LAYER（raw）                          │
│                                                                   │
│  trading_date_ref / stock_info_ref       ← Reference Data         │
│  price_daily / price_limit / market_index_tw / market_ohlcv_tw    │
│  price_adjustment_events（精簡版,4 欄事件）                          │
│  stock_suspension_events                                          │
│  institutional_investors_tw / margin_purchase_short_sale_tw       │
│  securities_lending_tw / foreign_investor_share_tw                │
│  holding_shares_per_tw / valuation_per_tw / day_trading_tw        │
│  monthly_revenue_tw / financial_statement_tw / cash_flow_tw       │
│  market_index_us / exchange_rate_tw                               │
│  market_margin_maintenance_tw / business_indicator_tw             │
│                                                                   │
│  寫入路徑：phase_executor → field_mapper → db.upsert（無 pivot/pack） │
│  Dirty 觸發：Bronze 寫入時插一筆 dirty_event 進 silver dirty queue   │
└───────────────────────────────────────────┬───────────────────────┘
                                            │ (dirty-detection)
                                            ▼
┌───────────────────────────────────────────────────────────────────┐
│                       SILVER LAYER（derived）                      │
│                                                                   │
│  price_daily_fwd / price_weekly_fwd / price_monthly_fwd  ← Rust    │
│  price_limit_merge_events                                ← Rust    │
│  institutional_daily_derived（pivot + gov_bank_net）      ← Python  │
│  margin_daily_derived（+SBL 6 欄）                       ← Python  │
│  foreign_holding_derived / holding_shares_per_derived   ← Python  │
│  valuation_daily_derived（+market_value_weight）         ← Python  │
│  day_trading_derived / monthly_revenue_derived          ← Python  │
│  financial_statement_derived（pack JSONB）              ← Python  │
│  taiex_index_derived / us_market_index_derived          ← Python  │
│  exchange_rate_derived                                  ← Python  │
│  market_margin_maintenance_derived（+市場融資融券）       ← Python  │
│  business_indicator_derived                             ← Python  │
│                                                                   │
│  Views: total_institutional_view / valuation_market_value_view /  │
│         total_margin_purchase_view                                │
│                                                                   │
│  共通欄位：is_dirty BOOL + dirty_at TIMESTAMPTZ + idx WHERE dirty   │
│  寫入路徑：dirty 事件觸發 → 對應 silver_builder → upsert            │
└───────────────────────────────────────────┬───────────────────────┘
                                            │
                                            ▼
┌───────────────────────────────────────────────────────────────────┐
│                       M3 LAYER（model）                            │
│                                                                   │
│  structural_snapshots（Wave / Pattern / S/R / Trendline）          │
│  indicator_values（Momentum / Volatility / Volume,JSONB params） │
│  core_dependency_graph                                            │
│                                                                   │
│  寫入路徑：本 collector 不負責；m3_compute（未動工）讀 Silver 算進 M3 │
└───────────────────────────────────────────────────────────────────┘
```

### 2.1 Layer 之間的資料契約

| 邊界 | 觸發 | 工具 |
|---|---|---|
| FinMind → Bronze | `phase_executor._run_api` segment | api_client + field_mapper（純 rename，**不做 pivot/pack**） |
| Bronze → Silver | `is_dirty=TRUE` 或 Bronze 表收到新 segment | `silver_builder/`（Python，per Silver 表一支模組） + Rust（後復權 + 週月K + price_limit_merge） |
| Silver → M3 | Silver `is_dirty=TRUE` 觸發 | `m3_compute/`（不在本 repo，未動工） |

---

## 三、Collector code 結構重構

### 3.1 現行 `src/` → 重構後 `src/`

```
src/
├── main.py                      # CLI entry（不變,但 phase 表擴到 7 階段）
├── config_loader.py             # TOML 解析（不變,toml 格式擴 Bronze/Silver 區段）
├── api_client.py                # FinMind HTTP（不變）
├── rate_limiter.py              # rate limit（不變）
├── sync_tracker.py              # api_sync_progress（不變,5 種 status 已 align）
├── date_segmenter.py            # 分段邏輯（不變）
├── stock_resolver.py            # 個股清單解析（小改:從 stock_info_ref 讀）
├── field_mapper.py              # 欄位 rename + computed_fields（不變,但砍掉 _validate_schema 內對 detail packing 的支援,改在 silver_builder 做）
├── db.py                        # PG writer + _table_pks/_table_columns（不變）
│
├── bronze/                      # 🆕 新增子套件
│   ├── __init__.py
│   ├── phase_executor.py        # 從原 phase_executor.py 拆出 Phase 0~6 Bronze 抓取段
│   └── dirty_marker.py          # 🆕 Bronze 寫入後在 silver dirty queue 標記
│
├── silver/                      # 🆕 新增子套件
│   ├── __init__.py
│   ├── orchestrator.py          # 🆕 Phase 7 排程入口,讀 dirty queue 跑對應 builder
│   ├── builders/
│   │   ├── institutional.py     # pivot 5 類法人 + gov_bank_net
│   │   ├── margin.py            # margin + SBL 合併
│   │   ├── foreign_holding.py
│   │   ├── holding_shares_per.py # pack JSONB
│   │   ├── valuation.py         # +market_value_weight
│   │   ├── day_trading.py
│   │   ├── monthly_revenue.py
│   │   ├── financial_statement.py # pack JSONB(目前 aggregators.pack_financial)
│   │   ├── taiex_index.py
│   │   ├── us_market_index.py
│   │   ├── exchange_rate.py
│   │   ├── market_margin.py
│   │   └── business_indicator.py
│   └── views.sql                # 3 個 view DDL
│
├── post_process.py              # 改名 silver/builders/dividend_policy_merge.py
│                                # 「寫進 price_adjustment_events 」邏輯保留;
│                                # capital_increase 偵測改在 silver/builders 觸發
│
└── rust_bridge.py               # 不變,只是 Phase 4 的 stock_ids 來源改從 silver dirty queue 讀
```

### 3.2 `aggregators.py` 砍除

| 現行函數 | v3.2 處置 | 搬遷目的地 |
|---|---|---|
| `pivot_institutional` | 從 Bronze 寫入路徑移除 | `silver/builders/institutional.py` |
| `pivot_institutional_market` | 同上 | `silver/builders/institutional.py`（_market_ 路徑） |
| `pack_financial` | 從 Bronze 寫入路徑移除 | `silver/builders/financial_statement.py` |
| `pack_holding_shares` | 同上 | `silver/builders/holding_shares_per.py` |
| `_filter_to_trading_days` | 保留（共用） | `silver/_common.py`（讀 trading_date_ref） |

整個 `aggregators.py` 在 v3.2 重構完後**會被砍掉**，邏輯散到 6 個 silver builder。

### 3.3 `phase_executor.py` 拆段

現行的 `PhaseExecutor.run(mode)` 跑 Phase 1~6 全部串聯（API 抓 → field_mapper → 可能 aggregators → db.upsert）。重構後：

```python
# bronze/phase_executor.py  ← 純 raw 抓取,不做 pivot
class BronzePhaseExecutor:
    async def run(self, phases: list[int], mode: str):
        # Phase 0: trading_date_ref
        # Phase 1: stock_info_ref + market_ohlcv_tw + market_index_tw
        # Phase 2: price_adjustment_events + stock_suspension_events + _dividend_policy_staging
        # Phase 3: price_daily + price_limit
        # Phase 5a: institutional_investors_tw (raw, 1 法人 1 row)
        # Phase 5b: margin_purchase_short_sale_tw + securities_lending_tw
        # Phase 5c: foreign_investor_share_tw + holding_shares_per_tw + day_trading_tw
        # Phase 5d: valuation_per_tw
        # Phase 6:  market_index_us + exchange_rate_tw + market_margin_maintenance_tw
        #           + business_indicator_tw + monthly_revenue_tw + financial_statement_tw
        # 寫入後 → dirty_marker.mark(table, market, stock_id, date_range)

# silver/orchestrator.py  ← 跑所有 Silver builder
class SilverOrchestrator:
    async def run(self, phases: list[str], mode: str):
        # Phase 7a 平行(11 張):institutional / margin / foreign_holding / 
        #         holding_shares_per / valuation / day_trading / monthly_revenue /
        #         taiex / us_market / exchange_rate / market_margin
        # Phase 7b 跨表依賴(2 張):financial_statement(需 monthly_revenue 對齊) /
        #         day_trading(需 price_daily volume 計算 day_trading_ratio)
        # Phase 7c tw_market_core 系列:price_daily_fwd / price_weekly_fwd / 
        #         price_monthly_fwd / price_limit_merge_events  ← 全 Rust
        # Phase 7d M3 寫入:本 collector 不負責,只標記 silver dirty 給 m3_compute
        #
        # 每張 silver 表的 builder 都是:
        #   1. 讀 dirty queue 找出待算的 (market, stock_id, date_range)
        #   2. 從對應 Bronze 表 SELECT
        #   3. 計算 + upsert 到 *_derived
        #   4. 清 is_dirty + 設 dirty_at = NOW()
```

### 3.4 `main.py` CLI 調整

```bash
# v2.0
python src/main.py backfill --phases 1,2,3,4,5,6
python src/main.py incremental

# v3.2(目標)
python src/main.py bronze --phases 0,1,2,3,5,6     # 只抓 raw
python src/main.py silver --phases 7a,7b,7c        # 算 derived
python src/main.py full                             # bronze + silver 一條龍
python src/main.py incremental                      # bronze 增量 + silver dirty 重算
```

---

## 四、Rust 計算層重構

### 4.1 現況

`rust_compute/src/main.rs`（505 行,單檔）做四件事：
1. `process_stock`：讀 `price_daily` + `price_adjustment_events.adjustment_factor` → 倒推 multiplier → 寫 `price_daily_fwd`
2. 週/月 K aggregation → 寫 `price_weekly_fwd / price_monthly_fwd`
3. `patch_capital_increase_af`：**UPDATE `price_adjustment_events.adjustment_factor`**（違反 v3.2 Medallion）
4. 收尾 `UPDATE stock_sync_status SET fwd_adj_valid=1`

### 4.2 v3.2 重構

| 現行 | v3.2 改法 |
|---|---|
| `process_stock` | 不變（核心算法是對的） |
| 週月 K aggregation | 不變 |
| `patch_capital_increase_af` UPDATE Bronze | 🔴 **改寫**：AF 計算改在 Silver 內部欄位（`price_adjustment_events_silver.adjustment_factor` 或 `price_daily_fwd.cumulative_adjustment_factor`,依 A-V3 結果），**不寫回 Bronze** |
| 直接讀 `price_adjustment_events` Bronze 取 AF | 改成：讀 Bronze 5 欄事件後，**Rust 內部現算 AF**（依 v3.2 §三 5 大來源 mapping）；Silver Bronze 邊界乾淨 |
| 沒做 `price_limit_merge_events` | 🆕 新增：Phase 7c 同步算「漲跌停合併事件」（Silver 表） |
| 直接從 `stock_sync_status.fwd_adj_valid=0` 取 stock 清單 | 改從 Silver dirty queue（`price_daily_fwd.is_dirty=TRUE`）取 |

### 4.3 Rust 模組拆分（建議）

```
rust_compute/
├── Cargo.toml
└── src/
    ├── main.rs                  # CLI + tokio 入口(~80 行)
    ├── schema_check.rs          # schema_metadata 驗證(從 main.rs 拆)
    ├── adjustment_factor.rs     # AF 計算(從 5 大 raw event 推導,新)
    ├── post_adjustment.rs       # 後復權主迴圈(現 process_stock)
    ├── ohlc_aggregation.rs      # 週/月 K
    ├── price_limit_merge.rs     # 🆕 漲跌停合併事件
    └── db_io.rs                 # SQL helpers
```

### 4.4 條件 ALTER（v3.2 §附錄 B 末段）

依 **A-V3 阻塞驗證**結果，可能對 `price_daily_fwd` 加：
- `volume_adjusted NUMERIC`
- `cumulative_adjustment_factor NUMERIC`
- `is_adjusted BOOLEAN`
- `adjustment_factor NUMERIC`（單日 AF，方便除錯）

A-V3 沒做完之前，**Rust 端 OHLC 計算先凍結**。

---

## 五、Schema 異動清單

### 5.1 改名（不破壞資料）

```sql
ALTER TABLE trading_calendar RENAME TO trading_date_ref;
ALTER TABLE trading_date_ref DROP COLUMN is_trading_day;

ALTER TABLE stock_info RENAME TO stock_info_ref;
ALTER TABLE stock_info_ref 
    DROP COLUMN listing_date,
    DROP COLUMN delisting_reason,
    DROP COLUMN is_active,
    DROP COLUMN last_updated;
```

### 5.2 既有 `price_adjustment_events` 砍欄

```sql
-- 砍 3 欄；adjustment_factor 移到 Silver 計算
ALTER TABLE price_adjustment_events 
    DROP COLUMN adjustment_factor,
    DROP COLUMN source_dataset,
    DROP COLUMN after_price;

-- 留：market, stock_id, event_date, event_type, before_price, 
--     reference_price, cash_dividend, stock_dividend, detail JSONB
```

### 5.3 既有 6 張 raw 改名拆 Silver（Phase 7 動工前）

```sql
-- 範例:institutional_daily(已 pivot)拆兩張
ALTER TABLE institutional_daily RENAME TO institutional_daily_derived_legacy;
-- 之後 Phase 5a 寫進新 Bronze institutional_investors_tw
-- silver/builders/institutional.py 從 Bronze pivot 寫 institutional_daily_derived
-- _legacy 表 EOL 後 DROP

-- 同樣處理:margin_daily / foreign_holding / valuation_daily / day_trading /
--          financial_statement / monthly_revenue / market_index_us / exchange_rate /
--          market_margin_maintenance
```

### 5.4 Bronze 4 張新增（v3.2 §附錄 B）

```sql
-- 已寫進 v3.2 spec §附錄 B,直接複製
CREATE TABLE market_ohlcv_tw (...)         -- TAIEX OHLCV
CREATE TABLE stock_suspension_events (...) -- 個股暫停
CREATE TABLE securities_lending_tw (...)   -- 借券明細
CREATE TABLE business_indicator_tw (...)   -- 景氣指標
```

### 5.5 Silver 14 張新增 + dirty 欄位

```sql
-- 13 張 *_derived(從 Bronze 計算後寫入)
-- + business_indicator_derived(v3.2 新)
-- 統一 dirty 欄位:
ALTER TABLE <silver_table>
    ADD COLUMN is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN dirty_at TIMESTAMPTZ;
CREATE INDEX idx_<table>_dirty ON <silver_table>(is_dirty) WHERE is_dirty = TRUE;
```

### 5.6 Silver Views 3 張

照 v3.2 §2.5 + §附錄 B 直接 CREATE VIEW。

### 5.7 schema_metadata bump

```sql
UPDATE schema_metadata SET value = '3.2' WHERE key = 'schema_version';
```

Rust `schema_check.rs` 內 hard-fail 字串對齊到 `'3.2'`。

---

## 六、Phase 0 動工順序（嚴格依序，不可亂跳）

對齊 v3.2 §10.3 的 7 步 + 本 blueprint 的 collector 落地順序：

| # | 任務 | 對應 v3.2 | 阻塞性 |
|---|---|---|---|
| 1 | **K-1**：chip_cores.md 移除 `MarginPoint.margin_maintenance` | §8.1 | 🔴 動工前必修 |
| 2 | **F-V1**：確認既有 facts 表 schema | §8.5 | 🟠 高 |
| 3 | **R-1**：建 `trading_date_ref`（rename + drop column） | §8.2 | 🔴 阻塞 prev_trading_day |
| 4 | **R-2**：建 `stock_info_ref`（rename + drop columns） | §8.2 | 🔴 阻塞 ETL 起始點 |
| 5 | **A-V3**：驗證 `price_daily_fwd.volume` 是否已隨除權息調整 | §8.5 | 🔴 阻塞 Rust ALTER |
| 6 | **G-ATR-1**：atr_core 與 neely_core ATR golden test | §8.5 | 🔴 阻塞 M3 動工（不影響 Collector） |
| 7 | **W-1**：WaveCore trait 草案固化 | §8.5 | 🔴 阻塞 M3（不影響 Collector） |
| 8 | **B-1 / B-2**：補 TAIEX OHLCV + 報酬指數並存 | §8.4 | 🟠 |
| 9 | **B-3**：`price_adjustment_events` 砍 3 欄（**先 dry-run 確認 Rust patch_capital_increase_af 移到 Silver 後生產線不斷**）| §8.4 | 🔴 |
| 10 | **B-4**：`stock_suspension_events` 抓取 | §8.4 | 🟠 |
| 11 | Bronze 6 張 raw 拆出（v2.0 已 pivot 表 → 重抓 raw） | §8.6 | 🟡 中（可分批） |
| 12 | Silver 14 張 + dirty 欄位 + 3 view | §8.7 | 🟡 中 |
| 13 | **B-5**：securities_lending_tw（借券明細） | §8.6 | 🟠 |
| 14 | **B-6**：business_indicator_tw（景氣指標） | §8.6 | 🟠 |
| 15 | Phase 7a/7b/7c 排程器 | §8.7 | 🟢 收尾 |

**第 1~10 為 Collector 動工硬阻塞**；11~15 可平行展開。

---

## 七、不變項（v3.2 重構**不**動的東西）

| 項目 | 原因 |
|---|---|
| `api_client.py` + `rate_limiter.py` | FinMind 抓取邏輯與 v3.2 無關 |
| `sync_tracker.py`（5 種 status）| v1.7 已對齊 |
| `date_segmenter.py` | 分段邏輯無關層級 |
| `field_mapper.py` 的 rename / computed_fields | Bronze 寫入仍需要 |
| `db.py` PG writer + `_table_pks` 動態查 | v1.7 已對齊 |
| `_dividend_policy_staging` + `dividend_policy_merge` post-process | v1.6 已對齊；v3.2 沒砍此邏輯，只是改放到 silver/builders 名稱下 |
| `schema_metadata` hard-fail 機制 | 只 bump version 字串 |
| Rust 後復權「先 push 再更新 multiplier」核心算法 | v1.5 已修正，與 v3.2 無關 |
| Rust 週月 K aggregation | 與 v3.2 無關 |
| `stock_sync_status.fwd_adj_valid` | Phase 4 仍用，但 Silver 全面 dirty 化後可考慮砍此欄（v3.3+） |
| Windows binary path 自動補 `.exe` | v1.5 修正，無關 |

---

## 八、現有 v2.0 資料的 migration 策略

**核心問題**：v2.0 現行 6 張 raw 表（institutional_daily / margin_daily / ...）目前**已是 pivot 後**狀態，沒有 raw 1-row-1-投資人 的歷史資料。要怎麼餵 v3.2 Bronze？

### 8.1 兩條路線

**Option A：保留 legacy + 新跑 raw**
- v2.0 既有 6 張 rename 加 `_legacy_v2` 後綴；不再寫入
- v3.2 Bronze 6 張新表全量重抓 FinMind raw（範圍：2005-01-01 ~ today）
- Silver `*_derived` 從 Bronze 重新計算
- **成本**：FinMind API 重抓 1700+ 檔 × ~6 個 Phase 5 API × 21 年 segment ≈ 約 **30~40 小時**（rate_limit 1600/h）
- **優點**：乾淨，符合 Medallion；舊資料留 legacy 可對照

**Option B：legacy → Bronze 假回填**
- 從 v2.0 legacy 表反推 raw 結構（pivot 後 → 解 pivot），灌進 v3.2 Bronze
- 例：`institutional_daily.foreign_buy / sell` 解成 2 row（buy, sell, name=Foreign_Investor）
- Silver 從 Bronze 重算，跟 legacy 對得上即驗證通過
- **成本**：寫 6 個 reverse-pivot 腳本，~半天到一天
- **優點**：省 FinMind 重抓
- **缺點**：解 pivot 不一定能還原 100%（`pack_financial` 的 detail JSONB 已 unpack 困難）

**建議**：**institutional / margin / foreign_holding / day_trading / valuation 走 Option B**（解 pivot 簡單），**holding_shares_per / financial_statement / monthly_revenue 走 Option A**（pack JSONB 反推風險高）。

### 8.2 切換期間生產不停機

```
T0  : 部署 v3.2 schema(rename + 新表 + Silver) 但不啟用 silver builder
T0+1: Bronze 開始雙寫(v2.0 路徑 + v3.2 raw 路徑)
T0+7: silver builder 上線,開始 dirty queue 計算
T0+14: Aggregation Layer / SKILL.md 切到 Silver
T0+21: v2.0 legacy 路徑停寫,_legacy 表 RENAME + 進入觀察期
T0+60: 觀察期通過 → DROP _legacy 表
```

---

## 九、未動工項（本 blueprint 範圍外）

| 項目 | 範圍 |
|---|---|
| `industry_chain_ref` 移到 Aggregation Layer | 不在 collector |
| M3 三表的計算（neely / 各 indicator）| 不在 collector，由 m3_compute 做 |
| `business_indicator_derived` 的 streak / 3m_avg | Aggregation Layer 即時算（v3.2 §6.3） |
| `borrowing_fee_rate` | v3.2 Beta 全砍，P3 後評估 |

---

## 十、建議 PR 切法

> **不要做一個大 PR**。建議切 7~9 個小 PR，每個 PR 單獨可驗證可 revert。

| PR # | 範圍 | 估時 |
|---|---|---|
| 1 | **K-1** + schema_metadata bump 到 3.2 + alembic migration 入口 | 0.5 天 |
| 2 | **R-1 + R-2**：trading_date_ref + stock_info_ref（rename + drop col + alembic） | 0.5 天 |
| 3 | **B-3**：price_adjustment_events 砍 3 欄 + Rust patch_capital_increase_af 改寫到 Silver 內部 + A-V3 驗證 | 2 天 |
| 4 | **B-1/B-2 + B-4**：market_ohlcv_tw + stock_suspension_events Bronze 抓取 | 1 天 |
| 5 | Bronze 6 張 raw 拆（5a/5b/5c）：institutional / margin / foreign_holding / day_trading / valuation | 2 天（含 reverse-pivot 腳本） |
| 6 | **B-5**：securities_lending_tw 抓取 + Silver margin_daily_derived 整合 SBL 6 欄 | 1 天 |
| 7 | **B-6**：business_indicator_tw + business_indicator_derived | 1 天 |
| 8 | Silver 14 張 + dirty 欄位 + dirty_marker.py + silver/orchestrator.py + Phase 7a/7b/7c 排程 | 3 天 |
| 9 | 3 個 Silver Views + total_institutional / valuation_market_value / total_margin_purchase | 0.5 天 |

合計 **~12 天淨開發**（不含 user 在本機 PG 17 的驗證 + FinMind 重抓時間）。

---

## 十一、風險與後路

| 風險 | 緩解 |
|---|---|
| Rust AF 計算改寫進 Silver 後算錯，price_daily_fwd 全表壞 | A-V3 阻塞驗證；新舊兩條路線並行寫 1 週後比對 |
| Bronze 6 張 raw 拆出後 FinMind rate_limit 重抓 30+ 小時 | Option B reverse-pivot 對 5 張表先省工 |
| dirty queue 設計不當導致 Silver 重算不收斂 | Phase 7 排程器加上 max_iterations + circuit breaker |
| schema_metadata 從 2.0 跳 3.2 後舊版 Rust binary 跑不起來 | Rust schema_check 字串硬阻塞，binary build 必對 |
| Aggregation Layer 已查 v2.0 表，rename 後 query 全炸 | T0+14 才切，T0~T0+14 雙寫期間 Aggregation Layer 還用 legacy |

---

## 十二、本 blueprint 與 v3.2 對齊檢查表

| v3.2 §節 | 本 blueprint § | 對齊狀態 |
|---|---|---|
| §一 35 Core 完整一覽 | §一(表級別) | ✅ 表級別 mapping 完整 |
| §二 Schema 物件總清單 | §五 Schema 異動 | ✅ Bronze 5 / Reference 2 / Silver 14 / View 3 / M3 3 全列 |
| §三 tw_market_core 還原因子 | §四 Rust 重構 | ✅ AF 改 Silver 計算對齊 |
| §四 stock_suspension_events | §一 + §五 | ✅ 列入 B-4 |
| §五 Reference Data 精簡 | §五.1 改名 | ✅ trading_date_ref / stock_info_ref |
| §六 business_indicator 降級 | §一 + §三 | ✅ Bronze + Silver 各 1 張，不做 Core |
| §七 Pipeline 規範 | §二 + §三 | ✅ TAIEX 平行路徑 + dirty 四視角 + 寫入三分流 |
| §八 Phase 0/1 動工拓撲 | §六 動工順序 | ✅ K-1 / R-1 / R-2 / B-1~B-6 完整列 |
| §九 原則對齊驗證 | §二.1 邊界 | ✅ Medallion 剛性 + Collector 唯一客戶是 Core |
| §附錄 B 完整 DDL | §五 Schema 異動 | ✅ 直接引用 v3.2 DDL |

---

**(blueprint r1 完)**

> 本 blueprint 不取代 v3.2 規格本身，僅作為**現行 v2.0 collector + rust 走到 v3.2 的施工圖**。
> 
> 動工前下一步：user review → K-1 修 chip_cores.md → 開 PR #1（schema_metadata + alembic 入口）。
