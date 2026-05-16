# tw-stock-collector 程式規格

> **版本**: v1.2  
> **日期**: 2026-04-15  
> **定位**: 資料蒐集程式的實作規格(補完 `tw_stock_architecture_review_v1.1` §五的執行細節)  
> **語言**: Python 3.11+(Collector 本體)+ Rust binary(Phase 4 計算層)  
> **依賴文件**: `tw_stock_architecture_review_v1.1.md`(Schema v2.3、欄位映射、後復權算法)
>
> **⚠️ v3.5 R1 後的模組路徑變更**(2026-05-16,本檔仍記錄 v1.2 原始設計):
> - `src/phase_executor.py` → `src/bronze/phase_executor.py`(orchestration only)
> - `src/aggregators.py` → `src/bronze/aggregators/` package(4 module)
> - `src/post_process.py` → `src/bronze/post_process_dividend.py`
> - 新加 `src/bronze/segment_runner.py`(從 phase_executor._run_api 抽 `_SegmentRunner`)
> - 完整當前模組地圖見 `CLAUDE.md` §模組地圖 + `docs/api_pipeline_reference.md`

---

## 一、系統架構總覽

```
tw-stock-collector/
├── config/
│   ├── collector.toml          # 主設定檔（API registry、排程、rate limit）
│   └── stock_list.toml         # 股票清單（獨立維護，可手動增減）
├── src/
│   ├── main.py                 # CLI 進入點
│   ├── config_loader.py        # TOML 解析 + 驗證
│   ├── rate_limiter.py         # Token Bucket 實作
│   ├── phase_executor.py       # Phase 排程引擎
│   ├── api_client.py           # FinMind HTTP client（統一出入口）
│   ├── date_segmenter.py       # 歷史回補日期分段
│   ├── stock_resolver.py       # stock_list.toml 解析 + stock_info 查詢
│   ├── db.py                   # SQLite 連線管理 + UPSERT 工具
│   ├── field_mapper.py         # 欄位映射 + rename + detail JSON 打包
│   ├── sync_tracker.py         # stock_sync_status 斷點續傳
│   └── rust_bridge.py          # 呼叫 Rust binary（Phase 4）
├── rust_compute/               # Rust binary 專案（Cargo workspace）
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # 後復權 + 週K/月K 聚合 + PriceMapper
├── data/
│   └── tw_stock.db             # SQLite 資料庫
├── logs/
│   └── collector_YYYYMMDD.log  # 日誌檔（每日一檔）
└── README.md
```

**執行流程**：

```
main.py (CLI)
  → config_loader 載入 collector.toml + stock_list.toml
  → logger 初始化（寫入 logs/collector_YYYYMMDD.log）
  → phase_executor 按 Phase 1→6 順序執行
    → 每個 Phase 內：
      → stock_resolver 提供股票迭代清單（per_stock 模式）
      → date_segmenter 切分日期區間（backfill 模式）
      → rate_limiter 控制 API 呼叫頻率
      → api_client 發送 HTTP request
      → field_mapper 轉換欄位 → db UPSERT
      → sync_tracker 更新進度
    → Phase 3 完成後 → rust_bridge 呼叫 Rust binary（Phase 4）
    → Phase 5-6 與 Phase 4 無依賴，可接續執行
```

---

## 二、Config Schema — `collector.toml`

### 2.1 全域設定

```toml
[global]
db_path = "data/tw_stock.db"
log_dir = "logs"
log_level = "INFO"                    # DEBUG | INFO | WARNING | ERROR
backfill_start_date = "2019-01-01"    # 歷史回補起始日
rust_binary_path = "rust_compute/target/release/tw_stock_compute"

[global.rate_limit]
calls_per_hour = 1600                 # Backer 方案上限
burst_size = 5                        # 允許連續快速發送的最大次數
cooldown_on_429_sec = 120             # 收到 HTTP 429 後冷卻秒數
min_interval_ms = 2250                # 兩次呼叫最小間隔（3600/1600*1000）

[global.retry]
max_attempts = 3                      # 單次 API call 最大重試次數
backoff_base_sec = 5                  # 指數退避基底秒數（5, 10, 20...）
backoff_max_sec = 60                  # 退避上限
retry_on_status = [429, 500, 502, 503, 504]  # 觸發重試的 HTTP status
```

### 2.2 API Registry

每個 FinMind Dataset 定義為一個 `[[api]]` entry。Collector 啟動時載入所有 entry，按 `phase` 分組後依序執行。

```toml
# ============================================================
# 參數模式說明：
#   "all_market"         → 只需 dataset + start_date (+ end_date)
#   "per_stock"          → 需 dataset + data_id + start_date + end_date
#   "per_stock_no_end"   → 需 dataset + data_id + start_date（無 end_date）
#   "all_market_no_id"   → 需 dataset + start_date + end_date（無 data_id，如 ParValueChange）
# ============================================================

# --- Phase 1: META（免費，全市場） ---

[[api]]
name = "stock_info"
dataset = "TaiwanStockInfo"
param_mode = "all_market"
target_table = "stock_info"
phase = 1
enabled = true
is_backer = false
segment_days = 0                      # 0 = 不分段，一次拉全部
field_rename = {}                     # 空 = 欄位名直接對應
detail_fields = []                    # 空 = 無需打包進 detail JSON
notes = "上市上櫃股票基本資料"

[[api]]
name = "stock_delisting"
dataset = "TaiwanStockDelisting"
param_mode = "all_market"
target_table = "stock_info"
phase = 1
enabled = true
is_backer = false
segment_days = 0
merge_strategy = "update_delist_date" # 特殊策略：只更新 stock_info.delist_date
notes = "下市櫃清單，合併寫入 stock_info"

[[api]]
name = "trading_calendar"
dataset = "TaiwanStockTradingDate"
param_mode = "all_market"
target_table = "trading_calendar"
phase = 1
enabled = true
is_backer = false
segment_days = 0
notes = "交易日曆"

[[api]]
name = "market_index_tw"
dataset = "TaiwanStockTotalReturnIndex"
param_mode = "all_market"
target_table = "market_index_tw"
phase = 1
enabled = true
is_backer = false
segment_days = 365
notes = "加權報酬指數"

# --- Phase 2: EVENTS（除權息/減資/分割/面額變更） ---

[[api]]
name = "dividend_result"
dataset = "TaiwanStockDividendResult"
param_mode = "per_stock"
target_table = "price_adjustment_events"
phase = 2
enabled = true
is_backer = true
segment_days = 0                      # 事件量少，不需分段
event_type = "dividend"               # 寫入時附加的 event_type 值
field_rename = { "stock_or_cache_dividend" = "_event_subtype", "stock_and_cache_dividend" = "_combined_dividend", "max_price" = "_max_price", "min_price" = "_min_price", "open_price" = "_open_price" }
detail_fields = ["_event_subtype", "_combined_dividend", "_max_price", "_min_price", "_open_price"]
computed_fields = ["adjustment_factor", "volume_factor", "cash_dividend", "stock_dividend"]
notes = "除權息結果 → price_adjustment_events (event_type=dividend)"

[[api]]
name = "dividend_policy"
dataset = "TaiwanStockDividend"
param_mode = "per_stock_no_end"
target_table = "_dividend_policy_staging"
phase = 2
enabled = true
is_backer = true
segment_days = 0
notes = "股利政策表，用於拆分「權息」混合事件 + 偵測純現增事件。非直接入庫，經 post_process 後合併。"
post_process = "dividend_policy_merge"

[[api]]
name = "capital_reduction"
dataset = "TaiwanStockCapitalReductionReferencePrice"
param_mode = "per_stock"
target_table = "price_adjustment_events"
phase = 2
enabled = true
is_backer = false
segment_days = 0
event_type = "capital_reduction"
field_rename = { "ClosingPriceonTheLastTradingDay" = "before_price", "PostReductionReferencePrice" = "after_price", "OpeningReferencePrice" = "reference_price", "ReasonforCapitalReduction" = "_reason", "LimitUp" = "_limit_up", "LimitDown" = "_limit_down", "ExrightReferencePrice" = "_exright_ref" }
detail_fields = ["_reason", "_limit_up", "_limit_down", "_exright_ref"]
computed_fields = ["adjustment_factor", "volume_factor"]
notes = "減資參考價"

[[api]]
name = "split_price"
dataset = "TaiwanStockSplitPrice"
param_mode = "per_stock"
target_table = "price_adjustment_events"
phase = 2
enabled = true
is_backer = false
segment_days = 0
event_type = "split"
field_rename = { "open_price" = "reference_price", "type" = "_split_type", "max_price" = "_max_price", "min_price" = "_min_price" }
detail_fields = ["_split_type", "_max_price", "_min_price"]
computed_fields = ["adjustment_factor", "volume_factor"]
notes = "分割參考價"

[[api]]
name = "par_value_change"
dataset = "TaiwanStockParValueChange"
param_mode = "all_market_no_id"
target_table = "price_adjustment_events"
phase = 2
enabled = true
is_backer = false
segment_days = 0
event_type = "par_value_change"
field_rename = { "before_close" = "before_price", "after_ref_close" = "after_price", "after_ref_open" = "reference_price", "after_ref_max" = "_limit_up", "after_ref_min" = "_limit_down", "stock_name" = "_stock_name" }
detail_fields = ["_limit_up", "_limit_down", "_stock_name"]
computed_fields = ["adjustment_factor", "volume_factor"]
notes = "面額變更，⚠️ 無 data_id 參數，全市場查詢"

# --- Phase 3: RAW PRICE ---

[[api]]
name = "price_daily"
dataset = "TaiwanStockPrice"
param_mode = "per_stock"
target_table = "price_daily"
phase = 3
enabled = true
is_backer = true
segment_days = 365                    # 一次拉一年，避免 response 過大
notes = "日K原始價格，歷史回補量最大的 API"

[[api]]
name = "price_limit"
dataset = "TaiwanStockPriceLimit"
param_mode = "per_stock"
target_table = "price_limit"
phase = 3
enabled = true
is_backer = true
segment_days = 365
notes = "漲跌停價格"

# --- Phase 4: RUST 計算（非 API，由 rust_bridge 觸發） ---
# Phase 4 不定義 [[api]] entry，由 phase_executor 特殊處理
# 見 §八 Rust Bridge 規格

# --- Phase 5: CHIP / FUNDAMENTAL ---

[[api]]
name = "institutional_daily"
dataset = "TaiwanStockInstitutionalInvestorsBuySell"
param_mode = "per_stock"
target_table = "institutional_daily"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "三大法人買賣超"

[[api]]
name = "margin_daily"
dataset = "TaiwanStockMarginPurchaseShortSale"
param_mode = "per_stock"
target_table = "margin_daily"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "融資融券"

[[api]]
name = "foreign_holding"
dataset = "TaiwanStockShareholding"
param_mode = "per_stock"
target_table = "foreign_holding"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "外資持股"

[[api]]
name = "holding_shares_per"
dataset = "TaiwanStockHoldingSharesPer"
param_mode = "per_stock"
target_table = "holding_shares_per"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "股權分散表"

[[api]]
name = "valuation_daily"
dataset = "TaiwanStockPER"
param_mode = "per_stock"
target_table = "valuation_daily"
phase = 5
enabled = true
is_backer = false
segment_days = 365
notes = "本益比/殖利率/淨值比"

[[api]]
name = "day_trading"
dataset = "TaiwanStockDayTrading"
param_mode = "per_stock"
target_table = "day_trading"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "當沖資訊"

[[api]]
name = "index_weight"
dataset = "TaiwanStockMarketValueWeight"
param_mode = "per_stock"
target_table = "index_weight_daily"
phase = 5
enabled = true
is_backer = true
segment_days = 365
notes = "指數權重"

[[api]]
name = "monthly_revenue"
dataset = "TaiwanStockMonthRevenue"
param_mode = "per_stock"
target_table = "monthly_revenue"
phase = 5
enabled = true
is_backer = true
segment_days = 0
notes = "月營收"

[[api]]
name = "financial_income"
dataset = "TaiwanStockFinancialStatements"
param_mode = "per_stock"
target_table = "financial_statement"
phase = 5
enabled = true
is_backer = true
segment_days = 0
notes = "損益表"

[[api]]
name = "financial_balance"
dataset = "TaiwanStockBalanceSheet"
param_mode = "per_stock"
target_table = "financial_statement"
phase = 5
enabled = true
is_backer = true
segment_days = 0
notes = "資產負債表"

[[api]]
name = "financial_cashflow"
dataset = "TaiwanStockCashFlowsStatement"
param_mode = "per_stock"
target_table = "financial_statement"
phase = 5
enabled = true
is_backer = true
segment_days = 0
notes = "現金流量表"

# --- Phase 6: MACRO（免費，全市場） ---

[[api]]
name = "market_index_us"
dataset = "USStockPrice"
param_mode = "per_stock"
target_table = "market_index_us"
phase = 6
enabled = true
is_backer = false
segment_days = 365
fixed_stock_ids = ["SPY", "^VIX"]     # 不走 stock_list，固定查這兩個
notes = "美股指數 SPY / VIX"

[[api]]
name = "exchange_rate"
dataset = "TaiwanExchangeRate"
param_mode = "all_market"
target_table = "exchange_rate"
phase = 6
enabled = true
is_backer = false
segment_days = 365
notes = "台幣匯率"

[[api]]
name = "institutional_market"
dataset = "TaiwanStockTotalInstitutionalInvestors"
param_mode = "all_market"
target_table = "institutional_market_daily"
phase = 6
enabled = true
is_backer = false
segment_days = 365
notes = "全市場三大法人買賣超"

[[api]]
name = "market_margin"
dataset = "TaiwanTotalExchangeMarginMaintenance"
param_mode = "all_market"
target_table = "market_margin_maintenance"
phase = 6
enabled = true
is_backer = true
segment_days = 365
notes = "整體市場融資維持率"

[[api]]
name = "fear_greed"
dataset = "CnnFearGreedIndex"
param_mode = "all_market"
target_table = "fear_greed_index"
phase = 6
enabled = true
is_backer = false
segment_days = 365
notes = "CNN 恐懼貪婪指數"
```

### 2.3 Config 驗證規則

`config_loader.py` 載入後須執行以下驗證：

1. 每個 `[[api]]` 的 `dataset` 不可重複（除非 `target_table` 不同）
2. `phase` 值必須在 1-6 之間
3. `param_mode` 必須是四種之一
4. 若 `param_mode` 為 `per_stock` 或 `per_stock_no_end`，且無 `fixed_stock_ids`，則必須依賴 `stock_list.toml`
5. 若有 `computed_fields`，必須包含 `adjustment_factor`（僅 Phase 2 事件表）
6. `segment_days` 為 0 表示不分段；大於 0 時必須是正整數
7. `field_rename` 中以 `_` 開頭的 value 會被導向 `detail` JSON

---

## 三、股票清單 — `stock_list.toml`

### 3.1 結構

```toml
# 股票清單設定
# 此檔案獨立於 collector.toml，可隨時手動編輯

[source]
# 清單來源策略：
#   "db"    → 從 stock_info 表動態讀取（Phase 1 完成後可用）
#   "file"  → 使用下方 [stocks] 靜態清單
#   "both"  → DB 為主，[stocks] 為補充（聯集）
mode = "db"

[filter]
# 僅 mode = "db" 時生效
market_type = ["twse", "otc"]         # twse=上市, otc=上櫃, emerging=興櫃
exclude_etf = false                   # 是否排除 ETF
exclude_warrant = true                # 排除權證
exclude_tdr = true                    # 排除 TDR
exclude_delisted = true               # 排除已下市櫃
min_listing_days = 30                 # 上市未滿 N 天的新股暫不納入

[stocks]
# 僅 mode = "file" 或 "both" 時使用
# 可用於開發測試：只跑少數幾檔
ids = [
    # "2330",  # 台積電
    # "2317",  # 鴻海
    # "2442",  # 新美齊（測試現增事件）
]

[dev]
# 開發模式：覆蓋 source.mode，強制使用 [stocks].ids
# 避免開發時跑全市場 1800 檔
enabled = false
```

### 3.2 stock_resolver 行為

```
stock_resolver.resolve(config, db) → List[str]

1. 若 dev.enabled = true → 直接回傳 stocks.ids，忽略其他設定
2. 若 source.mode = "file" → 回傳 stocks.ids
3. 若 source.mode = "db":
   a. 查詢 stock_info 表
   b. 套用 filter 條件
   c. 回傳符合條件的 stock_id 清單（按 stock_id 排序）
4. 若 source.mode = "both" → db 結果 ∪ stocks.ids
```

**Phase 1 先雞後蛋問題**：首次執行時 `stock_info` 表是空的。`phase_executor` 在 Phase 1 完成後重新呼叫 `stock_resolver.resolve()` 更新清單，Phase 2 起使用新清單。

---