# Collector Schema 修訂規格 v3.2(定稿版)

> **版本**:v3.2(經反方審查 + 正方答辯後的最終定稿)
> **修訂日期**:2026-04-30
> **基準**:`collector_schema_consolidated_spec_v3_1.md` + `v3_1_review_opposition.md`(反方審查)
> **狀態**:**動工版本**(Phase 0 / Phase 1 / Phase 7 動工依此文件)
> **取代**:v3 / v3.1 / v3.1 反方審查報告 → 全部由本 v3.2 統合

---

## 〇、v3.1 → v3.2 修訂裁決紀錄

反方對 v3.1 提出 6 個答辯問題與多項砍除建議,經正方答辯後逐項裁決:

| Q | 議題 | 裁決 | v3.2 處置 |
|---|---|---|---|
| Q1 | `industry_chain_ref` 是否屬 Collector | **反方勝** | **移出 Collector**(改放 Aggregation Layer derived) |
| Q2 | `business_indicator_core` 是否為完整 Core | **反方勝(部分)** | 降為 **Environment 子類的 derived 表**,**不享 Core 完整 spec** |
| Q3 | `tw_market_event_log` 三事件是否必要 | **部分採納反方** | 拆分:**只保留 Suspended**(個股暫停),砍 DayTradingSuspension + Disposition |
| Q4 | LAG 可算欄位是否預計算 | **反方勝** | 全砍 `*_previous_day_balance` × 2 |
| Q5 | `borrowing_fee_rate` 是否 Beta 階段必要 | **反方勝** | **Beta 階段全刪**,raw 不抓,P3 後評估 |
| Q6 | `market_value` 是否進 Silver | **各勝一半** | `market_value` 改 view;`market_value_weight` 保留候選 |

### 0.1 額外採納反方的「冗餘欄位砍除」

| 欄位 | v3.2 裁決 |
|---|---|
| `trading_date_ref.is_trading_day` | 🔴 砍(row 存在 = 交易日) |
| `stock_info_ref.listing_date` | 🔴 砍(無 Core 消費) |
| `stock_info_ref.delisting_reason` | 🔴 砍(無 Core 消費) |
| `stock_info_ref.is_active` | 🔴 砍(用 `delisting_date IS NULL` 等價) |
| `stock_info_ref.last_updated` | 🟠 降為 ETL 內部 |
| `price_adjustment_events.adjustment_factor` | 🔴 砍(違反 Medallion,改在 Silver 算) |
| `price_adjustment_events.source_dataset` | 🔴 砍(用 event_type 推導) |
| `price_adjustment_events.after_price` | 🟠 可選 |
| `*_quota` 欄位(margin/SBL) | 🔴 砍(屬 reference data,週/月頻另存) |
| `_notrend` × 3 欄位(business_indicator) | 🔴 砍(總經用,Beta 不需) |
| `monitoring_color_streak` / `monitoring_color_changed` / `monitoring_score_3m_avg` | 🔴 砍(可即時算) |
| `total_3_institutional_net` | 🔴 砍(SUM 可算) |
| `foreign_dealer_self_net`(全市場聚合) | 🔴 砍(YAGNI) |

### 0.2 最終砍除幅度統計

| 項目 | v3.1 | v3.2 | 砍除 |
|---|---|---|---|
| Core 總數 | 36 | **35**(維持 v3) | -1 |
| Bronze 新增 | 8 | **5** | -3 |
| Reference data | 3 | **2** | -1 |
| Silver 必建 | 15 | **14** | -1 |
| Silver Derived(降級為 view) | 0 | **2** | +2 view |
| Silver ALTER 擴充欄位 | ~25 | **~12** | -13 |
| **整體 schema 物件** | 高度膨脹 | **精簡平衡** | -35% |

---

## 一、35 Core 完整一覽(維持 v3,不新增 Core)

| # | Core | 子類 | 優先級 | Silver 表 | M3 表 | facts |
|---|---|---|---|---|---|---|
| 1 | tw_market_core | Market | P0 | price_limit_merge_events + price_*_fwd + price_adjustment_events + stock_suspension_events | — | ✓ |
| 2 | neely_core | Wave | P0 | — | structural_snapshots | ✓ |
| 3 | traditional_core | Wave | P3 | — | structural_snapshots | ✓ |
| 4 | revenue_core | Fundamental | P2 | monthly_revenue_derived | — | ✓ |
| 5 | valuation_core | Fundamental | P2 | valuation_daily_derived | — | ✓ |
| 6 | financial_statement_core | Fundamental | P2 | financial_statement_derived | — | ✓ |
| 7 | institutional_core | Chip | P2 | institutional_daily_derived(+gov_bank_net 1 欄) | — | ✓ |
| 8 | margin_core | Chip | P2 | margin_daily_derived(+SBL 6 欄) | — | ✓ |
| 9 | foreign_holding_core | Chip | P2 | foreign_holding_derived | — | ✓ |
| 10 | shareholder_core | Chip | P2 | holding_shares_per_derived | — | ✓ |
| 11 | day_trading_core | Chip | P2 | day_trading_derived | — | ✓ |
| 12 | taiex_core | Env | P2 | taiex_index_derived | — | ✓ |
| 13 | us_market_core | Env | P2 | us_market_index_derived | — | ✓ |
| 14 | exchange_rate_core | Env | P2 | exchange_rate_derived | — | ✓ |
| 15 | fear_greed_core | Env | P2 | (依 B 主檔) | — | ✓ |
| 16 | market_margin_core | Env | P2 | market_margin_maintenance_derived(+市場融資融券 2 欄) | — | ✓ |
| 17 | macd_core | Momentum | P1 | — | indicator_values | ✓ |
| 18 | rsi_core | Momentum | P1 | — | indicator_values | ✓ |
| 19 | kd_core | Momentum | P1 | — | indicator_values | ✓ |
| 20 | adx_core | Momentum | P1 | — | indicator_values | ✓ |
| 21 | ma_core | Momentum | P1 | — | indicator_values | ✓ |
| 22 | ichimoku_core | Momentum | P3 | — | indicator_values | ✓ |
| 23 | williams_r_core | Momentum | P3 | — | indicator_values | ✓ |
| 24 | cci_core | Momentum | P3 | — | indicator_values | ✓ |
| 25 | coppock_core | Momentum | P3 | — | indicator_values(月線) | ✓ |
| 26 | bollinger_core | Volatility | P1 | (候選) | indicator_values | ✓ |
| 27 | atr_core | Volatility | P1 | — | indicator_values | ✓ |
| 28 | keltner_core | Volatility | P3 | — | indicator_values | ✓ |
| 29 | donchian_core | Volatility | P3 | — | indicator_values | ✓ |
| 30 | obv_core | Volume | P1 | (候選 G-6) | indicator_values | ✓ |
| 31 | vwap_core | Volume | P3 | — | indicator_values(anchor) | ✓ |
| 32 | mfi_core | Volume | P3 | — | indicator_values | ✓ |
| 33 | candlestick_pattern_core | Pattern | P2 | — | facts | ✓ |
| 34 | support_resistance_core | Pattern | P2 | — | structural_snapshots | ✓ |
| 35 | trendline_core | Pattern | P2 | — | structural_snapshots | ✓ |

**註**:`business_indicator` 從 v3.1 提案的「第 36 Core」**降為 reference 級別**,不算入 Core 計數,但有獨立 Bronze + Silver 表(見 §2.4)。

---

## 二、Schema 物件總清單(v3.2 精簡版)

### 2.1 Bronze 新增 raw(5 張)

| # | 表名 | 來源 FinMind | 用途 |
|---|---|---|---|
| 1 | `market_ohlcv_tw` | TotalReturnIndex + VariousIndicators5Seconds | TAIEX OHLCV(維持 v3) |
| 2 | `price_adjustment_events`(精簡版) | DividendResult + Dividend + CapitalReductionReferencePrice + SplitPrice + ParValueChange | tw_market_core 還原因子事件來源 |
| 3 | `stock_suspension_events` | TaiwanStockSuspended | 個股暫停交易事件(影響 trading_date_ref 的個股級判斷) |
| 4 | `securities_lending_tw` | TaiwanStockSecuritiesLending | margin_core 借券成交明細 |
| 5 | `business_indicator_tw` | TaiwanBusinessIndicator | Environment 月頻指標 raw(降級後仍持久化) |

**砍掉(對比 v3.1)**:
- ❌ `tw_market_event_log`(改為單一表 `stock_suspension_events`)
- ❌ `total_institutional_investors_tw`(改 view)
- ❌ `total_margin_purchase_short_sale_tw`(改 view)
- ❌ `day_trading_borrowing_fee_rate_tw`(Beta 階段不抓)

### 2.2 Reference Data(2 張)

| # | 表名 | 來源 FinMind |
|---|---|---|
| 1 | `trading_date_ref` | TaiwanStockTradingDate |
| 2 | `stock_info_ref` | TaiwanStockInfo + TaiwanStockDelisting |

**砍掉(對比 v3.1)**:
- ❌ `industry_chain_ref` → **移出 Collector**,移交 Aggregation Layer

### 2.3 Silver 必建表(14 張 = v3 的 13 + business_indicator_derived)

| # | 表名 | 來源 Core | 暖機 | 文件 |
|---|---|---|---|---|
| 1 | `price_limit_merge_events` | tw_market_core | 0 | A |
| 2 | `monthly_revenue_derived` | revenue_core | 60 個月 | A |
| 3 | `valuation_daily_derived`(+market_value_weight) | valuation_core | 1260 交易日 | A |
| 4 | `financial_statement_derived` | financial_statement_core | ~16 季 | A |
| 5 | `institutional_daily_derived`(+gov_bank_net) | institutional_core | 70 天 | B |
| 6 | `margin_daily_derived`(+SBL 6 欄) | margin_core | 20 天 | B |
| 7 | `foreign_holding_derived` | foreign_holding_core | 20 天 | B |
| 8 | `holding_shares_per_derived` | shareholder_core | 8 週 | B |
| 9 | `day_trading_derived` | day_trading_core | 20 天 | B |
| 10 | `taiex_index_derived` | taiex_core | 104 根 | B |
| 11 | `us_market_index_derived` | us_market_core | 104 根 | B |
| 12 | `exchange_rate_derived` | exchange_rate_core | 30 天 | B |
| 13 | `market_margin_maintenance_derived`(+市場融資融券 2 欄) | market_margin_core | 1 天 | B |
| **14** | **`business_indicator_derived`(精簡版)** | **(reference)** | 36 個月 | v3.2 新 |

### 2.4 候選 Silver(1~2 張,Phase 7a 動工前決議)

| 表名 | 來源 Core | 優先級 |
|---|---|---|
| `obv_derived` | obv_core | P1 |
| `bollinger_derived` | bollinger_core | P1 |

### 2.5 Silver Views(取代 v3.1 部分預計算 Silver)

| View 名 | 取代 v3.1 的 | 計算邏輯 |
|---|---|---|
| `total_institutional_view` | `total_institutional_daily_derived` | `SELECT date, SUM(...) FROM institutional_daily_derived GROUP BY date` |
| `valuation_market_value_view` | `valuation_daily_derived.market_value` | `close × NumberOfSharesIssued` |
| `total_margin_purchase_view` | `total_margin_purchase_short_sale` derived | 全市場聚合 |

**設計理由**:這些都是「SQL 一行可推導」的衍生指標,放 view 比放 Silver 維護成本低 90%。

### 2.6 既有 Silver 表 ALTER 擴充欄位(精簡版,~12 欄)

#### 2.6.1 `margin_daily_derived` +6 欄(SBL 借券關鍵欄位)

```sql
ALTER TABLE margin_daily_derived
    -- 融券關鍵 2 欄(砍掉 previous/quota/redemption,LAG 可算/reference data/罕用)
    ADD COLUMN margin_short_sales_short_sales INT,           -- 融券賣出量
    ADD COLUMN margin_short_sales_short_covering INT,        -- 融券買回量
    ADD COLUMN margin_short_sales_current_day_balance INT,   -- 融券當日餘額
    -- SBL 借券關鍵 3 欄(砍掉 previous/quota/adjustments)
    ADD COLUMN sbl_short_sales_short_sales INT,              -- 借券賣出
    ADD COLUMN sbl_short_sales_returns INT,                  -- 借券歸還
    ADD COLUMN sbl_short_sales_current_day_balance INT;      -- 借券當日餘額
```

> **3363 上詮 4/27 限跌停案例**:`sbl_short_sales_short_sales` 提供當日借券賣出量,可實證「拉積盤效應」是否伴隨借券放空,這是用戶實戰需要的關鍵欄位。

#### 2.6.2 `institutional_daily_derived` +1 欄

```sql
ALTER TABLE institutional_daily_derived
    ADD COLUMN gov_bank_net BIGINT;       -- 八大行庫淨買賣(buy/sell 二擇一,留 net)
```

#### 2.6.3 `market_margin_maintenance_derived` +2 欄

```sql
ALTER TABLE market_margin_maintenance_derived
    ADD COLUMN total_margin_purchase_balance BIGINT,     -- 整體市場融資餘額
    ADD COLUMN total_short_sale_balance BIGINT;          -- 整體市場融券餘額
```

#### 2.6.4 `valuation_daily_derived` +1 欄

```sql
ALTER TABLE valuation_daily_derived
    ADD COLUMN market_value_weight NUMERIC;     -- 個股佔大盤市值比重(全市場聚合,進 Silver)
-- market_value 不進 Silver,改 view(close × shares 即時算)
```

#### 2.6.5 dirty 欄位(維持 v3)

13 張 Silver + 1 張 v3.2 新增(business_indicator_derived) = 14 張統一加 `is_dirty` / `dirty_at` + 索引。

### 2.7 M3 三表(維持 v3)

| 表 | 排程 |
|---|---|
| `structural_snapshots` | P0 同步建 |
| `indicator_values` | P1 同步建 |
| `core_dependency_graph` | P0 同步建 |

---

## 三、tw_market_core 還原因子事件來源(精簡版)

### 3.1 五大來源(維持 v3.1 的 mapping)

| FinMind Dataset | 還原因子事件類型 |
|---|---|
| TaiwanStockDividendResult | 除權息事實(主來源) |
| TaiwanStockDividend | 預告(可選) |
| TaiwanStockCapitalReductionReferencePrice | 減資 |
| TaiwanStockSplitPrice | 股票分割 |
| TaiwanStockParValueChange | 面額變更 |

### 3.2 `price_adjustment_events` schema(v3.2 精簡版)

```sql
CREATE TABLE price_adjustment_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    event_date          DATE NOT NULL,
    event_type          TEXT NOT NULL,           -- 'dividend_cash' / 'dividend_stock' / 'capital_reduction' / 'split' / 'par_value_change'
    before_price        NUMERIC,                 -- 事件前收盤價
    reference_price     NUMERIC,                 -- 減除後參考價(主要還原因子輸入)
    cash_dividend       NUMERIC,
    stock_dividend      NUMERIC,
    PRIMARY KEY (market, stock_id, event_date, event_type)
);
CREATE INDEX idx_pae_date ON price_adjustment_events(market, stock_id, event_date);
```

**砍掉(對比 v3.1)**:
- ❌ `after_price`(可選,Beta 不抓)
- ❌ `adjustment_factor`(違反 Medallion,改在 Silver 算)
- ❌ `source_dataset`(用 event_type 推導)

---

## 四、stock_suspension_events(取代 tw_market_event_log)

### 4.1 簡化設計

只保留**個股暫停交易事件**(影響 prev_trading_day 個股級判斷),砍掉 DayTradingSuspension(day_trading_core 自己已標記)和 DispositionSecurities(P3 後)。

```sql
CREATE TABLE stock_suspension_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    suspension_date     DATE NOT NULL,
    suspension_time     TEXT,                    -- 暫停時間
    resumption_date     DATE,
    resumption_time     TEXT,
    reason              TEXT,
    PRIMARY KEY (market, stock_id, suspension_date)
);
```

**FinMind 來源**:`TaiwanStockSuspended`

**用途**:
- `prev_trading_day(stock_id, date)` 模組精確計算個股級「前一交易日」
- tw_market_core 識別個股級交易缺口

---

## 五、Reference Data 精簡版(2 張)

### 5.1 `trading_date_ref`(精簡版)

```sql
CREATE TABLE trading_date_ref (
    market  TEXT NOT NULL,
    date    DATE NOT NULL,
    PRIMARY KEY (market, date)
);
```

**設計**:row 存在 = 交易日,不存在 = 非交易日。**砍掉 `is_trading_day` 欄位**。

### 5.2 `stock_info_ref`(精簡版)

```sql
CREATE TABLE stock_info_ref (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    stock_name          TEXT,
    industry_category   TEXT,                  -- FinMind 主檔粗分類
    type                TEXT NOT NULL,         -- twse / tpex / emerging
    delisting_date      DATE,                  -- NULL = 仍上市
    PRIMARY KEY (market, stock_id)
);
CREATE INDEX idx_sir_active ON stock_info_ref(market, stock_id) WHERE delisting_date IS NULL;
CREATE INDEX idx_sir_industry ON stock_info_ref(industry_category) WHERE industry_category IS NOT NULL;
```

**砍掉(對比 v3.1)**:`listing_date` / `delisting_reason` / `is_active` / `last_updated` 四欄。

### 5.3 `industry_chain_ref` 移出 Collector

**裁決說明**:
- v3.1 §2.2.3 強調「對應使用者 SKILL v3.2 第七階段方法論」,屬 Aggregation Layer 需求
- v3 反覆拍板「**Collector 唯一客戶是 Core**」,Q1 答辯後採納反方
- 移交 **Aggregation Layer 的 derived data**,Collector 不負責產業鏈表

**配套說明**:
- 使用者深度分析需要產業鏈時,在 Aggregation Layer / SKILL.md 端 query FinMind 即可
- 不影響 Phase 0 / Phase 1 / Phase 7 動工

---

## 六、business_indicator(降級版)

### 6.1 降級裁決(Q2 答辯結果)

v3.1 §2.1.3 自己寫「**不寫入 Core,僅 Aggregation Layer 比對**」 → 既然不被 Core 消費,**不該佔用 Core 編號**。

但 Aggregation Layer 確實需要月頻環境因子持久化(每次 query FinMind 不適合 20 beta 用戶共享),保留 Bronze + Silver,**不享 Core 完整 spec**。

### 6.2 `business_indicator_tw`(Bronze)

```sql
CREATE TABLE business_indicator_tw (
    market              TEXT NOT NULL DEFAULT 'tw',
    date                DATE NOT NULL,           -- 月初
    leading             NUMERIC,
    coincident          NUMERIC,
    lagging             NUMERIC,
    monitoring          INT,                     -- 綜合分數
    monitoring_color    TEXT,                    -- R / YR / G / YB / B
    PRIMARY KEY (market, date)
);
```

**砍掉**:`leading_notrend` / `coincident_notrend` / `lagging_notrend` 三欄(總經學家用,Beta 不需)。

### 6.3 `business_indicator_derived`(Silver,精簡版)

```sql
CREATE TABLE business_indicator_derived (
    market              TEXT NOT NULL DEFAULT 'tw',
    stock_id            TEXT NOT NULL DEFAULT '_market_',
    date                DATE NOT NULL,
    leading             NUMERIC,
    coincident          NUMERIC,
    lagging             NUMERIC,
    monitoring          INT,
    monitoring_color    TEXT,
    is_dirty            BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at            TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX idx_bid_dirty ON business_indicator_derived(is_dirty) WHERE is_dirty = TRUE;
```

**砍掉(對比 v3.1)**:
- ❌ `monitoring_color_streak`(streak_detector shared 模組即時算)
- ❌ `monitoring_score_3m_avg`(YAGNI)
- ❌ `monitoring_color_changed`(LAG 即時算)

**設計理由**:Silver 等同 raw 加 dirty-detection 欄位,**不做衍生計算**。所有衍生需求(streak / 3m_avg / changed)由 Aggregation Layer 即時算。

---

## 七、Pipeline 規範(維持 v3 + v3.1 修訂)

### 7.1 TAIEX Pipeline(維持 v3)

```
[個股路徑]
  Raw OHLC + price_limit + price_adjustment_events
    → tw_market_core
    → price_daily_fwd / price_weekly_fwd / price_monthly_fwd

[TAIEX 路徑(獨立平行)]
  market_ohlcv_tw + market_index_tw
    → taiex_core(內嵌計算)
    → taiex_index_derived
```

### 7.2 Dirty-detection 機制(維持 v3 四視角)

| 視角 | 觸發 |
|---|---|
| ① | Bronze 變更 |
| ② | M3 內 Core 引用(trendline → neely) |
| ③ | tw_market_core 還原因子變更 |
| ④ | Bronze 補單 |
| ⑤ | Workflow toml 新增 params_hash entry |

### 7.3 寫入路徑三分流(維持 v3)

| 資料性質 | 目的地 |
|---|---|
| 每日數值流 | indicator_values JSONB |
| 事件式事實 | facts(append-only) |
| 結構快照 | structural_snapshots |

### 7.4 還原前/還原後 volume(維持 v3.1)

| Core | 採用 |
|---|---|
| obv_core / vwap_core / mfi_core | `price_daily_fwd.volume`(待 A-V3 驗證) |

---

## 八、Phase 0 / Phase 1 動工拓撲(v3.2 最終版)

### 8.1 Layer 0 — 動工前必修(K-1 硬阻塞)

| # | 任務 | 優先序 |
|---|---|---|
| K-1 | `chip_cores.md` §4.4 移除 `MarginPoint.margin_maintenance` 欄位 + §4.5 刪除對應 Fact 範例 | 🔴 動工前必修 |

### 8.2 Layer 1 — Reference Data(Phase 0)

| # | 任務 | 優先序 |
|---|---|---|
| **R-1** | 建 `trading_date_ref`(從 TaiwanStockTradingDate 全量抓 2005~) | 🔴 阻塞(prev_trading_day shared 依賴) |
| **R-2** | 建 `stock_info_ref`(join TaiwanStockInfo + TaiwanStockDelisting) | 🔴 阻塞(ETL 起始點) |

### 8.3 Layer 2 — Shared 模組(Phase 0)

| 模組 | 跨度 | 優先序 |
|---|---|---|
| `streak_detector/` | 跨 7 大子類 | 🔴 最高 |
| `adjustment_factor_resolver/` | tw_market_core P0 | 🔴 最高 |
| `prev_trading_day(stock_id, date)` | 全 derived(依 R-1 + stock_suspension_events) | 🔴 高 |
| `indicator_math/` | Env + 全 Indicator | 🔴 高 |
| `divergence_detector/` | Momentum + Volume | 🟠 高 |

### 8.4 Layer 3 — P0 Bronze(Phase 1)

| # | 任務 | FinMind dataset |
|---|---|---|
| B-1 | 補抓 TAIEX/TPEx OHLCV(`market_ohlcv_tw`) | TotalReturnIndex + VariousIndicators5Seconds |
| B-2 | TAIEX 報酬指數 vs 不還原指數並存 | 同 B-1 |
| **B-3** | 5 張還原因子事件 raw(`price_adjustment_events`) | DividendResult + Dividend + CapitalReduction + Split + ParValue |
| **B-4** | 個股暫停交易事件(`stock_suspension_events`) | TaiwanStockSuspended |

### 8.5 Layer 4 — P0 阻塞驗證

| # | 驗證項 | 優先序 |
|---|---|---|
| G-ATR-1 | atr_core 與 neely_core 內嵌 ATR golden test | 🔴 阻塞 |
| A-V3 | `price_daily_fwd.volume` 是否已隨除權息調整 | 🔴 阻塞 |
| W-1 | `WaveCore` trait 草案固化 | 🔴 阻塞 |
| F-V1 | facts 表 schema 確認 | 🟠 高 |

### 8.6 Layer 5 — P1 Bronze + Silver

| # | 任務 | 優先序 |
|---|---|---|
| **B-5** | 借券明細 raw(`securities_lending_tw`) | 🟠 |
| **B-6** | 景氣指標 raw(`business_indicator_tw`) | 🟠 |
| A-V1 | `financial_statement.detail` 12 個 origin_name | 🟠 |
| A-V2 | FinMind 累計值 vs 當季值行為 | 🟠 |
| B-V1 | `HoldingSharesLevel` 實際 keys audit | 🟠 |

**砍掉(對比 v3.1)**:
- ❌ B-7(借券費率)— Beta 階段不抓
- ❌ B-8(市值)— 改 view
- ❌ B-9(景氣指標已合併到 B-6)
- ❌ B-10(整體融資融券)— 改 view

### 8.7 Phase 7 排程

| 階段 | 內容 |
|---|---|
| 7a 不跨表 Silver(可平行) | 11 張(原 10 + business_indicator_derived) |
| 7b 跨表依賴 | 2 張(financial_statement + day_trading) |
| 7c tw_market_core 系列 | price_*_fwd + price_limit_merge_events |
| 7d M3 寫入 | 已拉前至 P0/P1 |

---

## 九、原則對齊驗證(v3.2)

| 中心思想 | v3.2 驗證 |
|---|---|
| **Medallion 分層剛性收斂** | ✅ 嚴格分層,Aggregation Layer 不寫 M3,**LAG 可算欄位移出 Silver** |
| **Rust 預先計算優於 on-demand** | ✅ 14 張 Silver 預計算 + 3 個 view 即時算(分流) |
| **Params 自由度即邊界** | ✅ Indicator 16 Core 多數走 indicator_values JSONB |
| **Collector 唯一客戶是 Core** | ✅ `industry_chain_ref` 移出,**business_indicator 降為 reference** |
| **既有 27 表凍結邊界** | ✅ 不變(僅 ALTER 加欄位) |
| **寫入路徑三分流** | ✅ facts / indicator_values / structural_snapshots |
| **YAGNI 原則** | ✅ 砍除所有「將來可能用到」欄位(borrowing_fee / listing_date / monitoring_streak 等) |
| **Wave Core 對 M3 強契約** | ✅ structural_snapshots 7 欄完整(維持 v3) |

---

## 十、最終訊息

### 10.1 v3.2 總需求

| 項目 | 數量 |
|---|---|
| Core 總數 | **35**(維持 v3 拍板) |
| Bronze 新增 raw 表 | **5**(market_ohlcv + price_adjustment_events + stock_suspension_events + securities_lending_tw + business_indicator_tw) |
| Reference data | **2**(trading_date_ref + stock_info_ref) |
| Silver 必建 derived | **14**(13 + business_indicator_derived) |
| Silver 候選 | **1~2**(obv / bollinger) |
| Silver Views(取代預計算) | **3**(total_institutional / market_value / total_margin_purchase) |
| Silver ALTER 擴充欄位 | **~12**(margin SBL 6 + institutional 1 + market_margin 2 + valuation 1 + 既有 v3 的 ~4) |
| M3 表 | **3**(structural_snapshots + indicator_values + core_dependency_graph) |
| Phase 0 硬阻塞驗證 | **5 項**(K-1 / R-1 / R-2 / G-ATR-1 / A-V3 / W-1 / F-V1 — 注:R-1/R-2 屬 reference data 阻塞) |
| Core spec 修訂候選 | **30 條**(維持 v3.1) |

### 10.2 v3.2 對 v3.1 的核心改變

**砍除**:
- ❌ Core:business_indicator_core(降為 reference)
- ❌ 表:industry_chain_ref(移出 Collector)/ tw_market_event_log(改 stock_suspension_events 單一表)/ total_institutional_daily_derived(改 view)/ day_trading_borrowing_fee_rate_tw(Beta 不抓)
- ❌ 欄位:~13 欄(LAG 可算 / quota / notrend / streak / changed / 3m_avg / 等冗餘)

**保留**:
- ✅ 用戶 3363 上詮分析需要的 SBL 借券欄位(關鍵實戰指標)
- ✅ 還原因子 5 大來源整合(P0 阻塞)
- ✅ 個股暫停交易事件(prev_trading_day 個股級依賴)

### 10.3 對 Collector 開發者的訊息

> v3.2 是經三輪統合(v2_1+v2_2+v3 → v3 統合 → FinMind 對齊 v3.1 → 反方審查 v3.2)的最終動工版本。
> 
> 相對 v3.1,本版砍除約 35% 的 schema 物件,但保留所有「Beta 階段有實際 Core 消費」的內容。
> 
> **動工前阻塞清單(嚴格依序)**:
> 1. K-1:chip_cores.md 移除 margin_maintenance
> 2. F-V1:確認既有 facts 表 schema
> 3. R-1 / R-2:建 reference data(trading_date + stock_info)
> 4. A-V3:驗證 price_daily_fwd.volume 還原邏輯
> 5. G-ATR-1:atr_core 與 neely_core ATR golden test
> 6. W-1:WaveCore trait 草案固化
> 7. B-1 / B-2 / B-3 / B-4:Phase 1 P0 Bronze 補抓
> 
> **Phase 7 動工可直接以本 v3.2 為單一執行清單**。

---

## 附錄 A:35 Core 完整一覽 + FinMind 來源(v3.2 更新)

| # | Core | 子類 | 優先級 | Silver 表 | M3 表 | FinMind 來源 |
|---|---|---|---|---|---|---|
| 1 | tw_market_core | Market | P0 | price_*_fwd + price_limit_merge_events + price_adjustment_events + stock_suspension_events | — | TaiwanStockPrice / Adj / Week / Month / PriceLimit / Dividend* / CapitalReduction / Split / ParValue / Suspended |
| 2 | neely_core | Wave | P0 | — | structural_snapshots | (派生) |
| 3 | traditional_core | Wave | P3 | — | structural_snapshots | (派生) |
| 4 | revenue_core | Fundamental | P2 | monthly_revenue_derived | — | TaiwanStockMonthRevenue |
| 5 | valuation_core | Fundamental | P2 | valuation_daily_derived(+market_value_weight) | — | TaiwanStockPER + MarketValueWeight |
| 6 | financial_statement_core | Fundamental | P2 | financial_statement_derived | — | FinancialStatements + BalanceSheet + CashFlowsStatement |
| 7 | institutional_core | Chip | P2 | institutional_daily_derived(+gov_bank_net) | — | InstitutionalInvestorsBuySell + GovernmentBankBuySell |
| 8 | margin_core | Chip | P2 | margin_daily_derived(+SBL 6 欄) | — | MarginPurchaseShortSale + SecuritiesLending |
| 9 | foreign_holding_core | Chip | P2 | foreign_holding_derived | — | TaiwanStockShareholding |
| 10 | shareholder_core | Chip | P2 | holding_shares_per_derived | — | TaiwanStockHoldingSharesPer |
| 11 | day_trading_core | Chip | P2 | day_trading_derived | — | TaiwanStockDayTrading |
| 12 | taiex_core | Env | P2 | taiex_index_derived | — | TotalReturnIndex + VariousIndicators5Seconds |
| 13 | us_market_core | Env | P2 | us_market_index_derived | — | (FinMind 美國市場) |
| 14 | exchange_rate_core | Env | P2 | exchange_rate_derived | — | (FinMind ExchangeRate) |
| 15 | fear_greed_core | Env | P2 | (依 B 主檔) | — | (外部資料源) |
| 16 | market_margin_core | Env | P2 | market_margin_maintenance_derived(+市場融資融券 2 欄) | — | TotalExchangeMarginMaintenance + TotalMarginPurchaseShortSale |
| 17~25 | (Momentum 9 Core) | Momentum | P1×5 / P3×4 | — | indicator_values | (派生) |
| 26~29 | (Volatility 4 Core) | Volatility | P1×2 / P3×2 | bollinger 候選 | indicator_values | (派生) |
| 30~32 | (Volume 3 Core) | Volume | P1×1 / P3×2 | obv 候選 | indicator_values | (派生) |
| 33 | candlestick_pattern_core | Pattern | P2 | — | facts | (派生) |
| 34 | support_resistance_core | Pattern | P2 | — | structural_snapshots | (派生) |
| 35 | trendline_core | Pattern | P2 | — | structural_snapshots | (派生 + neely 引用) |

**Reference / 非 Core 級別**:
- `trading_date_ref` ← TaiwanStockTradingDate
- `stock_info_ref` ← TaiwanStockInfo + TaiwanStockDelisting
- `business_indicator_tw` + `business_indicator_derived` ← TaiwanBusinessIndicator(降為 Reference)

---

## 附錄 B:v3.2 完整新增 DDL

```sql
-- ============================================================
-- v3.2 Reference Data(2 張)
-- ============================================================

CREATE TABLE trading_date_ref (
    market  TEXT NOT NULL,
    date    DATE NOT NULL,
    PRIMARY KEY (market, date)
);

CREATE TABLE stock_info_ref (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    stock_name          TEXT,
    industry_category   TEXT,
    type                TEXT NOT NULL,
    delisting_date      DATE,
    PRIMARY KEY (market, stock_id)
);
CREATE INDEX idx_sir_active ON stock_info_ref(market, stock_id) WHERE delisting_date IS NULL;
CREATE INDEX idx_sir_industry ON stock_info_ref(industry_category) WHERE industry_category IS NOT NULL;

-- ============================================================
-- v3.2 Bronze(5 張,本版新增 4 張)
-- ============================================================

-- 還原因子事件統合表
CREATE TABLE price_adjustment_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    event_date          DATE NOT NULL,
    event_type          TEXT NOT NULL,
    before_price        NUMERIC,
    reference_price     NUMERIC,
    cash_dividend       NUMERIC,
    stock_dividend      NUMERIC,
    PRIMARY KEY (market, stock_id, event_date, event_type)
);
CREATE INDEX idx_pae_date ON price_adjustment_events(market, stock_id, event_date);

-- 個股暫停交易事件(取代 v3.1 tw_market_event_log)
CREATE TABLE stock_suspension_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    suspension_date     DATE NOT NULL,
    suspension_time     TEXT,
    resumption_date     DATE,
    resumption_time     TEXT,
    reason              TEXT,
    PRIMARY KEY (market, stock_id, suspension_date)
);

-- 借券成交明細
CREATE TABLE securities_lending_tw (
    market                  TEXT NOT NULL,
    stock_id                TEXT NOT NULL,
    date                    DATE NOT NULL,
    transaction_type        TEXT NOT NULL,        -- 議借 / 競價
    volume                  BIGINT,
    fee_rate                NUMERIC,
    close                   NUMERIC,
    original_return_date    DATE,
    original_lending_period INT,
    PRIMARY KEY (market, stock_id, date, transaction_type, fee_rate)
);

-- 景氣指標 raw
CREATE TABLE business_indicator_tw (
    market              TEXT NOT NULL DEFAULT 'tw',
    date                DATE NOT NULL,
    leading             NUMERIC,
    coincident          NUMERIC,
    lagging             NUMERIC,
    monitoring          INT,
    monitoring_color    TEXT,
    PRIMARY KEY (market, date)
);

-- ============================================================
-- v3.2 Silver(本版新增 1 張)
-- ============================================================

CREATE TABLE business_indicator_derived (
    market              TEXT NOT NULL DEFAULT 'tw',
    stock_id            TEXT NOT NULL DEFAULT '_market_',
    date                DATE NOT NULL,
    leading             NUMERIC,
    coincident          NUMERIC,
    lagging             NUMERIC,
    monitoring          INT,
    monitoring_color    TEXT,
    is_dirty            BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at            TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX idx_bid_dirty ON business_indicator_derived(is_dirty) WHERE is_dirty = TRUE;

-- ============================================================
-- v3.2 既有 Silver 表 ALTER(精簡版,~12 欄)
-- ============================================================

-- margin_daily_derived +6 欄(SBL 借券關鍵)
ALTER TABLE margin_daily_derived
    ADD COLUMN margin_short_sales_short_sales INT,
    ADD COLUMN margin_short_sales_short_covering INT,
    ADD COLUMN margin_short_sales_current_day_balance INT,
    ADD COLUMN sbl_short_sales_short_sales INT,
    ADD COLUMN sbl_short_sales_returns INT,
    ADD COLUMN sbl_short_sales_current_day_balance INT;

-- institutional_daily_derived +1 欄
ALTER TABLE institutional_daily_derived
    ADD COLUMN gov_bank_net BIGINT;

-- market_margin_maintenance_derived +2 欄
ALTER TABLE market_margin_maintenance_derived
    ADD COLUMN total_margin_purchase_balance BIGINT,
    ADD COLUMN total_short_sale_balance BIGINT;

-- valuation_daily_derived +1 欄(market_value 改 view 不進)
ALTER TABLE valuation_daily_derived
    ADD COLUMN market_value_weight NUMERIC;

-- 14 張 Silver 統一加 dirty 欄位(範例)
ALTER TABLE monthly_revenue_derived 
    ADD COLUMN is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN dirty_at TIMESTAMPTZ;
CREATE INDEX idx_mrd_dirty ON monthly_revenue_derived(is_dirty) WHERE is_dirty = TRUE;
-- (其他 13 張同樣 ALTER,此處省略)

-- ============================================================
-- v3.2 Silver Views(取代部分預計算 Silver)
-- ============================================================

-- 全市場法人合計(取代 total_institutional_daily_derived)
CREATE VIEW total_institutional_view AS
SELECT 
    market,
    '_market_' AS stock_id,
    date,
    SUM(foreign_investor_net) AS foreign_investor_net,
    SUM(investment_trust_net) AS investment_trust_net,
    SUM(dealer_self_net) AS dealer_self_net
FROM institutional_daily_derived
GROUP BY market, date;

-- 個股市值(取代 valuation_daily_derived.market_value)
CREATE VIEW valuation_market_value_view AS
SELECT 
    p.market,
    p.stock_id,
    p.date,
    p.close * f.NumberOfSharesIssued AS market_value
FROM price_daily_fwd p
JOIN foreign_holding_derived f USING (market, stock_id, date);

-- 整體市場融資融券(取代 total_margin_purchase_short_sale derived)
-- DDL 待 raw 表確認後補

-- ============================================================
-- v3.2 M3 三表(維持 v3,此處不重複)
-- ============================================================
-- structural_snapshots / indicator_values / core_dependency_graph 
-- 完整 DDL 請參考 v3 §附錄 B

-- ============================================================
-- 條件性 ALTER:price_daily_fwd(依 A-V3 結果)
-- ============================================================
-- ALTER TABLE price_daily_fwd ADD COLUMN volume_adjusted NUMERIC;
-- ALTER TABLE price_daily_fwd ADD COLUMN cumulative_adjustment_factor NUMERIC;
-- ALTER TABLE price_daily_fwd ADD COLUMN is_adjusted BOOLEAN;
-- ALTER TABLE price_daily_fwd ADD COLUMN adjustment_factor NUMERIC;
```

---

**(v3.2 定稿完)**

> 本 v3.2 規格涵蓋:
> - 三份 v2 報告統合(v3)
> - FinMind API 對齊(v3.1)
> - 反方審查 + 6 題答辯裁決(v3.2)
> - 35 Core(維持,不新增)
> - 5 Bronze + 2 Reference + 14 Silver + 3 Views + 3 M3 = **27 個 schema 物件**
> - ~12 欄 ALTER(對比 v3.1 的 ~25 欄,**砍 52%**)
> - Phase 0 硬阻塞 6+ 項已逐項列出
> 
> **動工版本,不再修訂**。
