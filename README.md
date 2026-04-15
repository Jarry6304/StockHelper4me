# StockHelper4me — tw-stock-collector

> 台股資料蒐集程式，透過 FinMind API 抓取歷史與即時資料，存入本地 SQLite，供波浪分析引擎使用。

---

## 系統需求

| 項目 | 版本 |
|------|------|
| Python | 3.11+ |
| Rust / Cargo | 1.75+（Phase 4 計算層） |
| SQLite | 3.35+（支援 UPSERT） |
| FinMind 方案 | Backer（1,600 次/小時） |

---

## 專案結構

```
StockHelper4me/
├── config/
│   ├── collector.toml          # 主設定檔（API registry、排程、rate limit）
│   └── stock_list.toml         # 股票清單（可手動增減）
├── src/
│   ├── main.py                 # CLI 進入點
│   ├── logger_setup.py         # 日誌初始化
│   ├── config_loader.py        # TOML 解析 + 驗證
│   ├── rate_limiter.py         # Token Bucket 流量控制
│   ├── phase_executor.py       # Phase 1-6 排程引擎
│   ├── api_client.py           # FinMind HTTP client
│   ├── date_segmenter.py       # 歷史回補日期分段
│   ├── stock_resolver.py       # 股票清單解析
│   ├── db.py                   # SQLite 連線管理 + UPSERT
│   ├── field_mapper.py         # 欄位映射 + detail JSON 打包
│   ├── sync_tracker.py         # 斷點續傳追蹤
│   ├── aggregators.py          # Phase 5-6 聚合策略（法人 pivot、財報 pack）
│   ├── post_process.py         # 除權息後處理 + 現增事件偵測
│   └── rust_bridge.py          # 呼叫 Rust binary（Phase 4）
├── rust_compute/               # Rust binary 專案
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # 後復權 + 週K/月K 聚合
├── collectorSpec/              # 程式規格文件
│   ├── tw_stock_collector_program_spec_v1.2_p1.md
│   ├── tw_stock_collector_program_spec_v1.2_p2.md
│   └── tw_stock_collector_program_spec_v1.2_p3.md
├── data/
│   └── tw_stock.db             # SQLite 資料庫（gitignore）
└── logs/
    └── collector_YYYYMMDD.log  # 每日日誌（gitignore）
```

---

## 快速開始

### 1. 安裝 Python 依賴

```bash
pip install aiohttp tomllib
```

### 2. 設定 FinMind Token

```bash
export FINMIND_TOKEN="your_token_here"
```

或直接寫入 `config/collector.toml`（見下方說明）。

### 3. 編譯 Rust binary（Phase 4）

```bash
cd rust_compute
cargo build --release
cd ..
```

### 4. 設定 collector.toml

將 `config/collector.toml` 中的 `token` 欄位填入你的 FinMind API Token：

```toml
[global]
token = "your_finmind_token"
```

### 5. 執行全量歷史回補

```bash
# 完整回補（約 4.5 天）
python src/main.py backfill

# 只跑 Phase 1-4（取得可分析的基礎資料，約 20 小時）
python src/main.py backfill --phases 1,2,3,4

# 開發測試：只跑特定股票
python src/main.py backfill --stocks 2330,2317,2442
```

### 6. 日常增量更新

```bash
python src/main.py incremental
```

---

## 執行流程（6 個 Phase）

```
Phase 1  META          →  stock_info、trading_calendar、market_index_tw
Phase 2  EVENTS        →  price_adjustment_events（除權息、減資、分割、面額變更、現增）
Phase 3  RAW PRICE     →  price_daily、price_limit
Phase 4  RUST 計算      →  price_daily_fwd、price_weekly_fwd、price_monthly_fwd（後復權）
Phase 5  CHIP/FUND     →  三大法人、融資融券、財報、月營收...
Phase 6  MACRO         →  SPY/VIX、匯率、恐懼貪婪指數
```

> **Phase 1-4 完成後**，即可開始波浪分析引擎開發。Phase 5-6 為輔助資料，優先度較低。

---

## CLI 指令

```
python src/main.py <command> [options]

commands:
  backfill        全量歷史回補
  incremental     增量同步（日常排程用）
  phase <N>       只跑指定 Phase
  status          顯示同步進度摘要
  validate        驗證 config 格式

options:
  --config <path>       指定 collector.toml 路徑（預設 config/collector.toml）
  --stock-list <path>   指定 stock_list.toml 路徑（預設 config/stock_list.toml）
  --stocks <id1,id2>    覆蓋股票清單（開發用）
  --phases <1,2,3>      只跑指定 Phase
  --dry-run             只印出計劃，不實際呼叫 API
  --verbose             DEBUG 級別日誌
```

---

## 開發進度

| Phase | 狀態 | 主要模組 / 說明 |
|-------|------|----------------|
| A — 基礎骨架 | ✅ 完成 | `logger_setup`, `config_loader`, `db`（17 張表）, `rate_limiter`, `api_client` |
| B — Phase 1 排程通路 | ✅ 完成 | `phase_executor`, `field_mapper`（schema 驗證 + 衍生欄位）, `stock_resolver` |
| C — Phase 2-3 斷點續傳 | ✅ 完成 | `date_segmenter`（年度分段）, `sync_tracker`（5 種狀態）, `post_process`（除權息拆分 + 現增）|
| D — Phase 4 Rust 計算層 | ✅ 完成 | `rust_bridge`（SIGTERM/SIGKILL）, `rust_compute`（後復權 + 週K/月K） |
| E — Phase 5-6 籌碼 & 總經 | ✅ 完成 | `aggregators`（三大法人 pivot、財報 pack）, 28 個 API 設定補全 |
| 缺漏修正 | ✅ 完成 | `DateSegmenter` 補傳 `sync_tracker`；`cooldown_on_429_sec` 從 config 讀取；`updated_at` 使用 Python 真實時間；`schema_mismatch` 寫入 `sync_tracker` |

---

## 資料庫 Schema（SQLite）

| 資料表 | 說明 | PK |
|--------|------|----|
| `stock_info` | 股票基本資料（名稱、市場、產業、上市日） | market, stock_id |
| `trading_calendar` | 交易日曆 | market, date |
| `market_index_tw` | 台灣加權報酬指數 | market, date |
| `price_adjustment_events` | 除權息 / 減資 / 分割 / 面額變更 / 現增 | market, stock_id, date, event_type |
| `price_daily` | 日K 原始收盤價 | market, stock_id, date |
| `price_limit` | 漲跌停價格 | market, stock_id, date |
| `price_daily_fwd` | 後復權日K（Rust Phase 4 產出） | market, stock_id, date |
| `price_weekly_fwd` | 後復權週K | market, stock_id, year, week |
| `price_monthly_fwd` | 後復權月K | market, stock_id, year, month |
| `institutional_daily` | 三大法人買賣超（外資/投信/自營商） | market, stock_id, date |
| `margin_daily` | 融資融券餘額 | market, stock_id, date |
| `foreign_holding` | 外資持股比例 | market, stock_id, date |
| `holding_shares_per` | 股權分散表（detail JSON 各級距） | market, stock_id, date |
| `valuation_daily` | 本益比 / 殖利率 / 淨值比 | market, stock_id, date |
| `day_trading` | 當沖買賣量 | market, stock_id, date |
| `index_weight_daily` | 指數成分權重 | market, stock_id, date |
| `monthly_revenue` | 月營收 + YoY / MoM | market, stock_id, date |
| `financial_statement` | 損益表 / 資負表 / 現金流量（detail JSON） | market, stock_id, date, type |
| `market_index_us` | SPY / VIX 美股指數 | market, stock_id, date |
| `exchange_rate` | 台幣匯率（spot_buy 為主欄位） | market, date, currency |
| `institutional_market_daily` | 全市場三大法人 | market, date |
| `market_margin_maintenance` | 整體融資維持率 | market, date |
| `fear_greed_index` | CNN 恐懼貪婪指數 | market, date |
| `api_sync_progress` | 斷點續傳進度（per segment） | api_name, stock_id, segment_start |
| `stock_sync_status` | 每支股票同步時間戳 + fwd_adj_valid | market, stock_id |

---

## 呼叫量估算（初次全量回補）

| Phase | 估算呼叫次數 |
|-------|-------------|
| Phase 1 | ~30 次 |
| Phase 2 | ~7,500 次 |
| Phase 3 | ~25,200 次 |
| Phase 5 | ~138,600 次 |
| Phase 6 | ~40 次 |
| **合計** | **~171,370 次** |

以 1,600 次/小時計算：**約 107 小時（4.5 天）**

---

## 規格文件

詳細實作規格見 `collectorSpec/` 目錄：

- `tw_stock_collector_program_spec_v1.2_p1.md` — 架構總覽、Config Schema、股票清單
- `tw_stock_collector_program_spec_v1.2_p2.md` — Rate Limiter、Phase Executor、API Client、Field Mapper、Rust Bridge
- `tw_stock_collector_program_spec_v1.2_p3.md` — Sync Tracker、Post-Process、Logging、CLI 介面

---

## License

MIT
