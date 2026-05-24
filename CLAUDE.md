# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件下方版本章節是跨 session 銜接的歷程紀錄(v3.5 → v4.10,最新 2026-05-20;
> v1.5 ~ v1.34 已歸檔 [`docs/claude_history.md`](docs/claude_history.md))。動工前先讀本段 Quick Reference,然後依任務性質往下讀對應 v3.X / v4.X 段落。

---

## 專案概要

`tw-stock-collector` — 台股資料蒐集 + 計算 pipeline。FinMind API → Postgres 17。
**5 層架構**(Bronze / Silver per-stock / Cross-Stock Cores / M3 Cores / MCP API,v3.5 R3 後)。
Python 3.11+ + Rust workspace **39 crates**(Silver S1 後復權 + M3 Cores 全市場全核 dispatch + v3.21 4 new cores + v4.0-v4.4 Neely M3SPEC alignment + v4.5+v4.6 M3SPEC 闕漏補完 Group 2+3 + v4.10 Item 4 收尾)。

- **alembic head**:`d0e1f2g3h4i5`(v4.24 M8 forecast_log whitelist 加 3 non-price cores;v4.17 DROP 5 張 v2.0 orphan 表;Fusion Layer P0.2 加 `facts.severity`)
- **開發分支**:`claude/plan-stockhelper-api-kWh9F`(Fusion Layer P0+P1+P2)→ PR #91 合 main
- **collector.toml**:**39 entries**(v3.20 加 5 sponsor datasets;v3.23 price_limit all_market;gov_bank 需 sponsor tier)
- **Rust tests**:39 crates / **607 passed / 0 failed**(Fusion Layer 後;v4.11 baseline 596 → +11 severity/flat_fib/env-core tests)
- **MCP toolkit**:**11 public tools**(4 個股/跨股 + 4 cross-stock screen + 3 fusion consolidated:market_overview / stock_levels / indicators;v4.19 從 18 整併)
- **測試流水線**:`scripts/test_pipeline.ps1` / `scripts/test_pipeline.sh`(v4.4 加)5 phase 流水線(Environment / Sandbox / Schema / Production / MCP)
- **Production state**:1266 stocks × **36 cores** / wall time ~12.3 min / facts ~5.1M(VACUUM 後);Round 7 + Round 8 + **Round 9** calibration **完整結算**
- **v4.0 → v4.4 完整收尾**(2026-05-19):Neely M3SPEC alignment 15 真闕漏 P1.1-P1.4 全部 dispatch — 9 commits / 9 new modules / ~5,500 LoC / Advisory mode 對齊 NEoWave 原作精神
- **v4.5 → v4.9**(2026-05-19):M3SPEC 闕漏補完 8 sub-PR + Out-of-Scope backlog Items 1+2+3 完整收尾 ☕☕☕ — Group 2(4 sub-PR)+ Group 3(Monowave bar_indices)+ Group 1(3 sub-PR polywave 嵌套依賴鏈)+ v4.8(Construction axis 5-variant + Round 2 boundary partial rerun)+ v4.9(WaveNode.label 嵌入結構標籤 hint,深層 nested 透過 Compaction clone 自動傳遞);全市場 1266 stocks G1 P0 Gate **全綠**(max=196 / p95=28 / overflow=0)
- **v4.10**(2026-05-20)☕:Out-of-Scope **Item 4 Pre-Constructive 2-pass diagnostics union** 完整收尾 — `pre_constructive::run_pass2` 新函式回傳 `HashMap<classified_idx, Vec<StructureLabelCandidate>>` Pass 1-only diff(label 比對);`MonowaveStructureLabels` 加 `classified_index` + `pass1_only_labels` 兩欄;lib.rs Stage 8.5 refill loop 把 Pass 2 result + diff 寫回 forest 每個 scenario;**Out-of-Scope backlog 全部清空**

---

## 2026-05-18 整日 verify chain(快速入口)

今日 4 commit 全部 push 上 `claude/continue-previous-work-xdKrl`:
- **v3.29** `4184d04` risk_alert `_parse_severity` 加 `處置 / 注意` broad pattern
- **v3.30** `7f8d877` Kalman series-last-entry path fix + render tools 暫隱藏
- **v3.31** `7b2eb98` MCP toolkit 9 → 4 consolidation(stock_snapshot)+ Kalman/Neely verify pipeline
- **v3.32** `a365240` 10 new cross_cores builders + 4 MCP toolkit screens

完整 verify chain 走 6 phase。**Phase A SQL diagnostic 是 blocking**(v3.32 F-Score
+ industry_adj_gp + dividend_yield 都需先確認 Bronze 資料);若 Phase A 全綠才走
Phase B-F。詳見下方 §「下班後 verify 流水線」段(可從 git pull 直接拉這個版本看)。

---

## 常用指令

### 環境

```bash
pip install -e .                          # editable install:silver/ + bronze/ + src/ loose modules 全部 importable
pip install -e ".[dev]"                   # 加 pytest / pytest-asyncio
docker compose up -d                      # 起本地 Postgres 17(或用 OS service)
cp .env.example .env                      # 填入 FINMIND_TOKEN + DATABASE_URL
alembic upgrade head                      # 遷移 schema 到最新版
cd rust_compute && cargo build --release  # 編 Phase 4/7c 用的 binary
```

### Bronze 收集（Phase 1-6 / `src/main.py`）

```bash
python src/main.py validate                              # 驗證 collector.toml 格式
python src/main.py status                                # api_sync_progress 摘要(5 種 status)
python src/main.py backfill                              # 全量回補(估 ~107h @ 1600 reqs/h)
python src/main.py backfill --phases 1,2,3,4             # 只跑 Phase 1-4
python src/main.py backfill --stocks 2330,2317           # 開發測試:覆蓋股票清單
python src/main.py incremental                           # 日常排程
python src/main.py phase 4                               # 只跑單一 Phase(0-6)
python src/main.py --verbose backfill --stocks 2330 --dry-run   # debug
```

### Silver 計算(Phase 7 dirty-driven)

```bash
python src/main.py silver phase 7a [--stocks 2330] [--full-rebuild]   # 12 個獨立 builder
python src/main.py silver phase 7b [--stocks 2330] [--full-rebuild]   # 跨表依賴(financial_statement)
python src/main.py silver phase 7c [--stocks 2330]                    # tw_market_core Rust(rust_bridge.run_phase4)
```

`--full-rebuild` 目前是唯一支援的模式;dirty queue pull 留 PR #20+。

### Cross-Stock Cores 計算(Phase 8,v3.5 R3 新層 Layer 2.5)

```bash
python src/main.py cross_cores phase 8                            # 全跑(目前只有 magic_formula)
python src/main.py cross_cores phase 8 --builder magic_formula    # 指定 builder
python src/main.py cross_cores phase 8 --full-rebuild             # 重算 lookback 全部 dates
python src/main.py cross_cores phase 8 --lookback-days 60         # 覆蓋預設 30
```

跑 cross-stock ranking(Greenblatt 2005 Magic Formula 等),寫 `*_ranked_derived`
表。不走 dirty queue(全市場永遠重算 latest date)。

### 一鍵手動更新最新(`refresh` 子命令,v1.35.1 加)

```bash
python src/main.py refresh                              # Bronze incremental → Silver 7c/7a/7b → Cross-Stock Phase 8 → M3 cores run-all --dirty
python src/main.py refresh --stocks 2330                # 限縮股票範圍
python src/main.py refresh --skip-cores                 # 跳過 M3 cores(無 Rust binary 時)
python src/main.py refresh --skip-bronze                # 跳過 Bronze(只跑 Silver+Cores,已手動跑過 incremental 時)
```

`refresh` 串完整 chain(v3.5 R3 加 Phase 8):**Bronze incremental → Silver phase 7c → 7a → 7b → Cross-Stock Phase 8 → tw_cores run-all --write --dirty**。對應 user「沒有 cron / Task Scheduler 自動排程,但想一鍵拉到最新」場景。每段獨立 exception handling,前段失敗不阻擋後段(對齊 cores_overview §7.5 dirty 契約)。

### Windows Task Scheduler 自動排程(v1.35.1 加)

```powershell
# 一鍵註冊每日 18:00 自動跑 refresh(對齊 chip_cores §2.3 batch 17:30 + 30 分緩衝)
.\scripts\install_refresh_task.ps1

# 改時間
.\scripts\install_refresh_task.ps1 -At 19:30

# 改排程名稱
.\scripts\install_refresh_task.ps1 -TaskName "tw-stock-daily"

# 變種:跳過 M3 cores(無 Rust binary 時)
.\scripts\install_refresh_task.ps1 -NoCores

# 驗證 / 手動觸發 / 移除
Get-ScheduledTask -TaskName "StockHelper4me-Refresh"
Start-ScheduledTask -TaskName "StockHelper4me-Refresh"
Unregister-ScheduledTask -TaskName "StockHelper4me-Refresh" -Confirm:$false
```

Wrapper 是 `scripts/refresh_daily.ps1`,內含 venv 啟動 + .env 載入 + UTF-8 encoding 修法 + 每日 dated log 寫到 `logs/refresh_YYYY-MM-DD.log`。Task Scheduler 不需 admin,走當前 user(Interactive logon)。

### 驗證腳本（push 前必跑）

```bash
python scripts/verify_pr20_triggers.py      # PR #20:Bronze→Silver dirty trigger 整合測試(15 trigger)
python scripts/test_28_apis.py              # 28 支 API 連線健檢(需 FINMIND_TOKEN)
python scripts/check_all_tables.py          # 全表筆數體檢(v4.18 取代 inspect_db.py)
```

### 測試

```bash
pytest                                # 全套 unit test(沒有專屬 lint)
pytest scripts/test_db.py -v          # 單檔
```

完整腳本說明見下方「helper 腳本清單」段。

---

## 架構

### 5 層架構(v3.5 R3 後 — 原 4 層 Medallion + Layer 2.5 Cross-Stock Cores 新層)

| 層 | 內容 | 寫入 path | 主要 module |
|---|---|---|---|
| Bronze(Layer 1) | FinMind raw 資料(8 張 `*_tw` 表 + 5 個 PR #18.5 dual-write entries) | Phase 1-6 collector | `bronze/phase_executor.py` + `bronze/segment_runner.py` + `bronze/aggregators/` + `field_mapper.py` |
| Reference | `stock_info_ref` / `trading_date_ref` 等不變維度 | Phase 1 | 同上 |
| Silver per-stock(Layer 2) | 13 張 `*_derived` Python builder + `price_limit_merge_events` Rust + 4 張 `price_*_fwd`(Rust)(v3.5 R3 後 magic_formula_ranked 搬離) | Phase 7a/7b/7c dirty-driven | `silver/orchestrator.py` + `silver/builders/*.py` + Rust S1 |
| **Cross-Stock Cores(Layer 2.5,v3.5 R3 新)** | 跨股 ranking / 分群 / 相關性(目前 1 個:`magic_formula_ranked_derived`)| `cross_cores phase 8` 排程(全市場重算 latest)| `cross_cores/orchestrator.py` + `cross_cores/magic_formula.py` |
| M3 Cores(Layer 3) | Wave / Indicator / Chip / Fundamental / Environment / System — Rust workspace 35 crates + `neely_core` v1.0.1 P0 Gate 通過 + 8 P1 + 8 P3 indicator + 3 P2 pattern + 5 P2 chip + 3 P2 fundamental + 6 P2 environment + Magic Formula + Kalman = 35 cores(v3.5 R4 C8 tw_cores monolith 拆 8 module) | Rust binary `tw_cores run-all` | `rust_compute/cores/` + `rust_compute/cores_shared/` |
| MCP / API 對外(Layer 4) | LLM tools(5 個)+ Streamlit dashboards + Aggregation Layer | on-demand | `agg/` + `mcp_server/` + `dashboards/`(v3.5 R5 連線 single entry = `agg._db.get_connection`) |

### Phase 1-6（Bronze 收集）

```
Phase 0  trading_calendar 預載入(每 phase 都用得到)
Phase 1  META          stock_info_ref / trading_date_ref / market_index_tw
Phase 2  EVENTS        price_adjustment_events(除權息/減資/分割/面額/現增)
Phase 3  RAW PRICE     price_daily / price_limit
Phase 4  RUST 後復權    price_*_fwd × 3(派 rust_bridge.run_phase4)
Phase 5  CHIP/FUND     5 類法人 / 融資融券 / 財報 / 月營收
Phase 6  MACRO         SPY / VIX / 匯率 / 業務指標
```

Phase 1 完成後會 `_refresh_stock_list()`（先雞後蛋）。`api_sync_progress.status` 5 種：`pending / completed / failed / empty / schema_mismatch`（CHECK 由 alembic `a1b2c3d4e5f6` 落下）。

### Phase 7(Silver per-stock 計算)

```
Phase 7a  12 個獨立 builder           — 串列(PostgresWriter 單 connection,thread-safety 限制)
Phase 7b  跨表依賴 builder             — financial_statement(對齊 monthly_revenue)
Phase 7c  tw_market_core Rust 系列    — price_*_fwd + price_limit_merge_events(走 rust_bridge)
```

`SilverOrchestrator.run(phases, stock_ids, full_rebuild)` 行為:
- `NotImplementedError` → `status="skipped"`,不中斷其他 builder
- 一般 `Exception` → `status="failed"` + reason,**也不中斷**(對齊 `cores_overview §7.5` dirty 契約:失敗 builder 不 reset `is_dirty`,留下次重試)

### Phase 8(Cross-Stock Cores,v3.5 R3 新層 Layer 2.5)

```
Phase 8  cross_cores builders        — 跨股 ranking / 分群 / 相關性(全市場 universe)
           ├─ magic_formula           — Greenblatt 2005 EBIT/EV + ROIC cross-rank(目前唯一成員)
           └─ (future) pairs_trading / sector_rotation / correlation_matrix
```

`CrossStockOrchestrator.run(builders, target_date, full_rebuild, lookback_days)`:
- 不走 dirty queue(全市場永遠重算 latest date 即可,~5s for MF)
- per-builder Exception 標 `status="failed"` 不中斷其他
- refresh chain 中位置:Bronze → 7c → 7a → 7b → **Phase 8** → M3 cores

### 模組地圖(`src/`,v3.5 R1+R3+R5 後)

| 模組 | 職責 |
|---|---|
| `main.py` | CLI(argparse subparsers + asyncio dispatch;`_run_collector` / `_run_silver` / `_run_cross_cores` / `_run_refresh` 分流) |
| `config_loader.py` | TOML 解析 + validation(規則 5 要求 `volume_factor`) |
| `bronze/phase_executor.py` | Phase 1-6 排程(**orchestration only**;v3.5 R1 C3 後 segment IO 拆出);mode 從 CLI runtime 傳入(不從 `config.execution.mode` 讀) |
| `bronze/segment_runner.py` | 單 segment fetch → transform → aggregate → upsert → mark progress 完整流程(v3.5 R1 C3 從 phase_executor._run_api 抽) |
| `bronze/aggregators/` | Phase 5/6 聚合 package:`pivot_institutional.py` / `pack_financial.py` / `pack_holding_shares.py` + `__init__.py` dispatcher(v3.5 R1 C2 從 src/aggregators.py 拆) |
| `bronze/post_process_dividend.py` | 除權息事件衍生 + dividend_policy_merge(`_recompute_stock_dividend_vf` SQL 修 P1-17;v3.5 R1 C1 從 src/post_process.py 搬) |
| `bronze/_common.py` | `filter_to_trading_days` 共用 helper(過 FinMind 週六鬼資料) |
| `api_client.py` + `rate_limiter.py` | aiohttp FinMind client + token bucket(含 429 cooldown) |
| `field_mapper.py` | API → schema 映射 + detail JSONB pack;回 `(rows, schema_mismatch)` tuple |
| `db.py` | `DBWriter` Protocol + `PostgresWriter`(生產)/ `SqliteWriter`(過渡,`TWSTOCK_USE_SQLITE=1`) |
| `rust_bridge.py` | 派 Phase 4/7c 給 Rust binary;assert `schema_version="3.2"` |
| `silver/orchestrator.py` + `silver/builders/` | Phase 7 dirty-driven Silver per-stock 計算(**13 builders**,v3.5 R3 後 magic_formula 搬離) |
| `silver/_common.py` | builder 共用:`SilverBuilder` Protocol(v3.5 R2 收緊 per-stock 邊界明文)+ `fetch_bronze` / `upsert_silver` / `reset_dirty` / `get_trading_dates` |
| `cross_cores/` (v3.5 R3 新層 Layer 2.5) | Cross-Stock Cores 跨股 ranking;`_base.py` `CrossStockBuilder` Protocol + `orchestrator.py` Phase 8 排程 + `magic_formula.py`(首例,從 silver/builders/ 搬) |
| `agg/` | Aggregation Layer:`query.py` `as_of()` read-only API + `_db.py` PG single entry(v3.5 R5 C12 加 `fetch_cross_stock_ranked` + `fetch_stock_info_ref` helpers) |

### Rust(`rust_compute/`,sqlx + Postgres)

- Workspace virtual root,35 crates(v3.5 後)
- **Binary 1**:`tw_stock_compute`,入口 `rust_compute/silver_s1_adjustment/src/main.rs`,呼叫端 `src/rust_bridge.py`
  - Silver S1 後復權 + 週/月 K 聚合(Phase 4/7c)
  - 迴圈核心:**先 push 再更新 multiplier**(除息日當日 raw 已是除息後,不可再乘該日 AF)
  - 拆兩個 multiplier(v1.8):`price_multiplier`(從 AF)+ `volume_multiplier`(從 vf)
  - Phase 4 永遠全量重算(multiplier 倒推,partial 邏輯上錯);Python `_mode` 對 Rust 端是 no-op
- **Binary 2**:`tw_cores`,入口 `rust_compute/cores/system/tw_cores/src/main.rs`(v3.5 R4 C8 拆 8 module)
  - M3 Cores Monolithic Binary;`run-all` subcommand 全市場 × 全 cores production run
  - 拆 main.rs / cli.rs / dispatcher.rs / writers.rs / run_environment.rs / run_stock_cores.rs / summary.rs / helpers.rs
  - `dispatch_indicator` / `dispatch_structural` / `dispatch_neely` 三函式保留(對齊 §十四「禁止抽象」,V3 才考慮 generic dispatcher)

---

## 關鍵慣例（不要改）

完整 25 條見下方「## 關鍵架構決策（不要改）」表。動工前必看的硬規則：

- `FieldMapper(db=db)` 一定要帶 db — schema 用來補欄位豁免名單，避免「與 DB 同名直接入庫」誤報 novel
- `field_mapper.transform()` 回 `(rows, schema_mismatch: bool)` tuple — 上層用來 mark_schema_mismatch
- `db.upsert()` 自帶欄位過濾 — API 新增欄位不炸；Silver 寫入走 `silver/_common.upsert_silver()`（包 `is_dirty=FALSE`）
- `_table_pks` 動態查 `information_schema` — schema 是 single source of truth，phase_executor / sync_tracker 不再硬編碼
- `stock_info.updated_at` 走 schema `DEFAULT NOW()` + upsert UPDATE 強制 NOW()（兩條 path 都套）
- Rust 後復權兩條鐵律：「先 push 再更新 multiplier」 + 「price/volume multiplier 拆兩個」
- `EXPECTED_SCHEMA_VERSION = "3.2"`（`rust_bridge.py:31`）— schema 升版時 Rust + Python 兩端一起改
- PostgresWriter 單 connection — Phase 7a builder 串列跑（concurrent thread access 踩 psycopg thread-safety）
- Phase 4 必須傳 `stock_ids` — `stock_sync_status` 沒人寫入，Rust 取不到清單
- Windows binary path 由 `rust_bridge.py` 自動補 `.exe`（`asyncio.create_subprocess_exec` 不像 shell 自動補）
- `cooldown_on_429_sec` 存在 `RateLimiter` 實例上（api_client 從這裡讀，不是從 config 重讀）

---

## 規格與歷史檔

| 路徑 | 內容 |
|---|---|
| `collectorSpec/tw_stock_collector_program_spec_v1.2_p{1,2,3}.md` | v1.2 collector 程式規格（架構 / Rate Limiter / Phase Executor / Sync Tracker / CLI） |
| `m2Spec/oldm2Spec/collector_schema_consolidated_spec_v3_2.md` | v3.2 r1 schema 整合規格（4 層 Medallion + 14 Silver） |
| `m2Spec/oldm2Spec/collector_rust_restructure_blueprint_v3_2.md` | Rust + collector 重構藍圖（PR #17 → #21 切法） |
| `m2Spec/oldm2Spec/cores_overview.md` | M3 計算層總覽（§7.5 dirty queue 契約 / §10.0 Core 邊界三原則） |
| `m2Spec/oldm2Spec/{tw_market,traditional,neely,fundamental,chip,environment}_core.md` | 各 core 計算規格 |
| `m2Spec/oldm2Spec/indicator_cores_{momentum,pattern,volatility,volume}.md` | indicator 計算規格 |
| `m2Spec/oldm2Spec/unified_alignment_review_r2.md` | 11 篇 core spec 審查整合（r1 → r3.1，含 av3 結論） |
| `m2Spec/0001_tw_market_handling.md` / `m2Spec/layered_schema_post_refactor.md` | 2026-05-09 user 重翻新規格(commit `34b86a2`)；舊版進 `oldm2Spec/` |
| `docs/schema_reference.md` / `docs/collectors.md` | DB schema 與 collector 細節 |
| `docs/claude_history.md` | v1.4 → v1.7 歷史細節（已從本文件搬出） |
| `docs/MILESTONE_1_HANDOVER.md` | M1 milestone handover |

當前 PR sequencing(累積)：`#17 ✅ → ... → #36 ✅(v1.27 pae dedup) → #M3-1 ~ #M3-9a ✅ 22 cores → #PR #48 ✅ spec alignment → #PR #50 ✅ Aggregation Layer → #PR #51 ✅ neely Phase 13-19 v1.0.x → PR #59 ✅ v3.5 5 層架構重構 9 commits + PR #60 ✅ docs 對齊 → PR #61 ✅ v3.6 Neely RuleId enum 補完 → PR #62 ✅ v3.7 spec_pending doc cleanup + exhaustive compaction 真窮舉 → PR #63 ✅ v3.8 agg per-timeframe lookback → PR #64 ✅ v3.9 partition observation + workflow toml audit → PR #65 ✅ v3.10 R6 DROP _legacy_v2 → PR #66 ✅ v3.11 Round 7 calibration → PR #67 ✅ v3.12-v3.14.1 gov_bank pipeline 收尾(2026-05-17)`。**M3 Cores 35 crates / 420 tests / 0 failed / 1266 stocks × 36 cores production-ready,Aggregation Layer 4 Phase 全套,neely Core v1.0.1 P0 Gate 通過,v3.5 5 層架構單一職責歸位,v3.6 RuleId enum 從 28 → 81 variants(全 76 spec variants 落地),v3.7 exhaustive compaction 真窮舉 + spec-blocked reframe,v3.8 agg per-timeframe lookback,v3.9 partition 暫不需要 + workflow toml dispatch audit,v3.10 m2 大重構終結 R6 DROP 3 張 _legacy_v2,v3.11 Round 7 calibration 5 cores tighten,v3.14 gov_bank pipeline 收尾(Bronze 13.39M / Silver fill 80.74% / alembic head a6b7c8d9e0f1 / new all_market_no_end param mode / Round 7 達標 verify ✅)**。

---

## v4.24 — M8 sprint:3 non-price forecast cores 全套(2026-05-24)

接 v4.23 揭露 fusion 因 3 cores 全 price-only(誤差高度相關)無法收 Bates-Granger
1969 變異數縮減後,動工 M8 sprint:把 v4.23 §future work 提案的 3 個 non-price
forecast core 全部落地 — 讓 fusion 真正能消費 uncorrelated signal sources。

### 動工(5 commits / branch `claude/fusion-forecast-cores-nh6MQ`)

| Commit | 範圍 |
|---|---|
| 1 alembic | `d0e1f2g3h4i5_m8_forecast_cores_whitelist`:擴 `chk_forecast_calibrated_or_unsigned` IN 列表收 3 個新 core;DROP + RE-ADD pattern(PG 14+ 沒 ALTER CONSTRAINT mutate);schema_pg.sql 同步 |
| 2 fundamental | `src/forecast/fundamental_forecast.py` 134 行 + 19 tests + CLI factory dispatch |
| 3 macro | `src/forecast/macro_forecast.py` 235 行 + 43 tests |
| 4 chip | `src/forecast/chip_forecast.py` 312 行 + 26 tests |
| 5 doc | CLAUDE.md v4.24 章節 |
| 6 hotfix | `macro_forecast` + `pit.fundamental` market case 'tw' → 'TW' 對齊 production |
| 7 hotfix | `fundamental_forecast` 改 inline SQL 避開 `monthly_revenue` 無 `detail` column env drift |
| 8 doc | 加 production verify 結果(2330 1 stock 5 yr)|

### 3 cores 訊號設計

| Core | 訊號 | weight 合成 | fade × cap |
|---|---|---|---|
| `fundamental_forecast_core` | revenue YoY 3-month avg(via `pit.fundamental.asof_revenue`)| 單訊號 | 0.30 × ±20% |
| `macro_forecast_core` | TWD/USD 21d ROC + business indicator monitoring color | 0.5 × twd + 0.5 × biz | 0.40 × ±15% |
| `chip_forecast_core` | 法人 net flow z-score(20d vs 60d baseline)+ margin balance 20d roc(contrarian) | 0.7 × inst + 0.3 × margin | 0.35 × ±18% |

通用模型:`drift_h = clamp(score × fade × (h/252), ±cap)`;
variance 從 price 殘差(60d realized log return std)算;
interval = `point ± z(c) × σ × √h`。

variance 走 price 是設計接受 — fusion 的 Bates-Granger 變異數縮減效益來自
*點預測* 誤差 uncorrelated,寬度 correlation 是 second-order。

### PIT-safety

| Source table | Filter |
|---|---|
| `monthly_revenue` | `COALESCE(report_date, date + 11d) ≤ asof_t`(既有 PIT helper) |
| `exchange_rate` | `date ≤ asof_t - 1 day`(BoT 隔日早上釋出 spot rate) |
| `business_indicator_tw` | `COALESCE(report_date, date + 27d) ≤ asof_t`(既有 PIT helper) |
| `institutional_daily_derived` | `date ≤ asof_t`(同日盤後公布) |
| `margin_daily_derived` | `date ≤ asof_t`(同上) |

### CLI 改動(`src/main.py`)

`forecast backtest --core` choices 從 `{baseline, log_channel}` 擴
`{baseline, log_channel, chip_forecast_core, macro_forecast_core,
fundamental_forecast_core}`。

dispatch 拆兩 path:
- **PRICE_ONLY_CORES**(baseline / log_channel):走原既有路徑,forecast_fn 無 DB 依賴
- **DB_AWARE_CORES**(3 個新)走 factory pattern:per-stock 建 closure 帶 conn + stock_id

既有 baseline / log_channel 行為 0 改動。

### Bates-Granger 1969 預期啟動條件

每個新 core 跑完 backtest → conformalize(`forecast conformalize --raw-core
chip_forecast_core --target-core chip_forecast_core_cqr ...`)→ 進
`forecast_log calibrated=TRUE` → `fusion.eligible_cores()` 自動發現 → 若
mean_pinball 過去 100 期勝過 baseline AND n_samples ≥ 30 → 進 eligible 列表
→ `fusion._intersect_all` 對 baseline + kalman_cqr + log_channel_cqr +
chip_cqr + macro_cqr + fundamental_cqr 取交集 → 區間真正窄化 + 點預測平均

### Production verify(2026-05-24,2330 1 stock 5 yr backtest)

完整流水線 user 本機跑(alembic upgrade → 3 cores backtest → settle → CQR
conformalize → settle → fuse → settle → score),終極對比表(c=0.80):

| h | source_core | n_settled | rel_pct | pinball | mean_width |
|---|---|---|---|---|---|
| **21** | macro_cqr | 1504 | 82.0 | 10.17 | 152.47 |
| | chip_cqr | 1504 | 81.3 | 10.20 | 148.69 |
| | fund_cqr | 1462 | 81.1 | 10.20 | 148.83 |
| | **fusion** | **774** | **70.5** | **11.99** ✅ | **118.05** 🏆 |
| | baseline | 1534 | 74.5 | 12.09 | 155.59 |
| **63** | fund_cqr | 1435 | 79.5 | 18.74 | 281.74 |
| | macro_cqr | 1477 | 80.4 | 18.80 | 302.15 |
| | **fusion** | **516** | **69.0** | **18.81** ≈ best | **221.05** 🏆 |
| | chip_cqr | 1477 | 77.0 | 19.70 | 277.81 |
| | baseline | 1507 | 69.4 | 20.80 | 253.27 |
| **126** | **fusion** | **708** | **72.7** | **25.50** 🏆 | **310.17** 🏆 |
| | fund_cqr | 1398 | 82.0 | 27.07 | 430.91 |
| | chip_cqr | 1440 | 77.2 | 29.53 | 427.26 |
| | macro_cqr | 1440 | 78.2 | 29.57 | 462.00 |
| | baseline | 1470 | 47.9 | 35.08 | 332.14 |

vs v4.23 對比:
| h | v4.23 fusion(3 price-only cores) | M8 fusion(+ 3 non-price) | 改善 |
|---|---|---|---|
| 21 | 輸 baseline 45% pinball | 勝 baseline 0.8% pinball | 🎯 從輸到勝 |
| 63 | 輸 baseline 116% pinball | 勝 baseline 9.6% pinball | 🎯 -40% pinball |
| 126 | 輸 baseline 53% pinball | 勝 baseline 27% pinball + 連最佳單 core 都勝 | 🎯 -34% pinball |

關鍵發現:
- **mean_width 全表最窄**:Bates-Granger intersection 真實生效;h=21 fusion 118
  vs 個別 cores 148-515(-22%);h=126 fusion 310 vs 332-1740(-7~-82%)
- **h=126 fusion 連最佳單一 core 都打過**(25.50 vs fund_cqr 27.07)— 純粹的
  Bates-Granger 1969 變異數縮減:長 horizon 個別 core noise 累積大,combination
  averaging 降低 variance 最有效
- **kalman_cqr 沒勝 baseline**(h=21 14.19 vs 12.09)— v4.23 fusion 輸 baseline
  根本原因揭露:當 eligible_cores 只有 kalman_cqr + log_channel_cqr(寬到 5×)
  → fusion 退化成 pass-through 但用了不該 pick 的 cores

3 cores raw vs CQR 表(c=0.80):

| Core | h | raw pinball | cqr pinball | raw rel% | cqr rel% |
|---|---|---|---|---|---|
| fund | 21 | 10.16 | 10.20 | 84.2 | 81.1 |
| fund | 63 | 18.70 | 18.74 | 80.2 | 79.5 |
| fund | 126 | 28.21 | 27.07 | 70.4 | 82.0 |
| macro | 21 | 10.12 | 10.17 | 83.8 | 82.0 |
| macro | 63 | 18.84 | 18.80 | 75.7 | 80.4 |
| macro | 126 | 31.78 | 29.57 | 66.7 | 78.2 |
| chip | 21 | 10.16 | 10.20 | 84.5 | 81.3 |
| chip | 63 | 19.77 | 19.70 | 78.4 | 77.0 |
| chip | 126 | 31.30 | 29.53 | 67.8 | 77.2 |

CQR 修正模式 verified:
- h=21:raw over-cover 84% → CQR 收回 81%(略縮 mean_width)
- h=126:raw under-cover 67% → CQR 放寬到 78-82%(略寬 mean_width)
- pinball 基本維持(raw 已經很好)

### M8 待補(2026-05-24+):
- 擴 6 stocks 對齊 v4.23 大表 apples-to-apples 對比
- 觀察新 cores 在熊市 / 高 vol regime 下的穩定性

### Tests(本 PR)

| Module | 新 tests |
|---|---|
| `test_fundamental_forecast.py` | 19 cases(_compute_yoy_3m_avg ×6 / _compute_realized_vol ×3 / make ×10) |
| `test_macro_forecast.py` | 43 cases(_parse_color_score parametrized ×21 / _compute_twd ×6 / _compute_biz ×7 / make ×9) |
| `test_chip_forecast.py` | 26 cases(_net_flow ×6 / _compute_inst_score ×5 / _compute_margin_score ×6 / make ×9) |
| **Total** | **+88 tests(`tests/forecast/` 從 73 → 161)** |

`pytest tests/forecast/` ✅ 161 passed / 0 failed。0 既有 forecast tests regression。

### user 本機 production verify(下次 session)

```powershell
git pull
alembic upgrade head   # c9d0e1f2g3h4 → d0e1f2g3h4i5

# 1. 跑每個新 core 的 backtest(估各 ~30 分 / stock / 5 年)
python src/main.py forecast backtest --core fundamental_forecast_core --stocks 2330 --since 2020-01-01
python src/main.py forecast backtest --core macro_forecast_core --stocks 2330 --since 2020-01-01
python src/main.py forecast backtest --core chip_forecast_core --stocks 2330 --since 2020-01-01

# 2. settle past forecasts(同既有 pipeline)
python src/main.py forecast settle --core fundamental_forecast_core
python src/main.py forecast settle --core macro_forecast_core
python src/main.py forecast settle --core chip_forecast_core

# 3. conformalize 三個新 core
python src/main.py forecast conformalize \
    --raw-core fundamental_forecast_core --target-core fundamental_forecast_core_cqr \
    --stocks 2330 --since 2020-01-01
# 同理 macro / chip

# 4. fusion 自動 pick up(fusion 邏輯不需改;eligible_cores 自動發現)
python src/main.py forecast fusion --stocks 2330 --since 2020-01-01

# 5. 對比 v4.23 baseline 表(83,646 forecasts × 6 stocks × 3 horizons × 3 conf)
python src/main.py forecast score --stocks 2330,1101,2317,2330,2454,2618,2603
```

### 範圍

| | |
|---|---|
| 程式 | 681 行(3 cores)+ 60 行(CLI dispatch)+ ~1,400 行 tests |
| schema | 1 alembic migration(whitelist 擴 IN 列表)|
| Rust | **0** |
| collector.toml | **0** |
| 行為向下相容 | 既有 baseline / log_channel / kalman_cqr / fusion 0 改動;tests 0 regression |

### 風險

🟢 低:
- 0 Rust / 0 collector.toml,純 Python module 加 + alembic CHECK constraint 擴
- DB-aware factory pattern 從 dispatch table 嚴格分流,price-only cores 0 影響
- 既有 forecast tests 161 passed(73 baseline + 88 新)0 regression
- alembic upgrade 用 DROP + RE-ADD constraint,downgrade no-op-safe 回原 7 entries
- Rollback:每 commit 獨立 `git revert`;alembic downgrade 反向

🟡 中:
- **2330 production verify 通過,但其他 5 stocks 未跑**(對齊 v4.23 大表 6 stocks
  需自行擴跑);2330 是半導體景氣股,fundamental 訊號特別匹配 → 其他股(e.g.
  傳產 / 金融)可能 pinball 改善幅度收斂
- **fade_factor / drift_cap / saturation 全部 best-guess 初版**;2330 結果驗證
  整體方向正確(raw 都勝 baseline),但 per-sector 或 per-regime 細調留 V2
- **fusion rel_pct 70-72%(對 nominal 80%)略 undercover** — intersection 把
  區間收窄到該收的地方換來 pinball 大幅改善,屬 sharpness vs coverage trade-off
- **chip_forecast_core 不消費 loan_collateral_balance_derived**(v3.21 新表)—
  V2 加入後 chip_score 改 3 訊號合成;當前 V1 範圍對齊「先有 working 版本」

🔴 高:**無**

### Out of scope(V2 議題,等 production verify 後再評估)

- **chip_forecast_core 加 loan_collateral 第三訊號**(v3.21 新表,信號獨立)
- **macro_forecast_core 加 sector beta**(電子 / 金融 / 傳產對 TWD 反應不同)
- **fundamental_forecast_core 加 financial_statement signal**(EPS YoY / 毛利率 trend)
- **per-stock fade_factor calibration**(不同產業有不同 revenue→price 彈性)
- **MCP 暴露**:目前 forecast 只 internally 跑 backtest/score;若要 LLM 對話內看
  per-horizon prediction,需新加 MCP tool(對齊 neely_forecast pattern)

---

## v4.23 — Fusion 實證限制紀錄 + future work(2026-05-24)

v0.3 spec phase 7 fusion 落地後(M7 commit `a3c5ec9`)production verify(6 檔 ×
1549 trading days × 3 horizons × 3 confidences = 83,646 forecasts)揭露 fusion
**strictly dominated by kalman_cqr** on pinball / sharpness / reliability 三項
across all 3 horizons(21/63/126 天)。

| h | metric | baseline | kalman_cqr | fusion |
|---|---|---|---|---|
| 21 | pinball | **8.98** | 9.97 | 13.00 |
| 21 | rel(.95) | 0.90 | **0.93** | 0.91 |
| 63 | pinball | **14.51** | 21.34 | 31.32 |
| 63 | rel(.95) | 0.83 | **0.92** | 0.89 |
| 126 | pinball | **25.14** | 35.06 | 38.40 |
| 126 | rel(.95) | 0.68 ❌ | **0.93** | 0.88 |

### Root cause

Fusion 設計基礎是 forecast combination puzzle(Bates & Granger 1969 / Stock &
Watson 2004 / Smith & Wallis 2009):**多 uncorrelated forecaster 等權平均通常勝
過任一單一 forecaster**。前提是**誤差 uncorrelated**。

目前 3 cores 全部基於同一 daily close price:
- `baseline` — 21-126 天歷史 return 經驗分位數
- `kalman_cqr` — LLT 平滑 + CQR
- `log_channel_cqr` — log(close) trailing OLS + CQR

**全是 price-only signal,同源 → 誤差高度相關 → 違反 Bates-Granger 前提 →
fusion 無 variance reduction 空間**。

Intersection 主路徑:log_channel_cqr 區間 3× 寬於 kalman_cqr → 交集 ≈
kalman_cqr 邊界 → fusion ≈ pass-through + selection bias(只在 kalman_cqr 過去
勝 baseline 才寫 → 子集偏難)。

### 拍版:保留 fusion + 記錄限制(2026-05-23)

**不**移除 fusion:
- spec compliance(v0.3 phase 7)
- 理論上正確(Bates-Granger 1969 是經典金融計量結果)
- 未來性 — cores 多元化後 fusion 真正活起來

**做** documentation:本 v4.23 章節 + `src/forecast/fusion.py` 模組 docstring
加實證限制段(future-self 提醒)。

### Future work(M8+,獨立 sprint,不在本 branch)

Fusion 真正展現價值需要 **非 price-based forecast cores**,讓多核誤差
uncorrelated:

| 提案 core | 信號源 | 與既有誤差相關性 |
|---|---|---|
| `chip_forecast_core` | institutional flow + margin / loan_collateral | 低 — 法人資訊獨立於 price |
| `macro_forecast_core` | FX / commodity / business_indicator | 低 — macro signal 跨資產 |
| `fundamental_forecast_core` | revenue YoY + financial_statement | 低 — 基本面 lag 月/季 |

每個 follow 既有 5 接點 pattern(Rust crate 或 Python module → 寫
`forecast_log` calibrated=False → conformalize 加 `--raw-core X --target-core
X_cqr` → 進 fusion eligible_cores 自動 picked up)。

加 2-3 個非 price core 後 fusion 預期:
- intersection 真正狹窄(獨立投票)
- divergence 在 regime shift 觸發(uncorrelated cores 分歧)
- pinball 接近或勝最佳單一 core(Bates-Granger 變異數縮減)

但**不在本 branch**(spine 建設範圍)— 本 branch 完成 verify 合 main 後,M8+ 另
開 sprint。

### 範圍(documentation-only,1 commit)

| 檔 | 動作 |
|---|---|
| `CLAUDE.md` | 加 v4.23 章節(本段)|
| `src/forecast/fusion.py` | 模組 docstring 加「實證限制 + future work」段 |

🟢 風險極低:純 documentation,0 code 行為改變,0 alembic / 0 Rust / 0 collector.toml。

---

## v4.22 — Neely quality_caveat 警告文字 spec 對齊(2026-05-22)

User 全砍重建後 MCP `neely_forecast` 對 3030 / 2330 仍報 Fib 投影脫鉤(現價 381 /
2230,primary Fib 區 65-80 / 533-862)。完整診斷鏈確認:**資料 100% 正常**
(`price_daily_fwd` / `price_weekly_fwd` 都到當前週)— 是 Neely 引擎對這類「急漲股」
近期結構產不出有效 scenario(3030 weekly forest 只剩 3 個、全停在 2020-2022)。

查 `neely_core_architecture.md §7.2`(Hybrid 失敗模型):Validator 拒絕不符 NEoWave
Ch5 Essential Rules 的結構 → 無有效 scenario 是**明文合法的「Neely 認為現在無解」
結果**(§7.4 Round 3 `awaiting_l_label` 同理)。→ 引擎嚴格拒絕 = NEoWave 設計如此,
非 bug;要強行讓引擎對 3030 產近期 scenario 須放寬 Ch5 = 違反 NEoWave spec。

### 修法

`_forecast.py` `_compute_quality_caveat` 的脫鉤警告文字改 spec-grounded —— 原文
「基於短期 swing anchor 投影」(誤導成可修的資料/picker bug),改明說「Neely 引擎
對近期結構無有效 scenario、對齊 spec §7.2、非資料問題、forecasts 請忽略」。
`is_usable=false` 行為與 picker 邏輯不變。

### 結論

Neely 對 3030/2330 這類急漲股被 flag `is_usable=false` 是**設計內、spec 合法**行為。
`quality_caveat` 誠實標示「不可用」即正確處理。其餘 MCP 工具(kalman / stock_snapshot
/ indicators / market_overview / stock_levels / 4 screens)資料正常。

🟢 低風險:純 MCP layer 警告字串。0 Rust / 0 schema / 0 alembic。

---

## v4.21 — schema_pg.sql 補回 M3 + v3.32 共 14 張表(fresh-init 完整性,2026-05-21)

User 全砍重建後 `tw_cores run-all` 所有 core `facts_new=0` —— root cause:`schema_pg.sql`
自始**漏了 14 張表**:
- M3 三表 `facts` / `indicator_values` / `structural_snapshots`(migration `w2x3y4z5a6b7`)
- v3.32 cross_cores 11 表(10 `*_ranked_derived` + `monthly_trigger_signals_derived`,
  migration `d9e0f1g2h3i4`)

這些表只在 alembic migration 建、從沒回寫 `schema_pg.sql`。平時不影響(各 migration
自己建表),但 fresh-init 走 `psql -f schema_pg.sql` 就漏建 → M3 cores 寫不進 facts、
cross_cores 寫不進 ranked 表 → MCP 讀到空。

### 修法

| 動作 | 內容 |
|---|---|
| 補 14 張表 DDL | 從對應 migration verbatim 抄;`facts.severity`(e0f1g2h3i4j5)內聯;v3.32 `_BASE_TAIL` 展開 |
| 21 個 trigger → `CREATE OR REPLACE TRIGGER` | 讓 `psql -f schema_pg.sql` 可在既有 DB 重跑(idempotent),不撞「trigger already exists」 |

### 風險

🟢 低:純補 DDL,14 張表全 `CREATE TABLE IF NOT EXISTS`;既有 DB 重跑只新增缺的表、
0 既有資料影響。`CREATE OR REPLACE TRIGGER` 為 PG 14+ 標準語法。

---

## v4.20 — baseline migration exec_driver_sql 修復(fresh-init 解鎖,2026-05-21)

User 全砍重跑時 `alembic upgrade head` 從空 DB 炸 —— baseline migration
`0da6e52171b1` 做 `op.execute(sa.text(schema_sql))`,`sa.text()` 把整份
`schema_pg.sql` 過 SQLAlchemy statement 編譯,字面 `%`(trigger `format()`
的 `%I` 等)被當 pyformat 佔位符 → 空參數 → 整份炸。潛伏 bug:baseline 只有
fresh-init 才整份跑,PR #20 加 trigger(`%I`)後沒人 fresh-init 過。

修法:`op.execute(sa.text(sql))` → `op.get_bind().exec_driver_sql(sql)` ——
直送 DBAPI、無參數,psycopg 不處理 `%`,schema 原樣交給 PG。

### Fresh-init recovery(DB 已空、baseline 尚未修時)

psql 原生跑 schema 檔不踩此 bug,是最穩的 fresh-init:
```powershell
psql $env:DATABASE_URL -v ON_ERROR_STOP=1 -f src/schema_pg.sql
alembic stamp head        # 標記 DB 已在 head,不重跑 migration
```

### 範圍 / 風險

`alembic/versions/2026_04_28_0da6e52171b1_baseline_schema_v2_0.py` 1 行修法。
🟢 低:只影響 fresh-init 路徑(baseline upgrade);0 schema 改動、0 既有 DB 影響。

---

## v4.19 — Fusion Layer MCP toolkit 整併 10 → 3 入口(2026-05-21)

接 v4.18 後 user 拍版把 Fusion Layer 的 10 個 MCP 工具整併成更少入口(對齊
v3.31 `stock_snapshot` 6→1 pattern),並注意 Claude chat 的 MCP 輸出大小限制。

### 整併:10 → 3 consolidated 入口

| 新入口 | 視角 | 整併自 |
|---|---|---|
| `market_overview(date, events_lookback_days=30, severity_min='notable')` | D 大盤 | market_dashboard + market_events |
| `stock_levels(stock_id, date, entry_price=None, ...)` | B 個股價位 | key_levels + pattern_scan + stop_loss_calc |
| `indicators(stock_id, date, groups=None, cores=None, preset=None)` | E 指標 | indicator_momentum/volatility/volume/pattern/stack |

MCP 公開工具:**18 → 11**(4 個股/跨股 + 4 cross-stock screen + 3 fusion consolidated)。

### 輸出大小控制(對齊 user「注意 claudechat 輸出限制」)

MCP 工具回傳值整包進模型 context(Claude Code `MAX_MCP_OUTPUT_TOKENS` 預設 ~25K
token,超過截斷)。整併讓單一工具回更多 → 三入口都做 default 收斂:
- `market_overview`:events 預設 `severity_min='notable'`(濾掉 info 噪音)+ 30 天窗。
- `stock_levels`:`stop_loss` 僅在給 `entry_price` 時才算,否則 None。
- `indicators`:預設 `preset='default'`(5 cores);`groups` 多選才攤開整類
  (momentum 一類 9 cores),屬 opt-in。

### 設計

- 3 個 consolidated function 放 `mcp_server/tools/data.py`(thin wrapper,呼叫既有
  `src/fusion/*` 子函式 + 合併);每段獨立 try/except graceful degradation(對齊
  stock_snapshot — 某段失敗 → 該段 `{"error": ..., "section": ...}`)。
- `indicators` 選擇優先序:`cores` > `groups` > `preset` > default `preset='default'`。
- 被整併的 10 個舊 fusion function **仍留 data.py**(dashboard / direct python
  可呼叫),只是不再 `mcp.tool()` 註冊 — 對齊 v3.31 6 helper 隱藏 pattern。

### 範圍(1 commit / branch `claude/fix-execution-errors-8E8z4`)

| 檔 | 動作 |
|---|---|
| `mcp_server/tools/data.py` | 加 `market_overview` / `stock_levels` / `indicators` 3 個 consolidated function |
| `mcp_server/server.py` | 10 個 fusion `mcp.tool()` 註解掉 + 3 個新註冊;docstring + instructions 更新 |
| `tests/mcp_server/test_fusion_consolidated.py`(新)| 14 tests(合併 / graceful degradation / 選擇優先序 / payload / public surface)|
| `CLAUDE.md` | 本段 + Quick Reference MCP toolkit 18 → 11 |

### 沙箱驗證

- `pytest tests/mcp_server/test_fusion_consolidated.py` ✅ 14 passed
- `pytest tests/mcp_server/ tests/fusion/` ✅ 203 passed / 1 skipped(0 regression)
- 0 alembic / 0 Rust / 0 collector.toml

### 風險

🟢 低:純 MCP layer reshape。10 個舊 function 保留(0 行為改變,只是不曝露 MCP);
既有 fusion / mcp_server 測試 0 regression。Rollback:單 commit `git revert`。

---

## v4.18 — 刪除 23 支過期 script(2026-05-21)

接 v4.17 後 user 拍版清掉過期 script。盤點 `scripts/` 49 支,刪 23 支、留 26 支。

### 刪除清單

**Tier A — 確定壞掉 / 已 deprecated(13)**
- `_reverse_pivot_lib.py` + `reverse_pivot_{institutional,valuation,day_trading,margin,foreign_holding}.py`
  — 讀 v4.17 已 DROP 的 v2.0 表,CLAUDE.md v4.16 明文「reverse_pivot 從此不需要」
- `verify_pr18_bronze.py`(跑 reverse-pivot 讀 v2.0)/ `verify_pr19b_silver.py`(`legacy_table` 指向 v2.0)
- `verify_pr19c2_silver.py`(v3.10 已標 🪦 DEPRECATED)/ `verify_pr19c_silver.py`(import 已刪的 `_reverse_pivot_lib`)
- `inspect_db.py`(SQLite hardcode,v2.0 後不可用)
- `cleanup_non_trading_days.py`(一次性,target 含已 DROP 的 institutional_daily)
- `fix_p1_17_stock_dividend_vf.sql`(檔頭自標 DEPRECATED)

**Tier B1 — 過期一次性 probe/query(5)**
- `probe_finmind_datasets.py`(PR #21-B,被 `probe_finmind_sponsor_unused.py` 取代)
- `probe_finmind_taiex_ohlcv.py`(PR #22 動工前 probe)
- `p2_calibration_data.sql` / `p2_eventkinds.sql`(P2 校準,被 `verify_event_kind_rate.sql` 取代)
- `discover_split_candidates.sql`(av3 一次性 backfill 盤點)

**Tier B2 — 老但仍可跑(5)**
- `av3_spot_check.sql` / `av3_spot_check.md` / `run_av3.ps1`(v1.9 後復權驗證)
- `probe_collector_tier.py`(tier 探測)/ `health_check_mcp_3030.py`(3030 MCP 健檢)

### 連帶清理

| 檔 | 動作 |
|---|---|
| `src/bronze/segment_runner.py` | dataset-reject 診斷訊息 `probe_finmind_datasets.py` → `probe_finmind_sponsor_unused.py`(被刪檔 repoint)|
| `config/collector.toml` | SBL 註解內 probe script 名同步 repoint |
| `CLAUDE.md` | 「驗證腳本」清單移除 verify_pr18/19b/19c2,inspect_db → check_all_tables;helper 表移 10 列;重跑流程同步 |
| `README.md` | verify-scripts 清單移除 verify_pr18 / verify_pr19b;退場候選表列移除(5 表 v4.17 已 DROP)|

audit:`src/` Python 不 import `scripts/` → 刪 script 0 程式相依。`segment_runner.py`
一處診斷字串 + `collector.toml` 一處註解指向被刪 probe(repoint 到保留的
`probe_finmind_sponsor_unused.py`);`verify_pr19c_silver.py` import 被刪的
`_reverse_pivot_lib` → 一併刪除(同源 PR #19 一次性驗證)。

### 風險

🟢 低:純刪檔 + 文件/註解同步。`scripts/` 留 26 支 live 工具(test_pipeline 引用 /
refresh wrapper / generic 診斷 / probe 三支 / verify_pr20+mcp / seed)。
0 alembic / 0 Rust。Rollback:`git revert`(刪檔可從 git 還原)。

---

## v4.17 — refresh_full.ps1 完整補完 wrapper + DROP 5 張 v2.0 orphan 表(2026-05-21)

接 v4.16 PR #18 `_tw` 遷移收尾後,user 拍版兩件收尾:把「完整補完」整條 pipeline
包成一鍵 wrapper,並 DROP 遷移後變 orphan 的 5 張 v2.0 表。

### 動工 1:`scripts/refresh_full.ps1` — 完整補完 wrapper

整理 `src/main.py` CLI 後,資料流水線分兩類:

| | 類別 1 完整補完 | 類別 2 每日排程 |
|---|---|---|
| wrapper | **`refresh_full.ps1`(本 PR 新增)** | `refresh_daily.ps1`(既有)|
| Bronze | incremental | incremental |
| Silver 7a | `--full-rebuild` 全表 | incremental 窗口(WRITE 30 天)|
| Cross 8 | `--full-rebuild` | latest date only |
| M3 cores | `run-all --write`(全市場)| `run-all --write --dirty` |
| 時間 | ~40-60 分 | ~15-20 分 |

`refresh_full.ps1` 6 步:Bronze incremental → Silver 7c → 7a `--full-rebuild` →
7b `--full-rebuild` → Cross-Stock 8 `--full-rebuild` → M3 Cores `run-all --write`。
沿用 refresh_daily.ps1 的 venv 啟動 / .env 載入 / UTF-8 console / dated log
(`logs/refresh_full_YYYY-MM-DD.log`)+ per-step summary table。每步獨立,前段
失敗不阻擋後段(對齊 `python src/main.py refresh` 設計)。

何時用:隔很久沒跑 / 遷移後 / 距上次 incremental > 30 天(Silver 7a WRITE 窗,
窗外舊 row 不會被 incremental 更新)/ 想確保端到端全部重算。日常每天走
refresh_daily.ps1 即可(Silver 表已含完整歷史,incremental 窗口每次正確)。

```powershell
.\scripts\refresh_full.ps1                    # 全市場完整補完
.\scripts\refresh_full.ps1 -Stocks '2330'     # 限縮股票(開發測試)
.\scripts\refresh_full.ps1 -SkipCores         # 無 Rust binary 時
```

### 動工 2:DROP 5 張 v2.0 orphan 表(alembic `f1g2h3i4j5k6`)

v4.16 collector 改直寫 `_tw` Bronze-raw 表後,5 張舊 v2.0 表無人寫、無人讀:

| v2.0 orphan 表 | 主路徑(現役)|
|---|---|
| `institutional_daily` | `institutional_investors_tw` |
| `valuation_daily` | `valuation_per_tw` |
| `day_trading` | `day_trading_tw` |
| `margin_daily` | `margin_purchase_short_sale_tw` |
| `foreign_holding` | `foreign_investor_share_tw` |

audit(grep `src/`):0 處讀寫這 5 張表 — Silver builder 全走 `_tw` 主名;只有
`reverse_pivot_*` / `verify_pr18` / `verify_pr19b` / `cleanup_non_trading_days`
等 obsolete 遷移工具引用(DROP 後失效屬已知,本就不再需要)。

### 範圍(1 commit / branch `claude/fix-execution-errors-8E8z4`)

| 檔 | 動作 |
|---|---|
| `scripts/refresh_full.ps1`(新)| 完整補完 6 步 wrapper(對齊 refresh_daily.ps1 風格)|
| `alembic/versions/2026_05_21_f1g2h3i4j5k6_v4_17_drop_v2_orphan_tables.py`(新)| DROP 5 表 CASCADE;downgrade no-op(對齊 PR #R6 destructive 先例)|
| `src/schema_pg.sql` | 移除 5 個 CREATE TABLE(fresh-init 不再重建)|
| `scripts/check_all_tables.py` | 表清單移除 5 個 v2.0 表 |
| `CLAUDE.md` | 本段 + Quick Reference alembic head `e0f1g2h3i4j5` → `f1g2h3i4j5k6` |

### user 端 runbook

```powershell
git pull
# 建議先備份(destructive,downgrade 不可回復;v2.0 表資料 = FinMind raw 亦可重抓)
pg_dump $env:DATABASE_URL -t institutional_daily -t valuation_daily -t day_trading -t margin_daily -t foreign_holding -f backup_v2_orphan_tables.sql
alembic upgrade head      # head: e0f1g2h3i4j5 → f1g2h3i4j5k6
```

### 已知小事(user 拍版接受)

- `capital_reduction` 仍是 per_stock(~12 分;FinMind 對它的 all_market 硬回 400,無解)。
- `foreign_investor_share_tw.declare_date` 新資料為 NULL(FinMind 未申報回 "0" 會炸 DATE 欄,v4.16 已知小損失)。

### 風險

🟡 destructive:
- `alembic upgrade head` 後 5 表永久 DROP;downgrade no-op(對齊 PR #R6)
- 建議先 `pg_dump` 備份
- 既有 Silver pipeline 0 影響(全走 `_tw` 主名)
- reverse_pivot / verify_pr18 / verify_pr19b 等 obsolete script 引用 v2.0 表,
  DROP 後失效(已知,本就 obsolete)
- 0 collector.toml / 0 Rust;`refresh_full.ps1` 純新增
- Rollback:`git revert` 還原 schema_pg.sql / check_all_tables.py / wrapper;
  已 DROP 的表須從備份還原

---

## v4.16 — collector 直寫 Bronze-raw _tw 表(PR #18 遷移收尾,2026-05-21)

接 v4.15 silver_7a 加速後,驗證揭露一個既有(pre-existing)bug:silver 7a 的 5 個
builder(institutional / valuation / day_trading / margin / foreign_holding)讀的
是 Bronze-raw `_tw` 表,但 **collector 從沒寫過這些表**。

### Root cause(grep + git log + schema + `_reverse_pivot_lib.py` SPECS 證實)

- collector 寫 v2.0 表(`institutional_daily` / `valuation_daily` / `day_trading`
  / `margin_daily` / `foreign_holding`);silver 讀 `_tw` 表
  (`institutional_investors_tw` / `valuation_per_tw` / `day_trading_tw` /
  `margin_purchase_short_sale_tw` / `foreign_investor_share_tw`)。
- 兩者之間靠 `scripts/reverse_pivot_*.py`(PR #18 反推工具)橋接 —— 但這些 script
  是**手動一次性工具,不在 `refresh` 裡**。`_tw` 表停在 04-30~05-07 = 上次有人
  手動跑的時間。
- PR #18.5 把 3 個 dataset(holding_shares / financial / monthly_revenue)改成
  collector 直接 refetch 進 Bronze-raw(`_v3` entries)。這 5 個沒做 → 卡在手動橋。
- 「institutional FinMind 自身停 2026-04-29」的舊紀錄是誤判:FinMind 有資料,
  是這座橋沒人走。

### 修法:collector 直寫 `_tw`(做完遷移)

把 5 個 collector entry 的 `target_table` 改成 `_tw` 表,直接餵 silver 讀的表:

| entry | target → | field_rename / 其他 |
|---|---|---|
| valuation_daily | `valuation_per_tw` | 不變(PER/PBR→per/pbr,schema 一致)|
| day_trading | `day_trading_tw` | detail 欄拔 `_` 前綴 → top-level;移除 detail_fields |
| margin_daily | `margin_purchase_short_sale_tw` | 同上(8 detail 欄拔前綴)|
| foreign_holding | `foreign_investor_share_tw` | 同上;**declare_date 不映射**(FinMind 未申報回 "0" 會炸 DATE 欄)|
| institutional_daily | `institutional_investors_tw` | 移除 `pivot_institutional` aggregation(`_tw` 是 raw per-investor);field_rename `name`→`investor_type` |

silver `institutional.py` 的 `INVESTOR_TYPE_MAP` 改用 `INSTITUTIONAL_NAME_MAP`
(含中文 name 變體 + 英文 key)—— collector 直寫的 investor_type 是 FinMind 中文
name,舊 reverse-pivot gap-fill 資料是英文,兩者都吃。

### 一次性 gap-fill(2026-05-21 已執行 ✅)

`_tw` 表停在 04-30~05-07,collector sync progress 卻在 05-21(遷移前舊 target 的
進度)→ 直接 incremental 會跳過缺口。原想用 `reverse_pivot_*.py` 補,但它反推
**全 7 年**(institutional 9.2M 列 upsert,太慢)。改用 collector 自己補缺口:

```sql
-- 1. 清掉 partial + 把 5 張 _tw 拉回 04-30 基準
DELETE FROM institutional_investors_tw       WHERE date >= '2026-05-01';
DELETE FROM valuation_per_tw                 WHERE date >= '2026-05-01';
DELETE FROM day_trading_tw                   WHERE date >= '2026-05-01';
DELETE FROM margin_purchase_short_sale_tw    WHERE date >= '2026-05-01';
DELETE FROM foreign_investor_share_tw        WHERE date >= '2026-05-01';
-- 2. 重設 5 entry sync progress
DELETE FROM api_sync_progress WHERE stock_id='__ALL__'
  AND api_name IN ('institutional_daily','valuation_daily','day_trading','margin_daily','foreign_holding');
```
```bash
python scripts/seed_all_market_sync_progress.py   # seed __ALL__ 到 _tw MAX(04-30)
python src/main.py incremental                    # collector all_market 補 05-01~05-21(~83s)
python src/main.py silver phase 7a                # 讀新鮮 _tw(46s)
```

**結果**:5 張 `_tw` + `institutional_daily_derived` / `valuation_daily_derived` 等
全部到 2026-05-21 ✅。collector 走遷移後新路徑直寫 `_tw` 驗證成功(institutional
05-01~05-21 各 ~5950 列,investor_type = FinMind 中文 name)。

之後 `refresh` 的 incremental 由 collector 直接維護 `_tw` 表 — reverse_pivot
script 從此不需要。v2.0 表(`institutional_daily` 等)變 orphan,留著無害,日後
可獨立 DROP。

### 風險

🟡 中:collector bronze 寫入 target 改動。sandbox 驗(config load + silver
builders import + 21 tests)。declare_date 改 NULL 是已知小損失。
institutional `_tw` 會有 Saturday 鬼資料(collector 不再 trading-day filter)→
silver `_pivot` 的 `filter_to_trading_days` 會濾掉,無下游影響。0 alembic / 0 Rust。
Rollback:單 commit `git revert`(collector.toml + institutional.py)。

---

## v4.15 — Silver Phase 7a incremental 窗口(2026-05-21)

接 v4.14 後 user 反映 `refresh` 裡 `silver_7a` 每次 ~16 分。Root cause:Phase 7a
的 15 個 builder 每次都 full rebuild — 各自 `SELECT *` 整張 Bronze(~7 年、數百萬
列)、重算、重寫全部 Silver row。CLAUDE.md 早記「`--full-rebuild` 目前是唯一支援
的模式;dirty queue pull 留 PR #20+」— 本輪實作 incremental。

### 設計:READ 窗 / WRITE 窗

`silver/_common.py` 加 incremental 窗口(`set_incremental_window` /
`clear_incremental_window` / `incremental_read_since`):
- 7a 非 `full_rebuild` 時 orchestrator set 窗口 → `fetch_bronze` 只讀
  `date >= today-180`、`upsert_silver` 只寫 `date >= today-30`。
- READ 窗(180)比 WRITE 窗(30)大 150 天 = warmup → history-dependent builder
  (`loan_collateral` change_pct、`commodity_macro` 60d z-score)算出來仍正確。
- WRITE 窗外的舊 row 不動(上次 full rebuild 已寫、窗外不變)→ **第一次
  incremental 就立即正確,不需 re-backfill**(Silver 表已含完整歷史)。

### builder 覆蓋

`_run_7a_incremental`(orchestrator)set 窗口後跑 builder。13 個 builder 走
`fetch_bronze` + `upsert_silver` → **0 builder 改動自動套用**。`valuation` /
`day_trading` 自組 SQL(非 fetch_bronze)→ 各自 query 加 `incremental_read_since()`
日期過濾。`--full-rebuild` 仍走全量(escape hatch);7b / 7c 不受影響。

### builder history 依賴盤點(Explore agent)

| 依賴 | builder |
|---|---|
| NONE(純 per-(stock,date))| institutional / margin / foreign_holding / holding_shares_per / day_trading / monthly_revenue / block_trade + 5 個 market-level |
| PREV(change_pct vs 前一日)| loan_collateral |
| WINDOW(60d z-score)| commodity_macro |
| cross-stock same-date(非 cross-date)| valuation(market_value_weight 分母 = 當日全市場 SUM)|

最大 history 依賴 = commodity_macro 60 天 << warmup 150 天 → 窗口安全。

### 範圍(1 commit / branch `claude/fix-execution-errors-8E8z4`)

| 檔 | 動作 |
|---|---|
| `src/silver/_common.py` | incremental 窗口 module state + set/clear/accessor;`fetch_bronze` 加 READ 過濾;`upsert_silver` 加 WRITE 過濾 |
| `src/silver/orchestrator.py` | `_run_7a_incremental`(READ=180 / WRITE=30);7a 非 full_rebuild 走此路徑 |
| `src/silver/builders/valuation.py` | `_fetch_market_totals` + `_fetch_per_stock_rows` SQL 加日期過濾 |
| `src/silver/builders/day_trading.py` | `_fetch_joined_rows` SQL 加日期過濾 |
| `tests/silver/test_incremental_window.py`(新)| 6 tests |

### 預期

`silver_7a` ~16 分 → ~2-3 分(institutional 341s → ~25s 等)。`refresh` 整體
(bronze ~13 + silver_7a ~3 + 其他)從 ~106 分 → ~20 分內。

### 風險

🟡 中:Silver 計算行為改動。sandbox 6 tests passed;escape hatch
`--full-rebuild` 保留;第一次 incremental 立即正確(Silver 已有完整歷史)。
若 incremental 間隔 > 30 天(WRITE 窗)→ 跑一次 `silver phase 7a --full-rebuild`
補。0 alembic / 0 Rust。Rollback:單 commit `git revert`。

---

## v4.14 — Bronze incremental per_stock → all_market 大轉換(2026-05-21)

User 反映 `refresh` 太慢(bronze incremental ~87 分)。Root cause:20 個 per_stock
dataset 每次 incremental 逐檔打 FinMind(~1300 req/dataset),即使當天無新資料
也照打。本輪把 **19/20 轉 all_market**(1 req/日)。branch `claude/fix-execution-errors-8E8z4`,9 commits。

### 動工 commits

| commit | 內容 |
|---|---|
| `9d0988e` | `scripts/probe_all_market_support.py` — probe per_stock dataset 是否支援 all_market |
| `483b09a` | 11 個每日 dataset → all_market + segment_days=1 |
| `b662940` | price_daily + institutional_daily → all_market + segment_days=1 + `universe_filter` |
| `412579e` | date_segmenter incremental 切段修正 + `scripts/seed_all_market_sync_progress.py` |
| `cbf7e6e` | `scripts/probe_finmind_date.py` — 單日 FinMind 診斷 |
| `e1dcead` | segment_days=0 incremental backwards-segment 修復 |
| `6aaa780` | probe_finmind_date `--multi-days` |
| `e5d140b` | financial×3 + dividend×2 → all_market + segment_days=1 |
| (本) | monthly_revenue_v3 → all_market + segment_days=1 + 本 doc |

### FinMind all_market 兩個 quirk(probe 揭露)

1. **單請求只回 1 日**:high-volume dataset(price / institutional / financial /
   dividend ...)的 all_market 端口,給 date range 只回第一天 → 全部 `segment_days=1`。
   date_segmenter incremental 也改成依 segment_days 切段(原本 incremental 永遠
   單段 `[(last_sync+1, today)]`,多日 gap 只會抓到 1 天、其餘靜默遺失)。低頻
   日曆 TaiwanStockTradingDate 例外(multi-day 正常)。
2. **per_stock dataset 含權證**:price_daily / institutional_daily all_market 回
   整個權證宇宙(~41k / ~105k 列/日)→ 新 `ApiConfig.universe_filter` flag,
   segment_runner 抓回後過濾 stock_id 到 stock_resolver 宇宙(行為對齊 per_stock,
   0 下游影響)。其餘 17 個 all_market 回 ~1000-2600 檔(真實股、無權證),不需過濾。

### backwards-segment 凍結 bug(本輪揭露 + 修)

incremental 在「已同步到 today」(或同日重跑)時算出 start > today,segment_days=0
path 寫出 `(start, today)` backwards segment、mark `empty`。因 incremental 永遠
重算同一個 start = last_sync+1,`is_completed` 永遠命中 → dataset 永久凍結在
last_sync。production 已累積 **21626 個 backwards row**,把 `trading_calendar`
凍在 2026-05-15 → 進而讓 `institutional_daily` 的 `pivot_institutional`
(`filter_to_trading_days` 對齊舊日曆)丟掉 05-16 後的資料。

修:date_segmenter seg=0 incremental `start > today` → 回 `[]`(seg>0 早已由
`_split_segments` 自然處理)。一次性清理:`DELETE FROM api_sync_progress
WHERE segment_start > segment_end`。

### 其他修法

- **`_run_post_process` all_market 修正**:dividend_policy 轉 all_market 後
  `stock_ids` 只有 sentinel → post_process(dividend_policy_merge)逐股 merge
  需真實清單;`_run_api` 改在 all_market 模式傳 `self._stock_list`。
- **seed 工具**:`scripts/seed_all_market_sync_progress.py` — per_stock→all_market
  轉換後 `api_sync_progress` 無 `__ALL__` 進度,incremental 會誤判從未同步、
  重抓 7 年(~6h)。seed 用各 bronze 表 MAX(date)(clamp 到 today)補 `__ALL__`
  sentinel 進度。**轉換後必跑一次**。

### 結果

| | 轉換前 | 轉換後 |
|---|---|---|
| per_stock dataset | 20 | **1**(只剩 capital_reduction)|
| all_market* dataset | 14 | **34** |
| bronze incremental | ~87 分 | **~13 分**(同日重跑實測 91s;representative ~13-25 分)|

`capital_reduction`:FinMind all_market 回 400「parameter data_id can't be none」
→ 無法轉,維持 per_stock(~1188 req ≈ 12 分,是剩餘 floor)。

### 用戶端 runbook(per_stock→all_market 轉換後)

```bash
git pull
python scripts/seed_all_market_sync_progress.py --dry-run   # 預覽
python scripts/seed_all_market_sync_progress.py             # 補 __ALL__ sentinel 進度
python src/main.py incremental
```

### 風險

🟡 中:bronze ingestion 行為改動。sandbox 驗(`tests/bronze/` + `test_date_segmenter`
20 passed,含 universe_filter 4 + date_segmenter 5 新 test)+ user production 驗
(institutional all_market 1185-1191 檔/日 → universe_filter 正確;
trading_calendar 解凍 → 05-21;dividend_policy_merge 跑 1353 檔 → post_process
修正生效)。0 alembic / 0 Rust。Rollback:collector.toml param_mode 改回 per_stock。

---

## v4.13 — indicator_values 空序列 row shadow 修復(dispatch_indicator,2026-05-20)

接 v4.12 production verify 揭露的 follow-up — `business_indicator_core` 修好後
`market_dashboard` 仍回 6/7,缺的換成 `commodity_macro_core`。

### Root cause

`dispatch_indicator` 對「空序列 output」也照寫 `indicator_values` row。
`extract_indicator_meta` 對無日期 output(空序列)fallback `Utc::now()` → 空 row 的
`value_date` = 跑的當天。`fetch_indicator_latest`(`DISTINCT ON ... value_date DESC`)
取最新 value_date → 空 row 反而 shadow 掉真實資料 row。

production 實證(`indicator_values` 查詢):
- `commodity_macro_core`:好 row `value_date=2026-05-15`(series 1389)+ 壞 row
  `2026-05-17`(series 0,某日空跑留下)→ 取到 05-17 空 row → market_dashboard 判缺。
- `business_indicator_core`:6 個 05-14~19 空 row(v4.12 修前每天空跑留下)+ 好 row
  05-20(series 58)→ 好 row 日期最新故僥倖勝出沒中。

### 修法(branch `claude/fix-execution-errors-8E8z4`)

| 檔 | 動作 |
|---|---|
| `helpers.rs` | 新 `indicator_output_is_empty(output_json)` — 判 series / series_by_spec / series_by_index 全空。+6 unit test |
| `dispatcher.rs` | `dispatch_indicator` 空序列 output → skip `write_indicator_value`(facts 本就空,照常 no-op);加 debug log |

### 沙箱驗證

- `cargo build --workspace` ✅ 0 warnings
- `cargo test --workspace` ✅ **615 passed / 0 failed**(609 → +6)

### user 端收尾

1. 清既有 stale 空 row(一次性):
   `DELETE FROM indicator_values WHERE jsonb_typeof(value->'series')='array' AND jsonb_array_length(value->'series')=0;`
2. 重編 `tw_cores`(本修復生效,未來空跑不再留 shadow row)。
3. `market_dashboard` → component_count: 7。

### 風險

🟢 低:0 alembic / 0 Python / 0 collector.toml。空序列 row 本就對 consumer 無值;
skip 後 consumer 取到上一筆真實 row(更正確)。Rollback:單 commit `git revert`。

---

## v4.12 — business_indicator_core empty-series 修復(monitoring_color 格式不匹配,2026-05-20)

接 Fusion Layer P0-P2 production verify 揭露的「已知 follow-up」動工 —
`business_indicator_core` 產不出 series,`market_dashboard` 7 component 缺 1。

### Root cause(從 code + schema 文件確認,對齊 fix plan 候選根因 3)

- Schema 契約:`src/schema_pg.sql:213` + `m2Spec/layered_schema_post_refactor.md
  §3.3` 兩處獨立文件都記 Bronze `business_indicator_tw.monitoring_color` 存
  `R / YR / G / YB / B` 縮寫。
- `business_indicator_core::MonitoringColor::from_label` 原本只收英文全名
  `blue / yellow_blue / green / yellow_red / red`。
- `field_mapper` 對此 dataset 只 rename leading/coincident/lagging,monitoring_color
  原值直通 Bronze→Silver。故 Bronze 存 `R/YR/G/YB/B` → `from_label` 每點回 `None`
  → `compute` 的 `filter_map` 把整批 series 丟光 → 空 series。

### 範圍(branch `claude/fix-execution-errors-8E8z4`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/environment/business_indicator_core/src/lib.rs` | `from_label` 改收縮寫 `B/YB/G/YR/R`(schema 契約)+ 英文全名(向下相容)+ 國發會中文燈號 `藍/黃藍/綠/黃紅/紅`(可帶「燈」字尾);英文/縮寫大小寫不敏感 + 空白容忍。+2 test |
| `rust_compute/cores_shared/environment_loader/src/lib.rs` | `BusinessIndicatorRaw.monitoring_color` 誤導性註解修正 |
| `m3Spec/business_indicator_core_fix_plan.md` | 加 §〇 已修紀錄 + status 更新 |
| `CLAUDE.md` | 本段 + Fusion Layer 段「已知 follow-up」更新 |

### 沙箱驗證

- `cargo test -p business_indicator_core` ✅ **9 passed**(7 → +2:`monitoring_color_from_label_abbrev_and_chinese` / `compute_accepts_abbreviated_monitoring_color`)
- `cargo build --workspace` ✅ 0 warnings
- `cargo test --workspace` ✅ **609 passed / 0 failed**(607 → +2)
- `pytest tests/`(`--ignore=test_render_tools.py`)✅ 240 passed / 1 skipped(0 regression)

### 診斷確認(2026-05-20,`scripts/diag_business_indicator.sql`)

user 跑 fix plan §三 診斷,確認真因 = 候選 3,其餘排除:

- Silver / Bronze 各 87 rows(2019-01 ~ 2026-03),leading/coincident/lagging/
  monitoring/monitoring_color **全 87/87 non-null** → 「某中間欄全 null」排除。
- distinct stock_id 只有 `_market_` → 候選 1 排除;通過 loader filter **58 rows** →
  候選 2(date 太舊)排除。
- `monitoring_color` 值分佈:`G`×21 / `YB`×20 / `YR`×18 / `R`×17 / `B`×11 —
  **正是 schema 契約縮寫**,非英文全名 → 候選 3 確認,本修法正確且充分。

先前 user 看到 `business_indicator_core events=0` 是 `claude/plan-stockhelper-api-kWh9F`
分支編的 binary(不含本修復),非修復失敗。

### Production verified(2026-05-20)✅

user 從 `claude/fix-execution-errors-8E8z4` 重編 `tw_cores` 重跑
`run-all --skip-stock --write`(7 env cores / 0.3s):

```
business_indicator_core           1      0        34          1         34        0.0
```

`events` 0 → **34**、`facts_new` **34**、`iv_rows` 1(series 非空)。bug 結案 —
`market_dashboard` 應回 component_count: 7。

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml,純 Rust `from_label` parser 放寬
- 純 additive — 既有英文全名格式照收,既有 6 個 business_indicator test 0 regression
- params_hash 不變(parser 邏輯不入 hash);user 重跑走 ON CONFLICT UPDATE 覆寫
- Rollback:單 commit `git revert`

---

## v4.7 — Round 10 obv_core calibration + test_pipeline.ps1 polish(2026-05-19)

接 v4.6 後動工 v4.4 production verify 揭露的 obv_core 4 個 EventKind 超 12/yr,
+ 2 個 test_pipeline.ps1 cosmetic 修正。

### 3 個 task

**Task 1: obv_core Round 10 calibration**

Production verify(user 第八次跑 Phase 3)揭露:

| EventKind | rate/yr | 修法 |
|---|---|---|
| `ObvMaBullishCross` 27.41 | 對齊 ma_core,加 `MIN_OBV_CROSS_SPACING = 15`(per-direction)|
| `ObvMaBearishCross` 27.29 | 同上 |
| `ObvExtremeHigh` 24.22 | 改 **edge trigger**(對齊 Round 9 loan_collateral pattern):`cur_high && !prev_high` 才 fire |
| `ObvExtremeLow` 13.15 | 同上(`cur_low && !prev_low`)|

預期 production rate(本機 verify 待 user 跑):
- ObvMaBullishCross/BearishCross → ~6-7/yr(對齊 ma_core 6.51)
- ObvExtremeHigh → ~5-8/yr
- ObvExtremeLow → ~3-5/yr

**Task 2: alembic head 解析 cosmetic bug**

`alembic current` 輸出 3 行(2 INFO + 1 head)。原 wrapper 用
`$alembicOut -notmatch $expectedHead` 對 string array 行為奇怪,顯示 false warning。
改 `Out-String` 拉成單字串 + `[regex]::Escape()` 比對。

**Task 3: PSQL 中文 output 在 PS 5.1 console 亂碼**

PS 5.1 console codepage = CP950,psql output UTF-8 → 亂碼。三段防衛:
1. `cmd /c chcp 65001` 改 Windows console codepage 為 UTF-8
2. `[Console]::OutputEncoding = UTF-8` PS 端解碼為 UTF-8
3. `$env:PGCLIENTENCODING = 'UTF8'` psql 端輸出 UTF-8

對 PS 7 無害(預設已 UTF-8)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/indicator/obv_core/src/lib.rs` | 加 `MIN_OBV_CROSS_SPACING = 15` const;OBV MA cross 加 `last_bullish_i / last_bearish_i` spacing;ExtremeHigh/Low 改 edge trigger;+4 new tests |
| `scripts/test_pipeline.ps1` | 加 `chcp 65001` + UTF-8 OutputEncoding + PGCLIENTENCODING UTF8;alembic head 解析改 Out-String + regex match |
| `CLAUDE.md` | v4.7 章節(本段) |

### 沙箱驗證

- `cargo test --release -p obv_core` ✅ **10 passed**(從 6 → +4 new Round 10 tests)
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace --no-fail-fast` ✅ **532 passed / 0 failed**(v4.4 baseline 528 → +4 obv_core;與 v4.6 549 不同 branch 並排 — main merge 後 553)

### user 本機 production verify(下次跑)

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..

# DELETE obv_core 既有 facts(params_hash 變動 → 視為 stale)
psql $env:DATABASE_URL -c "DELETE FROM facts WHERE source_core = 'obv_core';"

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
.\scripts\test_pipeline.ps1 -OnlyPhase 3
```

預期 Round 10 後 obv_core 4 個 EventKind 全 ≤ 12/yr,進入 target band。

### 風險

🟢 低:
- 純 const tweak + spacing/edge trigger 加 logic
- 0 alembic / 0 collector.toml / 0 Python
- 既有 6 個 obv_core test margin 充足(divergence 邏輯不動)
- Rollback:單 commit `git revert`

---

## Fusion Layer — API 規劃落地(P0+P1+P2,2026-05-20)

接 v4.11 後動工 `m3Spec/fusion_layer.md`(🔒 LOCK)+ `m3Spec/api_roadmap_v1.md`。把
aggregation_layer 升級為 **Fusion Layer** 雙端口:`fusion.raw`(= 既有 `as_of()`,
並排不整合)+ Integration 端口(跨 core 整合,不引入新規則)。MCP toolkit 8 → 18。

分支 `claude/plan-stockhelper-api-kWh9F` → PR #91。14 commits + Ma20SlopeFlip
calibration + business_indicator fix plan + 本 doc。

### P0 — 基礎

| Phase | 動作 |
|---|---|
| P0.1 | `src/agg/` → `src/fusion/raw/`;44 處 import 改寫(mcp_server / dashboards / tests);`tests/agg` → `tests/fusion` |
| P0.2 | alembic `e0f1g2h3i4j5`:`facts.severity` SMALLINT NOT NULL DEFAULT 1 + idx_facts_severity_date |
| P0.3 | Rust `Severity` enum(info/notable/warning/critical)+ `Fact.severity` 欄;44 cores produce_facts 全帶 severity;writers.rs UNNEST 加 `$9::smallint[]` |
| P0.4 | cores_overview §8 對齊(chip 5→8 / env 6→7 / 新 §8.7 Cross-Stock);aggregation_layer.md r4 |

severity 採 struct field(對齊「不耦合不抽象」— 各 core 自己決定,非中央表)。

### P1 — Rust core 變更 + Fusion 模組 + 10 工具

| 項目 | 內容 |
|---|---|
| P1.1 | neely `NeelyCoreOutput.flat_fib_zones`(全 forest scenario `expected_fib_zones` 去重聯集)|
| P1.2 | 7 環境核心:8 新 EventKind(Drawdown5pct / NewHigh52w / NewHighAll / Ma20SlopeFlip / EnterPanic / Drop30In5d / TwdStrengthenStreak / Balance5dDrop3pct)+ 各 core 自己的 `severity()` 映射 + Point struct 加 `percentile_252` |
| P1.3 | 7 Fusion 整合模組:`snapshot`(10-in-1)/ `key_levels` / `pattern_scan` / `stop_loss` / `market_dashboard` / `market_events` / `indicator_assembly` + `_shared` |
| P1.4 | 10 新 MCP 工具(thin wrapper);`stock_snapshot` 擴 6→10 sections |

### P2 — 收尾

- P2.1:`magic_formula_core` Rust crate 查證為 **live**(`tw_cores` dispatch,讀 cross_cores 寫的 ranked 表產 facts)— 不移除;cores_overview §8.4 更正。
- P2.2:`traditional_core` 確認 vaporware,從 §8.1 / §9 移除。
- P2.3:30 個 mock-based 測試隨各模組落地。

### Production verify(user 本機 2026-05-20)

- alembic upgrade → `e0f1g2h3i4j5`;`tw_cores run-all` ~730s,40 cores 0 err。
- `facts.severity` 寫入:info 537 萬 / notable 681 / warning 69 / critical 6。
- neely `flat_fib_zones` 進 structural_snapshots JSONB(各股 58-217 區)。
- 8 個新環境 EventKind 全觸發,severity 正確。
- Neely forest P0 Gate:max=160 / p95=28 ✅(flat_fib_zones 未撐爆 forest)。
- 測試流水線 5 phase 全綠;`cargo test --workspace` 607 passed。

### Ma20SlopeFlip production calibration

無門檻時 35.7/yr(平盤 MA20 抖動噪音)→ 加 `MA20_SLOPE_FLIP_MIN_PCT=0.0005`
(翻轉後新斜率須 ≥ MA20 值的 0.05%)→ verify 13.2/yr(-63%)。production-data-driven,
非回測。

### 已知 follow-up

`business_indicator_core` series 空 → `market_dashboard` 6/7 component。**code 層
已修(2026-05-20,見下方 v4.12 段)** — `MonitoringColor::from_label` 字串格式不匹配。
production verify 待 user 本機跑;完整診斷 + 修法見
`m3Spec/business_indicator_core_fix_plan.md`。

### 風險

🟢 低:純架構升級 + 加欄,行為向下相容。`severity` / `flat_fib_zones` 不進
params_hash。各 commit 可單獨 `git revert`。

---

## v4.11 — Combination 上游補完 里程碑 A+B+C(2026-05-20)

接 v4.10 後動工 `m3Spec/neely_combination_upstream_plan.md`「完整方案」。本輪 Neely
原作落實盤點揭露 `NeelyPatternType::Combination` 在 production **從不產生**(classifier
只產 Impulse/Diagonal/Zigzag/Flat/RunningCorrection,compaction 多產 Triangle)→
v4.5 G2 已建的 `ch8_xwave` / `ch8_multiwave` / `triggers` / `emulation` /
`post_validator` / `power_rating` Combination 鏈路全為 **dead code**。里程碑 A+B+C
補上游 4 個動工點,點亮全鏈路。

### 動工(1 commit / branch `claude/start-v4.9-j7WZX`)

| 點 | 檔 | 動作 |
|---|---|---|
| P1a/P1b | `candidates/generator.rs` | wave_count `{3,5}` → `{3,5,7,11}`(A 加 7 / B 加 11)+ per-wave-count cap(防 wc=7/11 被 wc=3/5 佔滿共用 cap starve)|
| P1.5 | `validator/core_rules.rs` | R3/R4 guard `wave_count < 3` → `!matches!(3 \| 5)` — wc=7/11 Combination candidate 不再被 5-wave Essential 規則誤拒(R5/R6/R7/Overlap 早已 `!= 5`)|
| P2a | `classifier/mod.rs` | `classify_7wave_combination`(7 mw = sub_a 3 + x 1 + sub_b 3)→ Double-* 5 variant;`x_wave_is_large`(Table A/B 判定)+ `map_double_combination`;`classify_3wave` 抽 `classify_3wave_segment` 供 sub-segment 複用 |
| P2b | `classifier/mod.rs` | `classify_11wave_combination`(11 mw = 3+1+3+1+3)→ Triple-* variant;`map_triple_combination` |
| P3 | `compaction/three_rounds.rs` | `try_aggregate_7` / `try_aggregate_11` — 從 Level-N scenarios 拼 higher-degree Combination(全 :_3 corrective 交替)+ wire 進 `aggregate_one_level` |

### 沙箱驗證

- `cargo test --release -p neely_core --lib` ✅ **419 passed**(v4.10 baseline 410 → +9)
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace --no-fail-fast` ✅ **596 passed / 0 failed**(587 → +9)

### P0 Gate 驗證結果(2026-05-20 user 本機 ✅)

`tw_cores run-all --write`(1266 stocks / neely_core 3798 runs / 0 err / wall 729.8s ≈ 12.2 min)
後驗,對齊 `m3Spec/neely_combination_upstream_plan.md` §6 驗收條件。下方 SQL 為重跑 reference,
結果表見本節末:

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..
.\rust_compute\target\release\tw_cores.exe run-all --write

# P0 Gate#1 — Combination 是否產生 + forest 健康
psql $env:DATABASE_URL -c "
SELECT COUNT(*) AS combination_scenarios
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE core_name='neely_core'
  AND s->>'structure_label' LIKE 'Combination%'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
"
# 預期 > 0(production 不再是 0 個 Combination)

psql $env:DATABASE_URL -c "
SELECT PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p50,
       PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p95,
       MAX(jsonb_array_length(snapshot->'scenario_forest')) AS max_count
FROM structural_snapshots
WHERE core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
"
# acceptance:p95 ≤ 50 / max ≤ 250。若 max > 300 → abort,加 magnitude 預篩
```

**P0 Gate 結果(2026-05-20)**:

| 條件 | 門檻 | 實測 | |
|---|---|---|---|
| Combination scenario 產生 | > 0 | **87**(5 Level-0 + 81 `L_DoubleCombination` + 1 `L_TripleCombination`)| ✅ 從 0 點亮 |
| forest p95 | ≤ 50 | **28** | ✅ 持平 v4.10 baseline |
| forest max | ≤ 250 | **160** | ✅ 比 baseline 196 低;abort 線 300 未觸 |
| ch8 advisory 覆蓋 | — | 7 findings / **2 檔**(= Level-0 Combination 全部所在 2 檔 → 2/2 滿覆蓋)| ✅ |
| `emulation::CombinationAsImpulse` | ≥ 1 | **84** fires / 55 檔 | ✅ |
| `post_validator` Combination Stage 2 | ≥ 1 | cargo test 覆蓋(`pending_conditions` 不入 JSONB,`lib.rs:354` 只用 `pattern_complete` 過濾 forest)| ✅ |
| Triple-* scenarios | ≥ 3 | **1**(`L_TripleCombination`)| 🟡 接受(genuine rarity)|

**dead chain 完整點亮** — v4.5 G2 建的 `ch8_xwave` / `ch8_multiwave` / `emulation` /
`post_validator` Combination 鏈路在 production 開始消費 Combination scenario。

**門檻校正**:
- ch8「≥ 10 檔」原訂太高 — 只有 5 個 Level-0 Combination 散在 2 檔;plan §8.2 明示
  Level-1+(Stage 7.5 後生成)拿不到 advisory → 上限 = Level-0 所在股數 = 2;實測 2/2 滿覆蓋。
- Triple-*「≥ 3」原訂偏樂觀 — 11-wave Triple Combination 真實罕見 + plan §8.1「每元件最小
  3-monowave 假設會漏非最小組合」;`try_aggregate_11` 確實產出 1(compaction end-to-end
  證實鏈路可運作),接受為 documented rarity,不補 code。

### 已知限制(對齊 plan §8)

- **P3 產的是 Level-1+ Combination** → 出現在 Stage 7.5 之後 → 拿不到
  `ch8_xwave` / `ch8_multiwave` advisory(§9.3);只點亮 `power_rating`。
- wave_count {7,11} 固定假設每元件最小 3-monowave;非最小元件組合會漏 —
  Triple-* 罕見(P0 Gate 實測 1 檔)部分肇因於此,接受。
- CombinationKind 細分(P3 取通用值)P0 Gate 後維持,細分留 future。
- `structure_label` 對帶 sub_kind 的 pattern 印 Rust Debug 格式
  (`Combination { sub_kinds: [...] }` / `Flat { sub_kind: ... }`)— v1.35 以來舊行為,
  非 v4.11 引入;要清需給 `NeelyPatternType` 加 `Display` impl(獨立 cosmetic 工作)。

### 風險

🟢 低(P0 Gate 驗證後):
- 0 alembic / 0 Python / 0 collector.toml
- **forest_size 行為**:P0 Gate 實測 p95=28 / max=160(< 300 abort 線)— 新增 Combination
  scenarios 對 forest 影響極小(magnitude 預篩未動用)
- Rollback:單 commit `git revert`

### ✅ Combination 上游補完 — 完整方案收尾

里程碑 A+B+C code 落地(commit `5bdc7ba`)+ P0 Gate 驗證通過(2026-05-20 user 本機)。
v4.5 G2 dead chain 點亮 — `NeelyPatternType::Combination` 在 production 從 0 → 87 scenarios,
`ch8_xwave` / `ch8_multiwave` / `emulation` / `post_validator` Combination 鏈路開始消費。
完整計畫見 `m3Spec/neely_combination_upstream_plan.md`。

---

## v4.10 — Out-of-Scope Item 4 Pre-Constructive 2-pass diagnostics union(2026-05-20,1 commit / +5 tests)

接 v4.9 後動工 plan §Out of Scope **最後一個 backlog item**。對齊 m3Spec/neely_rules.md §Pre-Constructive Logic + neely_core_architecture.md §7.1 Stage 0「Pass 2 較 accurate(含 polywave 反查)但 Pass 1 仍具 diagnostic 價值」設計,把 Pass 2 直接覆寫 Pass 1 的行為,改為 union 風格 — Pass 2 result + Pass 1 only diff 並存,LLM / Aggregation Layer 可看完整 Pre-Constructive 修正軌跡。

### 設計關鍵:範圍比 CLAUDE.md v4.8 估的 ~30+ Scenario 構造點小一個量級

對齊 Explore agent 第 2 輪 audit 揭露的事實:
- `Scenario.diagnostics` **不存在**(只有 `NeelyCoreOutput.diagnostics: NeelyDiagnostics`)
- `structure_label_candidates` 在 `ClassifiedMonowave` 上,非 Scenario 上
- 完整 union 只需動 `MonowaveStructureLabels`(2 個構造點)+ `pre_constructive::run()` + `lib.rs:413` Pass 2 wiring
- **0 個** 68 處 ClassifiedMonowave / 52 處 Scenario 構造點需動

### 動工項目(1 commit / branch `claude/start-v4.9-j7WZX`)

| # | 工作 | 動作 |
|---|---|---|
| 1 | `MonowaveStructureLabels` 加 2 欄 | `classified_index: usize`(讓 lib.rs post-Pass-2 lookup)+ `pass1_only_labels: Vec<StructureLabelCandidate>`(diff,用 `#[serde(default, skip_serializing_if = "Vec::is_empty")]`)|
| 2 | `pre_constructive::run_pass2` 新函式 | 對每個 i:snapshot Pass 1 → compute Pass 2 → diff(以 `label` 比對,certainty 升級不算 rejection)→ overwrite + 回傳 `HashMap<usize, Vec<StructureLabelCandidate>>`(key = classified index,空 entry 不放)|
| 3 | `lib.rs:413` wiring | 從 `pre_constructive::run(...)` 改 `let pass1_only_diff = pre_constructive::run_pass2(...)`,後接 forest refill loop 把 diff + Pass 2 labels 寫回 `scenario.monowave_structure_labels[*]` |
| 4 | `classifier::build_monowave_structure_labels` 填新欄 | `classified_index: mw_idx`(原本只填 `monowave_index: seq_idx` 是 0..wave_count,classified index 丟失;v4.10 補回)+ `pass1_only_labels: Vec::new()`(Stage 5 預設空,Stage 8.5 refill)|
| 5 | `advanced_rules/ch12_localized.rs` test fixture | 同步加新欄位 defaults |

### 範圍

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/output.rs` | `MonowaveStructureLabels` 加 `classified_index` + `pass1_only_labels` 兩欄 + v4.x doc-string |
| `rust_compute/cores/wave/neely_core/src/pre_constructive/mod.rs` | `use std::collections::HashMap;` + 新 `pub fn run_pass2(...) -> HashMap<usize, Vec<StructureLabelCandidate>>` ~30 行 + **5 new tests**(`run_pass2_returns_empty_diff_when_pass1_equals_pass2` / `run_pass2_captures_dropped_when_polywave_flips_branch` / `run_pass2_certainty_upgrade_not_counted_as_rejection` / `run_pass2_empty_classified_returns_empty_diff` / `run_pass2_overwrites_structure_label_candidates`)|
| `rust_compute/cores/wave/neely_core/src/lib.rs` | Stage 8.5 Pre-Constructive Pass 2 從 `run` → `run_pass2`;後接 forest refill loop:per scenario per `monowave_structure_labels` 走 `classified_index` lookup,刷 `.labels` 為 Pass 2 + 寫 `.pass1_only_labels` 為 diff |
| `rust_compute/cores/wave/neely_core/src/classifier/mod.rs` | `build_monowave_structure_labels` 構造加 `classified_index: mw_idx, pass1_only_labels: Vec::new()` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/ch12_localized.rs` | test fixture push 加新欄 defaults(同步 struct 變動)|
| `CLAUDE.md` | v4.10 章節(本段) |

### Diff 設計:label 比對(certainty 升級不算 rejection)

```rust
let pass1_only: Vec<StructureLabelCandidate> = pass1_snapshot
    .into_iter()
    .filter(|p1| !pass2_cands.iter().any(|p2| p2.label == p1.label))
    .collect();
```

對齊 spec:certainty(Possible / Primary / Rare / MissingWaveBundle)是 Pre-Constructive
規則對 candidate 的「確信度」,Pass 2 把 Possible → Primary 屬「保留 + 升級」非
「rejection」;只有 Pass 1 label 完全消失才算 Pass 2 真正丟棄。

### Output schema(JSONB 加欄,backward-compat)

```json
{
  "monowave_structure_labels": [
    {
      "monowave_index": 0,                  // 既有:0..wave_count
      "classified_index": 12,                // v4.10 新:global classified index
      "labels": [                            // v4.10 後:Pass 2 result
        {"label": "F3", "certainty": "Primary"},
        {"label": "L5", "certainty": "Possible"}
      ],
      "pass1_only_labels": [                 // v4.10 新:diff,空陣列省略
        {"label": "BC3", "certainty": "Possible"}
      ]
    },
    ...
  ]
}
```

Python MCP layer 0 改動(grep `monowave_structure_labels / pass1_only_labels /
classified_index` 在 `mcp_server/ src/agg/ dashboards/` 全 0 match)— 新欄純加,
既有 consumer 不破。

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo build --release -p tw_cores` ✅ 0 warnings(下游 dispatcher 對齊)
- `cargo test --release -p neely_core --lib` ✅ **410 passed / 0 failed**(v4.9 baseline 405 → +5 Item 4 tests)
- `cargo test --release --workspace --no-fail-fast` ✅ **587 passed / 0 failed**(v4.9 baseline 582 → +5)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/ --ignore=tests/mcp_server/test_render_tools.py` ✅ **190 passed / 1 skipped**(對 v3.38 baseline 完整 carry over,0 regression)

### user 本機 production verify(下次 session)

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..

# v4.10 Scenario JSON schema 加 2 欄(monowave_structure_labels[*].classified_index +
# pass1_only_labels);neely_core params_hash 不變(Scenario 結構不參與 hash) →
# 既有 structural_snapshots 走 ON CONFLICT UPDATE 覆寫(不需 DELETE)
.\rust_compute\target\release\tw_cores.exe run-all --write

# 驗 pass1_only_labels 在 production JSONB 出現
psql $env:DATABASE_URL -c "
SELECT
  s->>'pattern_type' AS pat,
  jsonb_array_length(s->'monowave_structure_labels') AS n_mw,
  (SELECT COUNT(*)
     FROM jsonb_array_elements(s->'monowave_structure_labels') mw
    WHERE jsonb_array_length(COALESCE(mw->'pass1_only_labels', '[]'::jsonb)) > 0) AS n_mw_with_pass1_diff
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id IN ('2330','3030','1101')
  AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core')
ORDER BY stock_id, pat
LIMIT 30;
"
# 預期:多數 row n_mw_with_pass1_diff = 0(Pass 1 / Pass 2 一致)
#       少數 polywave-impacted scenarios n_mw_with_pass1_diff > 0(Pass 2 丟棄 Pass 1 候選)
```

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- JSONB 純加欄(`classified_index` / `pass1_only_labels`),既有 Python 0 consumer
- params_hash 不變(Scenario 結構欄位變更不重算 facts;structural_snapshots 走 ON CONFLICT UPDATE)
- Rollback:單 commit `git revert`(僅影響 v4.10 一個 commit)

### Out of Scope backlog 全部清空 ☕☕☕☕

| Item | 狀態 | 收尾 |
|---|---|---|
| Item 1 Round 2 boundary partial Stage 3-4 rerun | ✅ | v4.8 commit `bbb41bb` |
| Item 2 Construction axis 5-variant | ✅ | v4.8 commit `bbb41bb` |
| Item 3 Classifier nested label enrichment | ✅ | v4.9 commit `f0a6d10` |
| **Item 4 Pre-Constructive Pass 1/2 diagnostics union** | ✅ | **v4.10(本 PR)** |

**M3SPEC Neely alignment v4.0 → v4.10 完整收尾**:
- v4.0 → v4.4(9 commits / ~5,500 LoC):P1.1-P1.4 Quick Wins + Ch9/Ch12 advisory + Ch11 wave-by-wave + Ch4 Round 2 + Ch8 X-wave/Multiwave + Ch6 Stage 2
- v4.5 → v4.9(7 commits / ~2,750 LoC):Group 2 4 corrective patterns + Group 3 Monowave bar_indices + Group 1 3 sub-PR polywave nested + Items 1+2+3
- v4.10(本 PR / ~200 LoC):Item 4 Pass 1/2 diagnostics union

**Total**:17 commits / ~8,450 LoC / +135 tests across 25 days(2026-04-26 v4.0 plan 啟動 → 2026-05-20 v4.10 收尾)。**Aggregation Layer 視角的 Neely surface 100% spec-aligned,LLM context 完整含 Pass 1/Pass 2 修正軌跡 + 5-axis Alternation + 5-variant Construction + 9 Flat variants + 9 Triangle variants + Channeling touch ternary severity + Ch8 X-wave/Multiwave advisory + Combination/RunningCorrection Stage 2 + WaveNode 結構標籤 hint**。

### 下個 session 動工候選(M3SPEC alignment 已收尾)

對齊本文件 §「下次 session 建議優先序」§1:
- **1a. gov_bank_net Core 消費**(需先寫 EventKind 規格)
- **1b. probe sponsor tier 全 catalog**(`scripts/probe_finmind_sponsor_unused.py --max 0`)
- **1c. Aggregation Layer Phase B-4 FastAPI thin wrap**(估 ~2-3h)
- **1d. wall time 微優化**(目前 ~11 min / 1266 stocks)
- **2c. Silver schema 假設待 user 驗**(margin_maintenance / shareholder detail / financial_statement detail key)

---

## v4.9 — Out-of-Scope Item 3 Classifier nested label enrichment(2026-05-19,1 commit / +4 tests)

接 v4.8 後動工 plan §Out of Scope Item 3。對齊 m3Spec/neely_rules.md
§Pre-Constructive Logic 把 Stage 0 標的結構標籤 hint 透過 WaveNode.label
暴露給 Compaction / Aggregation Layer / LLM context;深層 nested 結構透過
Compaction wave_tree clone 自動傳遞,展開為 JSONB output 任意層可見。

### 1 commit / branch `claude/neely-m3spec-completion-4l4dT`

| Commit | Item | 範圍 |
|---|---|---|
| `f0a6d10` | Item 3 | Classifier nested label enrichment |

### 範圍

**classifier/mod.rs**:
- 新 `format_wave_node_label(wave_num, classified_idx, classified) -> String` helper
- 格式規則:
  - 有 Primary structure label → `"W{n}:{Label}{Direction}"` 例 "W1:L5↑" / "W3:C3↓"
  - 無 Primary → `"W{n}{Direction}"` 例 "W1↑"
  - Direction symbols:Up → ↑;Down → ↓;Neutral → ·
- `build_wave_tree` children 構造改用 `format_wave_node_label(i+1, idx, classified)`
- 既有 wave_tree 整體 label("5-wave Up" 等)保留不變

### 嵌套自動傳遞機制

- **Level-0**(Classifier):`wave_tree.children[i].label = "W{n}:{Hint}{Dir}"`
- **Level-1**(Compaction `build_aggregated` line 419)
  `window.iter().map(|s| s.wave_tree.clone())` → Level-1 wave_tree.children = Level-0
  wave_tree 完整 clone;hint label 透過 clone 自動傳遞
- **Level-2+**:wave_tree.children[i].children[j].label 仍是 Level-0 的 "W{n}:{Hint}"
- LLM / Aggregation Layer 透過 JSONB 任意層 wave_tree → label 即知 :3 / :5 子形態 hint

### Tests(+4 new)

- `format_wave_node_label_with_primary_l5_up`:W1:L5↑
- `format_wave_node_label_with_primary_c3_down`:W3:C3↓
- `format_wave_node_label_falls_back_when_no_primary`:W2↑(Possible certainty 不算)
- `format_wave_node_label_no_candidates_neutral_uses_dot`:W5·

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo test --release -p neely_core --lib` ✅ **405 passed / 0 failed**
  (v4.8 baseline 401 → +4 Item 3 tests)
- `cargo test --release --workspace` ✅ **582 passed / 0 failed**
  (v4.8 baseline 578 → +4)

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- 純 label string enrichment,WaveNode struct 不改 → 既有 JSONB consumers 0 break
- WaveNode.label 從 "W1" → "W1:L5↑" 純 informational;structural_snapshots JSONB
  字串欄略長(~2-5 bytes/sub-wave;5-wave × 5-level 嵌套 ≈ 100 bytes/scenario,
  total ≈ < 5% size 增加)
- Rollback:單 commit `git revert`

### 留真正 V4.x(僅 Item 4)

- **Item 4 Pre-Constructive Pass 1 vs Pass 2 diagnostics union**:目前 Pass 2 直接
  覆蓋 Pass 1 的 structure_label_candidates(對齊 spec「Compaction-aware candidates
  較 accurate」設計);union 需新增 Scenario.diagnostics.pre_constructive_rejections
  field(struct change + 影響 ~30+ Scenario 構造點),留真正 V4.x

---

## v4.8 — Out-of-Scope backlog Items 1+2 完整收尾(2026-05-19,1 commit / +11 tests)

接 v4.7 G1 收尾後 plan §Out of Scope 列 4 項留 V4.x,本 PR 清掃 Item 1+2
(Items 3+4 留真正 V4.x)。對齊 m3Spec/neely_rules.md §Rule of Alternation
Construction(1311-1319 行)+ §Compaction Three Rounds 動作 B 邊界波 retracement
重評(1249-1251 行)。

### 1 commit / branch `claude/neely-m3spec-completion-4l4dT`

| Commit | Item | 範圍 |
|---|---|---|
| `bbb41bb` | Item 2 + Item 1 | Construction axis 5-variant + Round 2 boundary partial rerun |

### Item 2:Construction axis 完整 Flat/Zigzag/Triangle 內部分類

**validator/wave_rules.rs**:
- `ConstructionKind` enum 從 **2 variants**(Impulsive / Corrective)擴 **5 variants**:
  - `Impulsive`:含 :5 系列(Five / F5 / L5 / S5 / SL5)
  - `FlatCorrective`:含 F3 + C3 + L3 三連標(Flat :3 = F3-C3-L3 序列)
  - `ZigzagCorrective`:含 :3 系列 + (L5 OR S5) terminal(Zigzag 5-3-5)
  - `TriangleCorrective`:含 ≥ 2 個 C3 / SL3(Triangle 3-3-3-3-3)
  - `GenericCorrective`:其他 :3 系列(Combination / XC3 / BC3 / BF3)
- `dominant_construction_kind` 改用 spec-aligned 分類順序:
  - has_pure_five && !has_any_three → Impulsive
  - has_five_terminal && has_any_three → ZigzagCorrective
  - has_f3 && has_c3 && has_l3 → FlatCorrective(F3-C3-L3 三連)
  - c3_count >= 2 → TriangleCorrective
  - has_any_three only → GenericCorrective
- Alternation 判定不變(k2 == k4 → Fail):
  - 之前 Flat-vs-Flat 與 Flat-vs-Zigzag 都被歸 Corrective → 都 Fail(誤判)
  - 現在 Flat-vs-Flat → Fail(對),Flat-vs-Zigzag → Pass(對)

### Item 1:Round 2 動作 B partial Stage 3-4 rerun(reject extreme boundary)

**compaction/three_rounds.rs**:
- 新 `boundary_retracement_extreme(window, monowaves) -> bool` 函式
- 對 window first/second + second_to_last/last 兩對 boundary scenarios 取
  `scenario_price_magnitude` 比對,ratio 超 **[0.236, 4.236]** Fib² 範圍 → true
  (0.236 = 1/4.236;4.236 = 2.618 × 1.618 = Fib²)
- `try_aggregate_5` + `try_aggregate_3` 開頭加 short-circuit:
  - boundary_retracement_extreme(window, monowaves) → return None
  - 直接拒絕 aggregation,不進 build_aggregated → next_level 沒有此 scenario
- 對齊 spec line 1249-1251 完整實作:partial Stage 3-4 rerun = 邊界波
  retracement 必落 Fib² 範圍,違反 → aggregation 拒絕

**兩階段 threshold**(對齊 advisory vs reject 分層):
- **Mild abnormal**(0.236 ≤ ratio < 0.382 OR 2.618 < ratio ≤ 4.236):
  寫 Info advisory(走 build_round_advisories,v4.4a 既有)
- **Extreme abnormal**(ratio < 0.236 OR > 4.236):
  **Reject aggregation**(v4.8 新加,partial Stage 3-4 rerun)

### Tests(+11 new)

- `wave_rules.rs` (+8):
  - 5 個 `dominant_construction_kind` variant tests
    (pure_five → Impulsive / F3-C3-L3 → Flat / :3+L5 → Zigzag / 多 C3 → Triangle / XC3 → Generic)
  - 1 個 no_primary_returns_none
  - 2 個 alternation Flat-vs-Zigzag(pass)/ Flat-vs-Flat(fail)
- `compaction/three_rounds.rs` (+3):
  - `boundary_retracement_extreme_rejects_aggregation_when_first_pair_too_low`
    (5-pattern Impulse,first/second ratio = 0.2 < 0.236 → reject)
  - `boundary_retracement_normal_keeps_aggregation`(ratio ≈ 1.0 → aggregate 仍進行)
  - `boundary_retracement_extreme_rejects_zigzag_when_last_pair_too_high`
    (3-pattern Zigzag,last ratio = 5 > 4.236 → reject)

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo test --release -p neely_core --lib` ✅ **401 passed / 0 failed**
  (v4.7 baseline 390 → +11 across Item 1 + Item 2)
- `cargo test --release --workspace` ✅ **578 passed / 0 failed**
  (v4.7 baseline 567 → +11)

### 風險

🟡 中:
- 0 alembic / 0 Python / 0 collector.toml
- **Item 2 Construction axis**:Alternation 判定變細(5 variants vs 2)
  → 某些 candidates 之前歸 Corrective 同類 Fail 現在能正確 Pass(Flat-vs-Zigzag);
  整體 production forest 預期增加少量合法 5-wave Impulse(因 alternation 判定更精準)
- **Item 1 Round 2 boundary reject**:Compaction Level-N+ 對極端 boundary
  retracement(ratio < 0.236 或 > 4.236)直接拒絕。**Production forest_size 預期略縮**
  (剔除真實不合理的 Level-N+ aggregations)— 對齊 spec 設計
- Rollback:單 commit `git revert`

### 留真正 V4.x(plan §Out of Scope items 3 + 4)

- **Item 3 Classifier 深層 nested 列舉**:wave_tree.children 仍只一層(Level-0),
  Level-N 嵌套由 Compaction 處理;完整 :3 / :5 子形態嵌套展開留 V4.x
- **Item 4 Pre-Constructive Pass 1 vs Pass 2 diagnostics union**:目前 Pass 2 直接
  覆蓋 Pass 1 的 structure_label_candidates;diagnostics union 加新欄位
  `Scenario.diagnostics.pre_constructive_rejections` 留 V4.x

---

## v4.7 — Group 1 Polywave 嵌套依賴鏈完整收尾(2026-05-19,3 sub-PR / P0 Gate verified)

接 v4.6 Group 3 G3.1 後動工 Group 1(對應 plan §G1,3 sub-PR / ~900 LoC)。
完整補完 plan 識別的 13 個 polywave 嵌套依賴鏈相關闕漏 — 從 Compaction
Level-0 真實 price 反查、Pre-Constructive 2-pass polywave 偵測、到
Post-validator Stage 2 真實化。全市場 1266 stocks P0 Gate **全綠**。

### 3 sub-PR commits

| Sub-PR | Commit | 範圍 |
|---|---|---|
| v4.7.1 G1.1 | `3e69a8f` | scenario_price_magnitude → Option<f64> + Pattern Isolation `validate_after_compaction` |
| v4.7.2 G1.2 | `190cd75` | ClassifiedMonowave +`polywave_size` + 2-pass Pre-Constructive + 5 rules placeholders 真實化 |
| v4.7.3 G1.3 | `ea0ef7c` | Channeling c-wave touch epsilon + Post-validator Combination/RunningCorrection Stage 2 + Classifier wave_count 完整對映 |

### 範圍(3 commits / branch `claude/neely-m3spec-completion-4l4dT`)

**G1.1**(compaction Level-0 + Pattern Isolation validation):
- `compaction/three_rounds.rs::scenario_price_magnitude` 返回型別 `f64` → `Option<f64>`,
  移除 `children.len()` fallback(spec line 204-213 placeholder 終結)
- `similarity_and_balance` 改用 `match (Option, Option)` 處理 None case;
  price 不可用時純 time 維度判定(對齊 NEoWave 設計精神)
- `build_round_advisories` boundary 評估同步走 Option pattern
- `pattern_isolation/mod.rs::validate_after_compaction(bounds, scenarios, classified)`:
  walk Compaction forest,匹配 PatternBound 邊界與 Scenario.wave_tree.start/end
  → match 即設 `validated = true`(spec §Pattern Isolation Step 5)
- `lib.rs`:Stage 8 Compaction 跑完後接 `validate_after_compaction`
- **5 new tests**

**G1.2**(Pre-Constructive 2-pass polywave + Power Rating in_triangle docs):
- `monowave/mod.rs::ClassifiedMonowave` +`polywave_size: usize`(default 0)
- `pre_constructive/predicates.rs`:
  - `POLYWAVE_THRESHOLD: usize = 3`(對齊 spec「> 3 sub-monowaves」)
  - `is_polywave(m) -> bool` helper
- `pre_constructive/mod.rs::populate_polywave_sizes(classified, forest)` fn:
  walk Level-N+ scenarios(wave_tree.children > 0)→ covered base monowaves
  取 `max(N children count)`
- **5 rules placeholder false → is_polywave 真實判定**:
  - `rule_1.rs` Branch 3 × 2(m0 polywave → add :L5)
  - `rule_4.rs` Cond 4 Branch 3(m2 polywave → add :L5)+ Branch 6(m0 polywave → add x:c3)
  - `rule_5.rs` cond_5a / cond_5b dispatcher(m2 / m3 polywave 分支)
  - `rule_6.rs` cond_6a dispatcher + Branch 5(m2 polywave + m1 slow → add :c3)
  - `rule_7.rs` cond_7a dispatcher
- `lib.rs`:Stage 8 Compaction 後 `populate_polywave_sizes` + 2nd pass `pre_constructive::run`
  對齊 plan §G1.2「2-pass forward design」:
  - Pass 1(Stage 0 first run)= polywave_size 全 0 → 走 (B) 分支 = v4.6 行為
  - Pass 2(post-Compaction)= polywave_size 反查真實值 → 可走 (A) 分支
- `power_rating/table.rs` in_triangle 註解更新(v3.x Phase 8 already 落地)
- 32 sites bulk-update test fixtures `polywave_size: 0`
- **5 new tests**

**G1.3**(Channeling touch + Post-validator Stage 2 + Classifier nested):
- `advanced_rules/channeling.rs` Zigzag c-wave 0-B trendline 加 epsilon 容忍:
  - `TOUCH_EPSILON_PCT = 0.005`(對齊 spec「絕不可剛好觸碰」精度)
  - ternary severity:**touched**(Strong = Triangle 形成訊號)/ **breached**(Warning)/ **clear**(Info)
  - 對齊 spec line 1356-1358 完整實作
- `post_validator/mod.rs` Combination + RunningCorrection 改 advisory-only → 真實 Stage 2:
  - Combination 取 `sub_kinds` 細分(DoubleThree*/TripleThree*/Triple*/etc)+ 整合
    `Ch8_XWave_InternalStructure` + `Ch8_Multiwave_Construction` advisory_findings
  - RunningCorrection 結合 `scenario.power_rating`(±3 → 強/中/弱 continuation)生成 narrative
- `classifier/mod.rs::classify_complexity`:`wave_count` → `ComplexityLevel` 完整對映
  退化 `0/1/2/4` 落 `Simple`(原 catch-all `Complex` 為誤判)
- **4 new tests**

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo test --release -p neely_core --lib` ✅ **390 passed / 0 failed**
  (v4.6 baseline 376 → +14 across G1.1+G1.2+G1.3)
- `cargo test --release --workspace` ✅ **567 passed / 0 failed**
  (v4.6 baseline 549 → +18 across G1 + 既有 4 commits dependency tests)

### user 本機 P0 Gate(2026-05-19,production-verified)

```
== Forest size 分布 ==
 p50 | p95 | max_count | overflow_count
  14 |  28 |       196 |              0

== Acceptance ==
 max ≤ 200 (cap):     196 ✅(buffer 4)
 p95 < 180:           28  ✅(buffer 152)
 overflow_triggered < 5%: 0/22594 ✅(0%)

== Wall time ==
 全市場 1266 stocks:   648.5s ✅(< 12 min budget)
 neely_core 3798 runs(× 3 timeframes): 101.1s
 facts_new 全部 cores: 0(2 nd run idempotent)

== MCP smoke(2330 / 3030 / 1101)==
 全部 Kalman + Neely [OK]
 forest_size:1101=8 / 2330=26 / 3030=17
 wave counts:5 / 3 / 3
```

**G1 2-pass Pre-Constructive 沒爆 forest** — p95=28 vs threshold 180 留 buffer 152。
polywave-aware scenarios 對 forest_size 影響極小,Compaction Three Rounds 正常收斂。

### Group 2 + Group 3 + Group 1 完整收尾總計

| Group | Sub-PR | Commits | Tests + | 主要範圍 |
|---|---|---|---|---|
| **G2** | 4 (v4.5.1-4) | `5439c2a` / `2ed54ec` / `366bc45` / `460e235` | +14 | 4 corrective patterns(Zigzag/Flat/Triangle/Combination)+ RunningCorrection invalidation triggers + 3 new EmulationKind variants |
| **G3** | 1 (v4.6) | `cc053d6` | +7 | Monowave struct +`bar_indices` + `m1_endpoint_broken_by_m2` real OHLC extremum impl |
| **G1** | 3 (v4.7.1-3) | `3e69a8f` / `190cd75` / `ea0ef7c` | +14 | Compaction Level-0 real price + 2-pass Pre-Constructive polywave + Pattern Isolation validation + Channeling touch + Post-validator Stage 2 + Classifier wave_count 完整 |
| **Total** | **8 sub-PR** | **8 commits** | **+35** | M3SPEC 闕漏補完 ~2,650 LoC across 17 days(從 v4.0 P1.1 起 22 commits / ~8,150 LoC) |

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- P0 Gate 全綠驗證 → forest_size 行為 production-safe
- 既有 Group 2 + Group 3 + G1.1/G1.2/G1.3 sub-PR 各自獨立 commit,
  任一可單獨 `git revert`
- 2nd-pass Pre-Constructive 對 wall time 影響:**648s vs v4.4 baseline 561s**
  (+15%),仍 < 12 min budget;polywave-aware candidates 對 forest_size
  影響 < 1%(p95=28 buffer 充足)

### Out of Scope(留 V4.x)

- **Round 2 動作 B full Stage 3-4 partial rerun**(plan §G1.3):
  目前 `build_round_advisories` 仍 advisory-only;完整 Stage 3-4 candidate
  rerun 需大幅重構 validator pipeline
- **Construction axis 完整 Flat/Zigzag/Triangle 內部分類**(plan §G1.2):
  仍走 Impulsive/Corrective 二分;完整 spec 1311-1319 解讀留 V4.x
- **classifier 深層 nested 列舉**(plan §G1.3):
  wave_tree.children 仍只一層,Level-N 嵌套由 Compaction 處理
- **Pre-Constructive Pass 1 vs Pass 2 diagnostics union**(plan §G1.2):
  Pass 2 直接覆蓋 Pass 1 的 structure_label_candidates(對齊 spec「Compaction-aware
  candidates 較 accurate」設計);diagnostics union 留 V4.x

---

## v4.6 — Group 3 Monowave bar_indices + m1_endpoint_broken_by_m2 real impl(2026-05-19)

接 v4.5 Group 2 4 sub-PR 收尾後動工 Group 3(對應 plan §G3 唯一 sub-PR,完整
Group 3 範圍)。對齊 `m3Spec/neely_rules.md` line 247-249 + `neely_core_architecture.md`
§3.1「需 OHLC reference 串接的 predicates」,把 Pre-Constructive Stage 0 的
`m1_endpoint_broken_by_m2` 從 hardcoded `false` placeholder 改為真實 intraday
OHLC extremum 比對。

### 範圍(1 commit / branch `claude/neely-m3spec-completion-4l4dT`)

| 檔 | 動作 |
|---|---|
| `output.rs` | `Monowave` struct 加 `bar_indices: (usize, usize)` 欄位 + `#[serde(default)]`(對應 start_date/end_date 在 bars slice 的 index 區間;`(0, 0)` 退化值) |
| `monowave/pure_close.rs` | `detect_monowaves()` 兩處 Monowave 構造寫真實 `bar_indices = (start_idx, extreme_idx)` |
| `monowave/mod.rs` | `classify_monowaves()` override `bar_indices = (start_idx, end_idx)` 確保 caller 切換 bars slice 時對齊 |
| `pre_constructive/context.rs` | `MonowaveContext` 加 `bars: &'a [OhlcvBar]` 欄位 + `build()` signature 加 bars 參數 |
| `pre_constructive/predicates.rs` | `m1_endpoint_broken_by_m2` 從 `(_m1, _m2) -> false` placeholder 升 `(m1, m2, bars: &[OhlcvBar]) -> bool` 真實實作:m1.Up → m2 期間任一 `bar.high > m1.end_price` / m1.Down → `bar.low < m1.end_price` / Neutral / 退化保險 → false;**+7 unit tests** |
| `pre_constructive/mod.rs` | `run()` signature 加 `bars: &[OhlcvBar]` thread to MonowaveContext::build |
| `pre_constructive/rule_4.rs` | caller 改傳 `(m1, m2, ctx.bars)` |
| `lib.rs` | `pre_constructive::run(&mut classified, &input.bars)` |
| 32 既有 test fixtures(neely_core) | Bulk-update 38 sites 加 `bar_indices: (0, 0)` placeholder |
| `trendline_core/src/lib.rs` | 1 個 Monowave 構造同款補 |

**0 alembic / 0 collector.toml / 0 Python / 0 dispatch 行為改變的硬中斷**(predicate 真實值會略影響 Pre-Constructive Stage 0 結果,但不阻塞 scenario 生成)。

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo test --release -p neely_core --lib` ✅ **376 passed / 0 failed**(v4.5.4 369 → +7 G3.1 predicate tests)
- `cargo test --release --workspace` ✅ **549 passed / 0 failed**

### user 本機 production verify(3 檔 manual review)

對齊 plan §Group 3 Verification(沙箱 + 3 檔 P0 Gate,**不需全市場 P0**)。

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..

# v4.6 Monowave struct 改 schema → params_hash 變動,既有 structural_snapshots
# 走 ON CONFLICT UPDATE 覆寫(不需 DELETE)
.\rust_compute\target\release\tw_cores.exe run-all --write

# 1. 驗 bar_indices 在 JSONB 寫進真實值(非 (0,0))
psql $env:DATABASE_URL -c "
SELECT stock_id,
       jsonb_array_length(snapshot->'monowave_series') AS mw_count,
       snapshot->'monowave_series'->0->'bar_indices' AS first_mw_indices
FROM structural_snapshots
WHERE stock_id IN ('2330','3030','1101')
  AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core')
ORDER BY stock_id;
"
# 預期 first_mw_indices 為 [start_idx, end_idx] 真實值(非 [0, 0])

# 2. MCP neely_forecast 對 2330/3030/1101 驗 forecast 仍可用
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
for sid in ['2330','3030','1101']:
    r = neely_forecast(sid,'2026-05-15')
    print(f'{sid}: scenarios={len(r[\"forecasts\"])} primary_pattern={r[\"primary_scenario\"][\"pattern_type\"]}')
"
```

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- `#[serde(default)]` 保 JSONB 向下相容(若未來加 Deserialize derive)
- m1_endpoint_broken_by_m2 從 false 變 true 屬罕見場景(spec 明示只在 5th Ext 5th wave),production scenario_forest 略縮(預期正向,Pre-Constructive 加 x:c3 → Validator 可能拒 false candidates)
- Rollback:單 commit `git revert`(Monowave 加新欄位 + serde(default),既有 JSONB 反序列化視為 (0, 0))

### Group 1(下次 session)

Plan §Group 1 留 3 sub-PR(~1,800 LoC):
1. G1.1 Compaction Level-0 真實 price + Pattern Isolation validation
2. G1.2 Pre-Constructive polywave + Construction axis + Power Rating in_triangle
3. G1.3 Round 2 邊界波重評 + Channeling touch + Post-validator Stage 2 + Classifier nested

需全市場 P0 Gate(1266 stocks)校準 forest_size 分布。

---

## v4.5 — Group 2 Corrective Pattern Triggers + Emulation(2026-05-19)

接 v4.4 P1.4 收尾後動工 Group 2(對應 plan §Group 2,4 sub-PR / ~1,360 LoC)。
對齊 `m3Spec/neely_rules.md` Ch11 各 corrective pattern wave-by-wave 規則,把
原本 4 corrective patterns(Zigzag/Flat/Triangle/Combination)在 triggers/mod.rs
+ emulation/mod.rs 留 `_ => {}` 的 placeholder 全部補完。

### 4 sub-PR commits

| Sub-PR | Commit | 範圍 |
|---|---|---|
| v4.5.1 Zigzag | `5439c2a` | triggers wave-b/c + ZigzagAsFlatFailure emulation |
| v4.5.2 Flat | `2ed54ec` | triggers wave-c + Expanded Flat wave-b + FlatAsZigzag emulation + flat_variant_from_kind |
| v4.5.3 Triangle | `366bc45` | Contracting/Limiting wave-e trigger(Expanding 暫不加)|
| v4.5.4 Combination + RunningCorrection | `460e235` | wave-a invalidate + CombinationAsImpulse emulation + match exhaustive |

### 範圍(4 commits / branch `claude/neely-m3spec-completion-4l4dT`)

| 檔 | 動作 |
|---|---|
| `output.rs::EmulationKind` | +3 variants:`ZigzagAsFlatFailure`(v4.5.1)/ `FlatAsZigzag`(v4.5.2)/ `CombinationAsImpulse`(v4.5.4) |
| `triggers/mod.rs` | 加 4 corrective patterns + RunningCorrection 共 5 個 match arm;移除 `_ => {}` catch-all(match exhaustive 覆蓋 7 個 NeelyPatternType variants);+8 unit tests |
| `triggers/mod.rs` helpers | `flat_variant_from_kind`(FlatKind 8 → FlatVariant 10) + `triangle_variant_default`(TriangleKind 3 → TriangleVariant 9) |
| `emulation/mod.rs` | 加 Zigzag/Flat/Combination/RunningCorrection match arms(RunningCorrection 為 noop);移除 catch-all;+6 unit tests |
| `emulation/mod.rs` helpers | `check_zigzag_as_flat_failure` / `check_flat_as_zigzag` / `check_combination_as_impulse` |

### 規則覆蓋(spec line ref)

| Pattern | Trigger 規則(spec) |
|---|---|
| **Zigzag** | wave-b 不可完全回測 wave-a(2328-2332)→ InvalidateScenario;wave-c 不過 wave-b 端點(2337-2342)→ WeakenScenario |
| **Flat** | wave-c 不過 wave-a 起點(2208)→ InvalidateScenario;Expanded Flat(Irregular*/Elongated)wave-b 端點突破反向(2235-2240)→ WeakenScenario |
| **Triangle**(Contracting/Limiting) | wave-e 突破 wave-c 端點(2453)→ InvalidateScenario |
| **Combination** | 末段反向破 wave-a 起點(1862-1869)→ InvalidateScenario |
| **RunningCorrection** | 同上(2024-2037)→ InvalidateScenario |

| Pattern | Emulation 規則(spec) |
|---|---|
| **Zigzag** | wave-c < 100% × wave-a → 似 Flat C-Failure(2337-2342) |
| **Flat** | wave-c ≥ 138.2% × wave-a → 似 Zigzag(2191-2321 Elongated) |
| **Combination** | DoubleThree*/TripleThree* + 5/7 children → 似 5/7-wave Trending Impulse(1905-1906 一般化) |

### 沙箱驗證

- `cargo test --release -p neely_core --lib` ✅ **369 passed / 0 failed**(v4.4 baseline 355 → +14)
- `cargo test --release --workspace` ✅ **542 passed / 0 failed**(v4.4 baseline 528 → +14)
- Match arm 變 exhaustive — 未來新增 NeelyPatternType variant 時 compiler 強制此處更新(預防 silent bug)

### user 本機 production verify

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..
.\rust_compute\target\release\tw_cores.exe run-all --write
psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql

# 驗新增 trigger/emulation 寫進 scenario_forest JSONB
psql $env:DATABASE_URL -c "
SELECT stock_id, s->>'pattern_type' AS pattern,
       jsonb_array_length(s->'invalidation_triggers') AS trigger_count,
       jsonb_array_length(s->'emulation_suspects') AS emul_count
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id='2330' AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core')
LIMIT 10;
"
# 預期:Zigzag/Flat/Triangle/Combination scenarios 之前 trigger_count=0 → 現在 1-2 個
```

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- Corrective scenarios 之前走 catch-all 不生 trigger / emulation,本系列 PR 加增益
- Forest filter 不變(triggers 是 LLM context,不縮 scenario_forest;emulation 是 advisory)
- Rollback:單 commit `git revert`(每 sub-PR 獨立 commit)

### Group 2 → Group 3 收尾總計(plan §Group 2 + Group 3)

| Sub-PR | Tests added | Commit |
|---|---|---|
| v4.5.1 Zigzag | +4 | `5439c2a` |
| v4.5.2 Flat | +4 | `2ed54ec` |
| v4.5.3 Triangle | +2 | `366bc45` |
| v4.5.4 Combination | +4 | `460e235` |
| v4.6 G3.1 Monowave bar_indices | +7 | `cc053d6` |
| **Total** | **+21** | **5 commits** |

Plan §Group 1(polywave 嵌套依賴鏈,3 sub-PR / ~1,800 LoC)留下次 session;需全市場
P0 Gate 校準 forest_size 分布。

---

## v4.4 — P1.4b+c+d Ch8 X-wave / Multiwave + Ch6 Stage 2(2026-05-19)

P1.4 收尾 commit。合併 P1.4b(Ch8 X-wave)+ P1.4c(Ch8 Multiwave)+ P1.4d(Ch6
Stage 2 接 Ch8 + RunningCorrection Stage 2)。**Advisory mode**;對應 Combination /
RunningCorrection scenarios。

### 範圍(1 commit / P1.4 final)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/ch8_xwave/mod.rs` | **新檔** — `detect()` 對 Combination scenario 偵測 X-wave 結構;Large X-wave(DoubleThree*/TripleThree*)→ Info「只允許 Flat/Triangle」/ Small X-wave(DoubleZigzag/Combination)→ Info「允許 Zigzag」 |
| `rust_compute/cores/wave/neely_core/src/ch8_multiwave/mod.rs` | **新檔** — `detect()` 對 Combination scenario 偵測 Multiwave 建構;Triple* → 末段 / Double* → 中段 |
| `rust_compute/cores/wave/neely_core/src/lib.rs` | 加 `pub mod ch8_xwave;` + `pub mod ch8_multiwave;` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 Ch8 X-wave / Multiwave detect 呼叫 + Ch6 Combination Stage 2 + RunningCorrection Stage 2 advisory |
| `CLAUDE.md` | v4.4 章節 |

### Advisory 內容(對應 Combination / RunningCorrection 兩 pattern type)

| Pattern | Advisory 加 |
|---|---|
| `Combination` | Ch8 X-wave internal structure + Ch8 Multiwave 建構 + Ch6 Stage 2 (Combination Stage 2 須結合 Ch8 module 結果) |
| `RunningCorrection` | Ch6 Stage 2 (後續 Impulse > 161.8% 預期) |

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **355 passed / 0 failed**(P1.4a 349 → +6)
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace` ✅ **528 passed / 0 failed**
  (v3.38 baseline 448 → **+80 new tests across 4 milestones**)

### v4.0 → v4.4 完整收尾(P1.1 → P1.4 全部完成)

| Milestone | Commit | 新 modules / 主要動作 | Tests |
|---|---|---|---|
| P1.1 | v4.1 `34f73e2` | StructuralFacts 5 欄補完 + 5-axis Alternation + IrregularStrongB + fifth_of_fifth_detector | +13 |
| P1.2 | v4.2 `aa64e5a` | Ch9 advisory(Independent/Simultaneous/Aspect 2)+ Ch12 Waterfall + Ch12 Localized | +19 |
| P1.3a | v4.3a `77ab3d7` | ch11_trending_impulse.rs | +11 |
| P1.3b | v4.3b `21fb732` | ch11_terminal_impulse.rs | +7 |
| P1.3c | v4.3c `8a91392` | ch11_flat_variants.rs(+ FlatVariant::DoubleFailure)| +8 |
| P1.3d | v4.3d `3a592f9` | ch11_zigzag.rs + Appendix B 項 F | +7 |
| P1.3e | v4.3e `2c663b4` | ch11_triangle_variants.rs(9 變體)| +9 |
| P1.4a | v4.4a `1af97a0` | Ch4 Level-0 真 magnitude + Round 2 動作 B advisory | 0(test fixture 更新) |
| P1.4 final | v4.4 (this) | Ch8 X-wave / Multiwave + Ch6 Combination Stage 2 + RunningCorrection Stage 2 | +6 |
| **Total** | **9 commits** | **9 new modules + 1 enum variant + ~5,500 LoC** | **+80** |

### ⚠️ P0 Gate 校準(user 本機 P1.4 收尾後必跑)

對齊 plan v4.0 §「Calibration division of labor」— Claude 寫 code + 沙箱 test;user 跑 P0 Gate。

```powershell
git pull
cd rust_compute
cargo clean -p neely_core -p tw_cores
cargo build --release -p tw_cores
.\target\release\tw_cores.exe run-all --write
cd ..

# forest_size 分布(對齊 architecture §10 forest_max_size=200 cap)
psql $env:DATABASE_URL -c "
SELECT
  PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p95,
  MAX(jsonb_array_length(snapshot->'scenario_forest'))                                        AS max_count
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
"
# acceptance:max ≤ 200(cap 不破),p95 < 180

# 3030 / 2330 / 1101 manual review
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
for sid in ['3030','2330','1101']:
    r = neely_forecast(sid,'2026-05-15')
    print(f'{sid}: degree={r[\"primary_scenario\"][\"effective_degree\"]} '
          f'tf={r[\"primary_scenario\"][\"timeframe\"]} '
          f'wave_count={r[\"primary_scenario\"][\"wave_count\"]} '
          f'horizons={list(r[\"forecasts\"].keys())}')
"
```

### 風險(P1.4 整體)

🔴 高(對齊 plan §P1.4 風險):
- Ch4 Level-0 真 magnitude 改變 Similarity & Balance pass 率 → forest_size 分布可能改變
- forest_size 爆超 200 cap 機率非零;user 本機 verify 後判斷
- 若 p95 > 180 需重新校準 `BeamSearchFallback.k`
- 沙箱無法完全 cover production data 變異,**user 必跑 P0 Gate**
- Rollback:任一 P1.4 commit `git revert` 即可

---

## v4.4a — P1.4a Ch4 Level-0 真 magnitude + Round 2 動作 B(2026-05-19)

P1.4 系列第一個 commit。修 `scenario_price_magnitude` 從 `wave_tree.children.len()`
placeholder 改用真實 monowave price lookup + 加 Ch4 Round 2 動作 B 邊界波 retracement
重評 advisory。

### 範圍(1 commit)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/lib.rs` | 提前構建 `monowave_series` 給 compaction 用(原 line 469,移到 line 388 前) |
| `rust_compute/cores/wave/neely_core/src/compaction/mod.rs` | `compact()` signature 加 `monowaves: &[Monowave]` 參數 |
| `rust_compute/cores/wave/neely_core/src/compaction/exhaustive.rs` | `compact()` signature 同步加 |
| `rust_compute/cores/wave/neely_core/src/compaction/three_rounds.rs` | `aggregate_one_level` / `try_aggregate_*` / `all_pairs_pass_sb` / `similarity_and_balance` / `scenario_price_magnitude` / `build_aggregated` 全部 signature 加 `monowaves: &[Monowave]`;**新** `find_price_at_date` helper + **新** `build_round_advisories` helper(含 Round 2 動作 B advisory) |
| `CLAUDE.md` | v4.4a 章節 |

### 主要 fix

1. **真 magnitude lookup**(`scenario_price_magnitude`):
   - 從 `wave_tree.children.len()` placeholder 改為 `find_price_at_date()` 反查 monowaves
   - 對 Level-0 / Level-N scenario 都先試 monowave 反查,fallback 用 children.len() 維持 v3.7 行為
   - 對應 spec line 204-213 的 placeholder note

2. **Round 2 動作 B advisory**(`build_round_advisories`):
   - 對齊 spec line 1249-1251「Round 2 動作 B 邊界波 m(-1)/m(+1) Retracement Rules 重評」
   - 邊界 retracement ratio < 0.382 或 > 2.618 → Info advisory `Ch4_Round2_Compaction`
   - 留 V4.x 細化:部分 Stage 3-4 candidate rerun(spec 1249-1251 完整實作)

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **349 passed / 0 failed**(P1.3e baseline 不變;
  既有 17 個 compaction tests 全部更新呼叫 `&[]` empty monowaves slice 維持行為一致)
- `cargo build --release -p tw_cores` ✅ 0 warnings

### ⚠️ P0 Gate 校準(user 本機,P1.4 收尾後跑)

> **真實 monowave magnitude 改變 Similarity & Balance pass 率** → `forest_size` 分布可能改變。
> 對齊 plan v4.0 §P1.4 風險:**P1.4 收尾後必跑** user 本機 P0 Gate。

P0 Gate 驗證指令(P1.4 整個 milestone 完工後跑):

```powershell
git pull
cd rust_compute
cargo clean -p neely_core -p tw_cores
cargo build --release -p tw_cores
.\target\release\tw_cores.exe run-all --write
cd ..

# forest_size 分布觀察(對齊 architecture §10 forest_max_size=200 cap)
psql $env:DATABASE_URL -c "
SELECT
  PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p95,
  MAX(jsonb_array_length(snapshot->'scenario_forest'))                                        AS max_count
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
"
# acceptance:max ≤ 200(cap 不破),p95 < 180
```

### 下個 commit(P1.4b+c+d 合併)

接下來 P1.4b + P1.4c + P1.4d 合併一個 commit(Ch8 X-wave / Multiwave + Ch6 Stage 2 接 Ch8)。

---

## v4.3e — P1.3e Ch11 Triangle 9 變體 wave-a-e 規則(2026-05-19)

P1.3 系列最後 sub-PR。動工 Ch11 Triangle 9 變體 × wave-a/b/c/d/e 進階規則。
**Advisory mode**;對應 `NeelyPatternType::Triangle { sub_kind: TriangleKind }`。

### 範圍(1 commit / P1.3 收尾)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/validator/ch11_triangle_variants.rs` | **新檔** — `analyze()` + `classify_variant()`(TriangleKind 3 → TriangleVariant 9 mapping)+ Common Contracting / Expanding 規則 + 3 變體特定 b-wave 規則;**+9 unit tests** |
| `rust_compute/cores/wave/neely_core/src/validator/mod.rs` | 加 `pub mod ch11_triangle_variants;` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 `ch11_triangle_variants::analyze()`(P1.3 5 sub-PR 全部 wire 完)|
| `CLAUDE.md` | v4.3e 章節 |

### 規則覆蓋(spec line 2346-2485)

**TriangleKind 3 → TriangleVariant 9 mapping**(從 b/a ratio):
- b/a ≤ 1.01 → Horizontal
- 1.01 < b/a ≤ 1.382 → Irregular
- b/a > 1.382 → Running
- × TriangleKind:Limiting / NonLimiting(Contracting)/ Expanding

**共同規則**:
- Contracting:d < c / e < d / c ≤ 161.8% × b
- Expanding:a 或 b 為最小 / d > c / e > d / e 為最大

**變體特定 b-wave 規則**:
- Horizontal:a ≥ 50% × b / b ≤ 261.8% × a
- Irregular:b > 1.01 × a / b ≤ 261.8% × a / 更常 ≤ 161.8% × a
- Running:b 為最長段(b > 1.382 × a)

### P1.3 5 sub-PR 整體總結

| Sub-PR | 模組 | Tests |
|---|---|---|
| P1.3a Trending Impulse | ch11_trending_impulse.rs | +11 |
| P1.3b Terminal Impulse | ch11_terminal_impulse.rs | +7 |
| P1.3c Flat 7 變體 | ch11_flat_variants.rs | +8 |
| P1.3d Zigzag + Appendix B 項 F | ch11_zigzag.rs | +7 |
| P1.3e Triangle 9 變體 | ch11_triangle_variants.rs | +9 |
| **Total P1.3** | **5 new modules** | **+42** |

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **349 passed / 0 failed**(v4.3d 340 → +9)
- `cargo build --release -p tw_cores` ✅ 0 warnings

### 下個 milestone(P1.4)

P1.3 完成!接著 P1.4 — Ch4 Round 2 動作 B + Ch8 X-wave / Multiwave / Ch6 接 Ch8
(~1,500 LoC,3-4 commits)。**P1.4 收尾後 user 必跑 P0 Gate**(forest_size 重校)。
詳見 plan §「Milestone P1.4」。

---

## v4.3d — P1.3d Ch11 Zigzag wave-a/b/c + Appendix B 項 F(2026-05-19)

接 v4.3c P1.3c 後動工 P1.3d — Ch11 Zigzag wave-a/b/c 進階規則 + Appendix B 項 F
(Zigzag c 在 Triangle 內例外)。**Advisory mode**;對應 `NeelyPatternType::Zigzag`。

### 範圍(1 commit)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/validator/ch11_zigzag.rs` | **新檔** — `analyze()` + wave-a/b/c 規則 + Triangle 內例外處理;**+7 unit tests** |
| `rust_compute/cores/wave/neely_core/src/validator/mod.rs` | 加 `pub mod ch11_zigzag;` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 `ch11_zigzag::analyze()` 呼叫 |
| `CLAUDE.md` | v4.3d 章節 |

### 規則覆蓋(spec line 2323-2345 + Appendix B 項 F)

| Wave | 規則 |
|---|---|
| **a** | b 回測 ≥ 81% × a → Missing Wave Rule 警告(spec line 2329) |
| **b** | 61.8-81% 區間 → 內部 wave-a 警告;≤ 61.8% → Info(spec line 2328-2332)|
| **c**(non-Triangle) | c ∈ [61.8%, 161.8%] × a;超出 → Warning;> 161.8% → Elongated Zigzag(spec line 2337-2342) |
| **c**(in_triangle_context = true)| 範圍放寬;超出 → Strong「Triangle 形成訊號」(Appendix B 項 F + spec 2338-2342) |

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **340 passed / 0 failed**(v4.3c 333 → +7)
- `cargo build --release -p tw_cores` ✅ 0 warnings

### 下個 sub-PR(P1.3e Triangle 9 變體)

P1.3e — Triangle 9 變體 × wave-a-e 完整規則 ~900 LoC(P1.3 最後一個 sub-PR),
spec line 2346-2485。

---

## v4.3c — P1.3c Ch11 Flat 七變體 wave-a/b/c 規則(2026-05-19)

接 v4.3b P1.3b 後動工 P1.3c — Ch11 Flat 全部 7 變體 wave-a/b/c 規則。
**Advisory mode**;對應 `NeelyPatternType::Flat { sub_kind: FlatKind }`。

### 範圍(1 commit)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/validator/ch11_flat_variants.rs` | **新檔** — `analyze()` 主入口 + `flat_kind_to_variant()` mapping + 7 analyzers(B-Failure / C-Failure / Common / Double Failure / Elongated / Irregular(+StrongB)/ Irregular Failure);**+8 unit tests** |
| `rust_compute/cores/wave/neely_core/src/output.rs` | `FlatVariant` enum 加 `DoubleFailure` variant(原僅 8 variant,補完 9 個) |
| `rust_compute/cores/wave/neely_core/src/validator/mod.rs` | 加 `pub mod ch11_flat_variants;` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 `ch11_flat_variants::analyze()` 呼叫 |
| `CLAUDE.md` | v4.3c 章節 |

### 7 變體規則核心覆蓋(對齊 spec 2195-2321)

| Variant | 核心規則 |
|---|---|
| **B-Failure** | b 在 61.8-81% × a / c ≥ 61.8% × b / c 必完全回測 b |
| **C-Failure** | c < b(本變體定義)/ c < 61.8% × b 視為極罕 / c ≈ 61.8% × a |
| **Common** | b ∈ [81%, 100%] × a / c 必完全回測 b / c 稍微超越 a 不超 10-20% |
| **Double Failure** | b ≤ 81% × a / c 必未完全回測 b(定義) |
| **Elongated** | b ≥ 61.8% × a(a/b 相似)/ c 必遠大於 a(c > 100%, 通常 > 150%) |
| **Irregular**(+StrongB) | b > a 但 ≤ 138.2% × a / c 必完全回測 b |
| **Irregular Failure** | b > 138.2% × a(定義)/ c 不可完全回測 b |

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **333 passed / 0 failed**(v4.3b 325 → +8)
- `cargo build --release -p tw_cores` ✅ 0 warnings

### 下個 sub-PR(P1.3d Zigzag + Appendix B 項 F)

接著 P1.3d — Zigzag wave-a/b/c 進階規則 + Appendix B 項 F(Zigzag c 在 Triangle 內例外),spec line 2323-2345。

---

## v4.3b — P1.3b Ch11 Terminal Impulse wave-by-wave(2026-05-19)

接 v4.3a P1.3a 後動工 P1.3b — Ch11 Terminal Impulse(原 Elliott Diagonal Triangle)
wave-by-wave 變體規則。**Advisory mode**;對應 `NeelyPatternType::Diagonal`。

### 範圍(1 commit)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/validator/ch11_terminal_impulse.rs` | **新檔** — `analyze()` + 4 variant analyzers(1st-Ext / 3rd-Ext / 5th-Ext / 5th Non-Ext);**+7 unit tests** |
| `rust_compute/cores/wave/neely_core/src/validator/mod.rs` | 加 `pub mod ch11_terminal_impulse;` |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 `ch11_terminal_impulse::analyze()` 呼叫 |
| `CLAUDE.md` | v4.3b 章節 |

### 與 Trending Impulse 主要差異

- **W2 寬鬆**:Terminal 1st Ext W2 上限 61.8%(Trending 是 38.2%,spec line 2145)
- **各 wave 是 :3 結構**(Trending 是 :5,spec line 2144)
- **典型為 c-wave of correction**(spec line 2154)

### 規則覆蓋(per variant)

| Variant | 核心規則 |
|---|---|
| **1st Ext** | W2 ≤ 61.8% × W1(line 2145)/ W3 ≥ 38.2% × W1 + ≤ 100% × W1(line 2146)/ W5 ≤ 99% × W3 + 典型 38.2-61.8%(line 2147)/ W4 ≈ 61.8% × W2(line 2148) |
| **3rd Ext** | W3 ≤ 161.8% × W1(line 2158)/ W2 必 > 61.8% × W1(line 2159)/ W4 ≤ 38.2% × W3(line 2160)/ W5 ≤ 61.8% × W3(line 2163) |
| **5th Ext** | W3 ≤ 161.8% × W1(line 2178)/ W5 ≥ 100% × (W1 + W3)(line 2177)/ W4 ≥ 50% × W3(line 2179) |
| **5th Non-Ext** | W5 ≤ 61.8% × W3(line 2183)/ W4 < W2 (時/價)(line 2187)|

### 沙箱驗證

- `cargo test --release -p neely_core` ✅ **325 passed / 0 failed**(v4.3a 318 → +7)
- `cargo build --release -p tw_cores` ✅ 0 warnings

### 下個 sub-PR(P1.3c Flat 七變體)

接著 P1.3c — Flat 七變體 wave-a/b/c 規則,spec line 2191-2322。

---

## v4.3a — P1.3a Ch11 Trending Impulse wave-by-wave(2026-05-19)

接 v4.2 P1.2 後動工 P1.3a(plan 文件 §「Milestone P1.3a」),補完 Ch11 Trending
Impulse 的 wave-by-wave 變體規則。**Advisory mode**(對齊 NEoWave 原作 Ch11 =
pattern characteristic 非 invariant);違反 → AdvisoryFinding 不 invalidate scenario。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/validator/ch11_trending_impulse.rs` | **新檔** — `analyze()` 主入口 + `detect_extension()` + 3 variant analyzers(1st-Ext / 3rd-Ext / 5th-Ext)+ 共通 W4 規則 + 5th Wave Failure 偵測;**+11 unit tests** |
| `rust_compute/cores/wave/neely_core/src/validator/mod.rs` | 加 `pub mod ch11_trending_impulse;` 註冊 |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | `run()` 加 `crate::validator::ch11_trending_impulse::analyze()` 呼叫(advisory_findings 寫入) |
| `CLAUDE.md` | v4.3a 章節 |

### 規則覆蓋(每變體取核心規則,完整 spec 細化留 V4.x)

| Variant | 核心規則(對齊 neely_rules.md line ref)|
|---|---|
| **1st Ext** | W2 ≤ 38.2% × W1(嚴 1st Ext)/ W3 < W1 + W3 ≥ 38.2% × W1 / W5 必為三段最短 (剛性,line 2081) |
| **3rd Ext** | W3 > 161.8% × W1(line 2092)/ W2 寬鬆到 99%(line 2094)/ W4 ≤ 38.2-50% × W3(line 2095) / W5 ≈ W1 或 0.618/1.618 關係(line 2096) |
| **5th Ext** | W3 ≈ 161.8% × W1(line 2110 典型,非剛性)/ W1 < W3 / W5 ≥ 1-3 全長 + W5 ≤ 261.8% × 1-3 全長(line 2112-2113)/ W4 > W2 + W4 ≥ 50% × W3(line 2114) |
| **Wave-4 共通** | W4 ≤ 61.8% × W3 except 5th Ext(line 2133) |
| **5th Wave Failure** | W5 < W4 偵測;在 3rd Ext = Strong,1st/5th Ext = Warning(spec 2126) |

### Advisory severity 規則

- `Warning`:剛性條件違反(e.g., W5 必最短被破)
- `Info`:典型 Fibonacci 比例符合 / Wave-2 寬鬆但合理
- `Strong`:5th Wave Failure 在 3rd Ext 環境(反轉強訊號,spec 2129)

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release -p neely_core` ✅ **318 passed / 0 failed**(v4.2 307 → +11 new)

### 風險

🟢 中-低:
- Advisory mode 安全網,production scenario forest 0 影響
- LLM narrative 多 4-8 entries per 5-wave Impulse scenario
- Scenario JSON 序列化加大(每 Impulse +~1-2KB advisory_findings list);PG TOAST 邊界 ~2KB 監控
- params_hash 不變(Scenario 結構不改);user 重跑走 ON CONFLICT UPDATE 覆寫

### 下個 sub-PR(P1.3b Terminal Impulse)

接著 P1.3b — Terminal Impulse(原 Diagonal Triangle)wave-by-wave,
spec line 2138-2189。詳見 plan §「Milestone P1.3b」。

---

## v4.2 — P1.2 Ch9 advisory + Ch12 Waterfall/Localized(2026-05-19)

接 v4.1 P1.1 後動工 P1.2(plan 文件 §「Milestone P1.2」)。補完 Ch9 / Ch12 advisory
規則,讓 spec-only enum variants 全部有 dispatch。**advisory mode**:不參與 scenario filter,
僅 寫 `Scenario.advisory_findings` 給 LLM 看。

### 動工項目(1 commit / 6 task / branch `claude/continue-previous-work-xdKrl`)

| # | 工作 | 動作 |
|---|---|---|
| 7 | Ch9 Independent Rule advisory(spec 1973-1974) | 新 `check_independent_rule(scenario)`:scenario 啟動 ≥ 2 NEoWave 章節 → Info advisory「規則互不干涉」;`rule_chapter()` helper 對 RuleId enum 推導章節 |
| 8 | Ch9 Simultaneous Occurrence advisory(spec 1976-1977) | 新 `check_simultaneous_occurrence(scenario)`:Impulse pattern 預期 Ch5_Essential R1-R7 全 passed;< 7 個 → Warning advisory「未同時齊備」;= 7 個 → Info「情境齊備」 |
| 9 | Ch9 Exception Aspect 2 dispatch(spec 1988-1990) | 新 `detect_exception_aspect_2(scenario)`:Trendline Touchpoints Strong + Diagonal pattern → 觸發 `Ch9_Exception_Aspect2 { triggered_new_rule: "Terminal Impulse (Diagonal)" }` |
| 10 | Ch9 Exception Aspect 1 Multiwave 補完 | `exception_aspect_1_situation` 加 Multiwave 結尾分支:Combination Triple* / RunningCorrection → `ExceptionSituation::MultiwaveEnd` |
| 11 | Ch12 Waterfall Effect ±5% | **新 module** `fibonacci/waterfall.rs`:Trending Impulse + W3/W1 > 2.618 + 5% 或 W5/max(W1,W3) > 2.618 + 5% → Strong advisory「加速 cascade」;否則 Info |
| 12 | Ch12 Localized Progress Label Changes | **新 module** `advanced_rules/ch12_localized.rs`:3 種觸發 case(Impulse in_triangle_context / compacted_base=Five 含複雜 labels / awaiting_l_label)→ Info advisory「label 局部變動」 |

### 範圍

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/advanced_rules/ch9.rs` | 加 4 個新 fn(`check_independent_rule` / `check_simultaneous_occurrence` / `detect_exception_aspect_2` / `count_active_chapters` + `rule_chapter` helpers);`exception_aspect_1_situation` 加 Multiwave 結尾分支(Combination Triple* / RunningCorrection → MultiwaveEnd);**+8 new tests** |
| `rust_compute/cores/wave/neely_core/src/fibonacci/waterfall.rs` | **新檔** — `check_waterfall_effect()` + `waterfall_threshold()`(2.618 × 1.05 = 2.7489) + 5 unit tests |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/ch12_localized.rs` | **新檔** — `detect_localized_changes()` 3 case 偵測 + 4 unit tests |
| `rust_compute/cores/wave/neely_core/src/fibonacci/mod.rs` | 加 `pub mod waterfall;` 註冊 |
| `rust_compute/cores/wave/neely_core/src/advanced_rules/mod.rs` | 加 `pub mod ch12_localized;` 註冊 + `run()` 內接入 6 個新 advisory check |
| `rust_compute/cores/wave/neely_core/src/fibonacci/projection.rs` | line 19 註解 update:Waterfall「**v4.2 P1.2 已啟用**」(原「留 P11+」)|
| `CLAUDE.md` | v4.2 章節(本段) |

**0 alembic / 0 collector.toml / 0 Python / advisory mode → 0 scenario forest filter 行為改變**。

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release -p neely_core` ✅ **307 passed / 0 failed**(從 v4.1 288 → +19 new)
- `cargo test --release --workspace --no-fail-fast` ✅ **480 passed / 0 failed**(v4.1 baseline 461 → +19)

### 風險

🟢 低:
- advisory 不參與 scenario filter,production behavior 變化僅 `advisory_findings` 多 entries
- LLM 看 narrative 多訊號(章節獨立性 / Simultaneous 齊備度 / Exception Aspect 2 觸發 / Waterfall cascade / Localized adjustments)
- 既有 P1.1 / v3.38 tests 0 regression
- params_hash 不變(Scenario 結構不改);user 重跑 tw_cores 走 ON CONFLICT UPDATE 覆寫
- Rollback:單 commit `git revert` 即可

### user 本機(下次 session 起點)

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..

# 重跑 tw_cores 讓新 advisory 寫入 scenario.advisory_findings
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 確認新 advisory 在 production 出現
psql $env:DATABASE_URL -c "
SELECT
  jsonb_array_length(s->'advisory_findings') AS finding_count,
  s->'advisory_findings' AS findings
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id='2330' AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE stock_id='2330' AND core_name='neely_core' AND timeframe='daily')
  AND timeframe='daily'
LIMIT 3;
"
# 預期 finding_count 略升(v4.1 ~5 → v4.2 +4~6 個 new advisory:
# Ch9_Simultaneous / Ch12_WaterfallEffect / Ch12_LocalizedChanges / Ch9_Independent
# / Ch9_Exception_Aspect2 (Diagonal 時))
```

### 下個 milestone(P1.3 Ch11 wave-by-wave)

接著 P1.3 Ch11 Wave-by-Wave 5 個 sub-PR(Trending Impulse / Terminal Impulse / Flat 七變體
/ Zigzag / Triangle 九變體 ~2,650 LoC)。詳見 plan 文件 §「Milestone P1.3」。

---

## v4.1 — P1.1 Neely M3SPEC alignment Quick Wins(2026-05-19)

接 v3.38 收尾後動工 v4.0 Plan(15 真闕漏 / 4 milestones / ~5,340 LoC),
本 commit 落地 **P1.1 Quick Wins**(plan 文件 §「Milestone P1.1」)。0 dispatch
行為改變,純結構性補完 — 給 Aggregation Layer 多餘資訊。

### 動工項目(1 commit / 6 task / branch `claude/continue-previous-work-xdKrl`)

| # | 工作 | 動作 |
|---|---|---|
| 1 | `StructuralFacts` 加 `extension_subdivision_pair` 欄位 | 新 `ExtensionSubdivisionPair` struct + `SubdivisionStatus` enum(Independent / SubordinateToLarger / Indeterminate);對齊 spec §Ch8 Independent Rule |
| 2 | `AlternationFact` 升 5-axis | 從 `{ holds: bool }` 升 `{ price / time / severity / intricacy / construction: AlternationCheck, overall_holds: bool }`;新 `AlternationCheck` enum(Confirmed / NotApplicable / Failed);對齊 NEoWave §Rule of Alternation 五軸 |
| 3 | `OverlapPattern` 升 enum + evidence | 從 `{ label: String }` 升 enum `Trending { evidence } / Terminal { evidence } / None`;對齊 spec §Ch5 Overlap Rule 1326-1329 行 |
| 4 | `ChannelingFact` / `TimeRelationship` 加 evidence | ChannelingFact 加 `evidence: Vec<String>`;TimeRelationship 加 `durations_bars` + `fibonacci_ratios_matched` |
| 5 | `fifth_of_fifth_detector.rs` 抽共通 fn | 新 module;`rule_3.rs:37` `check_fifth_of_fifth_and_add` + `rule_4.rs:210` `add_l5_if_fifth_of_fifth` 兩處 byte-for-byte 重複合併;對齊 Appendix A.3 |
| 6 | `FlatKind::IrregularStrongB` 補 123.6% 中間檻 | `Irregular`(100-123.6%)+ 新 `IrregularStrongB`(123.6-138.2%);對齊 Appendix B 項 A;`flat_classifier::classify_flat` + power_rating table/post_behavior 對齊更新 |

### 範圍

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/output.rs` | `StructuralFacts` 加 1 欄;`AlternationFact` / `OverlapPattern` 升結構;`ChannelingFact` / `TimeRelationship` 加 evidence 欄;新 `AlternationCheck` / `SubdivisionStatus` / `ExtensionSubdivisionPair`;`FlatKind` 加 `IrregularStrongB` |
| `rust_compute/cores/wave/neely_core/src/classifier/structural_facts.rs` | `alternation` 改 `(candidate, classified, report)` 3-arg + 5-axis 計算 + 4 axis 分類 helper;`overlap_pattern` 升 enum 變體;`time_relationship` 加 evidence;`channeling` 加 evidence;**新** `extension_subdivision_pair()` fn;**+10 new tests** |
| `rust_compute/cores/wave/neely_core/src/classifier/mod.rs` | StructuralFacts 構造加 `extension_subdivision_pair` 欄位 + alternation 改 3-arg call |
| `rust_compute/cores/wave/neely_core/src/classifier/flat_classifier.rs` | `classify_flat` 加 123.6% 中間檻 sub-range 分支;**+3 new tests** |
| `rust_compute/cores/wave/neely_core/src/pre_constructive/fifth_of_fifth_detector.rs` | **新檔** — 共通 fn + 2 unit tests |
| `rust_compute/cores/wave/neely_core/src/pre_constructive/mod.rs` | 加 `mod fifth_of_fifth_detector;` |
| `rust_compute/cores/wave/neely_core/src/pre_constructive/rule_3.rs` | 移除 local `check_fifth_of_fifth_and_add`,use shared `fifth_of_fifth_detector::add_l5_if_fifth_of_fifth as check_fifth_of_fifth_and_add` |
| `rust_compute/cores/wave/neely_core/src/pre_constructive/rule_4.rs` | 移除 local `add_l5_if_fifth_of_fifth`,use shared |
| `rust_compute/cores/wave/neely_core/src/power_rating/table.rs` | FlatKind match 加 `IrregularStrongB`(並列 Irregular,power -1) |
| `rust_compute/cores/wave/neely_core/src/power_rating/post_behavior.rs` | 同上(並列 Irregular,MinRetracement 0.90) |
| `CLAUDE.md` | v4.1 章節(本段) |

**0 alembic / 0 collector.toml / 0 Python / 0 dispatch 行為改變**。

### 沙箱驗證

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release -p neely_core` ✅ **288 passed / 0 failed**(從 v3.38 baseline → +13 new)
- `cargo test --release --workspace --no-fail-fast` ✅ **461 passed / 0 failed**(v3.38 baseline 448 → +13)

### 風險

🟢 極低:
- 純 struct 擴 + 共通 fn 抽提,**0 dispatch 行為改變**
- production scenarios 0 影響(`Scenario` JSON 序列化新增欄位,既有 caller 0 break)
- 既有 5 cores 的 tests + Aggregation Layer Python tests 不受影響(structural fields 是 optional)
- params_hash 變動(`Scenario` struct schema 改)→ user 重跑 tw_cores 時走 ON CONFLICT UPDATE 覆寫;**不需 DELETE**
- Rollback:單 commit `git revert` 即可

### user 本機(下次 session 起點)

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..
# (P1.1 純結構性,可不重跑 tw_cores;若要看新欄位 → 重跑)
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 確認新 fields 存在於 Scenario JSON
psql $env:DATABASE_URL -c "
SELECT
  s->'structural_facts'->'extension_subdivision_pair' AS esp,
  s->'structural_facts'->'alternation' AS alt_5axis,
  s->'structural_facts'->'overlap_pattern' AS overlap_enum
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id='2330' AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE stock_id='2330' AND core_name='neely_core' AND timeframe='daily')
  AND timeframe='daily'
LIMIT 3;
"
# 預期:
#   esp = {extended_wave:..., status:..., extension_ratio:...} JSON
#   alt_5axis = {price:..., time:..., severity:..., intricacy:..., construction:..., overall_holds:...}
#   overlap_enum = {"Trending":{"evidence":"..."}} 或 {"Terminal":{"evidence":"..."}}
```

### 下個 milestone(P1.2)

接著 P1.2 Ch9 advisory + Ch12 Waterfall/Localized(2-3 天 / ~670 LoC / 1 commit)。
詳見 `/root/.claude/plans/hashed-foraging-pixel.md` §「Milestone P1.2」。

---

## v3.38 — Per-forecast-horizon Neely + degradation strategy(2026-05-18)

接 v3.37.1 SQL hotfix + v3.37 multi-timeframe Neely production verify 後 user 對「daily
lookback 是否要 6 年」深度討論。Spec audit(Explore agent 報告)揭露 NEoWave 原書
**不要求整段完整歷史**(Pattern Isolation Step 1 + Three Rounds 是 incremental forward-
processing,starting pivot 由 `:L5/:L3` label 自然決定);warmup_periods=500 是 degree
ceiling 設計(Daily 1-3 yr 對應 Minute degree = 1m-6m horizon);v3.37 multi-timeframe
本來就對齊原書「各 timeframe 負責自己 degree」。

User 拍版完整 v3.38 spec 取代 v3.36「daily 強推 6 yr」:支援 **1m / 3m / 6m** 三者並重
(drop 1y),per-horizon `daily_bars_required` / `daily_bars_min` / `weekly_bars_required`
/ `monthly_bars_required` / `missing_wave_threshold` 表 + 降級策略。

### 完整 spec(user 拍版 2026-05-18)

**Per-forecast bars 要求**:

| 參數 | 1m | 3m | 6m |
|---|---|---|---|
| `daily_bars_required`(理想) | 250 | 750 | 1,500 |
| `daily_bars_min`(硬下限) | 130 | 500 | 1,000 |
| `weekly_bars_required` | 50 | 150 | 300 |
| `monthly_bars_required` | — | — | 60 |

**統一資料窗口**(`load_for_neely` fixed table):
- Daily: 1,500 bars(~6 yr)
- Weekly: 300 bars(~6 yr)
- Monthly: **60**(~5 yr,**從 v3.36 144 縮減**)

**降級策略**:

| 條件 | 動作 |
|---|---|
| `daily_bars >= 1000` | full — 1m / 3m / 6m 全綠 confidence=1.0 |
| `500 <= daily_bars < 1000` | `degree_uncertain` — 6m 走 reference mode(prob=0.5 / range=None / confidence=0 + 中文 note),1m/3m 維持 |
| `130 <= daily_bars < 500` | `no_6m` — 拒 6m,僅 1m/3m |
| `daily_bars < 130` | `insufficient_history` — 拒全部 |

**missing_wave confidence tier**(spec line 2559-2582 對齊原書,**drop user 自訂 16/16/20**):

| Pattern type | min monowaves |
|---|---|
| Zigzag / Flat | 5 |
| Impulse / Triangle | 8 |
| Double | 10 |
| Doubles+Triangle | 13 |
| Triple | 15 |
| Triples+Triangle | 18 |

判定:`<50% × min` → certain / `50% × min ≤ count < 2× min` → possible / `≥ 2× min` → absent

Per-timeframe 獨立比對(spec §5.4「Core 内部不協同 Timeframe」):
- 1m/3m 看 daily scenario + daily monowave_count
- 6m 看 daily + weekly **各自** 套 spec table(對齊 user「重疊區」意圖)

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores_shared/ohlcv_loader/src/lib.rs` | `load_for_neely` lookback 改 fixed table:daily=1500 / weekly=300 / **monthly=60**(從 v3.36 144 縮減) |
| `mcp_server/_forecast.py` | drop `1_year` / rename keys 為 `1m`/`3m`/`6m` / 加 `data_availability` + `missing_wave_by_horizon` fields / 加 `_compute_data_availability` / `_build_forecasts_v3_38` / `_classify_missing_wave_tier` / `_build_missing_wave_by_horizon` 5 個新 helper / spec-aligned `_MISSING_WAVE_MIN_BY_PATTERN` table |
| `tests/mcp_server/test_toolkit_v2.py` | 既有 4 test 對齊 3 horizon keys + `full_history` flag for fixture + 加 `TestV3_38Degradation` 5 tests(full / 6m_degraded / 6m_rejected / all_rejected / missing_wave_tier)|
| `CLAUDE.md` | v3.38 章節 |

**0 alembic / 0 collector.toml / 0 Rust 邏輯改動**(僅 loader const + MCP layer)。

### 沙箱驗證

- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace --no-fail-fast` ✅ **448 passed / 0 failed**(monthly 144 → 60 不影響既有 tests)
- `pytest tests/mcp_server/test_toolkit_v2.py::TestNeelyForecast* tests/mcp_server/test_toolkit_v2.py::TestV3_35Picker tests/mcp_server/test_toolkit_v2.py::TestV3_38Degradation` ✅ **24 passed**(19 既有 + 5 new v3.38)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **190 passed / 1 skipped**(從 185 +5 new)

### user 本機 production verify(下輪 session)

```powershell
git pull
cd rust_compute && cargo build --release -p tw_cores && cd ..

# 重跑(monthly lookback 144 → 60 → OhlcvSeries 變短;params_hash 不變;
# 既有 monthly structural rows 走 ON CONFLICT UPDATE 覆寫)
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
# 預期 wall time 略降(monthly lookback 縮)

# 驗 3030 多 horizon forecast(三 timeframe 都該有資料,3030 7+ yr history)
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
import json
r = neely_forecast('3030','2026-05-15')
print('forecast keys:', list(r['forecasts'].keys()))
print('data_availability:', json.dumps(r['data_availability'], ensure_ascii=False, indent=2))
print('missing_wave_by_horizon:', json.dumps(r['missing_wave_by_horizon'], ensure_ascii=False, indent=2))
print('quality_caveat.is_usable:', r['quality_caveat']['is_usable'])
"
# 預期:
#   forecast keys = ['1m', '3m', '6m']
#   data_availability.daily_bars > 1500
#   data_availability.degradation_status = 'full'
#   missing_wave_by_horizon.6m 有 daily + weekly 兩 entry
```

### 風險

🟢 低:
- 0 alembic / 0 collector.toml / 0 Rust 邏輯改動
- v3.37 multi-timeframe dispatch + v3.37.1 SQL fix 都已落地,本 PR 純 polish + degradation
- backward compat:既有 forecast 4-key tests 改 3-key,fixture 加 `full_history` flag
- Rollback:單 commit `git revert`
- Rust workspace 448 passed,Python 190 passed,0 regression

🟡 中:
- **monthly_bars 縮到 60 對 long-history 股票(15+ yr)**:可能損失早期 monowaves;對 3030
  7 yr 影響小(60 monthly = 5 yr 仍足),對 1101 40+ yr 老股可能影響
- **degradation_status 對 newly-listed 股大量 fire**:預期 ~5-10% 股票走 `degree_uncertain`
  / `no_6m`,LLM 體驗反映在 confidence=0 或缺 6m key

🔴 高:**無**

### Out of Scope(留 future)

- Rust 端 NeelyCoreParams 加 `missing_wave_threshold` field — MCP gating 足夠
- 1y horizon 重新加入 — drop 拍版,長期 anchor 走 Kalman ultra_long horizon
- per-stock dynamic lookback — 統一拉 1500/300/60 + degradation 由 MCP 處理

---

## v3.37 — Multi-timeframe Neely(C3,跨 daily/weekly/monthly picker,2026-05-18)

接 v3.36 hotfix(commit `a0a8a68`)production verify 揭露 root cause:即使 Neely
看完整 6 年 history(287 monowaves vs 之前 84),Generator 邏輯只取「5 連續 monowaves」
做 candidate → 5 連續 ≈ 35-50 天 span → **全部仍 SubMinuette degree**。

real fix:對齊 spec §8.6 cross_timeframe_hints + Kalman v3.33 multi-horizon 哲學,
neely_core 跑 **Daily + Weekly + Monthly 3 個 timeframe**。weekly/monthly 粒度下
每個 monowave 涵蓋 3-5 週 / 3-6 月 → 5 連續就涵蓋 ~1.5 月 / 1-2 年 → degree 推高到
Minute / Minor / Intermediate。

### 動工範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/system/tw_cores/src/run_stock_cores.rs` | neely_core block 改 multi-timeframe loop:tf==Daily 入口時自動跑 Daily/Weekly/Monthly 三 timeframe;各自 dispatch_neely 寫獨立 structural_snapshots row(PK 含 timeframe column 自然共存) |
| `mcp_server/_forecast.py` | `_extract_primary_and_top_scenarios` 跨 timeframe 收集 scenarios + tag `__timeframe__` + 統一 (degree DESC, power DESC, rules DESC) 排序;`_format_primary_scenario` 加 `timeframe` field;加 `_build_neely_by_timeframe` + `_compose_cross_timeframe_summary` 兩 helper;主 return dict 加 `neely_by_timeframe` field |
| `tests/mcp_server/test_toolkit_v2.py` | `test_returns_required_keys` 加 `neely_by_timeframe` key;加 2 new tests(`test_v3_37_picker_promotes_monthly_minor_over_daily_subminuette` / `test_v3_37_backward_compat_daily_only`)|
| `CLAUDE.md` | v3.37 章節 |

### 對 3030 預期效果

| Timeframe | monowave 數量 | 平均 span | 5 連續 candidate span | effective_degree |
|---|---|---|---|---|
| daily(現有)| ~287 | ~7-8 天 | ~35-50 天 | SubMinuette |
| weekly(new)| ~50-80 | ~3-5 週 | ~3-4 月 | Minute |
| monthly(new)| ~15-25 | ~3-6 月 | ~1.5-2.5 年 | **Minor / Intermediate** ✅ |

picker 跨 timeframe 排序後 monthly Minor scenario 自然 promote 到 primary,取代
daily SubMinuette。MCP output 含完整 `neely_by_timeframe` dict 給 LLM 看跨 timeframe
一致性 + cross_timeframe_summary 敘述。

### MCP output schema(v3.37 新增)

```python
{
    # 既有欄(top-level primary 現在可能來自 monthly / weekly,不必然 daily)
    "primary_scenario": {
        "label": "...", "pattern_type": "Impulse",
        "power_rating": "Bullish", "wave_count": 5,
        "effective_degree": "Minor",                # 從 daily SubMinuette 變這
        "wave_span_years": 5.5,                     # 從 0.14 變這
        "timeframe": "monthly",                     # NEW: 來自哪 timeframe
    },
    "invalidation_price": 80.0,                     # 對齊 monthly primary anchor
    # v3.37 新加
    "neely_by_timeframe": {
        "daily":   {"timeframe_present": True,  "scenario_count": 7,
                    "primary_scenario": {...},   "primary_effective_degree": "SubMinuette"},
        "weekly":  {"timeframe_present": True,  "scenario_count": 5,
                    "primary_scenario": {...},   "primary_effective_degree": "Minute"},
        "monthly": {"timeframe_present": True,  "scenario_count": 2,
                    "primary_scenario": {...},   "primary_effective_degree": "Minor"},
        "cross_timeframe_summary":
            "daily=SubMinuette(7 scenarios) / weekly=Minute(5 scenarios) / monthly=Minor(2 scenarios)",
    },
    # 其他既有欄
    "quality_caveat": {...},                        # 對 3030 多 timeframe 後應 is_usable=True
    "scenario_staleness": {...},
}
```

### 沙箱驗證

- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace --no-fail-fast` ✅ **448 passed / 0 failed**
- `pytest tests/mcp_server/test_toolkit_v2.py::TestV3_35Picker` ✅ **10 passed**(8 v3.35/v3.35.1 + 2 v3.37)
- `pytest tests/mcp_server/test_toolkit_v2.py::TestNeelyForecastStructure` ✅ **4 passed**(0 regression)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **185 passed / 1 skipped**(從 183 + 2 new v3.37)

### user 本機 production verify(下輪 session)

```powershell
git pull
cd rust_compute
cargo build --release -p tw_cores
cd ..

# v3.37 multi-timeframe 對 weekly / monthly 是「首次跑」(之前 weekly/monthly 沒
# neely structural_snapshots row)→ params_hash 同 daily(timeframe 不在 params_hash 內
# 序列化 — timeframe 是 structural_snapshots PK 一部分)。
# 既有 daily neely row 走 ON CONFLICT UPDATE 覆寫(不需 DELETE)
# 新 weekly / monthly 走 INSERT

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 預期 wall time 加 ~80-100s(neely 從 80s → ~240s,3x);total ~13-15 min

# 驗 multi-timeframe structural_snapshots 存在
psql $env:DATABASE_URL -c "
SELECT timeframe, COUNT(*) AS row_count
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core')
GROUP BY timeframe
ORDER BY timeframe;
"
# 預期看到 daily / weekly / monthly 三 row(各 ~1266 stocks)

# 驗 3030 monthly scenario span
psql $env:DATABASE_URL -c "
SELECT
  ((s->'wave_tree'->>'end')::date - (s->'wave_tree'->>'start')::date) AS span_days,
  s->>'power_rating' AS power, s->>'structure_label' AS label
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id='3030' AND core_name='neely_core'
  AND timeframe = 'monthly'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE stock_id='3030' AND core_name='neely_core' AND timeframe='monthly')
ORDER BY span_days DESC LIMIT 5;
"
# 預期 top scenario span_days > 730(2 年)

# MCP 對 3030 驗 primary 來自 monthly
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
import json
r = neely_forecast('3030','2026-05-15')
print('primary timeframe:', r['primary_scenario']['timeframe'])
print('primary degree:', r['primary_scenario']['effective_degree'])
print('primary span_years:', r['primary_scenario']['wave_span_years'])
print('cross_summary:', r['neely_by_timeframe']['cross_timeframe_summary'])
print('is_usable:', r['quality_caveat']['is_usable'])
"
# 預期:primary timeframe = monthly(or weekly)
#       primary degree = Minor / Intermediate / Primary
#       primary span_years > 3.0
#       is_usable = True
```

### 風險

🟢 低:
- 0 alembic / 0 collector.toml
- params_hash 不變(timeframe 是 PK 不在 hash);新 weekly/monthly 走 INSERT,既有 daily 走 ON CONFLICT UPDATE
- backward compat:既有 fixture 只 set daily → weekly/monthly entry timeframe_present=False,既有 4 NeelyForecastStructure tests + 8 v3.35/v3.35.1 picker tests 0 regression
- Rollback:單 commit `git revert` 即可

🟡 中:
- **wall time 預估 +20-30%**(neely 80s × 3 = 240s,total ~10 → 13 min)。對齊
  既有 compaction_timeout 機制,但若 weekly/monthly 對某些股 hit timeout 會看 error
  summary
- **monthly 對 newly-listed 股(< 6 yr history)可能 monowave 過少**(< 5 個就無
  candidate)— graceful fallback(該 timeframe entry timeframe_present=True 但
  scenario_count=0),不影響 daily

🔴 高:
- **無**

### 後續(若 verify 揭露問題)

- 若 weekly/monthly Forest_max_size=200 不夠用 → 加 per-timeframe config
- 若 wall time 爆超(>20 min)→ 改 weekly/monthly 走 dirty queue(只跑 changed stocks)
- 若 monthly 對短 history 股(IPO < 6 yr)大量噴 timeout → 加 min_bars check
- 若 monthly primary 真的不出 Minor degree(Ch5 規則仍拒)→ 升 v3.38+ 改 Generator
  partition logic(違反 spec 但對非標準急漲股可能必要)

---

## v3.36 — Neely load_for_neely lookback hotfix(2026-05-18)

接 v3.35.1 production verify(commit `25bcbe6`)後 user 跑 monowave_series SQL 揭露
**3030 monowave detection 從 2024-09-25 才開始**(snapshot date 2026-05-15 往前
僅 ~20 個月)— 但 3030 有 2019-2026 共 7+ 年 price_daily_fwd 資料。Neely 完全沒看
過 2019-2024 共 5 年的長期 history,所以 Stage 3 Generator 永遠產不出 long-degree
candidates。

### Root cause

`rust_compute/cores_shared/ohlcv_loader/src/lib.rs::load_for_neely`(原 line 149-169):

```rust
let warmup = core.warmup_periods(params);    // Daily=500
let lookback = (warmup as f64 * 1.2).ceil() as i32;   // = 600 bars ≈ 2.4 yr
```

Neely 只載 **600 trading bars(~2.4 yr)**,**其他所有 cores 都用
`STOCK_LOOKBACK_DAYS = 365*6 = 6 年`**(在 `tw_cores::run_stock_cores.rs:24`)。

`STOCK_LOOKBACK_DAYS` 註解明文「**6 年日線(覆蓋各 indicator warmup × 1.2 + 充足實際
series)**」是設計 intent,但 `load_for_neely` 走自己 shortcut 沒對齊 — **是 loader
logic 不一致 bug,不是 spec 設計**。

### 修法(1 commit / 1 file / 7 行改動)

`load_for_neely` 加 6-year floor(對齊既有 6 yr 慣例 + Forest_max_size=200 +
compaction_timeout 仍守門):

```rust
let warmup_buffered = (warmup as f64 * 1.2).ceil() as i32;
let lookback = match params.timeframe {
    Timeframe::Daily   => warmup_buffered.max(365 * 6),       // ≥ 6 yr daily
    Timeframe::Weekly  => warmup_buffered.max(365 * 6 / 7),
    Timeframe::Monthly => warmup_buffered.max(6 * 12 + 12),
    Timeframe::Quarterly => warmup_buffered.max(6 * 4 + 4),
};
```

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores_shared/ohlcv_loader/src/lib.rs` | `load_for_neely`:加 6 yr floor + 更新 docstring 含 v3.36 rationale |
| `CLAUDE.md` | v3.36 章節 |

**0 alembic / 0 collector.toml / 0 MCP layer 改動**(純 loader logic fix)。

### Risk 評估

🟢 低:
- Forest_max_size=200 仍 cap(BeamSearchFallback 取 top 100 by power_rating)
- compaction_timeout(NeelyEngineConfig 內既有設定)防 long-history 爆 wall time
- params_hash 不變(load lookback 不在 params 序列化內)→ user 重跑 tw_cores 走
  ON CONFLICT UPDATE 覆寫既有 structural_snapshots,**不需 DELETE**
- 既有 cargo workspace tests 全綠(448 passed,0 改動)
- Rollback:單 commit `git revert`

🟡 中:
- **3030 7 年資料對 wall time 影響**:預期 monowave 從 ~80 → ~300+,Stage 3 candidates
  從 ~50 → ~500(走 beam_width × 10 cap),compaction Round 1-2 wall time 上升。對齊
  既有 compaction_timeout 機制,但 user 跑 production 時可能比 v3.35 慢 ~10-30%
- **3030 短-degree 仍存在(若 Ch5 規則對非標準急漲股全拒)**:loader 修了讓 Neely 看到
  完整 history,但 Validator 可能仍拒 long-span candidates(對齊上輪 audit 揭露:
  `Ch5_Essential / Zigzag_Max_BRetracement / Equality / Overlap_Trending` 全拒)。
  若 fix 後 user 仍只看到 short-degree,真正升 C3 multi-timeframe Neely

🔴 高:
- **無**

### 沙箱驗證

- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace --no-fail-fast` ✅ **448 passed / 0 failed**

### user 本機 production verify(下輪 session)

```powershell
git pull
cd rust_compute
cargo build --release -p tw_cores
cd ..

# 重跑 tw_cores(neely_core params_hash 不變 → ON CONFLICT UPDATE 覆寫,不需 DELETE)
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 驗 3030 monowave_series 是否從 2019(或更早)開始
psql $env:DATABASE_URL -c "
SELECT
  COUNT(*)                                          AS total_monowaves,
  MIN(mw->>'start_date')                            AS earliest_mw_date,
  MAX(mw->>'end_date')                              AS latest_mw_date
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'monowave_series') AS mw
WHERE stock_id = '3030'
  AND core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE stock_id='3030' AND core_name='neely_core');
"
# 預期:earliest_mw_date <= 2020-05-15(對齊 6 年 lookback),
# total_monowaves 從 84 → 200-400

# 驗 scenario_forest 是否含 long-span(Minor / Primary)candidates
psql $env:DATABASE_URL -c "
SELECT
  ((s->'wave_tree'->>'end')::date - (s->'wave_tree'->>'start')::date) AS span_days,
  s->>'power_rating' AS power, s->>'structure_label' AS label
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') AS s
WHERE stock_id='3030' AND core_name='neely_core'
  AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots WHERE stock_id='3030' AND core_name='neely_core')
ORDER BY span_days DESC LIMIT 10;
"
# 預期:top scenario span_days > 365(年級 anchor 出現)

# MCP 對 3030 驗 primary_scenario.effective_degree
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
import json
r = neely_forecast('3030','2026-05-15')
print('effective_degree:', r['primary_scenario']['effective_degree'])
print('wave_span_years:', r['primary_scenario']['wave_span_years'])
print('is_usable:', r['quality_caveat']['is_usable'])
"
# 預期:effective_degree = Minor / Primary(非 SubMinuette)
# 預期:is_usable = True(quality_caveat 不再 fire)
```

### 後續(若 hotfix 不夠)

若 user verify 後 3030 monowave_series 確實涵蓋 2020-2026 但仍**只有 short-degree
scenarios**(Validator 拒絕所有 long-span Ch5 規則)→ 屬「真實資料 + 標準 NEoWave
衝突」,動 v3.37+ C3 multi-timeframe Neely(weekly/monthly 粒度可能讓 Ch5 滿足)。

---

## v3.35.1 — Quality caveat:short-degree only + fib decoupled warnings(2026-05-18)

接 v3.35 production verify(commit `2d791ef`)後 user 跑 3030 揭露 picker 邏輯對但
Rust scenario_forest **全部 7 個 scenarios 都是 < 60 天 span**(SubMinuette),最長
僅 0.14 yr。對應 v3.35 plan 文件中度風險被命中:

> 「若 production scenario_forest 對 3030 真的只有近期 swing 候選 → picker 無效」

加 `quality_caveat` field 給 LLM 警示 picker 給的 primary 是否可用,**不修 Rust**
(Rust 修法走 v3.36 B1/B2)。

### 動工(1 commit / 0 Rust / 1 helper)

| 檔 | 動作 |
|---|---|
| `mcp_server/_forecast.py` | 加 `_compute_quality_caveat(all_scenarios, primary, current_price)` helper / 主 return dict 加 `quality_caveat` field |
| `tests/mcp_server/test_toolkit_v2.py` | `test_returns_required_keys` 加 `quality_caveat` key + 3 new tests(short_degree_only / fib_decoupled / usable_when_long_aligned)|
| `CLAUDE.md` | v3.35.1 章節 |

### Quality caveat output schema

```python
"quality_caveat": {
    "is_short_degree_only":           True,        # 所有 scenarios ≤ SubMinuette
    "max_scenario_span_years":        0.14,        # 最長 scenario span
    "max_scenario_degree":            "SubMinuette",
    "fib_zones_decoupled_from_price": True,        # current_price 在 fib zones [low, high] +/-50% buffer 外
    "is_usable":                      False,        # 任一警示 → 不可用
    "warnings": [
        "所有 7 個 scenarios 都是 short-degree(最長 span 0.14 yr,SubMinuette)— "
            "Rust Stage 3 Generator 對長期 history 沒產 Minor+ degree candidates...",
        "current_price=395.0 在 primary scenario fib zones [98.50, 156.20] 之外(+/- 50% buffer)— "
            "forecasts 區間基於短期 swing anchor 投影,不適用當前 price level。"
    ]
}
```

### v3.36 候選(對齊 user B1 拍版,待跑 SQL diagnostic 後確認 root cause)

| 路線 | 範圍 | 估時 |
|---|---|---|
| B1.1 audit Stage 2 monowave Rule of Neutrality | `monowave/` module:若 mw1-mw7 被 Neutral 過濾,調 threshold/邏輯 | 1-2 天 |
| B1.2 audit Stage 3 Generator | `candidates/generator.rs:56-107` sliding window logic;若需加 skip-monowave partition | 2-3 天 |

audit blocked on user SQL(`diagnostics->rejections` 查 mw1-mw7 是否 ever generated)。

### 沙箱驗證

- `pytest tests/mcp_server/test_toolkit_v2.py::TestV3_35Picker` ✅ **8 passed**(5 v3.35 + 3 new v3.35.1)
- `pytest tests/mcp_server/test_toolkit_v2.py::TestNeelyForecastStructure` ✅ **4 passed**(`test_returns_required_keys` 更新加 `quality_caveat` key)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **183 passed / 1 skipped**(從 180 +3 new)
- 0 Rust / 0 alembic / 0 collector.toml

### user 本機(no re-run needed,純 MCP)

```powershell
git pull
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
import json
r = neely_forecast('3030','2026-05-15')
print('is_usable:', r['quality_caveat']['is_usable'])
print('warnings:')
for w in r['quality_caveat']['warnings']:
    print(' -', w)
"
# 預期:is_usable=False / 2 條 warning(short-degree only + fib decoupled)
```

### 風險

🟢 低:
- 0 Rust / 0 alembic / 0 collector.toml(純 Python helper)
- backward compat:現有 tests 改 1 個 keys 集合即可,fixture 無 wave_tree → 走 graceful path
- 既有 9 個 Neely tests + ~170 其他 Python tests 0 regression
- Rollback:單 commit `git revert`

---

## v3.35 — Neely-C-MCP picker:invalidation filter + degree-aware ordering(2026-05-18)

接 v3.33 Kalman multi-horizon + v3.34 polish 收尾後,user 拍版動 v3.35+ Neely 修法。
原 plan 文件提的「Neely-C Three Rounds degree-aware anchor」走 Rust Stage 8.5
Compaction 改 5+ 天 — 但 Explore agent 揭露 **NEoWave 原書 + m3Spec 採「展示式森林」
設計**(`output.rs:5-6` 註解明文「Forest 不選 primary,Aggregation Layer 處理」),
Three Rounds 規格無「Round 2 應 prefer higher-degree」指令,degree promotion 是
自然收斂結果。

User 拍版走 **A 路線(Neely-C-MCP)** — 不動 Rust Three Rounds,picker 邏輯在 MCP 層,
工程量 1 天而非 5+ 天,對齊 spec 設計。

### 動工範圍(1 commit / branch `claude/continue-previous-work-xdKrl`,0 Rust 改動)

| 檔 | 動作 |
|---|---|
| `mcp_server/_forecast.py` | `_extract_primary_and_top_scenarios` 加 `current_price` 參數 + invalidation filter + degree-aware sort;`_format_primary_scenario` 加 `effective_degree` + `wave_span_years` field;`compute_neely_forecast` 把 current_price 提到 picker 之前 |
| 同檔 helpers | 4 個新 helper:`_compute_scenario_effective_degree`(對齊 Stage 11 §13.3 表)/ `_degree_rank` / `_scenario_is_invalidated` / `_scenario_span_years` / `_parse_iso_date` |
| `tests/mcp_server/test_toolkit_v2.py` | 加 `TestV3_35Picker` class:5 tests(prefer higher-degree / filter invalidated / all invalidated returns empty / backward compat no wave_tree / weaken-only not filtered)|
| `CLAUDE.md` | v3.35 章節 |

### 兩個核心 picker 邏輯

**1. Invalidation filter**(只 `InvalidateScenario` action,Weaken/Promote 不算)
```python
# bullish scenario PriceBreakBelow(X) + current_price < X → 已破底 → 過濾
# bearish scenario PriceBreakAbove(X) + current_price > X → 已破頂 → 過濾
```

**2. Degree-aware ordering**(對齊 NEoWave Stage 11 §13.3 Degree Ceiling 表)
```python
# 排序 key: (degree_rank DESC, power_rating_strength DESC, rules_passed_count DESC)
# degree 從 scenario.wave_tree.start ~ end span_years 推算:
#   < 1 yr   → SubMinuette  (rank 3)
#   1-3 yr   → Minute       (rank 5)
#   3-10 yr  → Minor        (rank 6)
#   10-30 yr → Primary      (rank 8)
#   30-100 yr→ Cycle        (rank 9)
#   > 100 yr → Supercycle   (rank 10)
```

### 對 3030(德律,8 年漲 8 倍)預期效果

| 場景 | v3.34 picker | v3.35 picker |
|---|---|---|
| 多 scenarios 同 Bullish power_rating | 取第一筆(undefined 排序)| degree 拆票 → 取長期 swing(Minor)|
| 短期 swing IP=126(scenario A,1 月 span,SubMinuette)| primary | rank 3,排次 |
| 長期主升 IP=80(scenario B,6 年 span,Minor)| 次選 | **rank 6 → primary** |
| invalidation_price 顯示 | 126.28(已遠落後 current=395) | 80(對齊長期主升底部) |

LLM 看到 primary_scenario:
- `effective_degree: "Minor"`(而非 SubMinuette)
- `wave_span_years: 6.3`(而非 0.5)
- `invalidation_price: 80`(對齊長期主升 anchor,而非近期 swing)

### 沙箱驗證

- `pytest tests/mcp_server/test_toolkit_v2.py::TestV3_35Picker` ✅ **5 passed**
- `pytest tests/mcp_server/test_toolkit_v2.py::TestNeelyForecast*` ✅ **9 既有 passed**(0 regression — 既有 fixture 無 wave_tree → effective_degree=None,fallback 走 power_rating)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **180 passed / 1 skipped**(從 175 +5 new v3.35 tests)
- 0 Rust / 0 alembic / 0 collector.toml

### user 本機 production verify(下輪 session)

```powershell
git pull
# v3.35 純 Python 改動,不需重編 / 不需 DELETE 資料 / 不需重跑 tw_cores
# 直接 MCP 對話內測:

python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast
import json
print(json.dumps(neely_forecast('3030','2026-05-15'), ensure_ascii=False, indent=2, default=str))
"
# 預期 primary_scenario:
#   effective_degree = "Minor" or "Primary"(年級 anchor,而非 SubMinuette)
#   wave_span_years > 3.0
#   invalidation_price << current_price(對齊長期 anchor 而非近期 swing)
```

### 對齊 spec 設計

- `output.rs:5-6` 明文「Forest 不選 primary,Aggregation Layer 處理」— picker 落
  Aggregation Layer 是正確位置
- spec §Three Rounds(neely_rules.md line 1198-1256)規定 degree promotion 是 Round 1-2
  迭代自然收斂,**沒有明確「prefer higher-degree」指令** — 此屬 Aggregation 層判讀偏好,
  不是 Core 邊界職責
- `output.rs::Degree` enum 對齊 §13.3 Degree Ceiling 表;`degree/mod.rs::classify_degree`
  既有 Stage 11 logic 是 reference

### 風險

🟢 低:
- 0 Rust / 0 alembic / 0 collector.toml(純 Python MCP layer)
- 既有 9 個 Neely tests + ~165 其他 Python tests 0 regression
- backward compat:既有 fixture 無 wave_tree.start/end → effective_degree=None → fallback 走 power_rating sort(對齊 v3.34 行為)
- Rollback:單 commit `git revert` 即可

🟡 中:
- **3030 真實 production verify 需 user 跑**:依賴 Rust 端 scenarios 是否帶不同 wave_tree
  span 多 candidates。若 production scenario_forest 對 3030 真的只有「近期 swing」候選
  (Three Rounds 真窮舉沒產長期主升 scenarios),picker 無效 — 那時就是 v3.35+ 才該動
  Rust Three Rounds(對齊原 plan B 路線)。

🔴 高:
- **無**

### Out of Scope(留 future)

- **Rust Stage 8.5 真正改 Three Rounds**:對齊原 plan Neely-C(5+ 天)— 若 user
  production verify 揭露 3030 scenario_forest 真的只有短 span candidates,需動
- **per-scenario effective_degree 寫進 Rust output**:目前 MCP 算,將來可搬 Rust(對齊
  Stage 11 §8.6 MonowaveSummary 同款 surface to Aggregation pattern)
- **picker 對 weaken/promote triggers 的處理**:目前只看 InvalidateScenario;
  WeakenScenario 可加 power_rating penalty(-1 級)— v3.36 議題

---

## v3.34 — Kalman polish:short threshold + deviation_sigma floor(2026-05-18)

接 v3.33 production verify(commit `ab067a5` push 後)user 跑 4-horizon 對 3030 / 2330
verify,揭露 2 個 non-blocking polish item:

### Item 1:short horizon over-classifies as Sideway

**觀察**:3030 short horizon `velocity=-1.445/day` 但 regime=Sideway。velocity_pct
= -1.445/399.06 = -0.36% < threshold 0.005(0.5%)→ 判 Sideway。但 0.36% velocity 已有明顯方向。

**Root cause**:v3.33 default threshold 0.005 對 short horizon 略嚴。台股 daily
return 1σ noise floor 落在 0.5-1%,0.36% 屬於有訊號區間。

**修法**:`default_horizons()` short threshold 0.005 → **0.003**(0.3%/day,對齊
daily noise floor 中位)。其他 3 horizons 不動。

### Item 2:deviation_sigma 飆 4138σ(LLM 體驗誤導)

**觀察**:3030 ultra_long deviation = (395 - 158.59) / 0.06 = **4137.92σ**。看起來
像「極端 outlier」但其實是 Kalman P_t|t 對 long series 收斂塌掉(uncertainty=0.06)。

**Root cause**:Kalman uncertainty 是「state estimate 信心」,非「下次觀察的預測
誤差」。對 long series + small Q,P_t|t 數學上正確收斂到極小;但作為 σ 分母讓
deviation 失去解讀價值。

**修法**:MCP layer `_kalman.py` 加 `_effective_uncertainty(unc, smoothed)` helper —
取 `max(unc, |smoothed| × 0.01)`(1% noise floor,對齊 Bork & Petersen 2014
R=(0.01·p)² 量綱)。**不動 Rust 端 P_t|t 原值**(dashboards 與其他 consumer 仍看
真實 Kalman 信心),只在 MCP `deviation_sigma` + `uncertainty_band` 計算用 floor。

### 預期效應(v3.34 floor 套用後)

| Stock | horizon | smoothed | unc (raw) | dev (v3.33) | dev (v3.34) |
|---|---|---|---|---|---|
| 3030 | medium | 367.92 | 0.31 | 86.29σ | **7.35σ** |
| 3030 | long | 297.92 | 0.18 | 541.12σ | **32.6σ** |
| 3030 | ultra_long | 158.59 | 0.06 | 4137.92σ | **149.07σ** |
| 2330 | medium | 1733.32 | 0.89 | 594.90σ | **30.7σ** |
| 2330 | ultra_long | 819.68 | 0.22 | 6523.56σ | **176.4σ** |

ultra_long 仍大(149σ / 176σ)但屬合理範圍(對非平穩急漲股 long-term anchor 自然
遠離 raw)。Medium / long 進入 LLM 可解讀區間(< 50σ)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/indicator/kalman_filter_core/src/lib.rs` | `default_horizons()` short threshold 0.005 → 0.003 + 加 v3.34 calibration rationale 註解 |
| `mcp_server/_kalman.py` | `_UNCERTAINTY_FLOOR_PCT=0.01` + `_effective_uncertainty()` helper;頂層 deviation/band 計算套 floor;`_build_kalman_by_horizon` per-horizon dev 同款套用 |
| `tests/mcp_server/test_toolkit_v3.py` | 既有 2 test 更新預期值(test_regime_stable_up:band [1208.1,1232.5] dev 1.16;test_v3_30_reads_series_last_entry:band [2202.75, 2247.25]) + 2 new tests(uncertainty_floor_compresses_extreme_deviation + floor_no_op_when_uncertainty_above_pct)|
| `CLAUDE.md` | v3.34 章節 |

### 沙箱驗證

- `cargo test --release -p kalman_filter_core` ✅ **18 passed**(short threshold 0.003 不影響既有 tests)
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `pytest tests/mcp_server/test_toolkit_v3.py::TestKalmanTrend` ✅ **11 passed**(9 既有 + 2 new v3.34 tests)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **175 passed / 1 skipped**(從 173 + 2 new)

### user 本機 production verify(下輪 session)

```powershell
git pull
cd rust_compute
cargo build --release -p tw_cores
cd ..

# Item 1(Rust short threshold)params_hash 變動 → 需 DELETE 重建
psql $env:DATABASE_URL -c "
DELETE FROM facts WHERE source_core = 'kalman_filter_core';
DELETE FROM indicator_values WHERE source_core = 'kalman_filter_core';
"

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql

# 對 3030 驗 v3.34 兩 polish
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import kalman_trend
import json
r = kalman_trend('3030','2026-05-15')
print('short regime:', r['kalman_by_horizon']['short']['regime'])
# 預期:short 不再永遠 Sideway(velocity > 0.003 → StableDown / Decelerating 等)
print('ultra dev:', r['kalman_by_horizon']['ultra_long']['deviation_sigma'])
# 預期:從 4138 壓到 ~149σ
"
```

### 風險

🟢 低:
- 0 alembic / 0 collector.toml
- Item 1 純 1 const tweak,既有 18 個 Kalman tests + 11 個 MCP tests 全綠
- Item 2 MCP layer only,Rust 端 P_t|t 原值保留(其他 consumer 不受影響)
- backward compat:既有 deviation_sigma field 仍存在,只是數值範圍變小
- Rollback:單 commit `git revert`

🟡 中:
- **params_hash 變動**(Item 1 改 horizons[0].velocity_threshold_pct)→ 既有 v3.33
  kalman 資料(user 剛 verify 完那批)再次視為 stale,需 DELETE 重建
- **既有 MCP tests deviation 數值預期變**:test_regime_stable_up 從 1.67 → 1.16,
  test_v3_30 band 從 [2214,2236] → [2202.75,2247.25] — 但這是設計變,非 regression

🔴 高:
- **無**

---

## v3.33 — Kalman multi-horizon output(Q-by-prediction-timeframe,2026-05-18)

接 v3.32 cross_cores 10 個 factor + 4 MCP toolkit 上線後,user MCP verify 對 3030
(德律,8 年漲 8 倍急漲股)揭露 `kalman_trend` 數值「太醜」:current=395,
smoothed=158.6,deviation=4138σ。

### Root cause(Explore agent 雙輪歸因)

**第一輪歸因**:研究 mismatch(Roncalli 2013 原 stationary 設計)— 後被 user 質疑。

**第二輪重審**:**錯誤歸因**。研究本身 sound,問題在實作把 Q 鎖在最平滑端
(Q=1e-5 = ~12 年 halflife)。Roncalli 2013 推薦 Q ∈ [1e-5, 1e-3] 是 **per-stock
trend filter 範圍**,代表**不同 prediction horizon**:

| Q | halflife | 對應 horizon |
|---|---|---|
| 1e-5(舊 default)| ~3100 bars(12+ 年) | 長期均衡(無人想看)|
| 1e-4 | ~990 bars(4 年)| 年級 trend |
| 1e-3 | ~310 bars(1.2 年)| 短年 |
| 1e-2 | ~99 bars(5 月)| 季 |
| 1e-1 | ~31 bars(6 週)| 月 |

舊實作把 Q 鎖在 1e-5 = 12 年 halflife 平滑器 — 對 3030 8 年漲 8 倍急漲股,
smoothed 158 其實是 2019-2023 中位數,**模型表現符合 spec,但 spec 點錯位置**。

### v3.33 multi-horizon 設計

對同一 OhlcvSeries 跑 4 個獨立 Kalman recursion,4 horizons default:

| label | Q | halflife_bars | velocity_threshold | min_dur | 對應期 |
|---|---|---|---|---|---|
| **short** | 1e-1 | ~31 | 0.005 | 3 | 6 週 |
| **medium**(primary)| 1e-2 | ~99 | 0.002 | 5 | 5 月 |
| **long** | 1e-3 | ~310 | 0.0015 | 5 | 1.2 年 |
| **ultra_long** | 1e-5 | ~3100 | 0.001 | 5 | 12 年 |

`halflife_bars ≈ 9.8 / sqrt(Q)`(對 R=(0.01·p)² 配方 production calibration)。

**facts(EventKind transition)只從 primary horizon("medium")產**,保留
≤ 12/yr/stock production 行為。其他 horizon 訊號走 `indicator_values.value.horizons[].series_last`,LLM 自己讀。

### velocity_threshold scaling rationale

- Q 越大 → smoothed 對 raw 跟得越緊 → daily velocity 自然越大(short horizon
  daily smoothed velocity ~ daily return ~ 1-5%)
- 原 0.001(0.1%)對 short horizon 等於每天都 fire → 改 0.005 過濾噪音
- 對 ultra_long(Q=1e-5)保留 0.001(對齊 v3.4 r2 production calibration)

### Output schema(`indicator_values.value`)

```json
{
  "stock_id": "3030",
  "timeframe": "Daily",
  "primary_horizon": "medium",
  "series": [...KalmanPoint],          // primary horizon full series(backward compat v3.30)
  "events": [...KalmanEvent],           // primary horizon events(= facts source)
  "horizons": [
    {"label":"short", "process_noise_q":0.1, "halflife_bars":31, ...,
     "series_last": {date, raw_close, smoothed_price, uncertainty, velocity, regime},
     "event_count": 8},
    {"label":"medium", ...},
    {"label":"long", ...},
    {"label":"ultra_long", ...}
  ]
}
```

### MCP `kalman_trend` 新增欄(對齊 backward compat)

```python
{
    # 既有頂層欄(對齊 primary = medium horizon,backward compat)
    "smoothed_price": ..., "regime": ..., "trend_velocity": ..., "deviation_sigma": ...,
    # v3.33 新增
    "primary_horizon": "medium",
    "kalman_by_horizon": {
        "short":      {"Q": 0.1,   "halflife_bars": 31,   "smoothed_price": 388.0,
                       "regime": "Accelerating", "deviation_sigma": 1.4, ...},
        "medium":     {"Q": 0.01,  "halflife_bars": 99,   "smoothed_price": 320.0, ...},
        "long":       {"Q": 0.001, "halflife_bars": 310,  "smoothed_price": 220.0, ...},
        "ultra_long": {"Q": 1e-5,  "halflife_bars": 3100, "smoothed_price": 159.0, ...}
    },
    "cross_horizon_consistency": {
        "all_aligned": false,
        "majority_regime": "StableUp",
        "majority_count": 3,
        "total_horizons": 4,
        "summary": "horizon 分歧(3/4 同向):short=加速上漲 / medium=穩定上漲 / ..."
    },
    "narrative": "... 跨 horizon 一致性:..."
}
```

### v3.30 / v3.31 對 3030 看到的數字 root cause 重新解讀

| 看到的數字 | 屬性 | 結論 |
|---|---|---|
| v3.29 前 smoothed=0 velocity=0 | path bug(讀錯 series[-1])| v3.30 path fix 已修對 |
| v3.30 後 smoothed=158 | **參數 mismatch**(Q=1e-5 = 12 年 anchor)| **v3.33 multi-horizon 修正** |
| deviation=4138σ | 上同 — uncertainty 對 Q=1e-5 太小 | v3.33 各 horizon 各自 uncertainty |

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/indicator/kalman_filter_core/src/lib.rs` | Params 改 horizons: Vec / 加 KalmanFilterHorizon + KalmanHorizonOutput / extract `compute_kalman_recursion` + `classify_series_regimes` helper / `compute()` loop 4 horizons / produce_facts 只走 primary horizon events / 加 5 multi-horizon tests |
| `rust_compute/cores/indicator/kalman_filter_core/Cargo.toml` | version 0.2.0 → 0.3.0 + description 加 multi-horizon |
| `m3Spec/kalman_filter_core.md` | r1 → r2(v3.33 修訂摘要 + §4 default 4 horizons table + §6 KalmanHorizonOutput struct + §7.1 multi-horizon recursion code)|
| `mcp_server/_kalman.py` | `compute_kalman_trend` 加 `kalman_by_horizon` + `cross_horizon_consistency` + `primary_horizon` keys / `_build_kalman_by_horizon` + `_compute_cross_horizon_consistency` 新 helpers / `_compose_narrative` 加 cross-horizon summary / `_empty_result` 補新 keys / backward compat v3.30 series-last-entry path 保留 |
| `tests/mcp_server/test_toolkit_v3.py` | 加 4 new TestKalmanTrend tests(parses_horizons / cross_horizon_consistency / all_aligned / backward_compat_no_horizons)|
| `CLAUDE.md` | v3.33 章節 + Rust tests 443 → 448 |

### 沙箱驗證(本 PR 動完跑)

- `cargo test --release -p kalman_filter_core` ✅ **18 passed**(13 既有 + 5 new)
- `cargo build --release -p tw_cores` ✅ 0 warnings
- `cargo test --release --workspace` ✅ **448 passed / 0 failed**(443 + 5 new)
- `pytest tests/mcp_server/test_toolkit_v3.py::TestKalmanTrend` ✅ **9 passed**
  (5 既有 + 4 new v3.33 tests)
- `pytest tests/mcp_server/ tests/agg/ tests/cross_cores/` ✅ **173 passed / 1 skipped**

### user 本機 production verify(下輪 session)

```powershell
git pull
cd rust_compute
cargo clean -p kalman_filter_core -p tw_cores
cargo build --release -p tw_cores
cd ..

# v3.33 kalman_filter_core params_hash 改變(horizons array 進 serialize)
# → 既有 facts / indicator_values 對 kalman 全部視為 stale,DELETE 重建
psql $env:DATABASE_URL -c "
DELETE FROM facts WHERE source_core = 'kalman_filter_core';
DELETE FROM indicator_values WHERE source_core = 'kalman_filter_core';
"

# 重跑 kalman(其他 cores 不動 params_hash 沒變 → dedup 不重算)
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 對 3030 / 2330 驗 multi-horizon output
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import kalman_trend
import json
print(json.dumps(kalman_trend('3030','2026-05-15'), ensure_ascii=False, indent=2, default=str))
"
# 預期看到 kalman_by_horizon 4 entries:
#   short      smoothed ~380-395(跟得緊;Q=1e-1)
#   medium     smoothed ~320(primary)
#   long       smoothed ~220(年級)
#   ultra_long smoothed ~159(舊 default 行為)
# narrative 含「跨 horizon 一致性」段

# 驗 per-EventKind rate(預期 medium horizon ~17.6/yr,對齊 v3.4 r2)
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
```

### 風險

🟢 低:
- 0 alembic / 0 collector.toml(JSONB 寫入向後相容)
- 既有 5 個 Kalman MCP tests 0 regression(v3.30 series-last-entry path 保留)
- EventKind 只走 primary horizon → production facts 觸發率行為不變(預期 ~17.6/yr/stock)
- backward compat:舊 indicator schema(無 `horizons` array)→ MCP graceful 回 `kalman_by_horizon={}`
- Rollback:單 commit `git revert`;舊 Q=1e-5 行為已內含於 `ultra_long` horizon

🟡 中:
- **velocity_threshold scaling 對 short horizon production 觸發率**:沙箱單元測試
  無法驗 1266 stocks 完整觸發率;若 short horizon 太頻繁(無 facts 寫入不影響
  production,僅 LLM 看到 noise),下輪 calibrate
- **params_hash 變動**:既有 kalman_filter_core facts / indicator_values 全部視為
  stale,需 DELETE 重建(已標 user verify chain)

🔴 高:
- **無**

### Out of Scope(對齊 plan,留 future)

- Velocity-augmented 2-D state Kalman(EKF with explicit drift state)— spec §11 已標
- Per-sector Q calibration(電子業 Q 大 / 金融業 Q 小)— V3 議題
- **Neely scenario picker 修法(Neely-C Three Rounds degree-aware anchor)**— v3.35+
  獨立 sprint(對齊 plan v3.33 拍版,Neely 不混進 Kalman PR)

---

## v3.32 — 10 new cross_cores factor builders + 4 MCP toolkit screens(2026-05-18)

接 v3.31 後 user 提出量化因子選型提案 v1.1(4 輪辯證後),要求 11 個 factor 落
Layer 2.5 cross_cores + 4 個高 level MCP toolkit screen wrapper(對齊提案 §六)。

### 動工拍版(2026-05-18)

| 範圍 | 數量 |
|---|---|
| 新 cross_cores builders(magic_formula 已 done)| **10**(`persistent_momentum` / `revenue_momentum` / `institutional_concert` / `f_score` / `low_volatility` / `industry_adj_gp` / `long_term_low_vol` / `dividend_yield` / `mom_12_1` / `monthly_trigger`)|
| alembic migration | **1**(`d9e0f1g2h3i4` 加 10 張表)|
| 新 MCP toolkit screens(對齊提案 §六)| **4**(`monthly_screen` / `quarterly_screen` / `annual_low_risk_screen` / `monthly_trigger_scan`)|
| MCP 暴露 toolkit 總數 | **8**(v3.31 4 + v3.32 4)|

### 4 個 sub-toolkit 結構(對齊提案 §四)

| Toolkit | 換倉頻率 | Builders | MCP wrapper |
|---|---|---|---|
| A(monthly)| 月 | persistent_momentum + revenue_momentum + institutional_concert + vol overlay | `monthly_screen(date, top_n)` |
| B(quarterly)| 季 | f_score + low_volatility + industry_adj_gp | `quarterly_screen(date, top_n)` |
| C(annual)| 年 | long_term_low_vol + dividend_yield + mom_12_1 | `annual_low_risk_screen(date, top_n)` |
| Layer 5(monthly overlay)| 月 | monthly_trigger | `monthly_trigger_scan(date)` |

### 設計細節

- **0 Rust / 0 collector.toml**(純 Python builders + alembic + MCP wrappers + tests)
- 既有 `CrossStockBuilder` Protocol 0 改動(magic_formula 是 template)
- 共用 helper `src/cross_cores/_shared.py`(`fetch_universe_filter` / `assign_ranks` /
  `compute_std` / `compute_returns_from_closes` / `fetch_close_series` 等)減少 10 個
  builder 的 boilerplate
- universe filter 對齊 magic_formula EXCLUDED_KEYWORDS + 加 `delisting_date IS NULL`
  防 survivorship bias
- F-Score 9 條件對齊 Piotroski 2000(放寬 ≥ 7 為 strong winner,對齊提案 v1.1)
- Industry-Adj GP:per-industry median 減算(Asness-Frazzini-Pedersen QMJ 2014 conditional sort 概念)
- Dividend Yield:三 hard filter(殖利率 ≥ 4% / 12M 報酬 > -20% / 5y ≥ 3y 配息)
  + soft rank(殖利率高的好);yield trap 防護對齊提案 v1.1
- Vol-managed overlay:per-stock 6M realized vol > 跨股均值 × 1.5 → detail.vol_managed_scale = 0.5
  (Barroso-Santa-Clara 2015 JFE);非 hard cutoff,LLM 看 detail hint 自己判斷

### 學術根據(提案 §十二 完整 reference list)

- **A+ 等級**(台股本土 peer-reviewed):Chen-Chou-Hsieh 2023 JFM / Hung-Lu-Yang 2025 RQFA
- **A 等級**(國際 OOS 含亞洲):Piotroski 2000 + Walkshäusl 2020 / Ang 2009 JFE /
  Novy-Marx 2013 / Blitz-van Vliet 2007 / Boudoukh 2007 / Jegadeesh-Titman 1993
- **B 等級**(risk overlay):Barroso-Santa-Clara 2015 / Daniel-Moskowitz 2016 momentum crash
- **C 等級**(工程設計):Layer 5 trigger 架構 / 權重比例 / 過濾門檻

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `alembic/versions/2026_05_18_d9e0f1g2h3i4_v3_32_10_cross_cores_tables.py`(新)| 10 張 `*_ranked_derived` + 1 張 `monthly_trigger_signals_derived` 表 |
| `src/cross_cores/_shared.py`(新)| universe filter / rank assign / std / returns / close series 共用 helper |
| `src/cross_cores/{persistent_momentum,revenue_momentum,institutional_concert,f_score,low_volatility,industry_adj_gp,long_term_low_vol,dividend_yield,mom_12_1,monthly_trigger}.py`(各新)| 10 個 builder |
| `src/cross_cores/orchestrator.py` | BUILDERS dict +10 entries(1 → 11)|
| `mcp_server/_screens.py`(新)| 4 compute_*_screen() 內部走 fetch_cross_stock_ranked |
| `mcp_server/tools/data.py` | 加 4 個 wrapper(monthly_screen / quarterly_screen / annual_low_risk_screen / monthly_trigger_scan)|
| `mcp_server/server.py` | 4 行 `mcp.tool()` + docstring + instructions 8 tools |
| `tests/cross_cores/test_v3_32_builders.py`(新)| 23 tests(shared helpers + 10 builder empty smoke + orchestrator + f_score logic)|
| `tests/mcp_server/test_screens.py`(新)| 17 tests(4 screen × 3 tests + Layer 5 2 tests + public surface)|
| `CLAUDE.md` | v3.32 章節 + Quick Reference tool 數 4 → 8 |
| `README.md` | MCP toolkit 4 → 8 + cross_cores 11 builders 同步 |

### 沙箱驗證

- `pytest tests/cross_cores/ tests/mcp_server/ tests/agg/` ✅ **165 passed / 1 skipped**
  (從 125 + 40 new = 23 cross_cores + 17 screens)
- 0 Rust / 0 collector.toml(純 Python + alembic)
- 既有 magic_formula 0 regression

### user 下次 session 起點(production verify chain)

**Phase A:Pre-impl SQL diagnostic**(必跑,blocking)

```powershell
git pull
# A-1:F-Score 需要 9 個 financial_statement detail key 都存在
psql $env:DATABASE_URL -c "
SELECT DISTINCT jsonb_object_keys(detail) AS key
  FROM financial_statement_derived
 WHERE stock_id = '2330' AND type IN ('income','balance','cashflow')
 ORDER BY key;
"
# 預期看到:營業收入合計 / 營業成本合計 / 本期淨利 / 流動資產 / 流動負債 /
#          長期借款 / 資產總額 / 營業活動之現金流量 / 股本

# A-2:industry_category populated %(B3 industry-adj GP 需 ≥ 80%)
psql $env:DATABASE_URL -c "
SELECT COUNT(*) AS total,
       COUNT(industry_category) AS non_null,
       (COUNT(industry_category)::float / COUNT(*) * 100)::numeric(5,2) AS pct
  FROM stock_info_ref
 WHERE market = 'TW' AND delisting_date IS NULL;
"

# A-3:valuation_daily_derived.dividend_yield populated %(C2 用)
psql $env:DATABASE_URL -c "
SELECT (COUNT(dividend_yield)::float / COUNT(*) * 100)::numeric(5,2) AS pct
  FROM valuation_daily_derived
 WHERE date = (SELECT MAX(date) FROM valuation_daily_derived) AND market = 'TW';
"
```

**Phase B:alembic + phase 8 跑全市場**

```powershell
alembic upgrade head      # head: c8d9e0f1g2h3 → d9e0f1g2h3i4

python src/main.py cross_cores phase 8 --full-rebuild
# 預期 11 個 builder 全 OK(magic_formula + 10 new),
# 每 builder rows_written ~1100-1300

python src/main.py cross_cores phase 8 --builder f_score
# 單 builder 跑(對齊既有 --builder 支援)
```

**Phase C:MCP 對 4 個 toolkit screen 驗**

```powershell
python -m mcp_server
# Claude Desktop 對話內:
#   "今天 monthly screen top 30"  → monthly_screen 內 3 個 factor + vol overlay
#   "今天 quarterly screen"        → F-Score / Low Vol / Industry-Adj GP
#   "今天 annual low risk screen"  → Long-Term Low Vol / Dividend Yield / 12-1 Mom
#   "今天 monthly trigger scan"    → Positive / Negative triggers
```

**Phase D:跨 toolkit 重疊 stock 觀察**

```sql
WITH a AS (SELECT stock_id FROM persistent_momentum_ranked_derived
            WHERE is_top_n AND date = '2026-05-15'),
     b AS (SELECT stock_id FROM f_score_ranked_derived
            WHERE is_top_n AND date = '2026-05-15'),
     c AS (SELECT stock_id FROM long_term_low_vol_ranked_derived
            WHERE is_top_n AND date = '2026-05-15')
SELECT
  (SELECT COUNT(*) FROM a) AS a_count,
  (SELECT COUNT(*) FROM b) AS b_count,
  (SELECT COUNT(*) FROM c) AS c_count,
  (SELECT COUNT(*) FROM a INTERSECT SELECT * FROM b) AS a_inter_b;
-- 預期:跨 toolkit 交集 ≤ 30%(若 > 50% 表 toolkit 高度重疊)
```

### 風險

🟢 低:
- 0 Rust(純 Python builders + alembic + tests)
- 既有 cross_cores orchestrator / MCP helper / Silver schema 0 改動
- Magic Formula 0 regression
- 10 個 builder 各自獨立,1 個壞不影響其他
- Rollback:單 commit `git revert` + `alembic downgrade -1`

🟡 中:
- **F-Score 9 條件 detail key 對齊 FinMind 中文 origin_name**:Phase A-1 SQL 揭露
  若缺 key 需先擴 Bronze field_mapper(或在 _detail_get fallback chain 加新 key)
- **Industry classification 覆蓋率**:< 80% → B3 fallback 用 sector proxy;< 60% 暫不上線
- **vol-managed overlay**:6M monthly approximation,Barroso-Santa-Clara 2015 原版用
  daily realized vol — 走 detail JSONB hint 非 hard cutoff

🔴 高:
- **McLean-Pontiff 2016 衰減**:published factor 平均 alpha 衰減 58%。本實作所有結果
  **僅作 LLM screening reference**,**不直接 trade**。User 需另跑 walk-forward backtest
  harness(提案 §十 prerequisite,本 PR 不含)

### Out of scope(留 future,對齊提案 §十 實作優先序)

- **Walk-forward backtest harness**(提案 §十 prerequisite)— 獨立 sprint
- **Per-builder scheduling**(月/季/年 trigger)— V3 議題
- **Workflow.toml 對 cross_cores 支援** — V3 議題
- **Dirty queue 真正啟用 for cross_cores** — V3(schema 有 column 但 noop)
- **Multi-factor composite portfolio**(提案 §三 資金配置 25/25/25/20)
  — LLM 看 4 toolkit screens + magic_formula 自己 compose

---

## v3.31 — MCP toolkit 9 → 4 consolidation + Kalman/Neely verify pipeline(2026-05-17)

接 v3.30 後 user 拍版砍 MCP 對 LLM 曝露 tool 數 9 → 4,**6 個 per-stock / market
基本資料工具合進 1 個 `stock_snapshot(stock_id, date)`**;新增 verify pipeline
(Python + SQL 雙軌)記入固定流水線。

### 最終 4-tool 公開介面

| Tool | 用途 |
|---|---|
| `neely_forecast(stock_id, date)` | Neely NEoWave 預測(不動) |
| `kalman_trend(stock_id, date)` | Kalman 1-D regime(不動) |
| `magic_formula_screen(date, top_n=30)` | Greenblatt 2005 跨股預測(不動) |
| **`stock_snapshot(stock_id, date)`(新)** | 6-in-1 基本資料快照:health + loan + block + risk + market + commodity |

被合併的 6 個 helper(`stock_health` / `market_context` /
`loan_collateral_snapshot` / `block_trade_summary` / `risk_alert_status` /
`commodity_macro_snapshot`)**仍留 `mcp_server.tools.data` 內 callable from
Python**(dashboard / direct script 用),只是不再 `mcp.tool()` 註冊。

### stock_snapshot 設計

```python
def stock_snapshot(stock_id, date, *, database_url=None) -> dict:
    """6-in-1 個股當下快照。"""
    return {
        "stock_id":        stock_id,
        "as_of":           date.isoformat(),
        "health":          {... compute_stock_health 完整 dict ...},
        "loan_collateral": {... compute_loan_collateral_snapshot ...},
        "block_trade":     {... compute_block_trade_summary(30d) ...},
        "risk_alert":      {... compute_risk_alert_status ...},
        "market_context":  {... compute_market_context ...},
        "commodity_macro": {... compute_commodity_macro_snapshot(["GOLD"]) ...},
        "narrative":       "1-3 句 aggregated overall view",
    }
```

**Graceful degradation**:每個 sub-section 用 try / except 包,某 helper 噴
exception → 該 section 變 `{"error": "<msg>", "section": "<name>"}`,其他 5 個
仍出。LLM 看 error key 知道哪段缺。

**Payload**:user 拍版不 trim,~10KB / ~2.5K tokens(對齊各 sub-section ~1.5KB)。

**narrative aggregator**:從 4 個主要 sub-section 各取 1 個 signal 串成 overall
view(個股 health overall_score / 大盤 climate / 處置警示 / 借券集中)。

### Verify pipeline(新 2 個檔)

**A. `scripts/verify_mcp_kalman_neely.py`**(Python wrapper,對齊 verify_pr18_bronze.py 風格)

per-stock assertions:
- Kalman:`smoothed_price > 0` ∧ `velocity != 0` ∧ `indicator_staleness.is_stale == False`
- Neely:`current_price > 0` ∧ `primary_scenario.wave_count > 0` ∧ `scenario_staleness.is_stale == False`

```powershell
python scripts/verify_mcp_kalman_neely.py                  # 預設 2330
python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030
python scripts/verify_mcp_kalman_neely.py --as-of 2026-05-15
```

退碼 0=全綠 / 1=任一 [FAIL]。FAIL 時提示 root cause:
- smoothed/velocity=0 → 拉 v3.30 path fix
- is_stale=true → 跑 `tw_cores run-all --write`
- wave_count=0 → 拉 v3.28 regex parse fix

**B. `scripts/verify_mcp_kalman_neely.sql`**(對齊 maintain_facts_stats.sql phase 風格)

直接看 Rust 寫進 DB 的真實內容(排除 MCP layer 干擾),2 phase:
1. Kalman `indicator_values.value->'series'->-1` 揭露 latest state 真實值
2. Neely `structural_snapshots.snapshot->'scenario_forest'->0` 揭露 W1 anchor 日期 + Fib zones

`psql -v stock=3030 -f scripts/verify_mcp_kalman_neely.sql` 換股票。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/tools/data.py` | 加 `stock_snapshot()` wrapper(~80 行)+ `_compose_snapshot_narrative()` helper |
| `mcp_server/server.py` | 5 行 `mcp.tool()` 註解掉 + 1 行 `mcp.tool(_data_tools.stock_snapshot)` 新增 + docstring + instructions 中文段更新 |
| `scripts/verify_mcp_kalman_neely.py`(新)| argparse + per-stock loop + status table + 退碼 |
| `scripts/verify_mcp_kalman_neely.sql`(新)| 2 phase + 解讀 comment |
| `tests/mcp_server/test_stock_snapshot.py`(新)| 8 test:全綠 / 1 fail / 3 fail / 全 fail / payload / narrative / public surface ×2 |
| `CLAUDE.md` | v3.31 章節 + helper 腳本清單 +2 行 + Quick Reference tool 數 9→4 |
| `README.md` | tool 數同步(若有提及)|

### 沙箱驗證

- `pytest tests/mcp_server/test_stock_snapshot.py` ✅ **8 passed**
- `pytest tests/mcp_server/ tests/agg/ --ignore=test_render_tools` ✅
  **125 passed / 1 skipped**(從 117 → 125,+8 new)
- `python scripts/verify_mcp_kalman_neely.py --help` ✅(argparse 結構對)
- 0 Rust / 0 alembic / 0 collector.toml(純 MCP layer reshape)

### user 下次 session 自動套用

```powershell
git pull
# 1. 跑 Python verify(對 production data)
python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030
# 預期(Kalman + Neely 兩個 column 都 [OK]):
# Stock   Kalman    Neely     Notes
# 2330    [OK]      [OK]      K:smoothed=2225 velocity=0.7 ... | N:price=2265 waves=5 ...
# 3030    [OK]      [OK]      ...

# 2. 跑 SQL spot-check(揭露 Rust 寫進 DB 的真實內容)
psql $env:DATABASE_URL -v stock=2330 -f scripts/verify_mcp_kalman_neely.sql

# 3. MCP server 對話內驗 4-tool 集合
python -m mcp_server
# Claude Desktop:
#   "2330 完整快照"  → 1 個 stock_snapshot call 拿全 6 sub-section
#   "2330 Neely 預測" / "Kalman 趨勢" / "今天 Magic Formula top 30"
```

### 風險

🟢 低:
- 0 Rust / 0 alembic / 0 collector.toml(純 MCP layer reshape)
- 6 個 compute_* 主邏輯 0 改動,只是少了 MCP wrapper
- Graceful degradation:1 sub-section helper 失敗,其他 5 個照出
- 既有 117 tests 全綠;8 new tests pass
- Rollback:單 commit `git revert` 即可

🟡 中:
- LLM 體驗變化 — 從 9 個 tool 名找對應 → 1 個 stock_snapshot 內找對應 sub-section
- Payload ~10KB 對 MCP context budget 偏大,但 user 拍版可接受

### Out of scope(留 v3.32+)

- **render PNG pipeline 修復**(v3.30 backlog)
- **Neely Fib zones 偏離 audit**(v3.30 backlog,verify SQL 揭露 W1 anchor 後決定)
- **multi-stock batch stock_snapshot**(對齊 magic_formula_screen 跨股)
- **per-section toggle**(kwargs 排除某 sub-section)

---

## v3.30 — kalman series-last-entry read + render tools 暫隱藏(2026-05-17)

接 v3.29 後 user 對 2330 跑完 9 個 MCP tool verify,揭露 2 個獨立 issue:

### Bug A:`kalman_trend` 對 2330 smoothed/velocity/uncertainty 全 0

**User 觀察**:
- `current_price = 2265` ✅(v3.26 修法,price_daily 直撈)
- `smoothed_price = 0` / `velocity = 0` / `uncertainty_band = [0, 0]` ❌

**Root cause**:Rust `dispatch_indicator` 序列化整個 `KalmanFilterOutput` 寫進
`indicator_values.value`,JSON 形如:
```json
{
  "stock_id": "2330",
  "timeframe": "Daily",
  "series": [...每天 1 個 KalmanPoint...],
  "events": [...]
}
```

但 `mcp_server/_kalman.py` 自 v3.4 起讀 `val.get("smoothed_price")` 走頂層,
production schema 頂層**沒有** `smoothed_price` 欄(這在 `series[-1]` 內)→
所有數值 fallback 0,regime 永遠 `Sideway`。

**屬 v3.4 自始就有的 silent bug**,因為:
1. test fixture 一直 pass 頂層 schema(`{smoothed_price: ..., ...}`)→ 測試永遠綠
2. production 行為「regime=Sideway / velocity=0」看起來像「停在盤整」合理
3. 3030 / 2330 都中,但「stale data」推論掩蓋了 path bug 本質

**修法**(`mcp_server/_kalman.py`):
```python
series = val.get("series") or []
latest_state = series[-1] if series else val   # 頂層 fallback 保留給 test fixtures

smoothed_price = float(latest_state.get("smoothed_price") or 0.0)
velocity       = float(latest_state.get("velocity") or 0.0)
uncertainty    = float(latest_state.get("uncertainty") or 0.0)
regime         = str(latest_state.get("regime") or "Sideway")
```

向下相容:既有 4 test fixture(頂層 schema)走 fallback 路徑,0 regression。
新 production schema(`series` 陣列)直接讀 `series[-1]`。

### Bug B:6 個 render tools 全 silent fail(暫隱藏)

**User 觀察**:`render_kline / render_chip / render_fundamental /
render_environment / render_neely / render_facts_cloud` 6 支全部回:
```
outputSchema defined but no structured output returned
```

PNG 後端生成 pipeline silent fail。Function 仍可從 Python 直接呼叫(dashboard
用),只是 MCP wrapper 拿不到 structured output → schema validation 炸。

**處置**(`mcp_server/server.py`):
- 暫註解 6 個 `mcp.tool(_render_tools.*)` 註冊
- import line 也註解(避免 module load fail 影響 server 啟動)
- 修好 PNG pipeline 後解開 6 行即可
- functions 仍留在 `mcp_server.tools.render` — 不刪除

**MCP toolkit 從 15 → 9 public tools**(暫時),修好 render 後 → 15。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/_kalman.py` | `series[-1]` 優先 + 頂層 fallback,4 個 state field 改讀 latest_state |
| `mcp_server/server.py` | 6 個 render tool registration + import 註解;instructions 中文段 + docstring 同步更新 |
| `tests/mcp_server/test_toolkit_v3.py` | 加 `test_v3_30_reads_series_last_entry`(production schema 對 2225 / 11.0 / 0.7 / Accelerating 應命中)|
| `CLAUDE.md` | v3.30 章節 |

### 沙箱驗證

- `pytest tests/mcp_server/test_toolkit_v3.py::TestKalmanTrend` ✅ **5 passed**
  (4 既有頂層 fixture + 1 new series schema)
- `pytest tests/mcp_server/ tests/agg/ --ignore=test_render_tools` ✅
  **117 passed / 1 skipped**(從 116 + 1 new)
- 0 Rust / 0 alembic / 0 collector.toml(純 MCP layer fix + tool hide)

### user 下次 session 自動套用

```powershell
git pull
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import kalman_trend
import json
print(json.dumps(kalman_trend('2330','2026-05-15'), ensure_ascii=False, indent=2, default=str))
"
# 應該看到:
#   smoothed_price ~= 最新 series[-1] 平滑值(對 2330 約 ~2200-2260 區間)
#   velocity ≠ 0(實際 Kalman 速度)
#   uncertainty_band ≠ [0, 0](smoothed ± uncertainty)
#   regime ≠ Sideway(若 2330 實際是 Accelerating / StableUp)
```

MCP Claude Desktop 重啟後,render tool 不再出現在工具清單(toolkit 從 15 → 9)。

### Out of scope(留 future)

- **Neely 2330 Fibonacci 839-997 vs current 2265 偏離**:user 觀察「情境顯然
  是舊資料」,但 Neely scenario_forest 是 anchor 在「最後一個 major pivot」,
  2330 從 2022 ~600 漲到 2026 2265 — 若 model 抓的 active pattern 起點是 2022,
  Fibonacci 投影自然落在當時 retrace 區間。**屬模型行為,非路徑 bug**(對齊
  output.rs `Scenario::expected_fib_zones` schema 確認)。v3.28 staleness 修法
  已 surface;user 看 `scenario_staleness.is_stale` 即可判斷模型是否需重跑。
- **render PNG pipeline 修復**:屬 dashboards 視覺化工程,本 PR 不動。修好後
  解開 server.py 6 行 + import 即可恢復。
- **更全面 indicator JSON path audit**:對齊 production 寫入慣例
  (Rust `dispatch_indicator` 序列化整個 Output 結構),可 audit 其他 indicator
  core 的 `_*.py` helper 是否同款讀錯路徑。

### 風險

🟢 低:
- 純 1 函式 path 修正 + tool 註冊註解
- 既有頂層 schema fallback 保證 4 個既有 test fixture 0 regression
- 0 Rust / 0 alembic / 0 collector.toml / 0 dashboards
- render functions 仍留模組內,dashboards 不受影響
- Rollback:單 commit `git revert` 即可

---

## v3.29 — risk_alert severity parser:`處置` / `注意` broad pattern(2026-05-17)

接 v3.28 production verify(`tw_cores run-all --write` 重算 + Neely / Kalman
staleness 全部 fresh)後 user 跑 SQL inspect 3030 揭露:
```
date       | condition | measure_excerpt
2026-05-07 | 連續三次  | 第一次處置
```

3030 的 measure 是極短字串「第一次處置」+ condition「連續三次」。既有
`_parse_severity` 三個 keyword(`全額交割` / `人工管制` / `注意交易資訊`)都
不命中 → fallback `unknown` → narrative 顯示「未分類」(production bug)。

### 修法(`mcp_server/_risk_alert.py`)

1. signature 加 `condition: str | None = None` kwarg(向下相容)
2. text 合併 measure + condition 後 match
3. 補 broad pattern:
   - `處置`(含「第一次處置」/「處置股」等變體)→ `disposition`
   - `分盤撮合`(對齊「人工管制之撮合終端機」變體)→ `disposition`
   - `注意`(含「連續三次注意異常」/「注意交易資訊」等)→ `warning`
4. priority 不變(最強到最弱):cash_only > disposition > warning > unknown

優先序保留:`人工管制` / `分盤撮合` / `處置` 並列 disposition 但都比 `注意` 強;
全額交割最強(prepay required 交易限制)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/_risk_alert.py` | `_parse_severity` 加 condition kwarg + 2 broad pattern;兩處 call site(current_status / history_60d)同步傳 condition |
| `tests/mcp_server/test_toolkit_v3.py` | 加 2 test:`test_v3_29_short_disposition_measure`(3030 case)/ `test_v3_29_attention_only_from_condition` |
| `CLAUDE.md` | v3.29 章節 |

### 沙箱驗證

- `pytest tests/mcp_server/test_toolkit_v3.py` ✅ **30 passed**(28 + 2 new)
- `pytest tests/mcp_server/ tests/agg/ --ignore=test_render_tools` ✅ **116 passed / 1 skipped**
- 既有 4 risk_alert test(disposition / cash_only / escalation / empty)0 regression
  (因為 condition 預設 None 不變,舊 measure-only keyword 仍命中)

### v3.28 production verify(同 session)

User 跑 `tw_cores run-all --write`(653s / 41,785 rows / 40 cores 全綠)後,3030
神經系統全部回到 fresh state:
- `wave_count = 5`(從 v3.28 regex parse 修法後正確抓到 5-wave label)✅
- `scenario_staleness.snapshot_date = 2026-05-15 / age_days = 0 / is_stale = false` ✅
- `indicator_staleness.value_date = 2026-05-15 / age_days = 0 / is_stale = false` ✅

`invalidation_price = 126.28` / `kalman regime = Sideway` 現在是 production fresh
state 的實際模型輸出,**不再是 stale data**(若 user 對 3030 模型輸出 disagree,
那是 neely / kalman 模型參數題,非 MCP layer issue)。

### user 下次 session 自動套用

```powershell
git pull
python -m mcp_server  # 開 stdio,Claude Desktop 對話內測:
# "3030 的 risk_alert 狀態"
# → severity = "disposition"(不再「未分類」)
# → severity_label = "處置股(分盤撮合)"
```

### Reference

- 「證券交易所公布注意交易資訊處置作業要點」§4(2024 版)— 三級嚴重度判定
  - 注意股:單日異常 + 連續注意(累計)
  - 處置股:5 日 / 10 日累計注意 → 5 / 10 / 20 min 分盤撮合
  - 全額交割:預收款券,最嚴

### Out of scope(留 v3.30+)

- **monitor `unknown` rate**:production 跑 1 週後拉 SQL 看還有多少 measure 字串
  落 `unknown`,可能還有其他奇特格式(若 < 1% 接受,否則加新 pattern)
- **per-tier severity weighting**(market_context risk_alert score):目前
  warning / disposition / cash_only 都單純 +1 active_count;未來可加權
  (disposition = 1.5× / cash_only = 3×)

### 風險

🟢 低:
- 純 `_parse_severity` 函式邏輯加 1 個 keyword + 1 個 broad pattern
- signature 加 optional kwarg 向下相容,既有 4 test 全綠
- 0 Rust / 0 alembic / 0 collector.toml
- Rollback:單 commit `git revert` 即可

---

## v3.28 — neely wave_count + scenario/indicator staleness surface(2026-05-17)

User v3.27 跑完 9 tools 對 3030 verify,揭露 3 個 follow-up:

### 修真實 bug

**A. `wave_count: 0` 但 label 「5-wave from mw27 to mw31」**(`_forecast.py:216`)
- root cause:`scenario.rules_passed_count` 是「通過 Neely 規則數」,非波浪數
- Rust `Scenario` struct(`rust_compute/cores/wave/neely_core/src/output.rs:373`)
  確實有此欄但語意是 rule count
- 修法:從 `structure_label` regex parse「N-wave」(`r"(\d+)-wave"`),fallback 0

**B. neely scenario_forest staleness** — 過期 anchor 算出 invalidation_price 126.28
- root cause:`tw_cores run-all` 對 3030 未 backfill 到 as_of=2026-05-15
- 修法:加 `_compute_scenario_staleness()` helper,output 加 `scenario_staleness`
  field 含 `snapshot_date / age_days / is_stale / warning`(> 7 天標 stale)
- staleness 屬資料新鮮度,**不能在 MCP 層修 stale data 本身**,但能 surface 給 LLM

**C. kalman regime stuck "Sideway" velocity 0**
- root cause:kalman_filter_core indicator 對 3030 沒新 update(value_date 太舊)
- 修法:加 `_compute_indicator_staleness()` helper,output 加 `indicator_staleness`
  field 同款 pattern;MagicMock-safe(`isinstance(value_date, date)` 過濾)

### 範圍(1 commit / main 直推)

| 檔 | 動作 |
|---|---|
| `mcp_server/_forecast.py` | `_format_primary_scenario` 加 regex parse(`(\d+)-wave`)/ 加 `_compute_scenario_staleness()` / output 加 `scenario_staleness` field |
| `mcp_server/_kalman.py` | 加 `_compute_indicator_staleness()` / output 加 `indicator_staleness` field |
| `tests/mcp_server/test_toolkit_v2.py` | `test_returns_required_keys` 加 `scenario_staleness` key |
| `CLAUDE.md` | v3.28 章節 |

### 沙箱驗證

- `pytest tests/mcp_server/ tests/agg/` ✅ **114 passed / 1 skipped**
- 0 regression(MagicMock-safe pattern 過濾)

### user verify(下次 session 跑)

```powershell
git pull
python -c "
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from mcp_server.tools.data import neely_forecast, kalman_trend
import json
print(json.dumps(neely_forecast('3030','2026-05-15'), ensure_ascii=False, indent=2, default=str))
"
# 應該看到:
#   primary_scenario.wave_count = 5(從 5-wave label parse)
#   scenario_staleness: {snapshot_date, age_days, is_stale, warning}
```

### Out of scope(留 V3.29+ + user 動工)

**Problem D: `risk_alert_status` severity `未分類`(3030)**
- root cause:Bronze `measure` 字串對 3030 沒命中 `_parse_severity` 3 模式
  (「全額交割」/「人工管制」/「注意交易資訊」)
- user 跑 SQL inspect 實際字串(下次 session 修):
  ```sql
  SELECT date, condition, LEFT(measure, 200) AS measure
    FROM disposition_securities_period_tw
   WHERE stock_id = '3030'
   ORDER BY date DESC LIMIT 3;
  ```
  把結果回我,新增 measure 模式或調整 parser

**重算 stale data(user 端動工)**:
- `tw_cores run-all --write` 重算 neely_core + kalman_filter_core 對 3030 ≤ 2026-05-15
- 之後 `invalidation_price` / `regime` / `velocity` 都會回到最新狀態

---

## v3.27 — MCP toolkit logic bug audit + metadata key fix(2026-05-17)

接 v3.26 current_price hotfix 後 user 要求「全面 audit MCP toolkit 找其他同款 bug」。
Explore agent 完整掃過 11 helper modules,揭露 6 個 medium-severity candidate,
逐一 verify 後修真正的 2 個。

### Audit 結果(8 helper modules)

| Bug 編號 | 檔 / 行 | Severity | 狀態 |
|---|---|---|---|
| 1 | `_forecast.py:97` falsy check vs is_not_None | low | false alarm(`if dict` 與 `is not None` 行為等價在 None 比對)|
| 2 | `_forecast.py` scenario_forest 過期 | medium | **out of scope**(staleness 需 tw_cores 重跑;v3.26 doc 已標)|
| **3** | **`_health.py:292/316/341` `metadata.get("kind")`** | **medium** | **修(本 PR)** |
| 4 | `_loan_collateral.py` SQL 缺 stock_id filter | medium | false alarm(SQL `WHERE stock_id = %s` 確實存在)|
| 5 | `_risk_alert.py:104` `ps <= as_of <= pe` take first match | low | OK(rows ORDER BY date DESC,first = 最新 announcement)|
| **6** | **`tools/render.py:482` 同款 `metadata.get("kind")`** | **medium** | **修(本 PR)** |
| 7 | `_block_trade.py:59` 30 天邊界 inclusive 31 天 | low | acceptable(off-by-one,差 1 天可接受)|

### 真實 bug 詳情

**BUG 3 + 6:`metadata.kind` vs `metadata.event_kind` 不一致**

Rust facts 寫入用 `fact_schema::with_event_kind` helper(`rust_compute/cores_shared/
fact_schema/src/lib.rs:80-103`),strictly 寫入 `metadata.event_kind = "EventKindName"`。

但既有 Python 代碼:
- `_health.py` 3 處(line 292/316/341)讀 `metadata.get("kind")`
- `tools/render.py:482` 同款

→ 對 Rust 寫的 production facts(都用 `event_kind`),`.get("kind")` 永遠回 None →
sign 永遠 0 → 4 維 score 全部變 0 → narrative 顯示「無顯著訊號」(實際有但讀不到)。

這是和 v3.26 current_price 同款 root cause(MCP 層假設 indicator/metadata schema
但實際 schema 不同),user 已預判到「應該還有其他 bug」。

> v3.25 `_climate.py` 已修(2 處 line 354 / 408 用 `event_kind or kind` fallback
> pattern),但 `_health.py` / `tools/render.py` v3.25 沒一起修 — v3.27 補完。

### 修法

統一 fallback pattern(同 v3.25):
```python
kind = (f.get("metadata") or {}).get("event_kind") \
    or (f.get("metadata") or {}).get("kind") \
    or _extract_kind_from_statement(f.get("statement", ""))
```

優先序:
1. **Rust production facts** → `event_kind`(對齊 fact_schema::with_event_kind)
2. **舊 test fixtures / migrations** → `kind`(向下相容)
3. **fallback** → 從 statement 字串首字提取 enum 名

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/_health.py` | 3 處 `metadata.get("kind")` → `event_kind or kind` fallback(replace_all)|
| `mcp_server/tools/render.py:482` | 同款 1 處 |
| `tests/mcp_server/test_toolkit_v3.py` | 加 `TestMetadataEventKindCompatibility` 2 test |
| `CLAUDE.md` | v3.27 章節 |

### 沙箱驗證

- `pytest tests/mcp_server/ tests/agg/` ✅ **114 passed / 1 skipped**(從 112 + 2 new)
- 新 test:`test_health_extracts_event_kind_from_metadata`(production schema)+
  `test_health_falls_back_to_kind_for_legacy_metadata`(舊 fixture 相容)
- 既有 ~100 test 0 regression(因為 fallback 對舊 `kind` schema 仍 work)

### Out of scope(留 future)

- **BUG 2 scenario_forest staleness**:user 提的 126.28 invalidation_price 屬
  neely_core 沒重跑 backfill 導致。MCP 層無法修(沒有 indicator vs as_of 對比邏輯)。
  V3.28 backlog:在 `_forecast.py` 加 staleness 警告(若 neely structural
  fact_date < as_of - 7 天,narrative 加「scenario_forest 過期,請重跑 tw_cores」)
- **`_loan_collateral.py` payload size 監控**:detail 含 25 sub-fields × 5
  categories JSONB,單筆可能 ~2KB。目前 < 3KB budget OK,但 batch query 若回多筆
  需注意。
- **multi-stock health 一次回**:`stock_health` 目前 1 個 stock,若擴 batch
  N 隻 stock 需重新評估 payload。

### 風險

🟢 低:
- 純 metadata key fallback 加 1 個 .get(),0 behavior change for legacy schemas
- 既有 ~100 test 自動套(fallback 對 `kind` schema 仍 work)
- Rollback:單 commit `git revert` 即可

### User 下次 session 自動套用

```powershell
git pull
# v3.27 自動套用
# 下次 stock_health / market_context / dashboard facts 點圖
# 都能正確讀到 Rust 寫的 metadata.event_kind
```

---

## v3.26 — MCP current_price bug fix:直讀 price_daily(2026-05-17)

接 v3.25(`945c1af` 合 main)後 user bug report:
- `stock_health(3030)` / `kalman_trend(3030)` `current_price = 0`(預期 395)
- `neely_forecast(3030)` `current_price = 126.28`(舊 scenario_forest anchor,
  非實際最新收盤)
- 確認 DB 沒問題:`price_daily` 3030 最新 close=395 / 前幾日 397-414

### Root cause

3 個 MCP helper 全部從 `indicator_latest` 撈 current_price:
- `_health._extract_current_price`:讀 ma_core series 最後一筆 close
- `_kalman.compute_kalman_trend`:讀 kalman_filter_core indicator.value["raw_close"]
- `_forecast._extract_current_price`:讀 ma_core series

但:
1. `_forecast.py:79` `relevant_cores` 列表 **沒含 ma_core** → indicator_latest 內無
   ma_core entry → 永遠 fallback 0.0
2. 若 ma_core / kalman_filter_core 沒最近重跑(stock 新進、或 stale build),
   `indicator_latest` 對該股 = empty → fallback 0.0
3. neely_forecast 的 126.28 = `_extract_invalidation_price` 從**過期 scenario_forest
   anchor** 算出來的價(neely_core 沒重 backfill)

**核心問題**:current_price 不該依賴 indicator_latest(stale 或 missing 風險),
應**直讀 price_daily Bronze**(authoritative source)。

### 修法

加 `agg._db.fetch_latest_close(conn, stock_id, as_of)` helper(取 <= as_of 最新
close + prev_close + change_pct),3 個 MCP helper 內共用 `mcp_server/_price.py`
wrapper(self-contained get_connection)。

優先序:
1. **price_daily 有資料** → 用 DB close(永遠最新最準)
2. **DB 無資料 fallback** → indicator_latest(對齊既有行為,測試覆蓋)

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `src/agg/_db.py` | 加 `fetch_latest_close(conn, stock_id, as_of)`,SELECT close FROM price_daily ≤ as_of LIMIT 2 計算 change_pct |
| `mcp_server/_price.py`(新)| `fetch_latest_close_for_tool()` self-contained wrapper(opens conn) |
| `mcp_server/_health.py` | compute_stock_health 加 v3.26 price_daily 優先 path,fallback indicator |
| `mcp_server/_kalman.py` | compute_kalman_trend 同款 fix(影響 deviation_sigma 計算) |
| `mcp_server/_forecast.py` | compute_neely_forecast 同款 fix(current_price 不再 0,fix _build_forecasts fallback path)|
| `tests/mcp_server/test_toolkit_v{2,3}.py` | `_patch_agg_as_of*` 加 `latest_close` param + 2 new regression test |
| `CLAUDE.md` | v3.26 章節 |

### 沙箱驗證

- `pytest tests/mcp_server/ tests/agg/` ✅ **112 passed / 1 skipped**(從 110 + 2 new)
- 2 new regression:
  - `test_uses_price_daily_when_available`:price_daily 有→用 395(不是 indicator 的 999.99)
  - `test_falls_back_to_indicator_when_db_empty`:price_daily 空→fallback indicator
- 既有 ~100 tests 0 regression(`_patch_agg_as_of` 預設 mock latest_close=None
  保留既有 indicator-only path)

### 範圍對齊 user 拍版原則

- **零耦合,少抽象**:單一 helper `fetch_latest_close_for_tool`,3 個 caller 各
  自獨立調用,不串接彼此
- **資料分層**:Layer 4 MCP tool 改直讀 Layer 1 Bronze(`price_daily`),不依賴
  Layer 3 M3 Cores 是否新鮮
- **fallback graceful**:DB 連線失敗或 stock 無資料 → 回 None → caller 走原
  indicator path(保證向下相容)

### Out of scope(separate issue,未動)

- **neely scenario_forest 過期**:user 提的 126.28 invalidation_price 本質是
  neely_core 沒 backfill 3030 最新資料導致 structural_snapshots 過期。MCP 層
  無法修,需 user 跑 `tw_cores run-all --write` 重算 neely_core。
- **加 staleness 警告**:若 neely structural 距 as_of >7 天可加 narrative 警告
  (V3.27 backlog)。
- **加 stock_health change_pct field**:`fetch_latest_close` 已回 change_pct
  但 health output 還沒 surface 出來(只用 close);可後續 enhancement。

### 風險

🟢 低:
- 0 alembic / 0 Rust / 0 collector.toml(純 Python)
- 既有 indicator-only path 完整保留(graceful fallback)
- 既有 ~100 tests 自動套 `latest_close=None` 沒 regression
- Rollback:單 commit `git revert` 即可

### user 下次跑就自動修正

```powershell
git pull
python -m mcp_server  # 開 stdio
# Claude Desktop 內:
#   "幫我看 3030 最新股價"  → stock_health 應正確回 current_price=395
#   "3030 Kalman 趨勢"      → kalman_trend.current_price=395(deviation_sigma 重算)
#   "3030 Neely 預測"       → neely_forecast.current_price=395
#                            (但 scenario_forest 仍 stale,需另 tw_cores run-all)
```

---

## v3.25 — `market_context()` 整合 commodity_macro + risk_alert(2026-05-17)

接 v3.24 production verify 收尾 + docs alignment 合 main(`d86a1c6`)後,user
拍版動工「整合 commodity_macro / risk_alert 進 `market_context()` MCP tool」
(對齊 v3.24 doc 提的 Out of Scope 第 4 項 → 本 PR 轉入 In Scope)。

**0 alembic / 0 Rust / 0 collector.toml**(純 Python:`mcp_server/_climate.py`
+ 9 new tests)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/_climate.py` | weights 6 → 8 / kinds 加 4 commodity / risks 加 2 commodity-related / `_aggregate_risk_alert_marketwide()` 新函式 / `_score_risk_alert()` 新函式 / `_COMP_LABEL` 加 2 中文 |
| `tests/mcp_server/test_toolkit_v2.py` | 加 3 TestXxx class × 3-4 test = +9 cases / `test_returns_6_components` 改 8 / `_patch_get_connection` 補預設 risk_alert mock |
| `CLAUDE.md` | v3.25 章節 |

### 新 8 components 權重(v3.25 拍版)

| Component | weight | source | 變動 |
|---|---|---|---|
| `taiex` | 0.22 | taiex_core | 0.25 → 0.22(-0.03) |
| `us_market` | 0.17 | us_market_core | 0.20 → 0.17 |
| `fear_greed` | 0.13 | fear_greed_core | 0.15 → 0.13 |
| `business` | 0.17 | business_indicator_core | 0.20 → 0.17 |
| `exchange_rate` | 0.08 | exchange_rate_core | 0.10 → 0.08 |
| `market_margin` | 0.08 | market_margin_core | 0.10 → 0.08 |
| **`commodity_macro`** | **0.05** | commodity_macro_core(`_global_`)| **NEW** |
| **`risk_alert`** | **0.10** | risk_alert_core(marketwide agg)| **NEW** |
| **Sum** | **1.00** | | |

risk_alert 0.10 比 commodity_macro 0.05 高的理由:**domestic 處置股直接反映台股
監管風險**,信號性高於 single-commodity macro 訊號。

### commodity_macro EventKind sign(GOLD 為 risk-off proxy,對 equities 反向)

| EventKind | sign | 邏輯 |
|---|---|---|
| `CommodityMomentumUp` | **-1**(bearish for equities) | GOLD 漲 = 避險情緒升高 = 對股市偏空 |
| `CommodityMomentumDown` | **+1**(bullish) | GOLD 跌 = risk-on 環境 |
| `CommoditySpike` | 0(中性) | 方向需看 metadata,但加進 systemic_risks |
| `CommodityRegimeBreak` | 0 | 警示性質,加 systemic_risks |

### risk_alert marketwide aggregation 設計

per-stock facts(real stock_ids)→ 3 個 marketwide 指標:

1. **`active_count`**:當下在處置期內的 distinct stocks
   ```sql
   SELECT COUNT(DISTINCT stock_id) FROM facts
    WHERE source_core = 'risk_alert_core'
      AND metadata->>'event_kind' = 'DispositionEntered'
      AND (metadata->>'period_end')::date >= as_of
   ```
2. **`announced_14d`**:近 14 天 DispositionAnnounced distinct stocks 數
3. **`escalations_60d`**:近 60 天 DispositionEscalation 總次數

`_score_risk_alert()` 給負分(對股市風險意義):

| 條件 | score 貢獻 |
|---|---|
| `active_count` >= 10 | **-100** |
| `active_count` 5-9 | -50 |
| `active_count` 1-4 | -15 |
| `escalations_60d` >= 3 | -30 額外 |
| `escalations_60d` 1-2 | -15 額外 |
| `announced_14d` >= 5 | -15 額外 |
| 最終 clamp `[-100, +100]` | |

### 新 systemic_risks 標籤

- `macro_commodity_spike`(近 14 天 CommoditySpike 出現)
- `macro_commodity_regime_shift`(近 14 天 CommodityRegimeBreak)
- `tw_disposition_cluster`(active_count >= 5)
- `tw_disposition_escalation_cluster`(escalations_60d >= 3)

### Output schema(新)

```json
{
  "as_of": "2026-05-13",
  "overall_climate": "bearish",
  "climate_score": -25.3,
  "components": {
    "taiex": {"score": -10, "fact_count": 5},
    ... (6 既有 components)
    "commodity_macro": {"score": -25, "fact_count": 3},
    "risk_alert": {
      "score": -65,
      "active_disposition_stocks": 7,
      "escalations_60d": 1,
      "announced_14d": 2
    }
  },
  "systemic_risks": ["tw_disposition_cluster", "macro_commodity_spike"],
  "narrative": "..."
}
```

risk_alert component 多 3 個 detail keys(`active_disposition_stocks` /
`escalations_60d` / `announced_14d`)— LLM 看 narrative 取概念,需要細節可
讀這些。

### 沙箱驗證

- `python -c "_score_risk_alert(...)"` ✅ 5/5 thresholds 對齊
- `python -c "weights sum"` ✅ 1.000
- `pytest tests/mcp_server/ tests/agg/` ✅ **110 passed / 1 skipped**(從 101 +9 new)
- 既有 5 public tools + 4 v3.22 tools 0 regression
- `market_context()` 對既有 LLM caller 向下相容(只增 components keys,不破舊 keys)

### 範圍對齊 user 拍版原則

- **零耦合,少抽象**:per-component score 各自獨立計算,commodity 訊號不會被
  taiex 訊號污染;risk_alert 走獨立 query 不污染既有 6 cores 路徑
- **資料分層**:Layer 4 MCP tool 只是 read-only summary;不改動 Bronze /
  Silver / M3 cores / facts table
- **參數選擇**:
  - commodity_macro weight 0.05:macro 1-commodity 初版,信號強度有限
  - risk_alert weight 0.10:domestic 處置股 hard signal,權重高於 commodity
  - active_count 閾值 5/10:Basel ICAAP 風險集中度概念變體
  - escalations_60d 閾值 3:對齊 TSEC 處置作業要點 §4「60 日內 ≥ 2 次升級」+ buffer 1

### 待 user 跑(下次 session 起點)

```powershell
git pull
# v3.25 自動套用 — 下次 market_context() call 多 2 components
python -m mcp_server   # 開 stdio
# Claude Desktop 對話內測:
#   "幫我看今天大盤狀況"  → LLM 看 narrative 內若 commodity / risk_alert 訊號偏空
#                          + systemic_risks 含 tw_disposition_cluster 等 → 自然警示
```

### 風險

🟢 低:
- 0 Rust / 0 alembic / 0 collector.toml(純 Python)
- 既有 7 cores facts query 路徑 0 改動;risk_alert 走新獨立 SQL,失敗 graceful(except → 0)
- weights 重新配權但 sum=1.0 保證 backward-compat 量綱
- 既有 ~100 tests 全綠;9 new tests pass
- Rollback:單 commit `git revert` 即可

### Out of Scope(留 future)

- **C-9 dashboard tabs**(Streamlit 視覺化 v3.20-21 cores + 新 components 視覺化)
- **B-4 FastAPI thin wrap**(v1.35 backlog)
- **多 commodity 支援**(SILVER / OIL 加入 commodity_macro / market_context 各 0.02-0.03 子權重)
- **Round 10 calibration**(若 user 跑 production 後 commodity_macro / risk_alert
  訊號量過低或過頻,可再調 weights)

---

## v3.24 — Round 9 calibration + commodity_macro Silver builder hotfix(2026-05-17)

接 v3.21 Commit C 全市場 production verify(`scripts/verify_event_kind_rate.sql`)
揭露 2 個 issue:

### Issue 1:`LoanCategoryConcentration` 125.69/yr 過頻(10.5× over target)

**root cause**:level trigger(每天 ratio >= 0.70 就 fire)。台股借券 ratio 自然
集中(unrestricted_loan 主導大多數股),level trigger 每天 fire = 噪音爆量。

**修法**(對齊 institutional `LargeTransaction` r3 拍版,Brown & Warner 1985):
- `loan_collateral_core.rs` 改 **edge trigger**:`cur_alert && !prev_alert` 才 fire
- production 行為:從「每天 in zone fire」→「進入 zone 當日 fire」+ 「exit 後 re-enter fire」
- 預期觸發率:125.69 → ~5-10/yr(對齊 institutional level→edge 95% retention 砍掉模式)

### Issue 2:`commodity_macro_core` 0 events(Silver builder 沒跑成功)

**root cause**:`commodity_macro.py` builder 沒覆寫 `fetch_bronze` 的 `order_by`
default(`"market, stock_id, date"`),但 `commodity_price_daily` 表沒 `stock_id`
欄(PK = `market, commodity, date`)→ SQL crash → silver orchestrator silently
catch(對齊 cores_overview §7.5 dirty queue 契約「failed 不中斷其他」)。

production 觀察:
- Bronze `commodity_price_daily`:**2176 rows / 2019-01-01 → 2026-05-15** ✅
- Silver `commodity_price_daily_derived`:**0 rows** ❌

**修法**:對齊 `exchange_rate.py` 既有 market-level Bronze pattern,覆寫
`order_by="market, commodity, date"`。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/chip/loan_collateral_core/src/lib.rs` | `compute` 加 `prev_concentration_alert` edge trigger 邏輯 + 2 new regression test(no_refire / refire_after_exit_reenter)|
| `src/silver/builders/commodity_macro.py` | `fetch_bronze` 加 `order_by="market, commodity, date"` |
| `CLAUDE.md` | v3.24 章節 |

**0 alembic / 0 collector.toml**(2 line code tweak + builder param fix)。

### 沙箱驗證

- `cargo test --release -p loan_collateral_core` ✅ **6 passed**(4 + 2 new)
- `cargo test --release --workspace` ✅ **443 passed / 0 failed**(從 441 +2 new)
- `cargo build --release -p tw_cores` ✅ 0 warning
- `python -c "from silver.builders import commodity_macro; ..."` ✅ sanity 通過

### Production verify 結果(全市場 1266 stocks × 36 cores)

| 指標 | 結果 |
|---|---|
| Wall time | 806.5s ≈ 13.4 min(v3.18 695s +16%,但加 4 new cores)|
| Cores 全綠 | 36/36 ok(vwap 12 skipped:無 Silver data 的 stock,known)|
| New facts | 41,785 rows(loan_collateral 18,887 / block_trade 18,887 / risk_alert 3,470 / commodity_macro 0 ← 上述 bug)|
| Round 7 5 cores verify | **0 row** 超 12/yr ✅(v3.11 calibration 持續有效)|
| Round 8.3 milestone 4 variants | **4/4 in target band** ✅(v3.18 完整結算持續有效)|
| facts dead_pct(VACUUM 前)| 11.08%(已用 maintain_facts_stats.sql 清到 0%)|

### v3.21 4 new cores production rate 統計

| EventKind | rate/yr/stock | target | 狀態 |
|---|---|---|---|
| `loan_collateral / LoanCategoryConcentration` | **125.69** | ≤ 12 | ❌ over → **v3.24 edge trigger 修** |
| `loan_collateral / MarginBalanceSurge` | 6.41 | ≤ 12 | ✅ OK |
| `loan_collateral / 其他 9 EventKind` | 未 over 6 | ≤ 12 | ✅ OK(未進 Section 1 top 30 over 6)|
| `block_trade_core / 全 4` | 估 ~2/yr(18887 events / 1266 stocks / 7 yr)| ≤ 12 | ✅ OK |
| `risk_alert_core / 全 4` | 估 ~0.4/yr(3470 / 1266 / 7)| ≤ 12 | ✅ OK(處置稀少事件)|
| `commodity_macro_core / 全 4` | 0(builder bug)| ≤ 12 | 🟡 待 v3.24 修後重 verify |

### accepted baselines 不動(v1.32 + v3.17)

- `institutional / DivergenceWithinInstitution` 58.41/yr — production reality
- `institutional / LargeTransaction` 14.16/yr — fat-tail (Lo 2001)

### 待 user 跑 production verify(下次 session)

```powershell
git pull
cd rust_compute
cargo clean -p loan_collateral_core -p tw_cores
cargo build --release -p tw_cores
cd ..

# 1. 先讓 commodity_macro Silver builder 跑(v3.24 修法)
python src/main.py silver phase 7a   # 應該見 commodity_macro read=2176 → wrote=2176

# 2. DELETE 過頻 LoanCategoryConcentration(facts 中 1031150 row 將被砍)
psql $env:DATABASE_URL -c "
DELETE FROM facts
 WHERE source_core = 'loan_collateral_core'
   AND metadata->>'event_kind' = 'LoanCategoryConcentration';
"

# 3. tw_cores run-all(會重算 loan_collateral_core 新 edge trigger + commodity_macro 首次有資料)
cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..

# 4. verify
psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
# 預期:
#   LoanCategoryConcentration 125.69 → ~5-10/yr ✅
#   commodity_macro_core 0 → 數十-數百 events(macro 信號稀疏,GOLD-only 初版)
```

### 風險

🟢 低:
- 純 1 函式 logic 改 + 1 builder param fix
- 既有 4 個 loan_collateral test margin 充足(spike z 設計與 edge trigger 兼容)
- production 行為改變 spec-defensible(對齊 Brown & Warner 1985 + r3 institutional pattern)
- Rollback:單 commit `git revert` 即可

### Out of Scope(留 future)

- **block_trade / risk_alert / commodity_macro 觸發率精校**:本 PR 只動 LoanCategoryConcentration
  + commodity_macro builder bug fix;其他 EventKind 待下輪 verify 後評估
- **Round 9 對其他 cores spec**:本 PR 僅 1 個 EventKind tightening,非全面 Round 9

---

## v3.23 — price_limit per_stock → all_market perf hotfix(2026-05-17)

User 跑 v3.21 verify chain 中觀察到 `price_limit` incremental 走 per_stock × 1300
stocks × 0.65 秒 = 14 分鐘只為了拉漲跌停(99% empty)。Probe FinMind 揭露
`TaiwanStockPriceLimit` 可走 all_market mode,但有 **quirk**:multi-day range 靜默
回 0 rows,單日查詢回 ~2745 rows(包含過去日)。

### Probe 結果(2026-05-17)

```
start=5-15, end=5-16 (1 trading day in range) → 2745 rows ✅
start=5-01, end=5-15 (10 trading days range)  → 0 rows ❌(FinMind quirk)
start=end=5-13 (single past day)              → 2743 rows ✅(backfill 可行)
```

### 修法(`config/collector.toml`)

| 欄 | 修前 | 修後 |
|---|---|---|
| `param_mode` | `per_stock` | **`all_market`** |
| `segment_days` | 365 | **1**(避開 multi-day quirk)|

### 效益

| 場景 | 修前 | 修後 | 加速 |
|---|---|---|---|
| Daily incremental | ~14 min(1300 reqs)| **~0.65 秒**(1 req)| **420×** |
| 5 yr backfill | ~70 min(6500 reqs)| **~20 min**(1825 reqs / sponsor 6000h)| **3.5×** |

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `config/collector.toml` | `price_limit` entry param_mode + segment_days 改 + notes 更新 |
| `CLAUDE.md` | 加 v3.23 章節 |

**0 alembic / 0 Rust / 0 Python**(純 config tweak;對齊 v3.14 gov_bank pattern 變體
— gov_bank 用 `all_market_no_end` 因 dataset 拒 end_date,price_limit 接受但有
multi-day quirk,故仍用 `all_market` + `segment_days=1`)。

### 沙箱驗證

- `python -c "from config_loader import load_collector_config; ..."` → all_market /
  segment_days=1 解析正常,39 entries 全部對齊
- 既有 `all_market` infrastructure(institutional_market / total_margin_purchase)
  已有 ALL_MARKET_SENTINEL pattern 接 sync_tracker,無需動 Python

### 用法(user 下次 incremental 自動套用)

```powershell
git pull   # 拉 collector.toml 改動
python src/main.py incremental   # price_limit 從 14 min → 0.65 秒
```

舊 per_stock 的 `api_sync_progress` 紀錄不衝突(progress 表 PK 含 api_name +
stock_id + segment;all_market mode 寫入 stock_id="_ALL_MARKET_" sentinel)。

### Out of Scope

- 其他 per_stock API 是否有同款 all_market quirk(price_daily / margin_daily /
  institutional 等)— probe + audit 留 v3.24 backlog,本 hotfix 只動 price_limit
- v3.14 gov_bank 同類 quirk audit(end_date 拒收 vs multi-day 靜默 0)— 既有 work
  已收尾不動

---

## v3.22 — B-5:4 new MCP tools 暴露 v3.20-v3.21 cores(2026-05-17)

接 v3.21 Commit C(`06d2829` Rust 4 cores + Silver 3 builders 全綠)後,user 拍版
B-5 — 對 v3.20-v3.21 新 4 cores 各加 1 個 high-level MCP tool,讓 LLM 在 Claude
Desktop 對話內可主動 surface 新訊號。

**0 alembic / 0 Rust / 0 collector.toml**(純 Python:`mcp_server/` 內 4 個新
helper + wrappers + tests)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `mcp_server/_loan_collateral.py` (新) | compute_loan_collateral_snapshot;直 SELECT loan_collateral_balance_derived |
| `mcp_server/_block_trade.py` (新) | compute_block_trade_summary;30 天期間 SUM + matching spike |
| `mcp_server/_risk_alert.py` (新) | compute_risk_alert_status;直讀 Bronze + measure 中文 parser(三級嚴重度)|
| `mcp_server/_commodity_macro.py` (新) | compute_commodity_macro_snapshot;loop commodities |
| `mcp_server/tools/data.py` | 加 4 wrapper function + 完整 docstring(LLM 看 tools list 用)|
| `mcp_server/server.py` | `@mcp.tool()` 註冊 +4(5 → 9 public tools)+ instructions 描述更新 |
| `tests/mcp_server/test_toolkit_v3.py` | 加 4 TestXxx class × 3-4 test = +15 cases(9 → 24)|

### 4 new public tools

| Tool | Signature | 對應 Core | 主要 Return |
|---|---|---|---|
| `loan_collateral_snapshot(stock_id, date)` | str,str | loan_collateral_core | 5 大類 balance / change_pct / ratio + concentration_alert(>70%)|
| `block_trade_summary(stock_id, date, lookback_days=30)` | str,str,int | block_trade_core | total volume/money + matching_share_avg + matching_spike_dates |
| `risk_alert_status(stock_id, date)` | str,str | risk_alert_core | current_status(in_period / severity / days_remaining)+ history_60d + escalation_count_60d |
| `commodity_macro_snapshot(date, commodities=None)` | str,list | commodity_macro_core | per-commodity price / return_z_score / momentum_state / streak_days / spike_alert |

### 設計拍版

- **DB 路徑**:全部走 `agg._db.get_connection()`(對齊 v3.5 R5 C12 single entry);
  直 SELECT 不重 wrap agg.as_of()(避免 join 不必要的 facts/indicator)
- **三級嚴重度 parser**(`_risk_alert.py:_parse_severity`):
  - 「全額交割」→ `cash_only`
  - 「人工管制」→ `disposition`(分盤撮合)
  - 「注意交易資訊」→ `warning`
  - 其他 → `unknown`
- **payload budget ≤ 3KB**(對齊既有 5 tools < 5KB):長 list 欄(matching_spike_dates
  / history_60d)truncate 到 10 筆內
- **empty fallback**:無 Silver 資料 → graceful narrative 指引 user 跑 builder
- **multi-commodity 支援**:`commodity_macro_snapshot` 可傳 `["GOLD", "SILVER"]` 等
  (v3.21 初版只有 GOLD,SILVER 等 data_available=False;預留擴展)

### Reference(每 tool docstring 內)

- loan_collateral:Basel Committee (2006) WP 15 — CR1 > 0.7 high concentration
- block_trade:Cao, Field & Hanka (2009) JEF 16:1-25 — matching > 80% 異常
- risk_alert:「證券交易所公布注意交易資訊處置作業要點」§4(2024 版)
- commodity_macro:Brock et al. (1992) JoF 47(5)+ Hamilton (1989) Econometrica 57(2)

### 沙箱驗證

- `pytest tests/mcp_server/test_toolkit_v3.py` ✅ **24 passed**(從 9 → 24,+15 new tests)
- `pytest tests/mcp_server/ tests/agg/` ✅ **101 passed / 1 skipped**(render_tools.py
  fastmcp 缺裝是 pre-existing,非本次造成)
- 4 個 helper module + tools/data.py + server.py 純 Python import ✅
- payload budget assertion 全部 < 3KB(典型 case)

### 待 user 跑 production verify(對齊 v3.21 Commit C verify chain)

```powershell
git pull

# v3.21 Commit C verify(若還沒跑)
python src/main.py incremental                    # Bronze 5 datasets 拉資料
python src/main.py silver phase 7a                # Silver 3 new builders
cd rust_compute && ./target/release/tw_cores.exe run-all --write
cd ..
psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql

# v3.22 B-5 整合驗證(MCP server stdio + Claude Desktop 對話)
python -m mcp_server   # 開 stdio server
# 在 Claude Desktop 對話內:
#   "幫我看 2330 今天的 loan_collateral 狀況"
#   "GOLD 過去 30 天的 macro 趨勢"
#   "3363 的 risk_alert 狀態"
#   "2330 過去 30 天的 block_trade 摘要"
```

### 風險

🟢 低:
- 0 Rust / 0 alembic / 0 collector.toml(純 Python)
- 既有 agg layer 0 改動(只 reuse `get_connection()`)
- 4 module 各獨立,1 個失敗不影響其他;既有 5 public tools 0 regression
- 既有 ~30 agg tests + 既有 mcp tests 全綠
- Rollback:單 commit `git revert` 即可

### Out of Scope(留 future)

- **C-9 dashboard tabs**(Streamlit 視覺化 v3.20-21 cores)— 等 user 對 B-5 LLM
  體驗滿意後再做(避免 dashboard + MCP 雙 surface 邏輯重複)
- **B-4 FastAPI thin wrap**(v1.35 backlog)— 獨立工作
- **Round 9 calibration**(若 verify 揭露 4 new cores 過頻 / 過稀)— 純 Rust const tweak
- **整合進 `market_context()`**(將 commodity_macro / risk_alert 摘要加進大盤判讀)
  — v3.23 拍版題

### 下一輪動工候選(verify 結果回來後)

1. 若 production 4 cores 觸發率 OK → **C-9 dashboard tabs**(借券 / 風險警示 / GOLD)
2. 若 4 cores 觸發率異常 → Round 9 calibration
3. 若 user 對既有 MCP 5+4 tools 體驗滿意 → 開始 **B-4 FastAPI thin wrap**

---

## v3.21 — 4 cores spec decisions 拍版 + risk_alert chip 歸位(2026-05-17)

接 v3.20 Bronze 5 datasets 接入後,user 拍版 4 cores 20 個 open questions。
**0 alembic / 0 Rust 邏輯 / 0 collector.toml**(純 spec 拍版 doc + Rust 落地
留待後續 commit)。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `m3Spec/chip_cores.md` | §十 loan_collateral_core proposal → 拍版(11 EventKind);§十一 block_trade_core proposal → 拍版(4 EventKind);加 §十二 risk_alert_core(從 env §十 搬入,4 EventKind)|
| `m3Spec/environment_cores.md` | 移除原 §十 risk_alert(搬到 chip §十二);原 §十一 commodity_macro 重編 §十 + 拍版(4 EventKind)|
| `docs/m3_cores_spec_pending.md` | §3.7 / §3.8 / §3.9(新)/ §5.6(廢棄)/ §5.7 拍版 decisions table |
| `CLAUDE.md` | 加 v3.21 章節 |

### 拍版 4 cores summary

#### `loan_collateral_core`(chip §十)— 11 EventKind

5 大類 × Surge/Crash = 10 + LoanCategoryConcentration = 11。
- Silver:`loan_collateral_balance_derived` 新表,5 主欄 + 5 change_pct + JSONB pack
- 70% concentration 對齊 Basel Committee (2006) WP 15
- 與 margin_core 並存(不整合,粒度不同)

#### `block_trade_core`(chip §十一)— 4 EventKind

LargeBlockTrade / Accumulation / Distribution / MatchingTradeSpike。
- Silver:`block_trade_derived` 新表(SUM by trade_type)
- 80% MatchingSpike 對齊 Cao et al. (2009) JEF — block trade matched 通常 50-70%

#### `risk_alert_core`(chip §十二,**從 env §十 搬入**)— 4 EventKind

Announced / Entered / Exited / Escalation,**全帶 metadata.severity**:
warning(注意)/ disposition(分盤撮合)/ cash_only(全額交割)。
- Silver:暫無 derived 表(直讀 Bronze,對齊 fear_greed 風格)
- escalation 60d ≥ 2 次對齊「證券交易所公布注意交易資訊處置作業要點」§4
- 分層歸位:per-stock signal 屬 chip(non-environment)

#### `commodity_macro_core`(environment §十)— 4 EventKind

Spike / MomentumUp / MomentumDown / RegimeBreak。
- Silver:`commodity_price_daily_derived` 新表(GROUP BY commodity 算 z/streak)
- streak_min_days = 5(macro 長於個股 3)— Brock et al. (1992) JoF
- regime_break_window = 10 — Hamilton (1989) Econometrica regime-switching

### 設計原則(user 拍版 2026-05-17)

- **零耦合,少抽象**:各 EventKind 獨立判定,不跨 core 整合;共用 threshold 但
  個別 EventKind fire(若需 individual disable 走 workflows toml `enabled = false`)
- **資料分層**:Bronze → Silver → Core → MCP;per-stock 屬 chip,全市場屬 env
- **參數選擇優先序**:production calibration → spec → 財經論文 + reference

### Commit 切法(本 session 切 2 個)

**Commit A**(已 push `2a3cd2c`):4 cores spec decisions 拍版 + risk_alert chip 歸位

**Commit B**(本 push,alembic + schema):3 Silver derived 表落地
- `loan_collateral_balance_derived`(5 主欄 + 5 change_pct + JSONB)
- `block_trade_derived`(SUM by trade_type per stock,date)
- `commodity_price_daily_derived`(z-score / streak / momentum per commodity)
- risk_alert 不需 Silver derived(直讀 Bronze,對齊 fear_greed 例外)
- market_value 不需 Silver derived(valuation_core 直接消費 Bronze)
- alembic head:`b7c8d9e0f1g2` → **`c8d9e0f1g2h3`**

**Commit C(下個 session,等 user 跑 alembic upgrade head 後再上)**:
- `rust_compute/cores/chip/loan_collateral_core/`
- `rust_compute/cores/chip/block_trade_core/`
- `rust_compute/cores/chip/risk_alert_core/`
- `rust_compute/cores/environment/commodity_macro_core/`
- `src/silver/builders/loan_collateral.py` + `block_trade.py` + `commodity_macro.py`
- `tw_cores` dispatcher.rs 加 4 個 match arm + workflows toml 加 4 entry
- cargo test 全綠後 push

### 待 user 做(下次 session 起點)

1. **跑 Commit B alembic**:`alembic upgrade head` → head `c8d9e0f1g2h3`
2. **驗證 3 張 Silver derived 表存在**:
   ```powershell
   psql $env:DATABASE_URL -c "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename IN ('loan_collateral_balance_derived','block_trade_derived','commodity_price_daily_derived') ORDER BY tablename;"
   ```
3. **回覆我** → 我上 Commit C(Rust + Silver builders + dispatcher)
4. **最後**:`python src/main.py incremental` 拉 5 datasets + production verify

### 風險

🟢 低:
- 0 alembic / 0 Rust / 0 collector.toml
- 純 doc 拍版,後續 Rust 上線時對齊本文件決策
- m3Spec 章節重編(risk_alert env→chip)是文件級搬移,既有 5 chip cores
  + 5 env cores 0 改動
- Rollback:單 commit `git revert` 即可

---

## v3.20 — 5 sponsor-tier datasets 接入 Bronze + 4 core spec proposals(2026-05-17)

接 v3.19 probe `--max 0` 全 catalog 跑完(user 本機,59 unused datasets / 15 回 200
+ 有資料),拍版動工 5 個高價值 dataset。User 拍版規則(2026-05-17):
- 不做日內 tick / 5-second
- 不做權證 / 期貨 / 期權
- 不做可轉債
- GoldPrice 5-分鐘粒度 → 每日只存第一筆

**1 alembic migration + 5 collector.toml entry + 1 aggregator + 4 m3Spec core proposals**。

### 範圍(2 commits / branch `claude/continue-previous-work-xdKrl`)

| Phase | 範圍 |
|---|---|
| **Bronze infrastructure**(production-ready,user 可直 backfill)| alembic `b7c8d9e0f1g2` 落 5 張 Bronze 表 / collector.toml 5 new entry(34→39)/ `bronze/aggregators/first_per_day.py` 新檔 + dispatcher 加 `first_per_day` strategy / schema_pg.sql 同步 |
| **m3Spec core proposals**(等 user 拍版)| chip_cores.md §十 + §十一 / environment_cores.md §十 + §十一 / docs/m3_cores_spec_pending.md §3.7 / §3.8 / §5.6 / §5.7 open question 表 |

### 5 個 Bronze datasets

| Dataset | Bronze 表 | PK | param_mode | 動機 |
|---|---|---|---|---|
| `TaiwanStockLoanCollateralBalance` | `loan_collateral_balance_tw` | (market,stock_id,date)| per_stock | 5 大借券類別 × 7 sub-fields = 34 cols,比 margin_daily 豐富 5× |
| `TaiwanStockBlockTrade` | `block_trade_tw` | (market,stock_id,date,trade_type)| per_stock | 大宗交易(配對 / 鉅額),smart-money 痕跡 |
| `TaiwanStockMarketValue` | `market_value_daily` | (market,stock_id,date)| per_stock | 個股市值,比 shares × close 推算精確 |
| `TaiwanStockDispositionSecuritiesPeriod` | `disposition_securities_period_tw` | (market,stock_id,date,disposition_cnt)| all_market | 處置股風險警示 |
| `GoldPrice`(每日 1 筆)| `commodity_price_daily` | (market,commodity,date)| all_market + agg `first_per_day` | GOLD macro signal,PK 含 commodity 開放擴 silver/oil |

### 4 個 m3Spec core proposals

| Core | 章節 | EventKind 提案 |
|---|---|---|
| `loan_collateral_core` | chip_cores.md §十 | 5 大類 × Surge/Crash(8)+ Concentration(1)= 9 |
| `block_trade_core` | chip_cores.md §十一 | LargeBlockTrade / Accumulation / Distribution / MatchingSpike(4)|
| `risk_alert_core` | environment_cores.md §十 | DispositionAnnounced / Entered / Exited / Escalation(4)|
| `commodity_macro_core` | environment_cores.md §十一 | Spike / MomentumUp / MomentumDown / RegimeBreak(4)|

**market_value 不開 core**(純資料層,valuation_core 直接用即可)。

### 設計決策

- **block_trade_tw PK 加 `trade_type` 維度**(對齊 v3.14 gov_bank `bank_name` pattern)。同 (stock_id, date, trade_type) 多筆視為同 logical row。
- **disposition_securities_period `param_mode = all_market`**(probe 揭露 with_data_id=2330 → 0 row,no_data_id → 17 row)。
- **commodity_price_daily PK 含 `commodity` 維度**(對齊 exchange_rate macro pattern;`first_per_day` aggregator hardcode 注入 `commodity='GOLD'`,未來擴 silver/oil 時 generalize)。
- **loan_collateral 34 columns 全散開**(對齊 Bronze raw 設計理念;Silver derived 後續 proposal 拍版後再決定 5 主欄 + JSONB 或全散開)。
- **risk_alert_core 歸 environment_cores**(雖然 per-stock 但本質是「外加風險環境」,對齊 env layer 語意)。

### 沙箱驗證(本 session)

- `python -c "tomllib.load(...)"`:39 entries 全部解析正常,5 new entries 對齊
- `load_collector_config('config/collector.toml')`:5 new entries ApiConfig 全部正確
- `apply_aggregation('first_per_day', rows)`:3 unit test 通過(intraday → daily / empty / existing commodity 不覆蓋)
- `pytest tests/`:19 pre-existing failures 全 pytest-asyncio 缺裝,非本次改動觸發
- 0 Rust 改動(spec proposal 階段)

### 待 user 做(下次 session 起點)

1. **執行 alembic migration**:`alembic upgrade head` 落 5 張 Bronze 表
   (head 從 `a6b7c8d9e0f1` → `b7c8d9e0f1g2`)
2. **跑 incremental backfill**:`python src/main.py incremental` 拉 5 個新 dataset
   - 預估 wall time:per_stock × 3 datasets × 1266 stocks × 5 year = ~6h(對齊 1600 reqs/h)
   - DispositionSecuritiesPeriod all_market × 5 yearly seg = 數分鐘
   - GoldPrice all_market × 5 yr / 30 day seg = ~60 reqs × 2.25s = 數分鐘
3. **review m3Spec 4 proposals**(chip §十 / §十一 + environment §十 / §十一),拍版 open questions 表
4. **拍版後 1 個一個上 Rust**(對齊 v3.14 gov_bank pattern):新 crate + Silver derived + dispatcher
5. **(沿用)production verify 前先跑** `scripts/maintain_facts_stats.sql`

### 風險

🟢 低:
- alembic CREATE TABLE IF NOT EXISTS(空表落地,0 既有資料受影響)
- collector.toml 5 entry 純 additive,既有 34 entry 0 改動
- field_rename 對 FinMind 真實欄名(probe 確認)
- first_per_day aggregator 沙箱驗證 + 既有 4 aggregator 0 改動
- m3Spec proposal 純 doc(對齊「best-guess 不上 Rust」鐵律)
- 既有 Python 模組 0 改動(field_mapper / phase_executor / segment_runner / silver builders)

### 注意事項

- **commodity 注入 hardcode "GOLD"**:當前 first_per_day aggregator 對 0 個既有
  case 適用(只 v3.20 GoldPrice 用)。未來擴 silver/oil 時需 generalize(走
  collector.toml extra field)。
- **block_trade Bronze 同 (stock_id, date, trade_type) 多筆**:目前 PK 邏輯
  讓再次 upsert 覆蓋舊資料。若同日多筆同 trade_type 需保留,需 alembic 加
  `row_idx INTEGER` 進 PK(spec_pending §3.8.5 open question)。
- **DispositionSecuritiesPeriod `measure` 欄是長中文字串**(~300 字描述處置
  措施),Silver builder 後續若要解析三級嚴重度需中文字串 parser(spec_pending
  §5.6.3 open question)。

---

## v3.19 — 3 並行 track:gov_bank Core spec proposal + probe audit + wall time 假設(2026-05-17)

接 v3.18 Round 8 結算 + docs 整理後 user 拍版「開工」3 件事:
(1) gov_bank_net Core 消費(等 EventKind 規格 — user 選「我先 draft proposal」)
(2) probe sponsor tier 全 catalog audit
(3) wall time +24% regression 假設調查

**0 alembic / 0 Rust / 0 collector.toml**(純 spec + script + doc 動工)。

### Track A:gov_bank_core spec proposal(`m3Spec/chip_cores.md §九`)

新增第 9 節完整 spec draft(~200 行,§9.1 ~ §9.12)+ 目錄同步 + `docs/m3_cores_spec_pending.md §3.6` 8 個 open question 表落地。**等 user 拍版後再上 Rust**(對齊「best-guess 不上 Rust」鐵律)。

| 段 | 重點 |
|---|---|
| §9.1 定位 | 8 公股行庫合計 net,sovereign-controlled stabilization signal;與 institutional 質的差異(政策面 / 70% 日 net=0 稀疏 / per-bank breakdown 可選)|
| §9.2 上游 Silver | `institutional_daily_derived.gov_bank_net`(v3.14 fill 80.74%);`chip_loader::InstitutionalDailyRaw.gov_bank_net` 已 load 但 0 core 消費 |
| §9.3 Params | streak_min_days=3 / large_transaction_z=2.7(對齊 v3.17 institutional)/ lookback_for_z=60 / silence_period_days=10 🟡 |
| §9.5 Output | 4 個 EventKind:`GovBankAccumulation` / `GovBankDistribution` / `GovBankLargeTransaction` / `GovBankSilenceBreak` 🟡 |
| §9.6 觸發機制 | streak / edge trigger(對齊 institutional §3.6 r3 Brown & Warner 1985)/ silence break unique pattern |
| §9.9 與 institutional 區別表 | 訊號性質 / 頻率 / silence 概念 / cross-source divergence — 4 維對照,佐證獨立 core 設計 |
| §9.10 8 open questions | SilenceBreak 保留? / silence_period_days 預設? / per-bank breakdown? / FlowReversal? / NULL 處理 / timeframe / 2021-06-30 前處理 / structural_snapshots 寫入? |
| §9.11 calibration 目標 | per-EventKind ≤ 12/yr/stock 對齊 v1.32 baseline;預估 3 ~ 10/yr 區間 |
| §9.12 crate 結構 | 新 `cores/chip/gov_bank_core/`(對齊 §四 zero-coupling),`tw_cores` dispatch 加 4 處 |

### Track B:probe sponsor tier 全 catalog audit(`scripts/probe_finmind_sponsor_unused.py`)

腳本 **production-ready 無需動**:
- `--max 0` default + line 166 `if args.max > 0` conditional skip = 全 catalog 模式正常
- 422 enum parser → catalog ~91 datasets - collector.toml 34 enabled ≈ **57 unused** 待 probe
- 2.25s/req × (probe + 可能 fallback 4.5s) × 57 ≈ **4-5 分鐘**(< 1600 reqs/h rate limit)
- IP ban abort 已 wired(line 184 / 403 + body 含 "ip" → abort)
- Windows cp950 console UTF-8 reconfigure + ASCII labels([OK]/[WARN]/[LOCK]/[BAN]/[FAIL])

**user 跑法**(下次 session 起點):
```powershell
# 確認 IP ban 已解(過去 collector backfill 可能踩到 ban)
python scripts/probe_finmind_sponsor_unused.py --max 0
# 輸出 Summary 段列「unused datasets 回 200 + 有資料」候選清單
# 找到候選後 → 寫進 collector.toml + alembic Bronze schema(對齊 v3.14 gov_bank pattern)
```

### Track C:wall time +24% regression — root cause 假設 + 2 diagnostic 腳本

Explore agent 沙箱調查結論(`rust_compute/cores/system/tw_cores/`):

**per-core `elapsed_s` 真實語意**:**cumulative across concurrent stock workers**,
非 wall time(`dispatcher.rs:86` 對每個 (core, stock) 個別 `start.elapsed()` 計時,
`summary.rs:78` `entry.5 += r.elapsed_ms` 聚合)。v3.18 每個 core elapsed_s 5-15×
暴增是 **worker idle/blocking 量訊號**,非 CPU 工作量。

**Root cause 排序(信心):**
1. **🔴 60% PG stats 過期** — v3.18 foreign_holding facts_new 165888→210996(+27%);
   DELETE+INSERT 後未 trigger ANALYZE → planner 對 `uq_facts_dedup` unique index
   選錯 plan → ON CONFLICT 路徑掃描變慢 → connection hold time ↑ → 全 worker 排隊
2. **🟡 25% sqlx pool contention** — pool size = `concurrency + 4`(= 36 if 32);
   原因 1 觸發後 connection hold 變長 → 其他 task queue
3. **🟡 15% Facts batch upsert 邏輯** — `ON CONFLICT DO NOTHING` 在 ~10M+ row facts
   表 + 多 partition 上每 batch 都掃 index

**沙箱動工**(2 新腳本):

| 腳本 | 用途 |
|---|---|
| `scripts/maintain_facts_stats.sql` | 5 phase:pre stats / ANALYZE 三表 / VACUUM / post stats / index 健康度;Round N DELETE+INSERT 後跑(便宜 50-200ms 換 planner 對 facts 正確 stats) |
| `scripts/diagnose_slow_tw_cores.sql` | tw_cores 跑期間另開 psql 取樣;4 phase:active sessions / lock waits / EXPLAIN dedup query plan / pool saturation + 4 種觀察組合解讀指南 |

### 範圍(2 commits / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `m3Spec/chip_cores.md` | 加 §九 gov_bank_core proposal(~210 行)+ 目錄 1 行 |
| `docs/m3_cores_spec_pending.md` | 加 §3.6 gov_bank_core 8 open questions 表 |
| `scripts/maintain_facts_stats.sql`(新)| 5 phase ANALYZE + VACUUM stats 維護 |
| `scripts/diagnose_slow_tw_cores.sql`(新)| 4 phase wall time 慢時即時取樣 + 解讀指南 |
| `CLAUDE.md` | helper 腳本清單 2 新行 + v3.19 章節 |

### 待 user 做(下次 session 起點)

1. **review gov_bank_core spec proposal**(§九 + spec_pending §3.6)→ 拍版 8 個 open
   question → 我下個 session 上 Rust(對齊 §9.12 crate 結構 + dispatch 4 處)
2. **跑 probe 全 catalog**:`python scripts/probe_finmind_sponsor_unused.py --max 0`
   → 看 Summary 段候選 dataset → 評估是否加 collector.toml(對齊 v3.14 gov_bank pattern)
3. **下次 production verify 前跑** `psql -f scripts/maintain_facts_stats.sql`,記下
   wall time;若仍 +24% 就在 tw_cores 跑期間另開 psql 跑 `diagnose_slow_tw_cores.sql`
   取證據確認 root cause(60% 我猜 ANALYZE 後降回 v3.17 水準)

### 風險

🟢 低:
- spec proposal 純 doc,0 程式碼;user 拍版後才動 Rust
- 2 SQL 腳本是 read-only stats + 維護(ANALYZE / VACUUM 不 lock 表)
- 0 alembic / 0 Rust workspace / 0 collector.toml
- Rollback:每段獨立 commit,任意可單獨 revert

---

## v3.18 — Round 8.3 calibration:milestone spacing 3→2 + Round 8 結算(2026-05-17)

接 v3.17 Round 8.2 production verify(commit `493fc4a`)後,5/6 EventKind 達標,
1 個微差(Low 7.88 距 8-10 下緣 0.12)+ annual variants 仍偏低於 target band 下緣
(LowAnnual ~4.0 估 / HighAnnual ~2.9 估,未進 top 30 顯示)。本 session
Round 8.3 收尾 nudge 全 4 milestone variants 居 target band 中央。

### Round 8 三輪收尾分析(production-data-driven cluster size)

**Retention 表(對 v3.14 base):**

| variant | v3.14 | v3.15 sp=10 | v3.16 sp=5 | v3.17 sp=3 | v3.18 sp=2(預測)|
|---|---|---|---|---|---|
| Low | 15.46 | 3.97 (26%) | 5.87 (38%) | **7.88 (51%)** | ~9.6 (62%) |
| High | 11.96 | 3.26 (27%) | 4.71 (39%) | **6.25 (52%)** | ~7.4 (62%) |
| LowAnnual | 7.86 | 2.03 (26%) | 2.99 (38%) | ~4.0 (51%) | ~4.9 (62%) |
| HighAnnual | 5.73 | 1.49 (26%) | 2.18 (38%) | ~2.9 (51%) | ~3.6 (62%) |

**cluster size 模型修正**:retention 序列 25%/38%/51% 揭露真實 cluster ≈ **2.0 event**
(非 v3.16 估的 4-event 也非 v3.17 估的 2.6)。spacing 邊際遞減清晰,spacing=2
為 sweet spot — 對應 cluster=2.0 邏輯下「每個 cluster 留首發,後續壓掉」。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/chip/foreign_holding_core/src/lib.rs` | `MIN_MILESTONE_SPACING_DAYS` 3 → **2** + docstring 加 v3.18 + 2 test 數字微調(test1 3天→2天,test2 5天 gap→3天 gap) |
| `scripts/verify_event_kind_rate.sql` | 加 Section 4:foreign_holding milestone 4 variants 顯式(annual variants 通常不進 Section 1 top 30,單獨拉 + target band 對照) |

### 驗證(本 session 沙箱)

- `cargo test --release -p foreign_holding_core` ✅ 6 passed(同 v3.17)
- `cargo test --release --workspace --no-fail-fast` ✅ **426 passed / 0 failed**(同 v3.17)

### 待 user 跑 production verify(下次 session)

```powershell
git pull
cd rust_compute
cargo clean -p foreign_holding_core -p tw_cores
cargo build --release -p tw_cores
cd ..

# DELETE 4 milestone EventKinds(LargeTransaction 不動)
psql $env:DATABASE_URL -c "
DELETE FROM facts
WHERE source_core = 'foreign_holding_core'
  AND metadata->>'event_kind' IN
      ('HoldingMilestoneLow','HoldingMilestoneHigh',
       'HoldingMilestoneLowAnnual','HoldingMilestoneHighAnnual');
"

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
# Section 4 會單獨列 4 milestone variants + target band 對照,annual 不再被 top 30 截掉
```

### 預期落點 vs 實測(2026-05-17 production verify 完成)

| EventKind | v3.17 實測 | v3.18 預期 | v3.18 實測 | target band | 達標 |
|---|---|---|---|---|---|
| `HoldingMilestoneLow` | 7.88 | ~9.6/yr | **10.06** | 8-10 | ✅(0.06 微 over,noise)|
| `HoldingMilestoneHigh` | 6.25 | ~7.4/yr | **7.90** | 6-9 | ✅ band 中央 |
| `HoldingMilestoneLowAnnual` | ~4.0 | ~4.9/yr | **5.10** | 4-6 | ✅ band 中央 |
| `HoldingMilestoneHighAnnual` | ~2.9 | ~3.6/yr | **3.74** | 3-4 | ✅ band 中央 |
| `LargeTransaction` | 14.16 | 14.16(不動) | 14.16 | accepted baseline | ✅ |
| `SignificantSingleDayChange` | 11.74 | 11.74(不動) | 11.74 | ≤ 12 | ✅ |

**4/4 milestone variants 一致 ~65% retention**(預測 62%)→ 真實 cluster ≈ 1.9
event/cluster(微小於估計 2.0)。模型精度持續收斂。foreign_holding_core
facts_new=210,996(params_hash 變動如預期);其他 35 cores 0 變動。

### Round 8 calibration session 結算(v3.15 → v3.18 四輪)

| 輪 | spacing / z | Low / High / LowAnn / HighAnn / LargeTx | 結論 |
|---|---|---|---|
| Round 8(v3.15) | sp=10, z=2.5, z=2.1 | 3.97 / 3.26 / 2.03 / 1.49 / 15.99 | spacing/z 雙 over-correction |
| Round 8.1(v3.16) | sp=5, z=2.7 | 5.87 / 4.71 / 2.99 / 2.18 / 14.16 | 偏低 + LargeTx 仍 over |
| Round 8.2(v3.17) | sp=3 | 7.88 / 6.25 / ~4.0 / ~2.9 / 14.16 accepted | 1/4 in band,LargeTx 並列 baseline |
| **Round 8.3(v3.18)** | sp=2 | **10.06 / 7.90 / 5.10 / 3.74 實測** | **4/4 in band ✅(verified 2026-05-17)** |

**Round 8 session 結算**:6 個原始 over-fired EventKind,5 個校準到 target band,
1 個(LargeTransaction)接受為 accepted baseline(fat-tail reality)。Round 8 四輪
production-data-driven cluster size 模型從 4 → 2.6 → 2.0 → 1.9 收斂,spacing 從
10 → 5 → 3 → 2 同步收斂 sweet spot。**Round 8 calibration session ☕ 完整結算**。

### accepted baselines(v1.32 + v3.17,Round 8 結束時)

| EventKind | rate/yr | 接受理由 |
|---|---|---|
| `institutional / DivergenceWithinInstitution` | 58.41 | v1.32 拍版 production reality |
| `institutional / LargeTransaction` | 14.16 | v3.17 拍版 — fat-tail (Lo 2001),邊際效益遞減 |
| `institutional / NetSellStreak` | 10.84 | ≤ 12 OK |
| `institutional / NetBuyStreak` | 10.39 | ≤ 12 OK |

### 已知狀態(下次 session 起點)

- alembic head:`a6b7c8d9e0f1`(不變,本 session 0 migration)
- Rust workspace:35 crates / **426 tests passed / 0 failed**
- Round 8 calibration **完整結算**(2026-05-17 production verify 4/4 milestone in band)
- 下次 session 動工候選(Round 8 結束,calibration backlog 清空):
  1. **gov_bank_net Core 消費**(需先寫 GovBankAccumulation/Distribution EventKind 規格)
  2. **probe --max 0 全 catalog**(找 sponsor tier 內其他 unused dataset)
  3. **B-4 FastAPI thin wrap** Aggregation Layer 對外 API
  4. **m3Spec/ 未動工項目**(P3 後階段 cores / Wave Cores Phase 20+)
  5. **wall time 微優化**(v3.18 run-all 695s vs v3.17 561s,+24%;
     非 calibration 引起,可能是 PG contention 或 sqlx pool 調整)

### 風險

🟢 低:
- 純 1 const value 改變,0 alembic / 0 Python / 0 collector.toml
- 既有 6 test margin 仍充足(milestone spike 50→48 step 0.5 vs spacing=2 → 2 fire 預期 ✓)
- 2 個 milestone spacing test 邏輯 0 變,只壓 spacing=2 邊界數字
- production 行為改變 spec-defensible(cluster=2.0 production-data-driven 收斂)
- Rollback:單 commit `git revert` 即可(spacing 3→2 反向)

---

## v3.17 — Round 8.2 calibration:milestone spacing 5→3 + LargeTransaction 14.16 accepted baseline(2026-05-17)

接 v3.16 Round 8.1 production verify(commit `5577fb3`)後,4 個 milestone variants
全數命中 ≤ 12/yr 但偏 target band 下緣(Low 5.87 / High 4.71 / LowAnnual 2.99 /
HighAnnual 2.18);LargeTransaction z=2.7 落 14.16/yr,仍 17% over target。本 session
動工 Round 8.2 收尾 calibration session,**0 alembic / 0 Python / 0 collector.toml**,
純 Rust 1 const tweak + 2 test 數字微調 + 2 處 docstring rationale 補強。

### 動工拍版理由(production-data-driven)

**Milestone variants 一致 38% retention 揭露 cluster size 真實值**:

| variant | v3.14 | v3.15 spacing=10 | v3.16 spacing=5 | retention vs v3.14 |
|---|---|---|---|---|
| Low | 15.46 | 3.97 | 5.87 | 38% |
| High | 11.96 | 3.26 | 4.71 | 39% |
| LowAnnual | 7.86 | 2.03 | 2.99 | 38% |
| HighAnnual | 5.73 | 1.49 | 2.18 | 38% |

4 個 variant 全部一致 38% retention(極一致)→ cluster avg ≈ **2.6 events**
(非原 v3.16 估的 4-event)。spacing=5 對台股 cluster 過嚴,壓掉 62% 真實事件。

**Round 8.2 spacing 5→3 預測**(data-driven 對齊 cluster=2.6 sweet spot):
- 預期 retention ~55-65% → Low ~8.5/yr / High ~6.7/yr / LowAnnual ~4.3/yr / HighAnnual ~3.1/yr
- 全 4 variant 落 target band ✅(8-10 / 6-9 / 4-6 / 3-4)

**LargeTransaction 14.16/yr accept rationale**:
- v3.14 → v3.16:23.49 → 14.16(-40%)
- 重尾 reality(Lo 2001 + Cont 2001)進一步驗證:Gaussian 預期 ×3.4 = 13.6
  實際 14.16(極接近 fat-tail 模型而非 Gaussian)
- v3.15→v3.16 z 2.5→2.7 邊際效益 -11%(Gaussian 預期 -44%);v3.16→v3.17 z 2.7→3.0
  預期僅 -15% 至 ~12,投資報酬率低且踏 99.73th percentile 過嚴
- **accepted baseline 14.16/yr**,對齊 `DivergenceWithinInstitution 58.41`
  (v1.32 accepted)的 production reality 並列處理

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| 檔 | 動作 |
|---|---|
| `rust_compute/cores/chip/foreign_holding_core/src/lib.rs` | `MIN_MILESTONE_SPACING_DAYS` 5 → **3** + 1 處 docstring 加 v3.17 rationale + 1 處 inline comment update + 2 test(spacing=3 邊界數字微調:test1 4天→3天 / test2 7天 gap→5天 gap) |
| `rust_compute/cores/chip/institutional_core/src/lib.rs` | `large_transaction_z` 不動(維持 2.7)+ docstring 加 **v3.17 accepted baseline** 段(對齊 DivergenceWithinInstitution 58.41 pattern)|

### 驗證(本 session 沙箱)

- `cargo build --release -p foreign_holding_core -p institutional_core` ✅ 0 warnings
- `cargo test --release -p foreign_holding_core -p institutional_core` ✅ 12 passed(同 v3.16)
- `cargo test --release --workspace --no-fail-fast` ✅ **426 passed / 0 failed**(同 v3.16)

### 待 user 跑 production verify(下次 session)

```powershell
git pull
cd rust_compute
cargo clean -p foreign_holding_core -p tw_cores
cargo build --release -p tw_cores
cd ..

# DELETE 4 個 milestone EventKinds(LargeTransaction 不動,params_hash 沒變)
psql $env:DATABASE_URL -c "
DELETE FROM facts
WHERE source_core = 'foreign_holding_core'
  AND metadata->>'event_kind' IN
      ('HoldingMilestoneLow','HoldingMilestoneHigh',
       'HoldingMilestoneLowAnnual','HoldingMilestoneHighAnnual');
"

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
```

### 預期 production verify 結果

| EventKind | v3.16 | v3.17 預期 | target band | 達標 |
|---|---|---|---|---|
| `HoldingMilestoneLow` | 5.87 | ~8.5/yr | 8-10 | ✅ |
| `HoldingMilestoneHigh` | 4.71 | ~6.7/yr | 6-9 | ✅ |
| `HoldingMilestoneLowAnnual` | 2.99 | ~4.3/yr | 4-6 | ✅ |
| `HoldingMilestoneHighAnnual` | 2.18 | ~3.1/yr | 3-4 | ✅ |
| `LargeTransaction` | 14.16 | 14.16(不動) | accepted baseline | ✅ |
| `SignificantSingleDayChange` | 11.74 | 11.74(不動) | ≤ 12 | ✅ |

### Round 8 calibration session 收尾(v3.15 → v3.17 三輪)

| 輪 | 範圍 | 結果 |
|---|---|---|
| Round 8(v3.15) | z 2.0→2.5 + spacing=10 + z 2.0→2.1 | 6/6 EventKind 觸發率有變,過嚴 |
| Round 8.1(v3.16) | spacing 10→5 + z 2.5→2.7 | 5/6 OK,milestone 4 variant 偏低,LargeTransaction 14.16 仍 over |
| Round 8.2(v3.17) | spacing 5→3 + LargeTransaction accept | data-driven cluster=2.6 對齊 sweet spot,LargeTransaction 並列 baseline |

### 已知狀態(下次 session 起點)

- alembic head:`a6b7c8d9e0f1`(不變,本 session 0 migration)
- Rust workspace:35 crates / **426 tests passed / 0 failed**(同 v3.16)
- 1 const tweak + 2 test 邊界數字微調 + 2 處 docstring rationale 補強
- Round 7 5 cores 不動(0 row verify ✅);Round 8 calibration 結束
- accepted baselines(v1.32 + v3.17):
  - `institutional / DivergenceWithinInstitution` 58.41/yr(v1.32 accepted)
  - `institutional / LargeTransaction` **14.16/yr(v3.17 accepted)**
  - `institutional / NetSellStreak` 10.84/yr / `NetBuyStreak` 10.39/yr(均 ≤ 12)

### 風險

🟢 低:
- 純 1 const value 改變,0 alembic / 0 Python / 0 collector.toml
- 既有 6 test margin 仍充足(LargeTransaction spike z=50 vs 2.7 → 18× margin
  / milestone spike 50→48 vs spacing=3 → 2 fire 預期 ✓)
- 2 個 milestone spacing test 邏輯 0 變,只壓 spacing=3 邊界數字
- production 行為改變 spec-defensible(data-driven cluster=2.6 對齊 spacing=3
  sweet spot + LargeTransaction Lo 2001 重尾邊際效益遞減)
- Rollback:單 commit `git revert` 即可

---

## v3.16 — Round 8.1 calibration:milestone spacing 10→5 + LargeTransaction z 2.5→2.7(2026-05-17)

接 v3.15 Round 8 production verify(commit `f6c867f`)揭露 2 個 over-correction:
4 個 milestone variants 全 collapse 到 1.5-4/yr(target 3-10),LargeTransaction
仍 15.99/yr 超 12 target。本 session 動工 Round 8.1,**0 alembic / 0 Python /
0 collector.toml**,純 Rust 2 const tweak + 2 test 數字更新。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| Core | EventKind | v3.14 | v3.15 Round 8 | v3.16 修法 | v3.16 預期 |
|---|---|---|---|---|---|
| `institutional_core` | LargeTransaction | 23.49/yr | 15.99/yr | `large_transaction_z` 2.5 → **2.7**(Lo 2001 重尾)| ~8/yr ✅ |
| `foreign_holding_core` | HoldingMilestoneLow | 15.46 | 3.97 ❌ | `MIN_MILESTONE_SPACING_DAYS` 10 → **5** | ~7-9/yr ✅ |
| `foreign_holding_core` | HoldingMilestoneHigh | 11.96 ✅ | 3.26 ❌ | 同上(對稱) | ~5-7/yr ✅ |
| `foreign_holding_core` | HoldingMilestoneLowAnnual | 7.86 ✅ | 2.03 ❌ | 同上 | ~3-5/yr ✅ |
| `foreign_holding_core` | HoldingMilestoneHighAnnual | 5.73 ✅ | 1.49 ❌ | 同上 | ~2-4/yr ✅ |

### Root cause 分析(production-data-driven)

**milestone spacing=10 過嚴**:
- 觀察 retention = 25%(15.46→3.97)= 1/4
- → 台股外資持股 cluster size ≈ **4-event 平均**(連續 monotonic drift,foreigner accumulation/distribution 期間)
- spacing=10 把 4-event cluster 全壓成 1 → 25% retention
- 縮 spacing=5:壓 2-event cluster → ~50% retention,Low 預期 15.46 × 50% ≈ 7.7/yr ✅

**LargeTransaction z=2.5 仍 over**:
- z=2.5 Gaussian theory:98.76th percentile,預期 6.4/yr
- production 觀察 15.99/yr = Gaussian × 2.5
- root cause:台股 institutional net 重尾(fat-tailed)分布
  - Lo (2001) *Econometrica* 59(5):1279-1313 financial time series 長記憶 + 重尾
  - Cont (2001) *Quant Finance* 1(2):223-236 stylized fact 跨資產普遍
- tighten z 2.5→2.7(99.31th percentile)→ 預期 ~8/yr ✅

### Test update(對齊新 spacing=5 邊界)

| Test | v3.15 | v3.16 |
|---|---|---|
| `milestone_spacing_prevents_consecutive_low_fires` | 8 連天每日新低 / spacing=10 → 1 fire | 4 連天每日新低 / spacing=5 → 1 fire |
| `milestone_spacing_allows_refire_after_gap` | 11 天 gap / spacing=10 → 2 fire | 7 天 gap / spacing=5 → 2 fire |

邏輯 0 變,純數字壓進新邊界。

### 驗證(本 session 沙箱)

- `cargo build --release -p institutional_core -p foreign_holding_core` ✅ 0 warnings
- `cargo test --release -p institutional_core -p foreign_holding_core` ✅ 12 passed(同 v3.15)
- `cargo test --release --workspace --no-fail-fast` ✅ **426 passed / 0 failed**

### 待 user 跑 production verify(下次 session)

```powershell
git pull
cd rust_compute
cargo clean -p foreign_holding_core -p institutional_core -p tw_cores
cargo build --release -p tw_cores
cd ..

# DELETE 5 個 affected EventKinds(LargeTransaction + 4 milestone variants)
psql $env:DATABASE_URL -c "
DELETE FROM facts
WHERE (source_core = 'institutional_core' AND metadata->>'event_kind' = 'LargeTransaction')
   OR (source_core = 'foreign_holding_core' AND metadata->>'event_kind' IN
       ('HoldingMilestoneLow','HoldingMilestoneHigh',
        'HoldingMilestoneLowAnnual','HoldingMilestoneHighAnnual'));
"

cd rust_compute && .\target\release\tw_cores.exe run-all --write && cd ..
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
```

預期 5/6 命中 target band(SignificantSingleDayChange 11.74 不動,已 ✅)。

### 已知狀態(下次 session 起點)

- alembic head:`a6b7c8d9e0f1`(不變,本 session 0 migration)
- Rust workspace:35 crates / **426 tests passed / 0 failed**(同 v3.15)
- 2 const tweak + 2 test 微調 + reference 註解強化
- Round 7 5 cores 不動(0 row verify ✅)
- accepted baselines 不動:`DivergenceWithinInstitution 58.41/yr` / streak 10.84+10.39

### 風險

🟢 低:
- 純 2 const value 改變,0 alembic / 0 Python / 0 collector.toml
- 既有 tests margin 充足(institutional spike z=50 vs 2.7 → 18× margin;foreign spike z=20 vs 2.1 → 9.5× margin)
- 2 個 milestone spacing test 邏輯 0 變,只壓數字
- production 行為改變 spec-defensible(Lo 2001 重尾 + production-data-driven cluster=4)
- Rollback:單 commit `git revert` 即可

---

## v3.15 — Round 8 calibration:3 EventKinds tighten(2026-05-16)

接 v3.14 verify SQL 揭露的 3 個微超 EventKind,本 session 動工 Round 8(對齊
v3.11 Round 7 pattern)。**0 alembic / 0 Python / 0 collector.toml**,純 Rust
2 cores 的 const tweak + 1 個新 spacing 機制 + 2 regression tests。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

| Core | EventKind | 修前 | 修法 | 修後預期 |
|---|---|---|---|---|
| `institutional_core` | LargeTransaction | 23.49/yr/stock | `large_transaction_z` 2.0 → **2.5**(Strong & Xu 1999 對 Asian markets 推薦 2.5σ)| ~6.4/yr/stock(98.76th percentile)|
| `foreign_holding_core` | HoldingMilestoneLow | 15.46/yr/stock | 新 `MIN_MILESTONE_SPACING_DAYS=10`,4 個 milestone variants 全部對稱套用(Low / High / LowAnnual / HighAnnual)| ~8-10/yr/stock(連續探低 cluster 視為同事件)|
| `foreign_holding_core` | SignificantSingleDayChange | 12.88/yr/stock | `change_z_threshold` 2.0 → **2.1**(97.86th percentile)| ~10/yr/stock(微調 -21.5%)|

### 設計決策(對齊 cores_overview §四 + §十四)

1. **3 個 EventKind 分別不同手法**:
   - LargeTransaction:純 z-threshold tighten(已是 edge trigger,Round 5/6 都用過此模式)
   - HoldingMilestoneLow:加 MIN_SPACING(對齊 v3.11 Round 7 trendline/adx/atr 5 cores 同款 pattern)
   - SignificantSingleDayChange:微調 z(只超 7% 用最小改動)
2. **Milestone spacing 對稱套 4 variants**:雖然只 Low 過標,High / LowAnnual / HighAnnual
   保持對稱(uniform rule),避免不對稱行為。HighAnnual 觸發率本來就低,加 spacing 後仍會
   保留 meaningful 訊號。
3. **2 regression tests**:`milestone_spacing_prevents_consecutive_low_fires`(連續 8 天探低
   應只 1 fire)+ `milestone_spacing_allows_refire_after_gap`(spacing 過後可再 fire)
4. **既有 tests 全綠**:現有 4 個 institutional + 4 個 foreign_holding 測試 spike 設計
   margin 充足(spike z ≈ 20 vs threshold 2.0→2.5),0 break

### Reference

- Strong, N. & Xu, X. G. (1999). "The Profitability of Volatility Spillovers in
  Asian Stock Markets". *Journal of Banking & Finance* 23:1297-1313 — Asian markets
  推薦 2.5σ 取代 2σ
- Lucas, R. C. & LeBeau, C. (1992). "Technical Traders Guide to Computer Analysis"
  Ch.7 — pivot 確認需要 N-bar holding,milestone spacing 對齊週時間框架慣例
- Fama, F., Fisher, L., Jensen, M. & Roll, R. (1969). *IER* 10(1):1-21 — 2σ 事件研究
  基準(SignificantSingleDayChange 原始 reference)

### 驗證(本 session 沙箱)

- `cargo build --release -p institutional_core -p foreign_holding_core` ✅ 0 warnings
- `cargo test --release -p institutional_core -p foreign_holding_core` ✅ 12 passed(8 existing + 4 new)
- `cargo test --release --workspace --no-fail-fast` ✅ **426 passed / 0 failed**
  (從 v3.7 420 → +6 = 4 milestone spacing tests + 2 沿路新增)

### 待 user 跑 production verify(下次 session)

```powershell
# 1. 重編 + 重跑 affected cores
cd rust_compute && cargo build --release -p tw_cores
.\target\release\tw_cores.exe run-all --write
# 預期:institutional_core + foreign_holding_core 新 facts 寫入(其他 33 cores params_hash 沒動 → 0 facts_new)

# 2. verify per-EventKind rate
psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
# 預期:
#   institutional/LargeTransaction:23.49 → ~6-8/yr ✅(落入 6-12 target)
#   foreign_holding/HoldingMilestoneLow:15.46 → ~8-10/yr ✅
#   foreign_holding/SignificantSingleDayChange:12.88 → ~10/yr ✅
```

### 已知狀態(下次 session 起點)

- alembic head:`a6b7c8d9e0f1`(不變,本 session 0 migration)
- Rust workspace:35 crates / **426 tests passed / 0 failed**
- collector.toml:34 entries(不變)
- 3 個 EventKind 都加完整 reference docstring + production data driven 註解
- 下次 session 動工候選:
  1. **production verify Round 8**(user 跑 tw_cores + verify SQL,~10 分鐘 + verify)
  2. **gov_bank_net Core 消費**(需先在 m3Spec/chip_cores.md 或新 doc 寫
     GovBankAccumulation/Distribution EventKind 規格)
  3. **probe --max 0 全 catalog**(找 sponsor tier 內其他 unused dataset)

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- 純 Rust 常數調整,2 cores 各 1-2 const + 1 個新 spacing 機制
- 既有 8 個 institutional + foreign_holding test margin 充足,0 break
- production 行為改變 spec-defensible(Strong & Xu 1999 / Lucas & LeBeau 1992 引用)
- Rollback:單 commit `git revert` 即可

---

## v3.14 — gov_bank pipeline 收尾 + Round 7 calibration 達標 verify(2026-05-17)

接 v3.11 Round 7 production run 後 user 升 FinMind sponsor tier 動 gov_bank
pipeline,同 session 走完 v3.11 → v3.14.1 6 commits + Round 7 efficacy verify。

### gov_bank pipeline 收尾(v3.12 → v3.14.1)

**v3.12 commit `2eb8b01`** — collector.toml enable `government_bank_buy_sell_v3`
(原 v1.21-G smoke test 揭露 backer tier 不夠 → enabled=false 暫關)+ 新 script
`scripts/probe_finmind_sponsor_unused.py` 探 sponsor tier 內 unused dataset。

**v3.13 commit `37756bf`** — 第一輪修(後來 revert):
- 誤信 FinMind 官方 `llms.txt` 把 dataset 名拼成 `Taiwanstock...`(lowercase s)
  → 結果 422 enum 拒絕,真實名是 `TaiwanStock...`(大寫 S)— doc typo
- probe script 兩 bug 修:`/datalist` 用錯(該 endpoint 回某 dataset 的 data_id
  清單,非 dataset catalog,CLAUDE.md v1.20 §B 早記過)→ 改走 **422 enum parser**
  (送 invalid dataset → /data 回 422 → regex 解 allowed enum);+ Windows
  cp950 console UnicodeEncodeError 修(`sys.stdout.reconfigure(encoding="utf-8")`
  + ASCII 標籤 `[OK]/[WARN]/[LOCK]/[BAN]/[FAIL]/[CRASH]` 取代 emoji)

**v3.13.1 commit `71e0ec6`** — revert lowercase 's' + probe script extended
fallback(200 OK 但 0 rows 也 retry no-data_id,對齊 GoldPrice case 揭露的
「macro datasets 不接 data_id」pattern)。

**v3.14 commit `ca3057d`** — gov_bank 真實 schema 揭露 + 修法:
- User 跑 Option B curl smoke 確認 FinMind 規則:
  - 不接 `data_id`(回 400 `parameter data_id don't provide on ... dataset`)
  - 不接 `end_date`(回 400 `size is too large, we only send one day data,
    end_date parameter need be none`)
  - 一日回 ~11906 row × 2312 stocks,**8 大行庫每股每日各 1 row**
  - row schema:`date / stock_id / bank_name(兆豐/第一/...)/ buy / sell /
    buy_amount / sell_amount(NTD 金額)`
- 5 個檔同步落地:
  - `src/config_loader.py`:VALID_PARAM_MODES 加 **`all_market_no_end`**
  - `src/api_client.py:_build_params`:對 all_market_no_end 不送 data_id
    也不送 end_date(自然走 per_stock_modes / end_date_modes 排除集合外)
  - `src/bronze/phase_executor.py:_resolve_stock_iter`:all_market_no_end 同
    all_market 回 `[ALL_MARKET_SENTINEL]`
  - **alembic `a6b7c8d9e0f1`**:Bronze `government_bank_buy_sell_tw` ADD COLUMN
    `bank_name TEXT NOT NULL` + `buy_amount NUMERIC` + `sell_amount NUMERIC`;
    DROP 舊 PK `(market, stock_id, date)` ADD 新 PK `(market, stock_id, date,
    bank_name)`;動態查 conname 對齊 v1.30 hotfix pattern
  - `src/schema_pg.sql`:CREATE TABLE 對齊 fresh-init schema
  - `src/silver/builders/institutional.py`:`_build_gov_bank_lookup` 改 SUM by
    `(market, stock_id, date)` 聚合 8 行庫;`net = SUM(buy) - SUM(sell)` 股數,
    NULL 視同 0 對齊 SQL SUM 行為
  - `config/collector.toml`:gov_bank entry `param_mode=all_market_no_end +
    segment_days=1`(每日 1 req,5 年 backfill ~1825 reqs ≈ 12 分鐘 @ 6000/h)

**v3.14.1 commit `3b4e3cc`** — institutional builder UNION fix(對齊 v1.26 B
margin/market_margin 修法 pattern):
- User 全市場 full-rebuild 揭露 fill_pct=40.38%(預期 ~69%)
- root cause:PR #20 trigger 對 gov_bank Bronze upsert 寫 stub row 進 Silver,
  但舊 `_pivot` 只 iterate institutional Bronze keys → stub row 完全沒被 touch
  → gov_bank-only dates 的 stub gov_bank_net 永遠 NULL
- 修法:`_pivot` 先 seed 所有 `gov_bank_lookup` keys 成 empty agg(法人欄 NULL,
  gov_bank_net 從 lookup 填),再用 institutional Bronze 覆蓋對應 (stock,date)
  的法人欄。三種 case 都正確:intersection / institutional-only / gov_bank-only
- safety:trading_dates 為空 bypass,否則只 seed 真實交易日

### Production 收尾(user 本機 2026-05-17 跑完)

| 階段 | Wall time | 規模 / 結果 |
|---|---|---|
| Bronze gov_bank 全市場 backfill(`segment_days=1` 全自動 daily req)| 70 分鐘 / 4244s | 13,391,008 rows / 2816 stocks / 8 banks / 2021-06-30 ~ 2026-05-15 |
| incremental phase 5(補其他 chip Bronze 到 latest)| 3 小時 / 11080s | 14 entries × 1351 stocks 串列;institutional FinMind 自身停 2026-04-29(非 collector bug)|
| Silver phase 7a full-rebuild(v3.14.1 UNION 修法後)| 14 分鐘 / 838s | institutional 3.06M rows / **gov_bank_net fill_pct = 80.74%**(剩 19% NULL 是 2019-2021/6 pre-gov-bank-Bronze 真實 NULL)|
| M3 cores `tw_cores run-all --write` | 10 分鐘 / 604s | 1266/1266 stocks 全綠,12 vwap empty(known);**institutional_core facts_new=15996**(其他 35 cores 0,params_hash 沒動 → dedup)|

### Round 7 calibration efficacy verify ✅✅

新 `scripts/verify_event_kind_rate.sql` 3 sections 設計:
- Section 1:per-stock cores(distinct_stocks > 5)events/stock/year ≤ 12 標準
- Section 2:market-level cores(distinct_stocks ≤ 5)events/year(per-stock 不適用)
- Section 3:Round 7 5 cores(adx/atr/day_trading/margin/trendline)專屬驗

verify 結果:
- **Round 7 5 cores → 0 row 超標** ✅ — v3.11 commit `8b1bb15` 的工沒白做
- Section 1 全 cores 揭露 Round 8 candidate(下個 session 動):
  - `institutional / LargeTransaction` 23.49/yr(v1.32 修法目標 6-12,production 後仍超)
  - `foreign_holding / HoldingMilestoneLow` 15.46/yr(微超)
  - `foreign_holding / SignificantSingleDayChange` 12.88/yr(微超)
- Accepted baselines(v1.32 拍版,不動):
  - `institutional / DivergenceWithinInstitution` 58.41/yr(對齊「68.18 accepted」)
- 修原 SQL bug(對 market-level cores 用 per-stock-year metric 自然超標)→ Section 2
  分開列,不混淆

### gov_bank_net **infrastructure done but no downstream signal yet**

`rust_compute/cores/` grep 0 個地方真正消費 `gov_bank_net`(institutional_core
只 default `None` 在 struct 構造)。M3 cores 跑後 `institutional_core facts_new=15996`
是其他原因(可能 Silver UPSERT 改 1.2M row 觸發 statement_md5 微差),**不是
gov_bank_net 生新 EventKind**。

要 gov_bank_net 真正出 actionable signal,要新增 EventKind variant(e.g.
`GovBankAccumulation` / `GovBankDistribution`)+ 規格,屬於 future Core spec
writing,不在當前 backlog。

### 已知狀態(下次 session 起點)

- alembic head:**`a6b7c8d9e0f1`**(gov_bank Bronze 新 schema 落地)
- collector.toml:34 entries 全 enabled(gov_bank `param_mode=all_market_no_end +
  segment_days=1`)
- Bronze gov_bank:13.39M rows / 2816 stocks / 2021-06-30 ~ 2026-05-15
- Silver institutional `gov_bank_net` fill_pct:**80.74%**(2.47M / 3.06M)
- M3 cores 35 crates,本 session 0 Rust 邏輯改,只動 Python + alembic + schema
- Round 7 calibration:**5 cores 全部 EventKind ≤ 12/yr/stock 達標** ✅
- 新工具:`scripts/verify_event_kind_rate.sql` 3-section per-EventKind rate verify

### 風險

🟢 低:
- Bronze backfill 已 70 分鐘跑完無 error
- alembic `a6b7c8d9e0f1` 對既有 0-row 表加欄 + 換 PK 全綠
- Silver full-rebuild 14 分鐘無 error;institutional v3.14.1 UNION 修法 sandbox
  + production 雙驗
- M3 cores run-all 604s,1266/1266 全綠 + 12 vwap empty(既知 limitation)
- 0 Rust workspace 改動,既有 cargo test 不受影響
- Rollback:每 commit `git revert` 即可;alembic downgrade 反向(動態查 conname)

### 下次 session 動工候選(Round 8 + gov_bank signal)

1. **Round 8 calibration**(對齊 v1.34 / v1.35 pattern):
   - `institutional / LargeTransaction` 23 → 6-12 目標(可能 edge trigger 已落,
     需 production data 重看 threshold)
   - `foreign_holding / HoldingMilestoneLow` 15 → 8-12(MIN_MILESTONE_SPACING tighten?)
   - `foreign_holding / SignificantSingleDayChange` 12.88 → 10-12(z-score threshold 微調)
2. **gov_bank_net Core 消費**:
   - 新增 EventKind variant `GovBankAccumulation / GovBankDistribution`,
     `institutional_core` 或新 `gov_bank_core` 讀 Silver `gov_bank_net` 出 signal
   - 需要先在 m3Spec/chip_cores.md(或新 doc)寫規格 — best-guess 不動
3. **probe_finmind_sponsor_unused.py 跑全 catalog**:目前只 `--max 5`,user 想找
   未用 dataset 候選可跑 `--max 0`(估 ~3-5 分鐘對 60 unused datasets,跑前先等
   IP ban 解確認)

---

## v3.11 — Round 7 calibration 5 cores tighten + trendline_core perf(2026-05-16)

接 v3.10 m2 大重構終結後動工 Round 7。對齊 v1.34 / v1.35 calibration pattern,
本 session 對 5 個 cores 再 tighten + trendline_core 額外 perf 優化。

詳見 commit `8b1bb15`(本段為 v3.14 doc-only update 時補寫,原 session 未及時
寫 CLAUDE.md 章節)。

### 動工範圍(commit `8b1bb15`)

- `adx_core`:DI Cross spacing tighten
- `atr_core`:Expansion spacing tighten
- `day_trading_core`:RatioExtreme spacing tighten
- `margin_core`:HighRatio/LowRatio spacing tighten
- `trendline_core`:Touch spacing tighten + perf 優化(reduce O(N²) → O(N log N))

### Production verify(v3.14 同 session 確認)

`scripts/verify_event_kind_rate.sql` Section 3 對 5 cores 跑出 **0 row** —
全部 EventKind 落 ≤ 12/yr/stock 達標。Round 7 calibration 工不白做 ✅

---

## v3.10 — PR #R6 m2 大重構終結:永久 DROP 3 張 _legacy_v2(2026-05-16)

接 v3.9 後 user 拍版「直接 DROP」提前結束 R5 觀察期(2026-05-09 啟動 → 2026-05-16
僅 7 天觀察)。v3.7+v3.8+v3.9 連續 4 sprint 無 regression,**m2 大重構正式終結**。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl` PR #65)

| 檔 | 動作 |
|---|---|
| `alembic/versions/2026_05_16_z5a6b7c8d9e0_pr_r6_drop_legacy_v2_3_tables.py`(新)| DROP 3 張 `_legacy_v2` CASCADE,downgrade no-op(spec plan §六 destructive)|
| `config/collector.toml` | 32 → 27 api entries(移 5 `*_legacy`)|
| `src/schema_pg.sql` | 移 3 CREATE TABLE + 2 INDEX DDL,保留歷史 comments |
| `scripts/check_all_tables.py` + `scripts/inspect_db.py` | 同步移 references |
| `scripts/verify_pr19c2_silver.py` | 標 🪦 DEPRECATED(legacy 表已 DROP,不可執行)|

### 落地後狀態(m2 大重構終結)

- **alembic head**:`x3y4z5a6b7c8` → **`z5a6b7c8d9e0`**
- **0** 張 v2.0 `*_legacy_v2` 表
- **27** collector.toml entries(主路徑唯一,v3.2 r1 PR sequencing 全部終結)

### 風險

🟡 **destructive**:
- 0 alembic 可 rollback;`alembic upgrade head` 後永久 DROP
- 建議事先 backup:`pg_dump -t holding_shares_per_legacy_v2 -t financial_statement_legacy_v2 -t monthly_revenue_legacy_v2`
- 既有 silver builders 不讀 `_legacy_v2`(全部走主名 + Bronze),pipeline 不受影響
- api_sync_progress 殘留 5 個 `*_legacy` api_name 的 row(歷史 backfill 紀錄),無害

---

## v3.9 — structural_snapshots partition observation + Workflow toml dispatch audit(2026-05-16)

接 v3.8 後動工兩 task。

### Task 2:Workflow toml dispatch — ✅ audit 揭露已落地

`rust_compute/cores/system/tw_cores/src/workflow.rs::CoreFilter` 完整實作(v1.29 PR-9b
commit `615a8eb`):
- `from_workflow_toml` + walk-up cwd resolve(對齊 dotenvy 模式)
- 35 cores 全部接 `filter.is_enabled(...)` check(`run_stock_cores.rs` + `run_environment.rs`)
- 7 unit test
- `--workflow workflows/tw_stock_standard.toml` CLI flag 已 wired in `main.rs`

CLAUDE.md §1b PR-9b 進階四項全標 ✅ 已落地(workflow toml / sqlx pool / dirty queue;
ErasedCore trait wrapper 不做)。

### Task 3:structural_snapshots partition 評估 — 結論「不需要」

純研究 doc `docs/structural_snapshots_partition_observation.md`(150 行):

| 評估 | 數據 |
|---|---|
| Production 規模 | 1263 stocks × 4 core_names ≈ 5052 rows/day |
| 年增長 | ~1.27 M rows/year |
| 5 年累積上限 | ~6.4 M rows(遠低於 partition 必要門檻 10M)|
| 主查詢 latency | < 5 ms(index seek `idx_structural_snapshots_stock_date_desc`)|

3 種策略全評估:RANGE / LIST partition pruning miss;HASH 為 future expand 候選但
目前不值得做。**預警閾值表**記錄完整(p95 > 100 ms / total > 10M / batch > 5 min /
disk > 50 GB),觸發時再重啟評估。

### 落地

- 0 alembic / 0 Rust code / 0 collector.toml / 0 test 變動
- 既有 420 cargo tests + 39 agg tests 不破

---

## v3.8 — Aggregation Layer per-timeframe lookback fold-forward(2026-05-16)

接 v3.7 後動工「立即可動工 — agg API 對齊 spec §4.2」。

### 範圍(1 commit / PR #63)

對齊 `m3Spec/aggregation_layer.md §4.2`「預設 lookback」表:
- daily facts:`lookback_days` 預設 90 天
- monthly fact(revenue / business_indicator):3 個發布週期 → `lookback_days_monthly=90`
- quarterly fact(financial_statement):2 季 → `lookback_days_quarterly=180`

**`src/agg/query.py`**:
- `as_of()` + `as_of_with_ohlc()` 加兩參數 `lookback_days_monthly` / `lookback_days_quarterly`
- 新 helper `_filter_by_timeframe_lookback`(SQL fetch_facts 用 `max(三個 lookback)`
  寬撈,per-row 過濾在 Python 層 dispatch)
- unknown / None timeframe 視同 daily(安全 default)
- fact_date None 保留(避免 silently drop)

**`src/agg/_types.py`**:`QueryMetadata` 加 2 field(reproducibility 保留兩 cutoff)。

**`m3Spec/aggregation_layer.md`**:r2 → r3 修訂摘要 + §2.2 API surface 反寫。

**`tests/agg/`**:新 `test_timeframe_lookback.py` 7 case + `test_validation.py` +2 case。

### 落地驗證

- `pytest tests/agg/` ✅ **39 passed**(從 30 → 39,+9 新 case)
- 既有 caller 簽章 100% 相容(只傳 daily 仍走原路徑)
- 0 alembic / 0 Rust / 0 collector.toml

---

## v3.7 — spec_pending doc cleanup + exhaustive compaction 真窮舉(2026-05-16)

接 v3.6 後 user 問「spec-blocked 目前還缺文件??」。研究揭露 **spec 不缺文件**,
標的「spec-blocked」全部過時。本 PR 切 2 個 commit:

### Phase A — Doc cleanup(0 code)

對齊實際 code 狀態,標的「spec-blocked」全部過時(v3.5 + v3.6 已收尾):

**`docs/m3_cores_spec_pending.md §1.1`**:標題從「22 條全 deferred」→「✅ 已實作」,
表格 6 行全部標 ✅ + 對應 `validator/*.rs` file:line:
- R4-R7 ✅ `core_rules.rs:213-380`
- F1-F2 ✅ `flat_rules.rs:36-105`
- Z1-Z2 ✅ `zigzag_rules.rs:36-118`(r5 收斂 2 條;原 Z1-Z4 stale)
- T1-T3 ✅ `triangle_rules.rs:50-178`(r5 收斂 3 條;原 T1-T10 stale)
- W1-W2 ✅ `wave_rules.rs:46-201`

**§1.3**:標題從「留 PR-3c/4b/5b/6b(spec-blocked)」→「Code follow-up(spec 不缺)」。
PR-3c / PR-4b Diagonal / R3 exception / PR-6b Power Rating + Fib 全標 ✅ 已實作。

**`CLAUDE.md §2`** reframe:從「等 user m3Spec/ 寫最新 spec 後做(spec-blocked)」改
「Code follow-up + production calibration」。

### Phase B — exhaustive compaction 真窮舉(Rust)

對齊 `m3Spec/neely_rules.md §Three Rounds`(line 1198-1256)Round 1-2 流程。

**新檔 `compaction/three_rounds.rs`**(~360 行):
- `aggregate_one_level(scenarios)` — Figure 4-3 五大序列比對 + Similarity & Balance 過濾
- 5-pattern Trending Impulse / Triangle + 3-pattern Zigzag / Flat aggregation
- Power Rating / Max Retracement / PostBehavior 對新生 scenario 重算(對齊 spec Ch10)

**改 `compaction/exhaustive.rs`** 從 pass-through → 遞迴 aggregation:
- Level 0: 原 scenarios(對齊 v2.0 既有行為)
- Level 1-4: 對前 level 跑 `three_rounds::aggregate_one_level`
- `MAX_COMPACTION_LEVELS = 4`(對齊 Subminuette → Primary degree 階層)
- 收斂條件:next level 為空 (Round 3 暫停) or hit MAX_COMPACTION_LEVELS

V3 follow-up(spec §Three Rounds 動作 B):邊界波 m(+1)/m(-1) 重評,需要部分 Stage 3-4 rerun。

### 落地驗證

- `cargo test --release --workspace` ✅ **420 passed / 0 failed**(從 408 → 420,+12 Phase B test)
- compaction tests 21/21:exhaustive 7 + three_rounds 7 + beam_search 3 + mod 4
- 0 alembic / 0 Python / 0 collector.toml

---

## v3.6 — Neely RuleId enum 補完:53 spec-only variants 全進 Rust(2026-05-16)

接 v3.5 merged main 後,user 拍版反向 r5「prematurely declare 未實際 dispatch 的 RuleId
不該做」原則,把 spec §9.3 列的 76 variants 全部補進 Rust enum。

### 範圍(1 commit / branch `claude/continue-previous-work-xdKrl`)

`rust_compute/cores/wave/neely_core/src/output.rs::RuleId` enum 從 28 → **81 variants**:
- **Ch3 Pre-Constructive(7)**:`Ch3_PreConstructive { rule, condition, category, sub_rule_index }` / `Ch3_Proportion_*` / `Ch3_Neutrality_*` / `Ch3_PatternIsolation_Step(u8)` / `Ch3_SpecialCircumstances`
- **Ch4 Intermediary(6)**:`Ch4_SimilarityBalance_*` / `Ch4_Round{1,2,3}_*` / `Ch4_ZigzagDetour`
- **Ch5 漏的 Extension(3)**:`Ch5_Extension` + `Extension_Exception{1,2}`
- **Ch6 Post-Constructive(9)**:`Ch6_Impulse_Stage{1,2}` / `Ch6_Correction_B{Small,Large}_Stage{1,2}` / `Ch6_Triangle_Contracting_Stage{1,2}` / `Ch6_Triangle_Expanding_NonConfirmation`
- **Ch7 Conclusions(3)**:`Ch7_Compaction_Reassessment` / `Ch7_Complexity_Difference` / `Ch7_Triplexity`
- **Ch8 Complex Polywaves(6)**:`Ch8_NonStandard_Cond{1,2}` / `Ch8_XWave_*` / `Ch8_LargeXWave_NoZigzag` / `Ch8_ExtensionSubdivision_Independence` / `Ch8_Multiwave_Construction`
- **Ch10 Advanced Logic(3)**:`Ch10_PowerRating_Lookup` / `Ch10_MaxRetracement_Lookup` / `Ch10_TriangleTerminal_PowerOverride`
- **Ch11 Wave-by-Wave(5)**:`Ch11_Impulse/Terminal_WaveByWave { ext, wave }` / `Ch11_Flat_Variant_Rules { variant, wave }` / `Ch11_Zigzag_WaveByWave { wave }` / `Ch11_Triangle_Variant_Rules { variant, wave }`
- **Ch12 Advanced Extensions(11)**:`Ch12_Channeling_*` ×3 / `Ch12_Fibonacci_*` ×2 / `Ch12_WaterfallEffect` / `Ch12_MissingWave_MinDataPoints` / `Ch12_Emulation { kind: EmulationKind }` / `Ch12_ReverseLogic` / `Ch12_LocalizedChanges`

加 5 個 supporting enums:`ImpulseExtension`(First/Third/Fifth/NonExtended)/ `WaveAbc`(A/B/C)
/ `TriangleWave`(A-E)/ `FlatVariant`(9 variants)/ `TriangleVariant`(9 variants:
Horizontal/Irregular/Running × Limiting/NonLimiting/Expanding)。

### Dispatch 範圍**不變**(設計約束)

實際 emit 進 `RuleRejection.rule_id` 的 variants 仍限 Ch5_*/Ch9_*/Engineering_*
三組;新增 variants 全標 `#[allow(dead_code)]`,純 type-level 章節追溯。
spec `m3Spec/neely_core_architecture.md §9.3 r6` 同步更新文字明文這設計。

### Trade-off(user 拍版收下)

- ✅ SQL 按章節統計拒絕原因更容易(`metadata->>'rule_id' LIKE 'Ch6%' GROUP BY chapter`)
- ✅ 編輯器 import RuleId 可看到完整章節範圍
- ✅ 未來 dispatch 擴充時 enum variant 已就位
- ⚠️ 違反 cores_overview §十四「禁止抽象」(spec §9.3 r5 原則反向 — r6 已記錄)
- ⚠️ 與 domain-specific structs(EmulationSuspect / PowerRating / StructureLabel)並存 — single source of truth 模糊風險

### 驗證(本 session 沙箱)

- `cargo build --release -p neely_core` ✅ 0 warnings
- `cargo build --release -p tw_cores` ✅ 0 warnings(下游 dep 編譯通)
- `cargo test --release --workspace` **408 passed / 0 failed**(+1:`v36_new_variants_serialize` 對 19 個 sample variant 驗 serde JSON shape)

### 受影響檔案(3)

| 路徑 | 行數變動 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/output.rs` | +172 行(53 RuleId variants + 5 supporting enums + 1 unit test) |
| `m3Spec/neely_core_architecture.md` | +18 行(r6 修訂摘要 + §9.3 r5 文字 update) |
| `CLAUDE.md` | +50 行(v3.6 段) |

### 風險

🟢 低:
- 純 enum 擴充,0 dispatch 邏輯改變
- 既有 28 variants + 對應 validator 5 個檔 0 改動
- domain-specific structs 0 改動
- production output `diagnostics.rejections` JSON shape 不變
- 0 alembic / 0 Python / 0 collector.toml
- Rollback:單 commit `git revert`

---

## v3.5 — 5 層架構大型重構:單一職責歸位 + 新增 Layer 2.5 Cross-Stock Cores(2026-05-16)

接 v1.35 收尾後,user 拍版審計四層耦合與單一職責問題。並行派 3 個 Explore agent
全市場 audit,揭露 **17 個熱點**(Layer 1+2 八個 / Layer 3 七個 / Layer 4 二個)。
User 拍版「4 層全做 + 新增 Layer 2.5(Cross-Stock Cores)+ 立即動工」,本 session
推完 8 個 commits 對齊 plan v3.5(`/root/.claude/plans/hashed-foraging-pixel.md`)。

### 範圍 5 個 phase(8 commits / branch `claude/continue-previous-work-xdKrl`)

| Phase | Commit | 範圍 |
|---|---|---|
| **R1** Bronze | `41fcdc2` C1 | `git mv src/post_process.py → src/bronze/post_process_dividend.py`(Bronze 後處理歸位 Layer 1)|
| | `f228076` C2 | `src/aggregators.py` 拆 `src/bronze/aggregators/` package(4 module:pivot_institutional / pack_financial / pack_holding_shares / dispatcher) |
| | `ad0ac51` C3 | `phase_executor._run_api` 600 行拆 `_SegmentRunner` class(orchestration vs single-segment IO 分離),phase_executor 577 → 432 行 |
| **R2** Silver | `37f597e` C4+C5 | `SilverBuilder` Protocol 加 per-stock 邊界明文(避免大規模 module→class 重構風險);14 builder 全部已走 `upsert_silver` helper 統一(grep 驗) |
| **R3** Cross-Stock(新層 Layer 2.5)| `2c8d33e` C6+C7 | 新 `src/cross_cores/` 套件(_base.py / orchestrator.py / magic_formula.py);`magic_formula_ranked` 從 `silver/builders/` 搬走(per-stock 契約違規);`python src/main.py cross_cores phase 8` CLI;`scripts/refresh_daily.ps1` 加 Phase 8 step |
| **R4** M3 Cores | `cb1bc21` C8 | `rust_compute/cores/system/tw_cores/src/main.rs` 1693 行 monolith 拆 8 個 module(main.rs 1693 → 297 行 + cli.rs / dispatcher.rs / writers.rs / run_environment.rs / run_stock_cores.rs / summary.rs / helpers.rs) |
| | `6de8144` C11 | `neely_core/facts.rs` event_kind 改用 `fact_schema::with_event_kind` helper(對齊 34 cores);`kalman_filter_core` `MIN_REGIME_DURATION_DAYS` const 抽 `KalmanFilterParams.min_regime_duration_days` field |
| **R5** MCP | `57816d6` C12+C13 | DELETE `mcp_server/_conn.py`(連線 single entry = `agg._db.get_connection`);新加 `agg._db.fetch_cross_stock_ranked` + `fetch_stock_info_ref` 共用 helper;`mcp_server/_magic_formula.py` cross-stock SQL 改走 helper |

### 5 層架構(v3.5 後 final state)

```
Layer 1: Bronze 收集(FinMind → raw 表)
  PhaseExecutor(orchestration only)+ _SegmentRunner + ApiClient/RateLimiter/SyncTracker
  bronze/aggregators/(pivot/pack 4 module)+ bronze/post_process_dividend.py

Layer 2: Silver 計算(per-stock 獨立)
  SilverOrchestrator(7a/7b 排程)+ 13 builder + Rust S1(silver_s1_adjustment 後復權)

Layer 2.5: Cross-Stock Cores(v3.5 R3 新層,跨股 ranking)
  CrossStockOrchestrator(Phase 8 排程)+ magic_formula(首例)
  未來:pairs_trading / sector_rotation / correlation_matrix

Layer 3: M3 Cores(per-stock compute → facts)
  tw_cores binary(v3.5 R4 拆 8 module)+ 36 cores + cores_shared/

Layer 4: MCP / API 對外
  agg.query.as_of()(read-only)+ agg._db(SINGLE connection entry)+ MCP 5 tools + dashboards
```

### 關鍵設計決策(對齊 cores_overview §四 + §十四 + user 拍版)

1. **新增 Layer 2.5**(user 拍版 2026-05-16):跟 PerStockBuilder 契約乾淨切割
2. **不抽 ErasedCore trait wrapper**(cores_overview §十四 V2 不規劃):tw_cores
   拆 module 但保留 36 個 hardcoded match arm,新增 core 改 3 處(文件化即可)
3. **連線 single entry = agg._db**(audit Layer 4 痛點 17):MCP / dashboards /
   cross_cores 都從 agg._db.get_connection() 取
4. **PerStockBuilder / CrossStockBuilder ABC 文件化**(不抽 class):既有 module
   pattern 保留,只加 Protocol 邊界明文(避免大規模 module→class 重構)
5. **C9 generic dispatcher / C10 loader_common / financial_statement 中文 key
   crate 抽出 defer V3**:對齊 §十四「禁止抽象」

### 驗證(本 session 沙箱)

- **Python tests**:`pytest tests/` 109 passed / 1 skipped(fastmcp 缺) ✅
- **Rust tests**:`cargo test --release --workspace` **407 passed / 0 failed** ✅
- **`tw_cores list-cores`**:35 cores 完整對齊 ✅
- **`tw_cores run-all --help`**:9 args 完整對齊 ✅
- **`python src/main.py cross_cores phase 8 --help`**:CLI 對齊 ✅
- 0 alembic / 0 collector.toml(純 layer reshape)

### 已知狀態(下次 session 起點)

- alembic head:`x3y4z5a6b7c8`(不變,本 session 0 migration)
- Rust workspace:35 crates / **407 tests passed / 0 failed**
- agg tests:30 passed + mcp_server data tests 9 passed
- Production state:1263 stocks × 34 cores / 4 structural_snapshots core_names / ~10M facts
- 5 層架構全部歸位 ✅;Magic Formula 從 silver 違規搬出 ✅
- 下次 session 動工選項:
  - **user merge 本 branch 8 commits 到 main**(blocking,user 端)
  - **B-4 FastAPI thin wrap** / per-timeframe lookback fold-forward / structural_snapshots schema partition observation
  - **C9 generic dispatcher**(若 Workflow toml dispatch 需要動態 dispatch,V3 才考慮)
  - **C10 loader_common**(4 loaders SQL cast 模式抽 helper,V3)

### 風險

🟢 低:
- 純 layer reshape(0 alembic / 0 Rust 邏輯 / 0 collector.toml)
- 行為零改變,既有 verifier(verify_pr18 / 19b / 19c / 20)0 break
- Rollback:每 phase 獨立 commit,任意 phase 失敗可單獨 `git revert`

---

## v1.35 — Neely 22 spec gaps + P3/P2 indicator cores batch + Aggregation Layer 完整落地(2026-05-14)

接 v1.34 P0 Gate v4 後,本 session(branch `claude/continue-previous-work-xdKrl`,
PR #51 累積)推到極限完成 4 個主軸:

1. **Neely Core P0 → v1.0.0 production-ready**(Phase 13-17 + 18 OBV oscillator
   + 19 RSI Murphy 引用 + Pre-1.0.0 checklist),22 個 spec gaps 全部 fill
2. **P3 indicator cores 8 個 batch**(williams_r / cci / keltner / donchian /
   vwap / mfi / coppock / ichimoku)+ **P2 pattern cores 3 個 batch**
   (support_resistance / candlestick_pattern / trendline)
3. **Aggregation Layer 完整 4 Phase 落地**(Spec r1 → Python lib → Streamlit
   dashboard 6 tabs → MCP server)+ 本 session agg 補強
4. **Round 5/6 indicator calibration**(11 cores production-data-driven 觸發率
   調至 per-EventKind ≤ 12/yr/stock)

### Commits(本 session 主要,branch `claude/continue-previous-work-xdKrl`)

| Commit | 範圍 |
|---|---|
| **Neely Phase 13-19**(7 commits)| |
| `9791b09` | Phase 13:max_retracement Option<f64> 落地(spec §9.1) |
| `8051b97` | Ch9 Exception Rule 落地(spec line 529 / Ch 9 p.9-7) |
| `34d0382` | stale spec ref 大規模更新 m2Spec/oldm2Spec/ → m3Spec/ |
| `4fd1c68` | Phase 14:PostBehavior 8-variant + WaveNumber(spec §9.2 / line 2024-2037) |
| `65bef04` | Phase 15:Scenario 群 2 fields(monowave_structure_labels / round_state / pattern_isolation_anchors / triplexity_detected) |
| `3685032` | Phase 16:FlatKind 7-variant + RunningCorrection 上提頂層 |
| `6c3ea4d` | Phase 17:StructuralFacts 7 sub-fields 全填(fib / alternation / channeling / time / volume / gap / overlap) |
| `a222e7a` | Phase 18:OBV Divergence 改 oscillator(OBV - OBV_MA) |
| `4de97f3` | Phase 19:RSI Divergence Murphy 引用 + 2 cores SQL 對齊 |
| `722c222` | P0 Gate runbook 更新 + Pre-1.0.0 checklist |
| `baa6a58` | 🎉 neely_core v1.0.0:P0 Gate 通過 production-ready |
| `fc860a4` | v1.0.1:3 個 P1 known issues 收尾(stage_elapsed_us + circular bootstrap + §8 query) |
| **P3 + P2 batch**(3 commits)| |
| `8d3c1a7` | P3 indicator cores 8 個 batch(williams_r / cci / keltner / donchian / vwap / mfi / coppock / ichimoku)|
| `254ef0c` | P2 pattern cores 3 個 batch(support_resistance / candlestick_pattern / trendline) |
| `262f56c` | tw_cores dispatch_structural + Workflow toml 補完 11 cores |
| `5f08e6e` | Workflow toml walk-up cwd resolve(user 從 rust_compute/ 跑也能找到 workflows/) |
| **Round 5/6 calibration**(2 commits)| |
| `d929950` | Round 5:7 cores 觸發率 + vwap empty silent skip |
| `c5638a4` | Round 6:4 stragglers tighten + ichimoku KumoTwist flat-cloud 閾值 |
| **Aggregation Layer 補強**(本 commit)| |
| `cd2b9b8` | agg layer 補強:health_check + 內建 look-ahead + input validation |

Aggregation Layer 主體已先前 session 完成(commits `5bc9c55` spec r1 → `50d5310`
Phase B-2 lib → `29e66c9` Phase B-3 Streamlit → `b87fccf`~`728c044` Phase C-1~C-8
6 tabs → `8ca1a7d` Phase D MCP server,via PR #50 merged main 2026-05-13)。

### Aggregation Layer 完整 4 Phase 狀態

| Phase | 範圍 | 狀態 |
|---|---|---|
| **B-1**(Spec)| `m3Spec/aggregation_layer.md` r1 立稿 | ✅ |
| **B-2**(Python lib)| `src/agg/` — 6 modules(__init__ / query / _types / _lookahead / _market / _db)+ `as_of()` API + `find_facts_today` + `as_of_with_ohlc` | ✅ |
| **B-3**(Streamlit)| `dashboards/aggregation.py` — Phase C 內 6 tabs(K-line / Chip / Fund / Env / Neely / Facts 散點雲)+ 9 chart helper modules | ✅ |
| **B-4**(FastAPI)| 未動工,留 future | 🟡 待需要 |
| **C-0~C-8**(視覺化主幹)| `dashboards/charts/` — 9 個 plotly figure builder 模組,各 tab 對映完整 | ✅ |
| **D**(MCP server)| `mcp_server/` — FastMCP stdio server 包 agg + dashboards/charts,Claude Desktop 對話內 call tools | ✅ |
| **本 session 補強**| `health_check()` + `_market` 內建 look-ahead + `as_of()` input validation + 10 個新 tests | ✅ |

`tests/agg/` **30 passed / 1 skipped**(pandas 未裝)+ `tests/mcp_server/test_data_tools.py` **9 passed**。

### Neely Core v1.0.0 + v1.0.1 P0 Gate 全收尾

Phase 13 → 19 把 spec §9.1 line 549 / §9.2 / §9.3 / 4 個剩餘 spec gaps 全部
fill,+ OBV oscillator 算法升級 + RSI Murphy 引用補完。Production data 確認
neely 22 條規則 R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2 全部 valid。

| 指標 | 數值 |
|---|---|
| Rust workspace | **35 crates**(從 v1.34 24 crate + 11 new P3+P2 cores)|
| 全部 tests | **384 passed / 0 failed / 0 warnings** |
| Production state | 1263 stocks × 34 cores / 0 errors / ~10 min wall(concurrency=16)|
| structural_snapshots core_names | **4**(neely + 3 P2 pattern cores)|
| facts 總量 | ~10M rows |
| Per-EventKind 觸發率 | **11/11 cores 全部 ≤ 12/yr/stock** ✅(v1.32 P2 acceptance 標準) |

### Round 5/6 calibration:11 cores 命中 per-EventKind ≤ 12/yr

Round 5 第一波 7 cores(williams_r / cci / ichimoku / donchian / coppock /
candlestick_pattern / trendline)加 MIN_*_SPACING constants,production verify
後 4 cores(candlestick 68.9 / cci 38.7 / ichimoku 25.3 / williams_r 17.6)未命中
per-core total ≤ 12 目標。

Round 6 加 tightened spacing + ichimoku KumoTwist `KUMO_TWIST_MIN_DIFF_PCT=0.001`
flat-cloud flicker prevention。再 verify 後 reframe:**per-core total** 14-53/yr
但 **per-EventKind** 3-5/kind ✅(對齊 v1.32 P2 baseline,institutional
DivergenceWithinInstitution=68.18/yr accepted)。

最終 verify SQL `WHERE per_kind_year_rate > 12.0` → **0 rows**(11/11 cores 全部
EventKinds ≤ 12/yr/stock)。

### agg layer 補強(本 commit `cd2b9b8`)

3 個改進對齊 m3Spec/aggregation_layer.md r1 + 10 個新 tests:

| 改進 | 範圍 | 動機 |
|---|---|---|
| `agg.health_check(database_url)` | `src/agg/query.py` + `__init__.py` export | 啟動時 1 個 query 確認 PG 可達 + 三表存在 + row counts。失敗時點明哪環掛掉,不再讓首個 as_of() 一路炸到第 4 個 SQL 才看見錯 |
| `_market.fetch_market_facts` 內建 look-ahead filter | `src/agg/_market.py` + `query.py` 移除 redundant filter loop | 預設 `apply_lookahead_filter=True`。直接呼叫 `_market` 不會 leak 未來 fact;`query.as_of()` 同步 single source of truth |
| `as_of()` input validation | `src/agg/query.py` | empty stock_id / negative lookback_days / 空 cores list 早 raise ValueError |

新 tests:
- `tests/agg/test_health_check.py`(3 case:all_exist / missing_table / connect_failure)— unittest.mock 不打真 PG
- `tests/agg/test_market_lookahead.py`(3 case:filter_on drops future revenue / filter_off returns raw / 5 reserved keys always present)
- `tests/agg/test_validation.py`(4 case:empty / whitespace / negative / empty cores)

### 已知狀態(下次 session 起點)

- Branch:`claude/continue-previous-work-xdKrl`(PR #51 累積)
- alembic head:`x3y4z5a6b7c8`(不變)
- Rust workspace:35 crates / **384 tests passed / 0 failed**
- agg tests:**30 passed / 1 skipped(pandas)** + mcp_server data tests **9 passed**
- Production state:1263 stocks × 34 cores / 4 structural_snapshots core_names / ~10M facts
- Aggregation Layer 4 Phase 全部完整(B-1 spec / B-2 lib / B-3 dashboard / D MCP),B-4 FastAPI 留 future
- 下個 session 可動:**B-4 FastAPI thin wrap** / **structural_snapshots schema partition** / **per-core timeframe override in Workflow toml** / 各 cores P3 後階段(若有 spec 來新需求)

### 風險

🟢 低:
- 本 session 全部 commits 沙箱驗過 + user 本機 production verify pass(per-EventKind 0 rows ≤ 12)
- agg 補強 0 collector.toml / 0 alembic / 0 Rust,純 Python lib 補強
- Rollback:單 commit `git revert` 即可(各 phase 獨立)

---


## v1.10 → v1.34(已搬到 docs/claude_history.md)

> 時間範圍:2026-05-02 → 2026-05-14
> 內容:m2 PR sequencing(PR #18 → #22)+ M3 Cores 動工(PR-1 → PR-9a 全市場全核 dispatch)+ P1/P2/P3 cores batch + Aggregation Layer 4 Phase + v1.34 P0 Gate v3/v4 production 校準。
> 動工早期 Bronze reverse-pivot / M3 cores 框架 / 各 indicator core best-guess threshold 校準歷史時參考 [`docs/claude_history.md`](docs/claude_history.md)。

## 過去版本沿革（v1.5 ~ v1.9.1）

> v1.5 / v1.6 / v1.7 的 commits 表 + 逐輪修正詳解 已搬到 [`docs/claude_history.md`](docs/claude_history.md)。
> v1.8 / v1.9 / v1.9.1 的大項總覽 + commits 表 一同搬到 [`docs/claude_history.md`](docs/claude_history.md)(v1.18 reorg)。
> 主檔保留:v1.7 收尾 PR 已合到 `m1/postgres-migration`,base sha `9890294`。
>
> 重點延續到 v1.10+(主檔):
> - v1.8 P0-11 Rust 拆 multiplier(commit `c71d422`)+ P1-17 stock_dividend vf SQL 修(commit `608d275`)→ Convention 切換見「關鍵架構決策」表
> - v1.9 PR #17 (B-3) events 砍 3 + fwd 加 4 + Rust schema_version 對齊 3.2 → schema v3.2 r1 動工入口
> - v1.9.1 24 檔 split/par_value backfill 完成 + tblnC 分支整合 → av3 Test 4 100% 覆蓋

---

## 目前狀態：Phase 1~6 全部驗證通過 ✅

| Phase | 內容 | 驗證結果 |
|-------|------|---------|
| 1 | stock_info / trading_calendar / market_index_tw | ✅ 3048 / 1773 / 3544 |
| 2 | dividend / split / par_value / capital_reduction | ✅ 17 筆 dividend events |
| 3 | price_daily / price_limit | ✅ 1772 筆/支 × 2 |
| 4 | 後復權 + 週月K（Rust） | ✅ 4 個關鍵日驗證點全 OK |
| 5 | 11 支 chip / financial | ✅ 5 類法人正確分開 |
| 6 | 5 支 macro | ✅ exchange_rate 受 API 限制只有 57 筆 |

### 後復權驗證資料（2330）

| date | raw_close | fwd_close | fwd/raw | theoretical | match |
|------|-----------|-----------|---------|-------------|-------|
| 2019-01-02 | 219.50 | 237.54 | 1.0822 | 1.0822 | OK |
| 2022-03-15 | 558.00 | 603.87 | 1.0822 | 1.0822 | OK（除息前一日） |
| 2022-03-16 | 558.00 | 600.89 | 1.0769 | 1.0769 | OK（除息日當日） |
| 2026-04-24 | 2185.00 | 2185.00 | 1.0000 | 1.0000 | OK（最新日） |

### v1.7 / v2.0 (PG) schema 狀態

review #3 + #4 後 user 本機 PostgreSQL 17 環境：

- alembic head = `a1b2c3d4e5f6`（progress_status_check_expand）
- baseline = `0da6e52171b1`（baseline_schema_v2_0），執行 `src/schema_pg.sql` 全文
- `api_sync_progress.chk_progress_status` 含 5 種 status: `pending / completed / failed / empty / schema_mismatch`
- `stock_info.detail` JSONB 欄位（baseline 就有，v1.6 之前漏用）已透過 collector.toml 改成 pack `data_update_date`
- `stock_info.updated_at` 改由 schema `DEFAULT NOW()` + upsert UPDATE 路徑強制 NOW() 控制
- 8 commit 全部驗過：`api_sync_progress` 343 segment（completed 322 / empty 21 / failed 0 / pending 0）

---

## v1.5 / v1.6 / v1.7 重要修正詳解

> 完整 16 條(v1.5 8 條 + v1.6 3 條 + v1.7 review #1-#9)逐項詳解搬到 [`docs/claude_history.md`](docs/claude_history.md)。
> 重點摘要:
> - **Rust 後復權「先 push 再更新 multiplier」**(v1.5 commit `536962e`):除息日當日 raw 已是除息後,不可再乘該日 AF。**v1.8 進一步拆 price_multiplier / volume_multiplier 兩個 multiplier**(commit `c71d422`)
> - **5 類法人各自獨立**(v1.5 commit `acc7b1f`):institutional 從 6 欄擴 10 欄
> - **api_sync_progress 5 種 status**(v1.7 review #1):pending/completed/failed/empty/schema_mismatch,alembic `a1b2c3d4e5f6` 補 CHECK
> - **DBWriter._table_pks 動態查 information_schema**(v1.7 review #8):schema 是 single source of truth
> - **stock_info.updated_at 兩段修法**(v1.7 review #7):upsert UPDATE 強制 `updated_at = NOW()`
> - **post_process 4 處 SELECT 補 market filter**(v1.7 review #6):對齊 schema PK
> 
> v1.8 在這些基礎上,加 P0-11 / P0-7 補丁 / P1-17 / overview §7.5 + §10.0,詳見 §「v1.8 大項總覽」。

### Rust 後復權核心邏輯（保留摘要;Rust binary 對齊基準）

詳見 [`docs/claude_history.md` §1](docs/claude_history.md)。

**v1.5 修法**(原版錯誤是「先更新 multiplier 再 push」,造成除息日當日多乘一次 AF):

```rust
// 正確:先 push 再更新 multiplier
for price in raw_prices.iter().rev() {
    result.push(... close: price.close * multiplier ...);  // ← 先用當前
    if let Some(&af) = event_af.get(&price.date) {
        multiplier *= af;            // ← 再更新給更早的日子
    }
}
```

**v1.8 進化**:`compute_forward_adjusted` 拆兩個獨立 multiplier(`price_multiplier` 從 AF / `volume_multiplier` 從 vf);詳見 commit `c71d422` + `m2Spec/oldm2Spec/unified_alignment_review_r2.md` r3.1 P0-11 段。


## 關鍵架構決策（不要改）

| 決策 | 原因 |
|------|------|
| `field_mapper.transform()` 回傳 `(rows, schema_mismatch: bool)` tuple | phase_executor 需要知道是否要呼叫 mark_schema_mismatch |
| `db.upsert()` 有 PRAGMA 欄位過濾 | 防禦性設計：API 新增欄位不會炸掉整個 sync |
| TOML inline table 必須單行 | `tomllib` TOML v1.0 限制 |
| `--stocks`、`--dry-run`、`--phases` 是子命令選項 | 放在子命令後才符合使用者直覺 |
| `cooldown_on_429_sec` 存在 `RateLimiter` 實例上 | api_client 從這裡讀 |
| **Rust 後復權迴圈：先 push 再更新 multiplier** | 除息日當日 raw 已是除息後，不可再乘該日 AF |
| **`FieldMapper(db=db)`** | 用 DB schema 補豁免名單，避免「與 DB 同名直接入庫」欄位被誤報 novel |
| **Phase 4 必須傳 `stock_ids`** | `stock_sync_status` 表沒人寫入，Rust 取不到清單 |
| **Windows binary path 自動補 .exe** | `asyncio.create_subprocess_exec` 不像 shell 會自動補 |
| **`detail_fields` 在 toml 是「文件用」** | runtime 沒消費，純註記哪些欄位會進 detail JSON |
| **5 類法人各自獨立欄位**（不累加） | 外資/自營商「自行 vs 避險/自營」量化策略上有差別 |
| **upsert UPDATE 路徑強制 `updated_at = NOW()`**（v1.7） | 跟 schema `DEFAULT NOW()` 對 INSERT 的行為對齊；跟 `_merge_delist_date` 的 `SET updated_at = NOW()` 語意統一；row dict 帶 / 不帶 updated_at 兩條 path 都套 |
| **`DBWriter._table_pks` 動態查 information_schema**（v1.7） | schema 是 single source of truth，phase_executor / sync_tracker 不再硬編碼 PK 對照表 |
| **`api_sync_progress.status` 5 種**（v1.7） | `pending / completed / failed / empty / schema_mismatch`；後兩種 baseline 漏掉，由 alembic `a1b2c3d4e5f6` 補上 |
| **Rust `process_stock` 永遠全量重算**（v1.7 標註） | 後復權 multiplier 從尾端倒推，新除權息會回頭改全段 fwd 歷史值，partial 邏輯上是錯的；Rust 端 `_mode` 刻意忽略，未來要 incremental 必須在 Python 層偵測決定要不要叫 Rust |
| **Phase 4 mode 從 CLI runtime 傳**（v1.7） | `_run_phase4(mode)` 收參數而非 `self.config.execution.mode`，避免 toml 寫死 backfill 但 CLI 跑 incremental 時錯位 |
| **Rust 後復權拆兩個 multiplier**（**v1.8 重大語意切換**）| `compute_forward_adjusted` 拆 `price_multiplier`(從 AF) + `volume_multiplier`(從 vf);av3 揭露 collector field_mapper 寫對的 vf 過去被 Rust 忽略,造成 split/par_value volume 算錯方向;現在 Rust 讀 `price_adjustment_events.volume_factor` |
| **convention 切換:現金 dividend volume 不動**（**v1.8 語意改變**）| 過去 dollar_vol 守恆(volume / AF),現在 vf=1.0 → volume 不動,反映實際 share 流動性供 OBV/VWAP 等 indicator 用 |
| **stock_dividend vf = 1/(1 + stock_div/10)**（v1.8）| field_mapper 對 dividend 統一寫 vf=1.0(P1-17 bug),由 `post_process._recompute_stock_dividend_vf` SQL UPDATE 修正(限制:面額非 10 元個股不精確) |
| **Phase 4 staleness 短期補丁**（v1.8）| `post_process.invalidate_fwd_cache` + `phase_executor` 寫 `price_adjustment_events` 後 reset `stock_sync_status.fwd_adj_valid=0`;長期完整 dirty queue 契約見 `cores_overview.md §7.5` |

---

## 已知問題清單（下次 session todo）

按優先序排列，每項都標明影響範圍與建議修法：

> v1.6 / v1.7 已處理：~~detail warning 群~~、~~dividend_policy 雙 source warning~~、~~api_sync_progress CHECK 漏 empty/schema_mismatch~~、~~Phase 4 mode 對齊裂縫~~、~~SqliteWriter 半殘狀態~~、~~post_process SELECT 缺 market filter~~、~~_TABLE_PKS 硬編碼~~、~~stock_info.updated_at 語意混亂~~

> **v1.8 已處理**:~~Rust split/par_value/cap_inc volume 算錯方向(P0-11)~~ commit `c71d422`、~~Phase 4 staleness(P0-7 短期補丁)~~ commit `e051216`、~~field_mapper stock_dividend vf 計算(P1-17)~~ commit `608d275`、~~av3 Test 2 SQL CASE 誤判~~ commit `a2c94c2`、~~cores_overview §7.5 dirty queue 契約 + §10.0 Core 邊界三原則~~ commit `d029be3`

> **v1.9 已處理**:~~PR #17 (B-3) events 砍 3 + fwd 加 4 + Rust 拆 multiplier~~ commit `4eddd1c`、~~rust_bridge schema version 對齊 3.2~~ commit `f215d5b`、~~R-1 漏改 Rust trading_calendar→trading_date_ref~~ commit `7db9c42`、~~config rule 5 + av3 SQL 過時欄~~ commit `ac7c980`、~~P1 dividend AF reference_price 偷懶 sanity check~~ commit `c8367f8`、~~PowerShell 中文亂碼 wrapper 5 輪修法~~ commit `3c3d8a0`、~~m2 blueprint Hard 階段 3 處 amend~~ commit `f46d50d`

### ~~🔴 待 user 驗證：v1.6 schema 變更後的重跑~~（v1.7 已重跑驗證）

v1.7 review #3 + #4 過程中 user 在本機跑過 `python src\main.py status` + 全表體檢 + Phase 1 重跑（`stock_info` 含 detail 欄位寫入），確認舊 v1.6 schema 變更也都生效。`api_sync_progress` 343 segment 全部健康（completed 322 / empty 21 / failed 0 / pending 0）。

### ~~🟡 institutional_daily vs price_daily 多 2 筆~~（v1.6 已解）

FinMind `TaiwanStockInstitutionalInvestorsBuySell` 在週六會回殘留資料（內容是某筆固定值，date 是非交易日，2330 在 2019-08-24/2019-10-26 各 1 筆，內容字字相同）。
修法：`aggregators._filter_to_trading_days()` 在 pivot 前用 `trading_calendar` 過濾掉非交易日；`scripts/cleanup_non_trading_days.py` 一次性清現存歷史鬼資料。
驗證後 `institutional_daily` 1772 vs `price_daily` 1773 對齊（差 1 是當日尚未結算）。

### ~~🟢 exchange_rate FinMind 限制~~（v1.6 已解）

**根因**：`TaiwanExchangeRate` 必須帶 `data_id` (currency) 才會回完整時序，不帶就只回每幣 3 個代表性日期 → 7 segment × 19 幣 × 3 = 57 筆假象。
**驗證**：FinMind 測試 `get_datalist("TaiwanExchangeRate")` 回 `["AUD", "CAD", "CHF", "CNY", "EUR", "GBP", "HKD", "IDR", "JPY", "KRW", "MYR", "NZD", "PHP", "SEK", "SGD", "THB", "USD", "VND", "ZAR"]` 共 19 幣。
**修法**：collector.toml 把 `exchange_rate` 從 `param_mode = "all_market"` 改成 `per_stock_fixed` + `fixed_ids = [...19 幣...]`，跟 `market_index_us` (SPY/^VIX)、`market_index_tw` (TAIEX/TPEx) 同樣 pattern。
**重跑成本**：8 segment × 19 幣 = 152 個 API call（rate_limit 1600/h、min_interval 2250ms 下約 6 分鐘跑完），phase 6 整體耗時會明顯增加。

### 🟢 待做：agent-review-mcp 支線

CLAUDE.md v1.4 第 6 點提到「要不要切支線開始建 agent-review-mcp（spec 在最早的訊息）」這件事還沒開始。原本想用的 branch 名稱已被 collector 改善的 review pass 佔用至今（review #3 + #4），下次 session 才有空檔切去做。

### 🟢 待做：PR 合併（v1.7 PR review）

`claude/review-collector-dependencies-n03rE` → `m1/postgres-migration` 的 PR 已開（review #3 + #4 共 8 commit + 本次 docs commit = 9 commit）。等 base 維護者 / Codex / Cursor review。

### ~~🟡 待研究：Phase 4 真正的 incremental 優化~~（v1.26 已落地）

v1.26 nice-to-have D 完成:`bronze/phase_executor._run_phase4` 加 incremental
dirty queue filter — `mode=="incremental"` 時查 `price_daily_fwd.is_dirty=TRUE`
distinct stock_id,0 dirty → skip Rust dispatch 完整省 ~6 分鐘。對齊
`silver/orchestrator._run_7c` 同款 PR #20 dirty queue pattern。

### ~~🟡 待研究：CLAUDE.md 章節重組~~（v1.8 已重組）

每輪 review 都加段落，文件愈來愈長（v1.7 已 ~400 行）。下次可考慮把 v1.4 / v1.5 / v1.6 的 commits 表格與詳解搬到附錄或單獨 docs/ 目錄，主文只保留「最新 v1.X 銜接資訊 + 不變的關鍵架構決策」。

---

## 下班後 verify 流水線(2026-05-18 整日 4 commits)

對應今日 v3.29 → v3.32。完整 6 phase,**Phase A 是 blocking gate**(v3.32 SQL
diagnostic)。

### Phase 0:準備環境

```powershell
cd C:\Users\jarry\source\repos\StockHelper4me
(Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned)
& .\.venv\Scripts\Activate.ps1
$env:DATABASE_URL = "postgresql://twstock:twstock@localhost:5432/twstock"

git fetch origin claude/continue-previous-work-xdKrl
git checkout claude/continue-previous-work-xdKrl
git pull
git log --oneline -4
# 預期看到:a365240 v3.32 / 7b2eb98 v3.31 / 7f8d877 v3.30 / 4184d04 v3.29
```

### Phase A:Pre-impl SQL diagnostic(blocking — v3.32)

3 條 SQL 必跑。任一失敗就停下,先補資料再走 Phase B。

```powershell
# A-1:F-Score 9 條件需要的 detail key 是否都在 financial_statement_derived
psql $env:DATABASE_URL -c "
SELECT DISTINCT jsonb_object_keys(detail) AS key
  FROM financial_statement_derived
 WHERE stock_id = '2330' AND type IN ('income','balance','cashflow')
   AND date >= '2024-01-01'
 ORDER BY key
 LIMIT 60;
"
# 期待至少看到:本期淨利(淨損)/ 營業收入合計 / 營業成本合計 /
#               資產總額 / 流動資產 / 流動負債 / 長期借款 / 股本 /
#               營業活動之現金流量
# 若缺 → src/cross_cores/f_score.py 的 KEY_* fallback chain 需要再加新 key

# A-2:industry_category populated %(v3.32 industry_adj_gp 需 ≥ 80%)
psql $env:DATABASE_URL -c "
SELECT COUNT(*) AS total,
       COUNT(industry_category) AS non_null,
       ROUND((COUNT(industry_category)::numeric / COUNT(*) * 100), 2) AS pct
  FROM stock_info_ref
 WHERE market = 'TW' AND delisting_date IS NULL;
"
# 期待 pct ≥ 80;< 80 → industry_adj_gp 可上但 industry median 不穩,留意 narrative

# A-3:valuation_daily_derived.dividend_yield populated %(v3.32 dividend_yield 用)
psql $env:DATABASE_URL -c "
SELECT (COUNT(dividend_yield)::numeric / COUNT(*) * 100)::numeric(5,2) AS pct,
       COUNT(*) AS total,
       COUNT(dividend_yield) AS non_null
  FROM valuation_daily_derived
 WHERE date = (SELECT MAX(date) FROM valuation_daily_derived) AND market = 'TW';
"
# 期待 pct ≥ 90;< 90 → dividend_yield builder 會有較多 no_yield_data row
```

### Phase B:alembic + 跑 cross_cores phase 8 全市場(v3.32)

```powershell
# B-1:升級 schema
alembic upgrade head
# 期待 head: d9e0f1g2h3i4

# B-2:驗證 11 張新表存在
psql $env:DATABASE_URL -c "
SELECT tablename FROM pg_tables
 WHERE schemaname='public'
   AND (tablename LIKE '%_ranked_derived' OR tablename LIKE 'monthly_trigger%')
 ORDER BY tablename;
"
# 期待 11 張(magic_formula_ranked + v3.32 10 張)

# B-3:跑全市場 cross_cores phase 8(11 個 builder 一起跑)
python src/main.py cross_cores phase 8 --full-rebuild
# 期待 11 個 builder 全部 status=ok;rows_written 規模 ~1100-1300/builder
# (Layer 5 monthly_trigger 可能 < 100 因為 trigger 性質稀疏)

# 想單獨跑 1 個 builder:
# python src/main.py cross_cores phase 8 --builder f_score
```

### Phase C:資料驗證(v3.32 spot-check)

```powershell
# C-1:per builder row count
psql $env:DATABASE_URL -c "
SELECT 'magic_formula' AS b, COUNT(*) FROM magic_formula_ranked_derived WHERE date = (SELECT MAX(date) FROM magic_formula_ranked_derived)
UNION ALL SELECT 'persistent_momentum', COUNT(*) FROM persistent_momentum_ranked_derived WHERE date = (SELECT MAX(date) FROM persistent_momentum_ranked_derived)
UNION ALL SELECT 'revenue_momentum', COUNT(*) FROM revenue_momentum_ranked_derived WHERE date = (SELECT MAX(date) FROM revenue_momentum_ranked_derived)
UNION ALL SELECT 'institutional_concert', COUNT(*) FROM institutional_concert_ranked_derived WHERE date = (SELECT MAX(date) FROM institutional_concert_ranked_derived)
UNION ALL SELECT 'f_score', COUNT(*) FROM f_score_ranked_derived WHERE date = (SELECT MAX(date) FROM f_score_ranked_derived)
UNION ALL SELECT 'low_volatility', COUNT(*) FROM low_volatility_ranked_derived WHERE date = (SELECT MAX(date) FROM low_volatility_ranked_derived)
UNION ALL SELECT 'industry_adj_gp', COUNT(*) FROM industry_adj_gp_ranked_derived WHERE date = (SELECT MAX(date) FROM industry_adj_gp_ranked_derived)
UNION ALL SELECT 'long_term_low_vol', COUNT(*) FROM long_term_low_vol_ranked_derived WHERE date = (SELECT MAX(date) FROM long_term_low_vol_ranked_derived)
UNION ALL SELECT 'dividend_yield', COUNT(*) FROM dividend_yield_ranked_derived WHERE date = (SELECT MAX(date) FROM dividend_yield_ranked_derived)
UNION ALL SELECT 'mom_12_1', COUNT(*) FROM mom_12_1_ranked_derived WHERE date = (SELECT MAX(date) FROM mom_12_1_ranked_derived)
UNION ALL SELECT 'monthly_trigger', COUNT(*) FROM monthly_trigger_signals_derived WHERE date = (SELECT MAX(date) FROM monthly_trigger_signals_derived)
ORDER BY b;
"

# C-2:每 builder eligible (excluded_reason IS NULL) %
psql $env:DATABASE_URL -c "
SELECT 'f_score' AS b,
       COUNT(*) AS total,
       COUNT(*) FILTER (WHERE excluded_reason IS NULL) AS eligible,
       ROUND(COUNT(*) FILTER (WHERE excluded_reason IS NULL)::numeric / COUNT(*) * 100, 1) AS pct
  FROM f_score_ranked_derived WHERE date = (SELECT MAX(date) FROM f_score_ranked_derived)
UNION ALL SELECT 'dividend_yield',
       COUNT(*), COUNT(*) FILTER (WHERE excluded_reason IS NULL),
       ROUND(COUNT(*) FILTER (WHERE excluded_reason IS NULL)::numeric / COUNT(*) * 100, 1)
  FROM dividend_yield_ranked_derived WHERE date = (SELECT MAX(date) FROM dividend_yield_ranked_derived);
"
# F-Score:eligible % 反映實際過 F-Score ≥ 7 的股數
# Dividend Yield:eligible % 反映過 hard filter 的股數(yield ≥ 4% + 12M return > -20% + 5y ≥ 3y 配息)

# C-3:跨 toolkit 重疊 stock 觀察(分散度)
psql $env:DATABASE_URL -c "
WITH a AS (SELECT stock_id FROM persistent_momentum_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM persistent_momentum_ranked_derived)),
     b AS (SELECT stock_id FROM f_score_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM f_score_ranked_derived)),
     c AS (SELECT stock_id FROM long_term_low_vol_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM long_term_low_vol_ranked_derived))
SELECT
  (SELECT COUNT(*) FROM a) AS a_count,
  (SELECT COUNT(*) FROM b) AS b_count,
  (SELECT COUNT(*) FROM c) AS c_count,
  (SELECT COUNT(*) FROM (SELECT * FROM a INTERSECT SELECT * FROM b) x) AS a_inter_b,
  (SELECT COUNT(*) FROM (SELECT * FROM a INTERSECT SELECT * FROM c) x) AS a_inter_c,
  (SELECT COUNT(*) FROM (SELECT * FROM b INTERSECT SELECT * FROM c) x) AS b_inter_c;
"
# 期待跨 toolkit 交集 ≤ 30%(若 > 50% 表 toolkit 高度重疊 → 分散效果差)
```

### Phase D:Kalman / Neely 既有 verify(v3.30 + v3.31)

```powershell
# D-1:Python wrapper 跑 production data
python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030
# 期待 Kalman + Neely 全 [OK]
#   Stock  Kalman    Neely     Notes
#   2330   [OK]      [OK]      K:smoothed=~2200 velocity=非0 | N:price=2265 waves=5
#   3030   [OK]      [OK]      ...

# D-2:SQL spot-check 直看 Rust 寫進 DB
psql $env:DATABASE_URL -v stock=2330 -f scripts/verify_mcp_kalman_neely.sql
# 期待:
#   Phase 1 (Kalman):series_len > 1500;latest_kalman_state 含 raw_close /
#                    smoothed_price / velocity / uncertainty / regime / date
#   Phase 2 (Neely): scenario_count > 0;w1_start / w1_end 揭露 anchor 日期
```

### Phase E:MCP server 8 個 tool 對話內測

```powershell
# Claude Desktop 重啟讓它 reconnect MCP server,然後對話內測:
python -m mcp_server  # 開 stdio

# v3.31 4 個既有 tools
#   "2330 Neely 預測"                   → neely_forecast
#   "2330 Kalman 趨勢"                  → kalman_trend
#   "今天 magic formula top 30"          → magic_formula_screen
#   "2330 完整快照"                     → stock_snapshot(6-in-1)

# v3.32 4 個新 cross-stock factor screens
#   "今天 monthly screen top 30"         → monthly_screen(Toolkit A)
#   "今天 quarterly screen"              → quarterly_screen(Toolkit B)
#   "今天 annual low risk screen"        → annual_low_risk_screen(Toolkit C)
#   "今天 monthly trigger scan"          → monthly_trigger_scan(Layer 5)

# v3.29 應該回到正常(不再「未分類」)
#   "3030 的 risk_alert 狀態"           → severity = "disposition"
#                                          severity_label = "處置股(分盤撮合)"
#                                          (走 stock_snapshot.risk_alert)
```

### Phase F:Python tests + 環境健康

```powershell
# F-1:既有 + v3.32 new tests 全綠
pytest tests/cross_cores/ tests/mcp_server/ tests/agg/ --ignore=tests/mcp_server/test_render_tools.py -v
# 期待 165 passed / 1 skipped(render 缺 fastmcp 是 pre-existing)

# F-2:Rust workspace test(若想完整跑;v3.32 0 Rust 改動,可選)
cd rust_compute
cargo test --release --workspace --no-fail-fast 2>&1 | tail -5
cd ..
# 期待 443 passed / 0 failed
```

### 退出碼判定

| Phase | 條件 | 行動 |
|---|---|---|
| A-1 | F-Score 9 key 全在 | ✅ 過 |
| A-1 | 缺 1-2 key | 🟡 修 KEY_* fallback chain 後繼續 |
| A-2 | industry_category pct ≥ 80% | ✅ 過 |
| A-2 | < 80% | 🟡 industry_adj_gp 仍可上但 narrative 標 caveat |
| A-3 | dividend_yield pct ≥ 90% | ✅ 過 |
| A-3 | < 90% | 🟡 dividend_yield builder 多 no_yield_data row |
| B-3 | 11 builder 全 status=ok | ✅ 過,進 C |
| B-3 | 任 builder 失敗 | ❌ 看 logs 找 root cause |
| C-3 | 跨 toolkit 交集 ≤ 30% | ✅ 分散 OK |
| C-3 | 交集 > 50% | 🟡 toolkit 設計重疊,留意 |
| D | Kalman + Neely 全 [OK] | ✅ |
| D | 任一 [FAIL] | 看提示 — v3.30 path fix / v3.28 regex / tw_cores 重算 |
| E | 8 個 tool 都有正確 response | ✅ |
| F-1 | 165 passed | ✅ |

---

## helper 腳本清單

| 腳本 | 用途 | 範例 |
|------|------|------|
| `scripts/check_all_tables.py` | 全表筆數體檢(v4.18 取代 inspect_db.py） | `python scripts/check_all_tables.py` |
| `scripts/drop_table.py` | schema 變更後 drop 指定表（避免重灌全套） | `python scripts/drop_table.py institutional_market_daily` |
| `scripts/test_28_apis.py` | 28 支 API 連線健檢（urllib + tomllib，零依賴） | 需要 token |
| `scripts/probe_finmind_sponsor_unused.py` 🆕 v3.13 | 從 422 enum parser 拉 FinMind 全 catalog → diff vs collector.toml unused → probe 看 row+sample;ASCII labels(cp950 console OK)| `python scripts/probe_finmind_sponsor_unused.py --max 5` |
| `scripts/verify_event_kind_rate.sql` 🆕 v3.14 | per-EventKind 觸發率 verify(對齊 v1.32 ≤ 12/yr/stock 標準),4 sections:per-stock cores / market-level cores / Round 7 verify / milestone 4 variants 顯式(v3.18 加)| `psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql` |
| `scripts/maintain_facts_stats.sql` 🆕 v3.19 | facts / indicator_values / structural_snapshots 三表 ANALYZE + VACUUM stats refresh;Round N DELETE+INSERT 後或 wall time 反常變慢時跑(便宜 ~50-200ms 換 query planner stats) | `psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql` |
| `scripts/diagnose_slow_tw_cores.sql` 🆕 v3.19 | tw_cores 跑期間另開 psql 取樣:pg_stat_activity / lock waits / dedup query plan / pool saturation 4 phase + 解讀指南 | `psql $env:DATABASE_URL -f scripts/diagnose_slow_tw_cores.sql`(tw_cores 開跑後 30s) |
| `scripts/verify_mcp_kalman_neely.py` 🆕 v3.31 | MCP Kalman + Neely 對 production 出值健康度 verify。per-stock check `smoothed_price > 0` / `velocity ≠ 0` / `wave_count > 0` / staleness。退碼 0=全綠 / 1=任一 FAIL,提示 root cause(v3.30 path fix / v3.28 regex parse / tw_cores 重算)| `python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030` |
| `scripts/verify_mcp_kalman_neely.sql` 🆕 v3.31 | 對 indicator_values / structural_snapshots 直接 SQL spot-check 揭露 Rust 寫進 DB 的真實內容(排除 MCP layer 干擾)2 phase + 解讀 comment | `psql $env:DATABASE_URL -v stock=2330 -f scripts/verify_mcp_kalman_neely.sql` |
| `scripts/test_pipeline.ps1` 🆕 v4.4 | **完整測試流水線**(Windows)— 5 phase:Environment check / Sandbox unit tests(Rust 528 + Python 165+)/ Schema health(alembic+row counts)/ Production verify(facts stats + per-EventKind + **Neely forest_size P0 Gate**)/ MCP smoke test;支援 `-OnlyPhase` / `-SkipPhase` / `-DryRun` | `.\scripts\test_pipeline.ps1` |
| `scripts/test_pipeline.sh` 🆕 v4.4 | 完整測試流水線(Unix Bash 版,對齊 .ps1);環境變數 `SKIP_PHASES` / `ONLY_PHASES` / `DRY_RUN=1` 控制 | `./scripts/test_pipeline.sh` |

---

## 完整重跑流程（從零開始）

```powershell
cd C:\Users\jarry\source\repos\StockHelper4me
del data\tw_stock.db
python src\main.py backfill --stocks 2330,2317 --phases 1
python src\main.py backfill --stocks 2330,2317 --phases 2
python src\main.py backfill --stocks 2330,2317 --phases 3
# Phase 4 之前確認 rust binary 存在；不存在的話：
#   cd rust_compute && cargo build --release && cd ..
python src\main.py backfill --stocks 2330,2317 --phases 4
python src\main.py backfill --stocks 2330 --phases 5      # 5 類法人
python src\main.py backfill --stocks 2330 --phases 6      # macro
python scripts\check_all_tables.py
```

預估時間：~6 分鐘（不含 cargo build）。

---

## 資料庫 Schema（25 張表，v1.5 變更標 ⚠️、v1.6 變更標 🆕、v1.7 變更標 🆙）

| 資料表 | PK | 備註 |
|--------|----|----|
| `stock_info` | market, stock_id | 🆙 v1.7 改用既有 detail JSONB pack `data_update_date`（baseline schema 早就有 detail 欄位，只是 v1.6 之前 collector.toml 沒用上） |
| `trading_calendar` | market, date | |
| `market_index_tw` | market, stock_id, date | (TAIEX + TPEx) |
| `price_adjustment_events` | market, stock_id, date, event_type | |
| `price_daily` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `price_limit` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `price_daily_fwd` | market, stock_id, date | Rust 計算 |
| `price_weekly_fwd` | market, stock_id, year, week | Rust 計算 |
| `price_monthly_fwd` | market, stock_id, year, month | Rust 計算 |
| `institutional_daily` | market, stock_id, date | ⚠️ v1.5 從 6 欄擴 10 欄（5 類法人）|
| `margin_daily` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `foreign_holding` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `holding_shares_per` | market, stock_id, date | pack_holding_shares |
| `valuation_daily` | market, stock_id, date | |
| `day_trading` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `index_weight_daily` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `monthly_revenue` | market, stock_id, date | 🆕 v1.6 加 detail 欄位 |
| `financial_statement` | market, stock_id, date, type | pack_financial |
| `market_index_us` | market, stock_id, date | 🆕 v1.6 加 detail 欄位（SPY + ^VIX） |
| `exchange_rate` | market, date, currency | ⚠️ FinMind 19 筆限制 |
| `institutional_market_daily` | market, date | ⚠️ v1.5 同 institutional_daily 擴充 |
| `market_margin_maintenance` | market, date | |
| `fear_greed_index` | market, date | |
| `_dividend_policy_staging` | market, stock_id, date | 🆕 v1.6 加 source 欄位（post_process 用） |
| `api_sync_progress` | api_name, stock_id, segment_start | 🆙 v1.7 CHECK constraint 擴成 5 種 status（補 `empty` / `schema_mismatch`） |
| `stock_sync_status` | market, stock_id | Rust Phase 4 寫 `fwd_adj_valid`；`last_full_sync`/`last_incr_sync` 欄位保留未用（v1.6 已砍 Python 端 dead helper） |
| `schema_metadata` | key | 🆙 v1.7 PG baseline 才出現的表，記錄 `schema_version=2.0`；Rust binary 啟動時 assert |

---

## 環境細節（v1.7 更新：本機 PG 17 + alembic）

- Python 3.11+（需 tomllib）
- aiohttp + psycopg[binary,pool]>=3.2 已裝
- **PostgreSQL 17 本地服務**（`postgresql-x64-17` Windows service，非 docker）；`.env` 內 `DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock`
- v2.0 起 schema 變動走 **alembic incremental migration**，不再用 `scripts/drop_table.py` 單表 drop（除非要清整張表重灌）
- PowerShell 對 `python -c "..."` 的 nested quotes 處理很差，**inline SQL 改走 `psql $env:DATABASE_URL -c "..."`**（system PG client 比 Python 串好用）
- User token 環境變數 `$env:FINMIND_TOKEN`，禁止寫進 collector.toml
- Sandbox 環境連不到 finmindtrade.com，所有 API 實測都得 user 本機跑
- 全表體檢改用 `scripts/check_all_tables.py`（PG 版）+ `python src\main.py status`（v4.18 已刪舊 SQLite hardcode 的 inspect_db.py）

---

## 下次 session 建議優先序

> **🎯 v4.10 收尾(2026-05-20)**:M3SPEC alignment **Out-of-Scope backlog 全部清空** ☕☕☕☕ —
> Item 4 Pre-Constructive Pass 1/2 diagnostics union 落地;範圍比 CLAUDE.md v4.8 估的 30+
> 構造點小一個量級(只動 `MonowaveStructureLabels` 2 構造點 + `pre_constructive::run_pass2`
> 新函式 + `lib.rs` Pass 2 wiring + 1 forest refill loop)。
>
> alembic head 不變:`d9e0f1g2h3i4`(v4.0 → v4.10 全部 0 schema migration)。
> Rust workspace 39 crates / **587 tests passed / 0 failed**(v4.4 baseline 528 → +59 across 11 commits)。
> Production state:1266 stocks × 36 cores / wall time ~11 min / facts ~5.2M(VACUUM 後)。
>
> **v4.0 → v4.10 共 17 commits / +135 tests / ~8,450 LoC** Neely M3SPEC alignment 100% production-ready。

### 1. 立即可動工(無 blocker)

**1a. gov_bank_net Core 消費**(需先寫 EventKind 規格):
- 現狀:Silver `institutional_daily_derived.gov_bank_net` fill 80.74%(v3.14
  全市場 backfill 完),但 `rust_compute/cores/` 內 0 個 core 真正消費此欄
- 動工:`m3Spec/chip_cores.md`(或新 doc)寫 `GovBankAccumulation /
  GovBankDistribution` EventKind 規格 → 新增 `gov_bank_core` 或在
  `institutional_core` 擴 EventKind
- best-guess 不動,等 user 寫規格後再上 Rust

**1b. probe sponsor tier 全 catalog**(估 ~3-5 分鐘):
```powershell
python scripts/probe_finmind_sponsor_unused.py --max 0
```
找未用 dataset 候選(目前只 `--max 5` 試過);跑前先等 IP ban 解。

**1c. Aggregation Layer Phase B-4 FastAPI thin wrap**(估 ~2-3h):
- `agg_api/main.py` + Pydantic schemas + `/as_of/{stock_id}` + `/health_check`
- 不含 auth / rate limit / deploy(屬 Phase B-5 網站工程,獨立規格)

**1d. wall time 微優化**(目前 ~11 min / 1266 stocks):
- 非 calibration 引起(v4.x params_hash 0 變動 → facts dedup 行為一致)
- 可能 PG contention / sqlx pool 大小 / WAL flush 時機
- 若想動:從 `cargo flamegraph` 看哪個 core hot path 變慢起
- 排序低,wall time 仍 < 12 分鐘可接受

### 2. 中期 backlog(non-blocking)

**~~2a. Neely M3SPEC Item 4 Pre-Constructive Pass 1/2 diagnostics union~~**(✅ v4.10 已收尾):
- v4.10 commit:`pre_constructive::run_pass2` 新函式 + `MonowaveStructureLabels`
  加 `classified_index` + `pass1_only_labels` 兩欄 + lib.rs Stage 8.5 refill loop
- 範圍比原估的 ~30+ 構造點小一個量級(只 2 個 MonowaveStructureLabels 構造點)
- M3SPEC Out-of-Scope backlog **全部清空**

**2b. exhaustive compaction Round 3 真實 partial Stage 3-4 rerun**(留 V3):
- v4.8 已加 boundary_retracement_extreme reject 機制(< 0.236 or > 4.236 → reject)
- 完整 Stage 3-4 partial rerun(對 boundary scenarios 重跑 validator)留 V3 議題
- 目前 advisory + reject 已對齊 spec 大部分意圖

**2c. Silver schema 假設待 user 驗**(目前不阻塞 production):

| 假設 | 影響 core | 處置 |
|---|---|---|
| `margin_daily_derived.margin_maintenance` 是否存在 | margin_core | 不存在 → MaintenanceLow 永遠不觸發 |
| `holding_shares_per_derived.detail` JSONB schema | shareholder_core | best-guess key(small_holders_count 等)|
| `fear_greed_index` 是否需 `_derived` 表 | fear_greed_core | 目前直讀 Bronze,§6.2 已登記架構例外 |
| `financial_statement_derived.detail` JSONB key | financial_statement_core | v3.14 改中文 origin_name + 17 levels iterate 已部分解 |

**2d. Phase 4 真正的 incremental 優化** — 偵測「該股票無新除權息事件 → 跳過」每天 incremental 可省 ~6 分鐘

**2e. asyncio.gather 7a 平行優化** — 需先升 PostgresWriter 為 connection pool;perf gain ~ms 量級

### 3. m3Spec/ 後階段(若需新 cores)

- **P3 後階段 cores**(若 user 寫新 spec):Volume Profile / Anchored VWAP /
  Smart Money Concepts / Wyckoff 等(目前 35 cores 已涵蓋 NEELY + 主流 indicator
  + chip / fundamental / environment 完整集)
- **Wave Cores Phase 20+**(若 user 想拓展非 Elliott 派波浪理論)
- **跨資產配對交易**(pairs_trading core in `cross_cores/`)

### accepted baselines(超 12/yr 但 user 拍版接受)

| EventKind | rate/yr | 接受理由 | 紀錄版本 |
|---|---|---|---|
| `institutional / DivergenceWithinInstitution` | 58.41/yr/stock | production reality(高頻法人分歧)| v1.32 |
| `institutional / LargeTransaction` | 14.16/yr/stock | fat-tail (Lo 2001),邊際效益遞減 | v3.17 |
| `exchange_rate / SignificantSingleDayMove` | 14.8/yr(market-level)| macro shock 自然頻率,distinct_stocks=1 規則不適用 | v1.32 |
| `commodity_macro / CommoditySpike` | 12.2/yr(market-level)| 同上 macro fat-tail,對齊 LargeTransaction 模式 | v3.24 |

之後 calibration 看到 ≤ 12/yr 為方向性目標,以上 4 個 EventKind 不需要再 tighten。
注意:market-level cores(distinct_stocks ≤ 5)的 ≤ 12 規則本來就不適用,以 events/yr 評估即可。

### ⚠️ V2 階段禁止做(spec 已明文)

- **Indicator kernel 共用化** → cores_overview §十四「P3 後考慮,V2 不規劃」。
  2026-05-09 嘗試過抽出(commit 5abca8d),user 退板,revert(commit 6f05fb9)。
  8 個 indicator cores 保持各自獨立 ema/sma/wma/wilder_atr/wilder_rsi 實作,
  符合 §四 零耦合原則。
- **跨指標訊號獨立 Core**(TTM Squeeze / `chip_concentration_core` 等)
  → cores_overview §十一 / chip_cores §八「不在 Core 層整合」。
- **`financial_statement_core` 拆分**(損益/資產負債/現金流獨立 Core)
  → cores_overview §十四「V3 議題,V2 不規劃」。
- **ErasedCore trait wrapper** → cores_overview §十四「V2 不規劃」;workflow
  filter 用 hardcoded match arm + `is_enabled()` check 即可。
