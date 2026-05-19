# StockHelper4me — tw-stock-collector

> 台股資料蒐集 + 計算 pipeline。FinMind API → **PostgreSQL 17**,**5 層架構**(Bronze / Silver per-stock / Cross-Stock Cores / M3 Cores / MCP API),Python 3.11+ + Rust workspace(Silver S1 後復權 + M3 Cores 39 crates + Aggregation Layer + Cross-Stock Cores 11 builders + MCP toolkit 8 tools)。

**版本**:**v4.7**(alembic head `d9e0f1g2h3i4` 不變 / 2026-05-19,v4.5 G2 + v4.6 G3 + **v4.7 G1 完整收尾 — M3SPEC 闕漏補完 8 sub-PR 全部 production-verified**)
**測試流水線**:`scripts/test_pipeline.ps1`(Windows) / `scripts/test_pipeline.sh`(Unix)5 phase 流水線:Environment check / Sandbox unit tests / Schema health / Production verify / MCP smoke test。完整 verify chain 見 [CLAUDE.md §下班後 verify 流水線](CLAUDE.md)
**狀態**:**M3SPEC 闕漏補完 8 sub-PR 完整收尾 + P0 Gate verified** ☕ — Group 1 polywave 嵌套依賴鏈(2-pass Pre-Constructive + Compaction-aware polywave_size + Pattern Isolation validation + Channeling touch epsilon + Post-validator Stage 2)全部 production-verified;全市場 1266 stocks P0 Gate 全綠(max forest_size=196 / p95=28 / overflow=0 / wall time 648s)。**v4.0 P1.1-P1.4 + v4.5 G2 + v4.6 G3 + v4.7 G1** 共 **17 commits / ~7,300 LoC**。M3 Cores **39 crates** production-ready;Cross-Stock Cores **11 builders**;Aggregation Layer 4 Phase 全套;**Rust workspace 567 tests passed / 0 failed**(v4.4 baseline 528 → +39);**Python tests 165+ passed**;1266 stocks × 36 cores / wall time ~11 min / facts ~5.2M(VACUUM 後)

---

## 文件導覽

跨 session 銜接 / Schema 細節 / 動工前必看:

| 文件 | 用途 |
|---|---|
| **`CLAUDE.md`** | v1.35 → **v4.4** 跨 session 歷程紀錄(改 schema / 加 entry 前必看「關鍵架構決策」+ 最新 v4.X 段;v1.5 ~ v1.34 已歸檔 `docs/claude_history.md`)|
| **`m2Spec/layered_schema_post_refactor.md`** | Bronze + Silver schema 規範(主要 spec)|
| **`m3Spec/`** | M3 Cores 層 spec(13 份,涵蓋 indicator / pattern / chip / fundamental / environment / neely / agg layer)|
|   `m3Spec/neely_core_architecture.md` | Neely Wave Core(P0)架構,r6(v3.6 RuleId enum 76 variants)|
|   `m3Spec/neely_rules.md` | Neely 25+ 條規則 + Three Rounds + Power Rating 完整對照表 |
|   `m3Spec/cores_overview.md` | 各 Core 共用設計 + 「禁止抽象」原則 + dirty queue 契約 |
|   `m3Spec/aggregation_layer.md` | Aggregation Layer 規格 r3(v3.8 per-timeframe lookback)|
| `docs/api_pipeline_reference.md` | collector.toml entry × Bronze schema × code path 索引 |
| `docs/m3_cores_spec_pending.md` | Cores 落地進度 + 9 阻塞拍版紀錄 |
| `docs/structural_snapshots_partition_observation.md` | v3.9 partition 評估結論(暫不需要)|
| `docs/claude_history.md` | v1.4 → v1.34 詳細歷史(主檔已搬出 m2 PR sequencing + M3 PR-1~9a + P1/P2/P3 cores batch + Round 1-4 calibration + Aggregation Layer 落地)|

---

## 1. 架構總覽

### 5 層架構(v3.5 R3 後)

| 層 | 內容 | producer / 進入點 |
|---|---|---|
| **Layer 1 Bronze** | FinMind raw(8 張 `*_tw` + 21+ raw,7 個分類 B0~B6)+ Reference 2 表 | `python src/main.py backfill` / `incremental`(`bronze/phase_executor.py`)|
| **Layer 2 Silver per-stock** | 13 個 `*_derived` SQL builder + 4 個 fwd 表(Rust)+ `price_limit_merge_events` | `python src/main.py silver phase 7a/7b/7c`(`silver/orchestrator.py`)|
| **Layer 2.5 Cross-Stock Cores**(v3.5 R3 新)| 跨股 ranking(目前 1 個:`magic_formula_ranked_derived`)| `python src/main.py cross_cores phase 8`(`cross_cores/orchestrator.py`)|
| **Layer 3 M3 Cores** | **39 crates** Rust workspace 全市場全核 dispatch(Wave / Indicator / Chip 8 / Fundamental / Environment 7 / System)| `tw_cores run-all --workflow workflows/tw_stock_standard.toml --write` |
| **Layer 4 MCP / API 對外** | Aggregation Layer + Streamlit dashboards 6 tabs + FastMCP server **8 public tools**(v3.31 4 個 + v3.32 4 個 cross-stock factor screens:monthly / quarterly / annual_low_risk / monthly_trigger) | `agg.as_of()` / `dashboards/aggregation.py` / `mcp_server/server.py` / `mcp_server/_screens.py` |

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
   │   + 6 dashboards tabs + MCP 8 tools        │
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
├── alembic/versions/                # Schema migrations(40+ migrations 至 c8d9e0f1g2h3)
├── config/collector.toml            # 39 個 [[api]] entry(v3.20 加 5 sponsor datasets;v3.23 price_limit all_market)
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
│   ├── cores/chip/                  # 8 P2(day_trading / institutional / margin / foreign_holding / shareholder + v3.21:loan_collateral / block_trade / risk_alert)
│   ├── cores/fundamental/           # 3 P2(revenue / valuation / financial_statement)
│   ├── cores/environment/           # 7 P2(taiex / us_market / exchange_rate / fear_greed / market_margin / business_indicator + v3.21:commodity_macro)
│   ├── cores/system/tw_cores/       # M3 cores monolithic binary(v3.5 R4 拆 8 module + workflow.rs)
│   └── silver_s1_adjustment/        # Silver Phase 7c 後復權(舊 tw_stock_compute binary)
├── workflows/
│   └── tw_stock_standard.toml       # 35 cores workflow(dispatch via tw_cores run-all --workflow)
├── workflows/tw_stock_standard.toml # 39 cores workflow(v3.21 +4 entries)
├── dashboards/aggregation.py        # Streamlit 6 tabs(K-line / Chip / Fund / Env / Neely / Facts)
├── mcp_server/                      # FastMCP stdio server(v3.32:8 public tools = v3.31 4 + v3.32 4 cross-stock factor screens)
│   ├── _loan_collateral / _block_trade / _risk_alert / _commodity_macro  # v3.22 加 4 helper modules
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
alembic upgrade head                  # → c8d9e0f1g2h3(v3.21 加 3 張 Silver derived:loan_collateral / block_trade / commodity_macro)
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
| Bronze raw | ~28 | `price_daily` / `price_adjustment_events` / `institutional_investors_tw` / `financial_statement`(R3 主名)/ `monthly_revenue` / `government_bank_buy_sell_tw`(v3.14 加 bank_name 維度)|
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

### v3.5 → v3.10(2026-05-16)

```
PR #59 / #60 v3.5 5 層架構單一職責歸位(9 commits 拆 module + 4 cores trait 對齊)
PR #61 v3.6 Neely RuleId enum 28 → 81 variants(全 76 spec variants 落地)
PR #62 v3.7 spec_pending doc cleanup + exhaustive compaction 真窮舉(Round 1-2 遞迴 aggregation)
PR #63 v3.8 agg per-timeframe lookback(daily 90 / monthly 90 / quarterly 180)
PR #64 v3.9 partition observation(暫不需要)+ workflow toml audit(已落地)
PR #65 v3.10 R6 永久 DROP 3 張 _legacy_v2(m2 大重構終結 ✅,alembic head z5a6b7c8d9e0)
```

### v3.11 → v3.18(2026-05-17,Round 7 + Round 8 calibration)

```
v3.11 Round 7 calibration 5 cores tighten(adx/atr/day_trading/margin/trendline)
      + trendline_core O(N²) → O(N log N) perf 優化
v3.12-v3.14 gov_bank pipeline 收尾:
      - probe FinMind sponsor tier(`scripts/probe_finmind_sponsor_unused.py`)
      - 新 param_mode `all_market_no_end`(gov_bank 不接 data_id / end_date)
      - alembic a6b7c8d9e0f1:gov_bank Bronze 加 bank_name 維度
      - Bronze 13.39M rows / 8 行庫 / 2021-06-30 → 2026-05-15
      - institutional builder UNION fix(gov_bank-only dates 也填 stub)
      - Silver gov_bank_net fill 80.74%
      - Round 7 verify SQL 3 sections / 5 cores 0 row 達標 ✅
v3.15-v3.18 Round 8 calibration 四輪(sp=10 → 5 → 3 → 2,LargeTransaction accepted):
      - foreign_holding milestone 4 variants 一致 ~65% retention(cluster=1.9 收斂)
      - LargeTransaction 14.16/yr accepted baseline(fat-tail Lo 2001)
      - production verify 4/4 milestone in band:
        Low 10.06 / High 7.90 / LowAnn 5.10 / HighAnn 3.74
```

### v3.19 → v3.24(2026-05-17,5 datasets + 4 new cores + Round 9)

```
v3.19 gov_bank_core spec proposal(等 EventKind 拍版)+ probe audit 工具就緒
      + facts 表 stats 維護 SQL(scripts/maintain_facts_stats.sql)
v3.20 5 sponsor-tier datasets 接入 Bronze:
      - loan_collateral_balance_tw(35 cols 細項,5 大類 × 7 sub-fields)
      - block_trade_tw(PK 加 trade_type 維度)
      - market_value_daily(個股市值)
      - disposition_securities_period_tw(處置股 all_market mode)
      - commodity_price_daily(初版 GOLD,first_per_day aggregator)
      collector.toml 34 → 39 entries / alembic b7c8d9e0f1g2
v3.21 4 cores 拍版 + Rust 全套上線(alembic c8d9e0f1g2h3 + Silver 3 new builders):
      - loan_collateral_core(11 EventKind:5 類 Surge/Crash + Concentration)
      - block_trade_core(4 EventKind:LargeBlock / Acc / Dist / MatchingSpike)
      - risk_alert_core(4 EventKind + 三級嚴重度 measure 中文 parser)
      - commodity_macro_core(4 EventKind:Spike / Momentum / RegimeBreak)
      - workflows toml +4 entries / chip_loader +3 / env_loader +1
v3.22 B-5 MCP toolkit 從 5 → 9 public tools:
      - loan_collateral_snapshot / block_trade_summary
      - risk_alert_status / commodity_macro_snapshot
      - tests +15 new(toolkit_v3 從 9 → 24 cases)
v3.23 price_limit per_stock → all_market perf hotfix:
      - 14 min → 0.65 秒(420× incremental 加速,FinMind 1 req 回 2745 stocks)
      - segment_days=1 避開 FinMind multi-day range quirk
v3.24 Round 9 calibration + commodity_macro builder fix:
      - LoanCategoryConcentration level → edge trigger
        (對齊 v3.16 institutional r3 Brown & Warner 1985)
      - production verify:125.69/yr → ~1.16/yr ✅(events -82%,facts_new -99%)
      - commodity_macro Silver builder order_by fix(market-level Bronze pattern)
      - 4 new cores 全部 production-ready,觸發率合理
      - wall time 806s → 738s(-8.5%)
v3.25 market_context() 整合 commodity_macro / risk_alert(8 components)
v3.26 MCP current_price bug fix:3 helper 直讀 price_daily,不依賴 indicator_latest
v3.27 MCP toolkit metadata.kind 統一修(event_kind 優先 + kind fallback)
v3.28 neely wave_count regex parse + scenario / indicator staleness surface
v3.29 risk_alert _parse_severity 加 處置 / 注意 broad pattern + condition kwarg
v3.30 kalman series-last-entry 修(production smoothed=0 silent bug)
      + 6 個 render tools 暫隱藏(PNG 後端 silent fail)
v3.31 MCP toolkit 9 → 4 consolidation:
      - neely_forecast / kalman_trend / magic_formula_screen 不動
      - 新 stock_snapshot(stock_id, date)合併 6 個基本資料 tool
      - 6 個被合併的 helper 仍 callable from Python(dashboard 用)
      - 新 verify pipeline:scripts/verify_mcp_kalman_neely.py + .sql
      - tests 117 → 125 passed
v3.32 10 new cross_cores factor builders + 4 MCP toolkit screens(2026-05-18):
      Cross-Stock Cores 從 1 → 11 builders;對齊量化因子提案 v1.1
      - Toolkit A 月度:persistent_momentum + revenue_momentum
        + institutional_concert + vol-managed overlay
      - Toolkit B 季度:f_score(Piotroski ≥ 7)+ low_volatility 252d
        + industry_adj_gp(Novy-Marx 2013 + industry median)
      - Toolkit C 年度:long_term_low_vol 36M + dividend_yield(yield trap filter)
        + mom_12_1
      - Layer 5:monthly_trigger(positive/negative conviction adjustment)
      - alembic head c8d9e0f1g2h3 → d9e0f1g2h3i4(10 張 ranked + 1 signals)
      - MCP toolkit 4 → 8 public tools
      - tests 125 → 165 passed(+23 cross_cores + 17 screens)
```

### v3.33 → v3.38(2026-05-18 Kalman multi-horizon + Neely multi-timeframe)

```
v3.33 Kalman multi-horizon output:
      - Rust kalman_filter_core 重構 KalmanFilterParams 為 4 horizons
        (short Q=1e-1 / medium Q=1e-2 / long Q=1e-3 / ultra_long Q=1e-5)
      - facts(EventKind transition)只從 primary horizon("medium")產
      - MCP kalman_trend 加 kalman_by_horizon + cross_horizon_consistency
v3.34 Kalman polish:short threshold 0.005 → 0.003;deviation_sigma 1% smoothed floor
v3.35 Neely-C-MCP picker:invalidation filter + degree-aware ordering
      (Advisory Layer 處理,不動 Rust Three Rounds — 對齊 NEoWave 原作精神)
v3.35.1 quality_caveat:short-degree only + fib decoupled warnings
v3.36 load_for_neely 6 yr floor hotfix(原 600 bars / 2.4 yr → 6 yr 對齊其他 cores)
v3.37 Multi-timeframe Neely:Daily/Weekly/Monthly 三 timeframe 並行
      cross-timeframe picker(Aggregation Layer 跨 tf primary 選擇)
v3.37.1 hotfix:load_weekly/load_monthly SQL — use (year, week/month), not date
v3.38 Per-forecast-horizon Neely:1m/3m/6m(drop 1y)
      + degradation strategy(full / degree_uncertain / no_6m / insufficient_history)
      + spec-aligned missing_wave tier(pattern-specific min monowave table)
      + monthly lookback 144 → 60
```

### v4.0 → v4.4(2026-05-19 Neely Core M3SPEC alignment 完整收尾,9 commits)

```
v4.0 Plan:對 m3Spec/neely_core_architecture.md r6 + neely_rules.md 全篇對照 54 個
     .rs files,揭露 15 真闕漏 + 2 簡化降級。User 拍版全做 P1.1 → P1.4
     (~5,500 LoC / +80 tests / Ch11 Advisory mode 對齊 NEoWave 原作精神)。

v4.1 P1.1 Quick Wins(34f73e2):
     - StructuralFacts 加 extension_subdivision_pair 欄位(Ch8 Independent Rule)
     - AlternationFact 升 5-axis(Price/Time/Severity/Intricacy/Construction)
     - OverlapPattern 升 enum(Trending/Terminal/None with evidence)
     - ChannelingFact / TimeRelationship 加 evidence
     - 新 fifth_of_fifth_detector.rs(Appendix A.3 共通 fn 抽提)
     - FlatKind::IrregularStrongB(Appendix B 項 A 123.6% 中間檻)

v4.2 P1.2 Ch9 advisory + Ch12 Waterfall/Localized(aa64e5a):
     - Ch9 Independent Rule advisory(多 chapter 規則互不干涉)
     - Ch9 Simultaneous Occurrence(Impulse 預期 R1-R7 全 passed)
     - Ch9 Exception Aspect 2 dispatch(2-4 線突破觸發 Terminal Impulse)
     - Ch9 Exception Aspect 1 Multiwave 結尾分支補完
     - Ch12 Waterfall Effect ±5%(W3/W1 或 W5/max > 2.668)
     - Ch12 Localized Progress Label Changes

v4.3 P1.3 Ch11 wave-by-wave 全變體(5 sub-PRs):
     - P1.3a Trending Impulse(77ab3d7):1st/3rd/5th Ext + Failure + Wave-4 共通
     - P1.3b Terminal Impulse(21fb732):4 變體 + 與 Trending 差異(W2 寬鬆 / :3 結構)
     - P1.3c Flat 7 變體(8a91392):B-Failure/C-Failure/Common/Double Failure/
       Elongated/Irregular(+StrongB)/Irregular Failure;FlatVariant 加 DoubleFailure
     - P1.3d Zigzag(3a592f9):wave-a/b/c + Appendix B 項 F(Triangle 內 c 例外)
     - P1.3e Triangle 9 變體(2c663b4):Horizontal/Irregular/Running ×
       Limiting/NonLimiting/Expanding 9 variants

v4.4a P1.4a Ch4 真 magnitude + Round 2 動作 B(1af97a0):
     - scenario_price_magnitude 從 wave_tree.children.len() placeholder 升為
       真實 monowave price lookup(find_price_at_date helper)
     - compact() chain signature 加 monowaves: &[Monowave] 參數
     - Round 2 動作 B 邊界波 retracement Rules 重評 advisory(spec line 1249-1251)

v4.4 P1.4b+c+d Ch8 X-wave + Multiwave + Ch6 Stage 2(40d1996):
     - ch8_xwave/mod.rs:Combination 偵測 Large/Small X-wave(Table A vs B)
     - ch8_multiwave/mod.rs:Triple* 末段 / Double* 中段
     - Ch6 Combination Stage 2 advisory(後續走勢確認須結合 Ch8 module)
     - Ch6 RunningCorrection Stage 2 advisory(後續 Impulse > 161.8%)

Advisory mode 設計(對齊 NEoWave 原作 Ch11 = pattern characteristic 非 invariant):
- 違反規則 → 寫 AdvisoryFinding(Warning/Info/Strong)+ RuleId metadata
- 不 invalidate scenario / 不縮 forest size / production scenario forest 0 影響
- LLM 看 narrative 取信號;V4.x 可開 feature flag 升 hard reject

P0 Gate 校準(user 本機 P1.4 後必跑):
- forest_size 分布:max ≤ 200(cap),p95 < 180
- 3030 / 2330 / 1101 manual review:primary degree 不退化
- 若 p95 > 180 → 重校 BeamSearchFallback.k
```

### v4.5 → v4.6(2026-05-19 M3SPEC 闕漏補完 Group 2 + Group 3,5 commits)

```
背景:v4.0 → v4.4 收尾後仍有 15 真闕漏(報告 §)分組:
  - Group 2(corrective triggers / emulation,4 sub-PR / ~1,360 LoC)
  - Group 3(OHLC reference 串接,1 sub-PR / ~300 LoC)
  - Group 1(Polywave 嵌套依賴鏈,3 sub-PR / ~1,800 LoC,需全市場 P0)

v4.5 Group 2 corrective patterns 4 sub-PR(5439c2a → 460e235):
  v4.5.1 Zigzag triggers + ZigzagAsFlatFailure emulation
       - triggers/mod.rs:wave-b 不可完全回測 wave-a → InvalidateScenario
         wave-c 不過 wave-b 端點 → WeakenScenario
       - emulation/mod.rs:wave-c < 100% × wave-a → 似 Flat C-Failure
       - EmulationKind +1 variant(ZigzagAsFlatFailure)
  v4.5.2 Flat triggers + FlatAsZigzag emulation
       - triggers:wave-c 不過 wave-a 起點 + Expanded Flat wave-b 端點突破
       - emulation:wave-c ≥ 138.2% × wave-a (Elongated) → 似 Zigzag
       - EmulationKind +1 variant(FlatAsZigzag)
       - flat_variant_from_kind helper(FlatKind 8 → FlatVariant 10 mapping)
  v4.5.3 Triangle triggers(Contracting/Limiting wave-e)
       - Contracting/Limiting wave-e 突破 wave-c 端點 → InvalidateScenario
         (Expanding 本 PR 暫不加,屬未來細化)
       - triangle_variant_default helper
  v4.5.4 Combination + RunningCorrection triggers + CombinationAsImpulse emulation
       - Combination 末段反向破 wave-a 起點 → InvalidateScenario
       - RunningCorrection 同款 → InvalidateScenario
       - DoubleThree* / TripleThree* + 5/7 children → 似 Trending Impulse
       - EmulationKind +1 variant(CombinationAsImpulse)
       - Match arm 變 exhaustive(移除 catch-all `_ => {}`,覆蓋全 7 variants)

v4.6 Group 3 G3.1 Monowave bar_indices + m1_endpoint_broken_by_m2(cc053d6):
  - Monowave struct +`bar_indices: (usize, usize)` + #[serde(default)]
    對應 start_date/end_date 在 bars slice 的 index 區間
  - monowave/pure_close.rs detect_monowaves:populate 真實 (start_idx, extreme_idx)
  - monowave/mod.rs classify_monowaves:override 對齊 caller bars slice
  - MonowaveContext +`bars: &'a [OhlcvBar]` 欄位
  - pre_constructive/predicates.rs::m1_endpoint_broken_by_m2 真實實作:
    m1.direction Up → m2 期間 bar.high > m1.end_price → true
    m1.direction Down → m2 期間 bar.low < m1.end_price → true
    退化保險(empty bars / 越界 / (0,0))→ false
  - pre_constructive/rule_4.rs caller:傳 ctx.bars
  - lib.rs:pre_constructive::run(&mut classified, &input.bars)
  - 38 sites bulk-update test fixtures 加 bar_indices: (0, 0)
  - 7 new predicates tests

驗證:
- cargo test --release -p neely_core --lib: 376 passed / 0 failed
- cargo test --release --workspace: 549 passed / 0 failed(v4.4 baseline 528 → +21)
```

### v4.7 Group 1 — Polywave 嵌套依賴鏈(2026-05-19,3 sub-PR / 14 new tests / P0 Gate verified)

```
v4.7.1 G1.1 Compaction Level-0 真實 price + Pattern Isolation validation(3e69a8f):
  - compaction/three_rounds.rs::scenario_price_magnitude:
    返回型別 f64 → Option<f64>;移除 children.len() fallback
    similarity_and_balance 改用 match (Option, Option) 處理 None case
  - compaction/three_rounds.rs::build_round_advisories 同步更新 Option pattern
  - pattern_isolation/mod.rs::validate_after_compaction (新 pub fn):
    walk Compaction forest,匹配 PatternBound 邊界與 Scenario.wave_tree
    對齊的 PatternBound 設 validated=true(spec §Pattern Isolation Step 5)
  - lib.rs:Stage 8 Compaction 跑完後接 validate_after_compaction
  - 5 new tests(compaction Option<f64> behavior + pattern_isolation validation)

v4.7.2 G1.2 Pre-Constructive polywave 2-pass + Power Rating in_triangle docs(190cd75):
  - ClassifiedMonowave +`polywave_size: usize`(default 0)
  - pre_constructive/predicates.rs +`POLYWAVE_THRESHOLD: usize = 3`
    +`is_polywave(m: &ClassifiedMonowave) -> bool` helper
  - pre_constructive/mod.rs +`populate_polywave_sizes(classified, forest)` fn:
    walk Level-N+ scenarios → covered base monowaves 取 max(N children)
  - 5 rules placeholder false → is_polywave 真實判定:
    rule_1.rs Branch 3(m0 polywave)/ rule_4.rs Branch 3 + Branch 6
    / rule_5.rs cond_5a + cond_5b dispatcher / rule_6.rs cond_6a + Branch 5
    / rule_7.rs cond_7a dispatcher
  - lib.rs:Stage 8 Compaction 後 populate_polywave_sizes + 2nd pass pre_constructive::run
    對齊 plan §G1.2「2-pass forward design」
  - power_rating/table.rs in_triangle 註解更新(已在 v3.x Phase 8 落地)
  - 32 sites bulk-update test fixtures 加 polywave_size: 0
  - 5 new tests(populate_polywave_sizes + is_polywave threshold)

v4.7.3 G1.3 Channeling touch + Post-validator Stage 2 + Classifier nested(ea0ef7c):
  - advanced_rules/channeling.rs Zigzag c-wave 0-B trendline 加 epsilon 容忍:
    TOUCH_EPSILON_PCT = 0.005;ternary severity:
    touched(Strong = Triangle 形成訊號)/ breached(Warning)/ clear(Info)
  - post_validator/mod.rs Combination + RunningCorrection 改 advisory → 真實 Stage 2:
    Combination 取 sub_kinds 細分 + 整合 Ch8_XWave/Multiwave advisory_findings
    RunningCorrection 結合 power_rating 生成 continuation 強度 narrative
  - classifier/mod.rs::classify_complexity:wave_count → ComplexityLevel
    完整對映;退化 0/1/2/4 落 Simple(原 catch-all Complex 為誤判)
  - 4 new tests(classify_complexity 完整對映)

P0 Gate verified(2026-05-19):
- forest_size 分布:max=196 / p95=28 / p50=14 / overflow=0
  (acceptance:max ≤ 200 ✅ / p95 < 180 ✅ / overflow < 5% ✅)
- 1266 stocks 全綠 wall time 648.5s
- MCP smoke(2330/3030/1101)Kalman + Neely 全 [OK]

cargo test --release --workspace: 567 passed / 0 failed
  (v4.4 baseline 528 → +39 across G2+G3+G1 八個 sub-PR)

整體 Group 2 + Group 3 + Group 1 收尾總計:8 sub-PR / 8 commits + 1 docs commit
+39 new tests / ~2,650 LoC across 9 commits
```

---

## 8. 測試流水線

### 8.1 一鍵測試流水線(推薦,v4.4 新)

`scripts/test_pipeline.ps1`(Windows) / `scripts/test_pipeline.sh`(Unix)— 5 phase 完整測試:

```powershell
# Windows / PowerShell
.\scripts\test_pipeline.ps1                       # 跑全套 Phase 0-4
.\scripts\test_pipeline.ps1 -OnlyPhase '0,1'      # 只跑 sandbox(無 PG;多 phase 用引號)
.\scripts\test_pipeline.ps1 -SkipPhase '3'        # 跳過 production verify(P0 Gate)
.\scripts\test_pipeline.ps1 -DryRun               # 列計畫不執行
# 多 phase 用引號 '2,3,4' 避免 PS 5.1 array binding 不穩;支援逗號/空白/分號分隔
```

```bash
# Unix / Bash
./scripts/test_pipeline.sh                            # 全套
ONLY_PHASES="1"   ./scripts/test_pipeline.sh          # 只 sandbox
SKIP_PHASES="3 4" ./scripts/test_pipeline.sh          # 跳 PG-heavy phase
DRY_RUN=1         ./scripts/test_pipeline.sh          # 計畫模式
```

| Phase | 用途 | 需 PG | 預估時間 |
|---|---|---|---|
| **0** Environment check | Python venv / Rust toolchain / .env / psql / tw_cores binary | ❌ | 數秒 |
| **1** Sandbox unit tests | Rust workspace(567 tests,v4.7 後)+ Python pytest agg/mcp/cross_cores(165+) | ❌ | ~5-8 分鐘 |
| **2** Schema health | alembic head + M3 表 row counts + 11 cross_cores tables | ✅ | < 5 秒 |
| **3** Production verify | facts stats(VACUUM)+ per-EventKind rate + **Neely forest_size P0 Gate**(v4.4a acceptance:max ≤ 200,p95 < 180) | ✅ | ~1 分鐘 |
| **4** MCP smoke test | `verify_mcp_kalman_neely.py` 對 2330 / 3030 + 8 toolkit 公開介面 importable | ✅ | ~30 秒 |

退碼:0 = 全綠 / 1 = 任一 phase fail。

### 8.2 個別 test 指令(進階,需精確控制時用)

```bash
# Python tests(agg + silver + cross_cores + mcp_server)
pytest tests/agg/                       # 39 passed / 1 skipped(pandas 未裝)
pytest tests/mcp_server/ --ignore=tests/mcp_server/test_render_tools.py   # 100+ passed
pytest tests/cross_cores/               # 30+ passed
pytest tests/                           # 全套 unit test

# Rust workspace tests(39 crates / 567 passed / 0 failed,v4.7 後)
cd rust_compute && cargo test --release --workspace --no-fail-fast
```

push 前必跑 verifier(已整合進 Phase 1):
```bash
python scripts/verify_pr18_bronze.py        # 5 張 Bronze 反推 round-trip
python scripts/verify_pr19b_silver.py       # 5 個簡單 builder 對 v2.0 legacy 等值
python scripts/verify_pr19c_silver.py       # 5 個 market-level builder
python scripts/verify_pr20_triggers.py      # 15 個 trigger 整合測試
```

production calibration verify(已整合進 Phase 3):
```bash
psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql       # ANALYZE + VACUUM stats
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql     # per-EventKind ≤ 12/yr
# Section 1: per-stock cores ≤ 12 events/stock/year
# Section 2: market-level cores events/year(distinct_stocks ≤ 5)
# Section 3: Round 7 verify(adx/atr/day_trading/margin/trendline)— 0 row = 達標
# Section 4: foreign_holding milestone 4 variants(Round 8.3 verify)
```

P0 Gate forest_size verify(v4.4a 後必跑,已整合進 Phase 3):
```bash
psql $env:DATABASE_URL -c "
SELECT
  PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p95,
  MAX(jsonb_array_length(snapshot->'scenario_forest')) AS max_count
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
"
# Acceptance:max ≤ 200(cap 不破),p95 < 180
# 若 max > 200 → 重校 rust_compute/cores/wave/neely_core/src/compaction/beam_search.rs::BeamSearchFallback.k
```

---

## 9. License

MIT
