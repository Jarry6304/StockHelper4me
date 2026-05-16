# StockHelper4me — tw-stock-collector

> 台股資料蒐集 + 計算 pipeline。FinMind API → **PostgreSQL 17**,**5 層架構**(Bronze / Silver per-stock / Cross-Stock Cores / M3 Cores / MCP API),Python 3.11+ + Rust workspace(Silver S1 後復權 + M3 Cores 35 crates + Aggregation Layer)。

**版本**:v3.10(alembic head `z5a6b7c8d9e0` / 2026-05-16)
**狀態**:**m2 大重構正式終結** ✅,M3 Cores 35 crates production-ready,Aggregation Layer 4 Phase 全套(spec / lib / dashboard / MCP),Neely Core v1.0.1 P0 Gate 通過,RuleId enum 76 spec variants 全落地,exhaustive compaction 真窮舉,workflow toml dispatch 35 cores

---

## 文件導覽

跨 session 銜接 / Schema 細節 / 動工前必看:

| 文件 | 用途 |
|---|---|
| **`CLAUDE.md`** | v1.X → v3.10 跨 session 歷程紀錄(改 schema / 加 entry 前必看「關鍵架構決策」+ 最新 v3.X 段)|
| **`m2Spec/layered_schema_post_refactor.md`** | Bronze + Silver schema 規範(主要 spec)|
| **`m3Spec/`** | M3 Cores 層 spec(13 份,涵蓋 indicator / pattern / chip / fundamental / environment / neely / agg layer)|
|   `m3Spec/neely_core_architecture.md` | Neely Wave Core(P0)架構,r6(v3.6 RuleId enum 76 variants)|
|   `m3Spec/neely_rules.md` | Neely 25+ 條規則 + Three Rounds + Power Rating 完整對照表 |
|   `m3Spec/cores_overview.md` | 各 Core 共用設計 + 「禁止抽象」原則 + dirty queue 契約 |
|   `m3Spec/aggregation_layer.md` | Aggregation Layer 規格 r3(v3.8 per-timeframe lookback)|
| `docs/api_pipeline_reference.md` | collector.toml entry × Bronze schema × code path 索引 |
| `docs/m3_cores_spec_pending.md` | Cores 落地進度 + 9 阻塞拍版紀錄 |
| `docs/structural_snapshots_partition_observation.md` | v3.9 partition 評估結論(暫不需要)|
| `docs/claude_history.md` | v1.4 → v1.9.1 詳細歷史(主檔已搬出)|

---

## 1. 架構總覽

### 5 層架構(v3.5 R3 後)

| 層 | 內容 | producer / 進入點 |
|---|---|---|
| **Layer 1 Bronze** | FinMind raw(8 張 `*_tw` + 21+ raw,7 個分類 B0~B6)+ Reference 2 表 | `python src/main.py backfill` / `incremental`(`bronze/phase_executor.py`)|
| **Layer 2 Silver per-stock** | 13 個 `*_derived` SQL builder + 4 個 fwd 表(Rust)+ `price_limit_merge_events` | `python src/main.py silver phase 7a/7b/7c`(`silver/orchestrator.py`)|
| **Layer 2.5 Cross-Stock Cores**(v3.5 R3 新)| 跨股 ranking(目前 1 個:`magic_formula_ranked_derived`)| `python src/main.py cross_cores phase 8`(`cross_cores/orchestrator.py`)|
| **Layer 3 M3 Cores** | 35 crates Rust workspace 全市場全核 dispatch(Wave / Indicator / Chip / Fundamental / Environment / System)| `tw_cores run-all --workflow workflows/tw_stock_standard.toml --write` |
| **Layer 4 MCP / API 對外** | Aggregation Layer + Streamlit dashboards 6 tabs + FastMCP server 5 tools | `agg.as_of()` / `dashboards/aggregation.py` / `mcp_server/server.py` |

```
              FinMind / 外部資料
                       │
   ┌── Layer 1 Bronze ──────────────────────────┐
   │   ~28 raw 表(price / events / chip /      │
   │   fundamental / environment)+ Reference    │
   └────────────────┬───────────────────────────┘
                    │  PG triggers(15 個 Bronze→Silver dirty)
                    ▼
   ┌── Layer 2 Silver per-stock ────────────────┐
   │   13 SQL builders(Phase 7a/7b)            │
   │   + Rust S1 後復權 4 fwd 表(Phase 7c)     │
   └────────────────┬───────────────────────────┘
                    │
                    ▼
   ┌── Layer 2.5 Cross-Stock Cores(Phase 8)───┐
   │   magic_formula_ranked_derived(全市場排名)│
   └────────────────┬───────────────────────────┘
                    │
                    ▼
   ┌── Layer 3 M3 Cores(35 crates)────────────┐
   │   1 Wave(neely)+ 8 P1 indicator +         │
   │   8 P3 indicator + 3 P2 pattern +          │
   │   5 chip + 3 fund + 6 environment + 2 v3.4 │
   │   → facts / indicator_values /             │
   │     structural_snapshots(3 張 M3 表)      │
   └────────────────┬───────────────────────────┘
                    │  agg._db.get_connection()(連線 single entry)
                    ▼
   ┌── Layer 4 MCP / API 對外 ──────────────────┐
   │   agg.as_of(stock_id, date) read-only API  │
   │   + 6 dashboards tabs + MCP 5 tools        │
   └────────────────────────────────────────────┘
```

---

## 2. 系統需求

| 項目 | 版本 | 備註 |
|---|---|---|
| Python | 3.11+ | `tomllib` / `asyncio.TaskGroup` |
| **PostgreSQL** | **17+** | JSONB / partial index / generic trigger function |
| Rust / Cargo | 1.75+ | Silver S1 + M3 Cores(`rust_compute/` workspace 35 crates)|
| FinMind 方案 | Backer(1,600 reqs/h) | sponsor tier 為 nice-to-have(`government_bank_buy_sell_v3` 需要)|
| psycopg | `psycopg[binary,pool]>=3.2` | PG 連線 |
| sqlx | `0.7+` | Rust → PG 連線(per-stock concurrent 32)|

---

## 3. 專案結構

```
StockHelper4me/
├── alembic/versions/                # Schema migrations(40+ migrations 至 z5a6b7c8d9e0)
├── config/collector.toml            # 27 個 [[api]] entry(v3.10 移除 5 _legacy)
├── src/
│   ├── main.py                      # CLI(collector / silver / cross_cores / refresh / status / validate)
│   ├── bronze/                      # v3.5 R1 拆解
│   │   ├── phase_executor.py        # Bronze 排程(Phase 1-6,orchestration only)
│   │   ├── segment_runner.py        # 單 segment fetch → transform → upsert
│   │   ├── aggregators/             # pivot/pack 4 個 module
│   │   ├── post_process_dividend.py # dividend → events 拆分
│   │   └── _common.py               # filter_to_trading_days helper
│   ├── silver/                      # v3.5 R2 強化
│   │   ├── orchestrator.py          # Phase 7a/7b/7c 排程
│   │   ├── _common.py               # SilverBuilder Protocol + fetch_bronze + upsert_silver
│   │   └── builders/                # 13 個 per-stock builder
│   ├── cross_cores/                 # v3.5 R3 新層 Layer 2.5
│   │   ├── _base.py                 # CrossStockBuilder Protocol
│   │   ├── orchestrator.py          # Phase 8 排程
│   │   └── magic_formula.py         # Greenblatt 2005 cross-rank
│   ├── agg/                         # v3.8 per-timeframe lookback
│   │   ├── query.py                 # as_of(stock_id, date) → AsOfSnapshot
│   │   ├── _db.py                   # PG single entry(get_connection 等)
│   │   ├── _lookahead.py            # look-ahead bias 過濾
│   │   ├── _market.py               # 5 保留字 stock_id 並排
│   │   └── _types.py                # AsOfSnapshot / QueryMetadata 等
│   ├── api_client.py                # FinMind aiohttp client
│   ├── rate_limiter.py              # token bucket(1600/h, 2250ms, 429 cooldown)
│   ├── field_mapper.py              # API → schema 映射 + detail JSONB pack
│   ├── db.py                        # DBWriter + PostgresWriter
│   ├── rust_bridge.py               # subprocess 派 Rust binary
│   └── schema_pg.sql                # 完整 schema DDL(給 fresh DB init)
├── rust_compute/                    # Rust workspace 35 crates
│   ├── cores_shared/fact_schema/    # Fact + IndicatorCore / WaveCore trait + params_hash
│   ├── cores/wave/neely_core/       # P0 Wave Core(Stage 1-10 完整 + v3.6 RuleId 81 variants + v3.7 真窮舉 compaction)
│   ├── cores/indicator/             # 8 P1 + 8 P3 + 3 P2 pattern + ATR / Bollinger / OBV
│   ├── cores/chip/                  # 5 P2(day_trading / institutional / margin / foreign_holding / shareholder)
│   ├── cores/fundamental/           # 3 P2(revenue / valuation / financial_statement)
│   ├── cores/environment/           # 6 P2(taiex / us_market / exchange_rate / fear_greed / market_margin / business_indicator)
│   ├── cores/system/tw_cores/       # M3 cores monolithic binary(v3.5 R4 拆 8 module + workflow.rs)
│   └── silver_s1_adjustment/        # Silver Phase 7c 後復權(舊 tw_stock_compute binary)
├── workflows/
│   └── tw_stock_standard.toml       # 35 cores workflow(dispatch via tw_cores run-all --workflow)
├── dashboards/aggregation.py        # Streamlit 6 tabs(K-line / Chip / Fund / Env / Neely / Facts)
├── mcp_server/                      # FastMCP stdio server(5 tools 包 agg + dashboards)
├── scripts/                         # verifier / inspect / reverse-pivot 工具
├── docs/
│   ├── api_pipeline_reference.md    # entry × table × code 索引
│   ├── m3_cores_spec_pending.md     # Cores 落地進度 + 9 阻塞拍版紀錄
│   ├── structural_snapshots_partition_observation.md  # v3.9 partition 評估結論
│   └── claude_history.md            # v1.4 → v1.9.1 詳細歷史
├── m2Spec/                          # Bronze / Silver spec(layered_schema_post_refactor)
├── m3Spec/                          # 13 份 M3 Cores spec
└── CLAUDE.md                        # 跨 session 歷程紀錄
```

---

## 4. 快速開始

### 4.1 設定 Postgres + 環境變數

```bash
# 1. 起 PG 17(本機 service or docker)
#    docker compose up -d  或  Windows: postgresql-x64-17

# 2. .env 填 FINMIND_TOKEN + DATABASE_URL
cp .env.example .env
# DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock
# FINMIND_TOKEN=your_finmind_token

# 3. pip install + alembic upgrade head 落 schema
pip install -e .                      # editable install,src/silver / src/bronze / src/agg 全部 importable
alembic upgrade head                  # → z5a6b7c8d9e0(v3.10 R6 DROP _legacy_v2 後)
```

### 4.2 編 Rust workspace(雙 binary)

```bash
cd rust_compute
cargo build --release -p tw_stock_compute    # Silver S1 後復權(Phase 7c)
cargo build --release -p tw_cores            # M3 cores monolithic binary
cd ..
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

### 4.5 Cross-Stock Cores(Phase 8,v3.5 R3)

```bash
python src/main.py cross_cores phase 8                            # 全跑(目前只有 magic_formula)
python src/main.py cross_cores phase 8 --builder magic_formula
```

### 4.6 M3 Cores production run(v1.29 PR-9b 並行 + v3.9 workflow toml)

```bash
# Stage 1: dry-run smoke(~30 秒)
./rust_compute/target/release/tw_cores run-all --limit 5

# Stage 2: 全市場 1263 stocks × 35 cores(預估 ~9 分鐘 @ concurrency=32)
./rust_compute/target/release/tw_cores run-all \
    --workflow workflows/tw_stock_standard.toml \
    --write \
    --concurrency 32

# 只跑 dirty queue(對齊 silver/orchestrator dirty pattern)
./rust_compute/target/release/tw_cores run-all --dirty --write
```

### 4.7 一鍵 refresh(v1.35.1)

```bash
python src/main.py refresh                              # Bronze incremental → Silver 7c/7a/7b → Cross-Stock 8 → M3 cores
python src/main.py refresh --stocks 2330                # 限縮股票
python src/main.py refresh --skip-cores                 # 跳過 M3 cores(無 Rust binary 時)
```

### 4.8 Aggregation Layer 查詢(v3.8 per-timeframe lookback)

```python
from datetime import date
from agg.query import as_of

# 對齊 spec §4.2 預設 lookback:
#   daily 90 / monthly 90(3 publication cycles)/ quarterly 180(2 quarters)
snap = as_of("2330", date(2026, 5, 16))
print(snap.facts)              # daily / weekly / monthly / quarterly facts 各自 cutoff
print(snap.indicator_latest)   # 每 (core, timeframe) 最新一筆
print(snap.structural)         # neely / pattern cores 結構快照
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
  cross_cores phase 8   Layer 2.5 Cross-Stock Cores
  refresh               一鍵 chain: bronze → silver 7c → 7a → 7b → cross_cores 8 → M3 cores
  status                api_sync_progress 5-status 摘要
  validate              collector.toml 格式檢查

tw_cores binary(rust_compute/target/release/tw_cores):
  list-cores                            列出已連結 35 cores
  run --stock-id <id> [--write]         單股 neely_core 完整 Pipeline
  run-all [--write]                     全市場 × 全 cores production run
    --workflow <path>                   Workflow toml 動態 dispatch
    --concurrency <N>                   per-stock 並行度(default 32)
    --dirty                             只跑 is_dirty=TRUE stocks
    --limit <N>                         限制前 N 檔(test)

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
| Silver derived(per-stock)| 13 | 12 `*_derived` + 4 fwd 表 + `price_limit_merge_events`(Rust)|
| Cross-Stock Cores(Layer 2.5)| 1 | `magic_formula_ranked_derived`(v3.5 R3 新層,從 Silver 搬出)|
| M3 Cores(Layer 3)| 3 | `facts` / `indicator_values` / `structural_snapshots`(35 cores 寫入)|
| 退場候選(spec §7.3)| 5 | `institutional_daily` / `margin_daily` / `foreign_holding` / `day_trading` / `valuation_daily`(對齊 verify_pr19b)|
| 系統 | 3 | `schema_metadata` / `stock_sync_status` / `api_sync_progress` |
| **總計** | **~55 張** | v3.10 R6 後 3 張 `_legacy_v2` 永久 DROP |

---

## 7. 主要 PR 里程碑

### v3.2 r1(2026-05 初)

```
#17 Rust schema_version + AF 拆 multiplier 對齊
#18 5 張 reverse-pivot Bronze raw + 全市場驗證
#19a/b/c Silver 14 表 schema + 13 builder 實作
#20 15 個 Bronze→Silver dirty trigger ENABLE
#21-A/B 衍生欄補完 + Bronze 3 條
#22 TAIEX/TPEx daily OHLCV
```

### m2 大重構 R1~R6(2026-05-09 → 2026-05-16)

```
#25 R1 source 欄補回 3 張 _tw
#26 R2 v2.0 舊 3 表 rename _legacy_v2
#28 R3 _tw Bronze 升格主名(去 _tw 後綴)
#29 R4 v2.0 entry name 加 _legacy 後綴
#65 R6 永久 DROP 3 張 _legacy_v2(2026-05-16,m2 大重構終結 ✅)
```

### M3 Cores 動工 → production(2026-05)

```
M3 PR-1 → PR-9a(22 cores + Aggregation Layer 4 Phase + neely v1.0.x)
PR #48 spec alignment(neely r5)
PR #50 Aggregation Layer Phase B-D 全套
PR #51 Round 5/6 calibration(11 cores per-EventKind ≤ 12/yr)
```

### v3.5 → v3.10(2026-05-16,本批 5 PR)

```
PR #59 / #60 v3.5 5 層架構單一職責歸位(9 commits 拆 module + 4 cores trait 對齊)
PR #61 v3.6 Neely RuleId enum 28 → 81 variants(全 76 spec variants 落地)
PR #62 v3.7 spec_pending doc cleanup + exhaustive compaction 真窮舉(Round 1-2 遞迴 aggregation)
PR #63 v3.8 agg per-timeframe lookback(daily 90 / monthly 90 / quarterly 180)
PR #64 v3.9 partition observation(暫不需要)+ workflow toml audit(已落地)
PR #65 v3.10 R6 永久 DROP 3 張 _legacy_v2(m2 大重構終結 ✅,alembic head z5a6b7c8d9e0)
```

---

## 8. 測試

```bash
# Python tests(agg + silver + cross_cores)
pytest tests/agg/                       # 39 passed / 1 skipped(pandas 未裝)
pytest tests/                           # 全套 unit test

# Rust workspace tests(35 crates / 420 passed / 0 failed)
cd rust_compute && cargo test --release --workspace
```

push 前必跑 verifier:
```bash
python scripts/verify_pr18_bronze.py        # 5 張 Bronze 反推 round-trip
python scripts/verify_pr19b_silver.py       # 5 個簡單 builder 對 v2.0 legacy 等值
python scripts/verify_pr19c_silver.py       # 5 個 market-level builder
python scripts/verify_pr20_triggers.py      # 15 個 trigger 整合測試
```

---

## 9. License

MIT
