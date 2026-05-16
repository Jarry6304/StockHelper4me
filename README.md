# StockHelper4me — tw-stock-collector

> 台股資料蒐集 + 計算 pipeline。FinMind API → **PostgreSQL 17**,採 **4 層 Medallion** 架構(Reference / Bronze / Silver / M3 預留),Python 3.11+ + Rust 後復權計算層。

**版本**:v3.2 r1(alembic head `v1w2x3y4z5a6` / 2026-05-09)
**狀態**:m2 大重構 + nice-to-haves + Bronze data 質量修 全部收尾;R5 觀察期 21~60 天啟動

---

## 文件導覽

跨 session 銜接 / Schema 細節 / 動工前必看:

| 文件 | 用途 |
|---|---|
| **`m2Spec/layered_schema_post_refactor.md`** | Bronze + Silver schema 規範(主要 spec)|
| **`docs/api_pipeline_reference.md`** | collector.toml entry × Bronze schema × code path 索引(配套本檔)|
| **`CLAUDE.md`** | v1.X 跨 session 歷程紀錄(改 schema / 加 entry 前必看 v1.27 + 「關鍵架構決策」)|
| `m2Spec/data_refactor_plan.md` | R1~R6 重構 plan |
| `m2Spec/oldm2Spec/cores_overview.md` 等 | 各 Core 計算規格(M3 動工 reference)|
| `m3Spec/` | M3 / Cores 層 spec(動工中,首份 `chip_cores.md`)|
| `collectorSpec/` | v1.2 collector 程式架構規格(歷史)|

---

## 1. 架構總覽

### 4 層 Medallion(spec v3.2 r1)

| 層 | 內容 | producer / 進入點 |
|---|---|---|
| **Reference** | `trading_date_ref` / `stock_info_ref` | collector(`bronze/phase_executor`) |
| **Bronze**(原始)| 21+ FinMind raw 表(7 個分類 B0~B6)| collector backfill / incremental |
| **Silver**(進階計算)| 14 個 `*_derived` 表 — 12 SQL builder + 4 Rust fwd + price_limit_merge | orchestrator(`silver/orchestrator`)+ Rust binary |
| **M3 / Cores**(預留)| chip / fundamental / environment / indicator / wave cores | `m3Spec/`(spec 寫中,code 未動工) |

```
                 FinMind / 外部資料
                       │  (HTTP)
                       ▼
       ┌─── Bronze ──────────────────────────┐
       │  21+ raw 表(B0_calendar / B1_meta /│
       │  B2_events / B3_price_raw / B4_chip /│
       │  B5_fundamental / B6_environment)   │
       └────────────────┬────────────────────┘
                        │  (PG trigger)
                        ▼
       ┌─── Silver(進階計算)────────────────┐
       │  S1_adjustment(Rust fwd 4 表)     │
       │  S4_chip / S5_fundamental /          │
       │  S6_environment(SQL builders)        │
       └────────────────┬────────────────────┘
                        │  (PyO3, 未動工)
                        ▼
                 M3 / Cores layer
```

詳細 entry × table × code 對映見 `docs/api_pipeline_reference.md`。

---

## 2. 系統需求

| 項目 | 版本 | 備註 |
|---|---|---|
| Python | 3.11+ | `tomllib` / `asyncio.TaskGroup` |
| **PostgreSQL** | **17+** | JSONB / partial index / generic trigger function |
| Rust / Cargo | 1.75+ | Phase 4 後復權計算層(`rust_compute/`) |
| FinMind 方案 | Backer(1,600 reqs/h) | sponsor tier 為 nice-to-have(`government_bank_buy_sell_v3` 需要)|
| psycopg | `psycopg[binary,pool]>=3.2` | PG 連線 |

⚠️ v2.0 後從 SQLite 遷移到 PG 17;舊版 README 提到的 SQLite path 已棄用。

---

## 3. 專案結構

```
StockHelper4me/
├── alembic/                          # Schema migration(2019_~ 2026_)
│   ├── env.py / alembic.ini
│   └── versions/                     # 30+ migrations 包括 v3.2 r1 / R1~R4 / v1.26 / v1.27
├── config/
│   ├── collector.toml                # 39 個 [[api]] entry(38 enabled)+ rate limit
│   └── stock_list.toml               # dev mode 股票清單
├── src/
│   ├── main.py                       # CLI(collector / silver / cross_cores / status / validate)
│   ├── bronze/                          # v3.5 R1 拆解
│   │   ├── phase_executor.py         # Bronze 排程(Phase 1-6,orchestration only)+ Rust 7c 派工
│   │   ├── segment_runner.py         # 單 segment fetch → transform → upsert(v3.5 R1 C3 抽)
│   │   ├── aggregators/              # pivot/pack 4 個(v3.5 R1 C2 從 aggregators.py 拆 package)
│   │   ├── post_process_dividend.py  # dividend_policy → events 拆分(v3.5 R1 C1 從 src/post_process.py 搬)
│   │   └── _common.py                # filter_to_trading_days helper
│   ├── silver/
│   │   ├── orchestrator.py           # Silver 排程(7a/7b/7c)
│   │   ├── _common.py                # fetch_bronze / upsert_silver / get_trading_dates
│   │   └── builders/                 # 13 個 per-stock builder(v3.5 R3 後 magic_formula 搬走)
│   ├── cross_cores/                    # v3.5 R3 新層:Layer 2.5 Cross-Stock Cores
│   │   ├── _base.py                  # CrossStockBuilder Protocol
│   │   ├── orchestrator.py           # Phase 8 排程
│   │   └── magic_formula.py          # Greenblatt 2005 cross-rank(從 silver/builders/ 搬)
│   ├── api_client.py                 # FinMind aiohttp v4 client + rate limit
│   ├── rate_limiter.py               # token bucket(1600/h, 2250ms, 429 cooldown 120s)
│   ├── sync_tracker.py               # api_sync_progress 5-status 斷點續傳
│   ├── date_segmenter.py             # backfill 段切割
│   ├── field_mapper.py               # API → schema 映射 + detail JSONB pack
│   ├── db.py                         # DBWriter + PostgresWriter
│   ├── rust_bridge.py                # subprocess 派 Rust binary
│   ├── stock_resolver.py             # stock 清單解析
│   └── schema_pg.sql                 # 完整 schema DDL(給 fresh DB init)
├── rust_compute/                     # Rust binary 專案
│   ├── Cargo.toml                    # workspace virtual root
│   └── cores/system/tw_cores/src/    # M3 cores monolithic binary(v3.5 R4 C8 拆 8 module)
│       ├── main.rs                   # entrypoint + run-all
│       ├── cli.rs                    # Cli + Command struct
│       ├── dispatcher.rs             # dispatch_indicator/structural/neely
│       ├── writers.rs                # PG IO helpers
│       ├── run_environment.rs        # 6 environment cores
│       ├── run_stock_cores.rs        # 17 stock-level cores
│       ├── summary.rs                # CoreRunSummary + print_summary
│       └── helpers.rs                # parse_timeframe + extract_indicator_meta
├── scripts/                          # verifier / inspect / reverse-pivot 工具
├── docs/
│   └── api_pipeline_reference.md     # entry × table × code 索引(配套本檔)
├── m2Spec/                           # 主要 spec(layered_schema_post_refactor + data_refactor_plan)
│   └── oldm2Spec/                    # 各 Core 規格(M3 reference)
├── m3Spec/                           # M3 / Cores 層 spec(動工中)
│   ├── .gitkeep
│   └── chip_cores.md                 # chip cores spec
├── collectorSpec/                    # v1.2 collector 架構規格(歷史)
└── CLAUDE.md                         # v1.X 跨 session 歷程紀錄
```

---

## 4. 快速開始

### 4.1 設定 Postgres + 環境變數

```bash
# 1. 起 PG 17(本機 service or docker)
#    docker compose up -d 或 Windows: postgresql-x64-17

# 2. .env 填 FINMIND_TOKEN + DATABASE_URL
cp .env.example .env
# DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock
# FINMIND_TOKEN=your_finmind_token

# 3. pip install + alembic upgrade head 落 schema(包含 v1.27 trigger)
pip install -e .                      # editable install:src/silver / src/bronze / src/ 全部 importable
alembic upgrade head                  # → v1w2x3y4z5a6
```

### 4.2 編 Rust binary

```bash
cd rust_compute && cargo build --release && cd ..
```

### 4.3 Bronze 全量回補(估 ~107h @ 1600 reqs/h)

```bash
python src/main.py backfill                              # 全 phase
python src/main.py backfill --phases 1,2,3,4             # 只到 Phase 4(可分析的最小集)
python src/main.py backfill --stocks 2330,2317           # 開發測試:覆蓋股票清單
```

### 4.4 Silver 進階計算

```bash
# 7c 必須先跑(產 fwd 4 表)
python src/main.py silver phase 7c [--stocks ...] [--full-rebuild]

# 7a 12 個獨立 builder(讀 fwd.volume 算 day_trading_ratio 等)
python src/main.py silver phase 7a [--stocks ...] [--full-rebuild]

# 7b financial_statement(跨表依賴 monthly_revenue)
python src/main.py silver phase 7b [--stocks ...] [--full-rebuild]
```

### 4.5 日常 incremental(含 v1.26 dirty queue skip)

```bash
python src/main.py incremental                           # 全 phase
python src/main.py incremental --phases 4                # Phase 4 dirty queue 為空 → skip Rust
python src/main.py incremental --phases 5 --stocks 2330  # 部分股
```

---

## 5. CLI 指令

```
python src/main.py <command> [options]

commands:
  backfill              全量歷史回補(Phase 1-6 + Rust)
  incremental           增量同步(日常排程)
  phase <N>             只跑指定 Phase(0-6)
  silver phase <X>      Silver 計算層(7a / 7b / 7c)
  status                api_sync_progress 5-status 摘要
  validate              collector.toml 格式檢查

options:
  --config <path>       指定 collector.toml 路徑
  --stocks <id1,id2>    覆蓋股票清單(開發用)
  --phases <1,2,3>      只跑指定 Phase(collector 用)
  --full-rebuild        Silver 忽略 dirty queue 全部重算
  --dry-run             只印計劃,不呼叫 API
  --verbose             DEBUG 級別日誌
```

---

## 6. Schema 概覽

完整詳見 `docs/api_pipeline_reference.md` 與 `m2Spec/layered_schema_post_refactor.md`。

| 層 | 數量 | 例 |
|---|---|---|
| Reference | 2 | `trading_date_ref` / `stock_info_ref` |
| Bronze raw | ~28 | `price_daily` / `price_adjustment_events` / `institutional_investors_tw` / `financial_statement`(R3 主名)/ `monthly_revenue` |
| Silver derived | 14 | 12 `*_derived`(SQL builder)+ 4 fwd 表(Rust)+ `price_limit_merge_events` |
| 觀察期 legacy_v2 | 3 | R5 觀察 21~60 天,R6 後 DROP |
| 退場候選(spec §7.3)| 6 | `institutional_daily` / `margin_daily` / `foreign_holding` / `day_trading` / `valuation_daily` / `index_weight_daily` |
| 系統 | 3 | `schema_metadata` / `stock_sync_status` / `api_sync_progress` |
| **總計** | **~56 張**(觀察期 + 退場候選全退後 → ~47 張)| — |

---

## 7. 主要 PR 里程碑(2026-05 m2 重構 + nice-to-haves + 質量修)

```
v3.2 r1(2026-05-初):
  #17 Rust schema_version + AF 拆 multiplier 對齊
  #18 5 張 PR #18 reverse-pivot Bronze raw + 全市場驗證
  #19a/b/c Silver 14 表 schema + 13 builder 實作
  #20  18 個 Bronze→Silver dirty trigger ENABLE
  #21-A/B 衍生欄補完 + Bronze 3 條
  #22  TAIEX/TPEx daily OHLCV
  #21  deprecated path 全砍

m2 大重構(2026-05-09):
  #25 R1 source 欄補回 3 張 _tw
  #26 R2 v2.0 舊 3 表 rename _legacy_v2
  #27 hotfix sync_tracker fromisoformat
  #28 R3 _tw Bronze 升格主名(去 _tw 後綴)
  #29 R4 v2.0 entry name 加 _legacy 後綴
  #30 v1.26 nice-to-haves(Rust dirty queue / Phase 4 incremental skip / margin UNION 等 5 項)
  #32 docs api_pipeline_reference 屬性分層版
  #33 m3Spec/ 資料夾預留
  #34 docs 對齊 m2Spec/layered_schema_post_refactor.md + 補 Cores 接點

v1.27 質量修(2026-05-09):
  #35 day_trading builder 改 LEFT JOIN price_daily_fwd(spec §6.4 + chip_cores §7.2)
  #36 pae dedup par_value_change + split 16 對 + 防衛 trigger
```

---

## 8. License

MIT
