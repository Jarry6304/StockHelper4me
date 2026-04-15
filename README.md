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

| Phase | 狀態 | 模組 |
|-------|------|------|
| A — 基礎骨架 | ✅ 完成 | logger_setup, config_loader, db, rate_limiter, api_client |
| B — Phase 1 | ✅ 完成 | phase_executor, field_mapper, stock_resolver |
| C — Phase 2-3 | ✅ 完成 | date_segmenter, sync_tracker, computed_fields, post_process |
| D — Phase 4 Rust | ✅ 完成 | rust_bridge, rust_compute |
| E — Phase 5-6 | 待實作 | 法人籌碼、基本面 API |

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
