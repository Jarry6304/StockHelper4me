# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件下方版本章節是跨 session 銜接的歷程紀錄(v3.5 → v3.18,最新 2026-05-17;
> v1.5 ~ v1.34 已歸檔 [`docs/claude_history.md`](docs/claude_history.md))。動工前先讀本段 Quick Reference,然後依任務性質往下讀對應 v3.X 段落。

---

## 專案概要

`tw-stock-collector` — 台股資料蒐集 + 計算 pipeline。FinMind API → Postgres 17。
**5 層架構**(Bronze / Silver per-stock / Cross-Stock Cores / M3 Cores / MCP API,v3.5 R3 後)。
Python 3.11+ + Rust workspace 35 crates(Silver S1 後復權 + M3 Cores 全市場全核 dispatch)。

- **alembic head**:`a6b7c8d9e0f1`(v3.14 gov_bank Bronze 加 bank_name 維度;v3.15 → v3.18 純 Rust const tweak 0 migration)
- **開發分支**:`claude/continue-previous-work-xdKrl`
- **collector.toml**:34 entries(gov_bank 需 FinMind sponsor tier)
- **Rust tests**:35 crates / **426 passed / 0 failed**
- **Production state**:1266 stocks × 35 cores / wall time ~10 min / facts ~10M;Round 7 + Round 8 calibration **完整結算**(6/6 over-fired EventKind = 5 校準 + 1 accepted baseline,v3.18 production verify 4/4 milestone in band)

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
python scripts/verify_pr18_bronze.py        # Bronze 5 張反推 round-trip(5/5 OK)
python scripts/verify_pr19b_silver.py       # Silver 5 個簡單 builder 對 v2.0 legacy 等值
python scripts/verify_pr19c_silver.py       # Silver 5 個 market-level builder
python scripts/verify_pr19c2_silver.py      # Silver 3 個 PR #18.5 依賴 builder(預設 1101,2317,2330)
python scripts/verify_pr20_triggers.py      # PR #20:Bronze→Silver dirty trigger 整合測試(15 trigger)
python scripts/test_28_apis.py              # 28 支 API 連線健檢(需 FINMIND_TOKEN)
python scripts/inspect_db.py 2330           # ⚠️ v1.6 之前的 SQLite hardcode,v2.0 後不可用,改用 check_all_tables.py
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

## helper 腳本清單

| 腳本 | 用途 | 範例 |
|------|------|------|
| `scripts/inspect_db.py` | 檢視 db 各表筆數 + 特定股票詳細內容 + Phase 6 全市場資料 + 後復權驗證 | `python scripts/inspect_db.py 2330` |
| `scripts/drop_table.py` | schema 變更後 drop 指定表（避免重灌全套） | `python scripts/drop_table.py institutional_market_daily` |
| `scripts/test_28_apis.py` | 28 支 API 連線健檢（urllib + tomllib，零依賴） | 需要 token |
| `scripts/av3_spot_check.sql` | av3 fwd 後復權驗證(Test 1~6 + 5b)+ 75 處中文段全用 COPY...TO STDOUT 走 server transcode | 不直接跑,改用 wrapper 👇 |
| `scripts/run_av3.ps1` 🆕 v1.9 | PowerShell wrapper:三層 console UTF-8 + LC_MESSAGES=C + temp file roundtrip 完整修中文亂碼 | `.\scripts\run_av3.ps1` |
| `scripts/_reverse_pivot_lib.py` 🆕 v1.10 | PR #18 共用 helper:SPECS dict + 5 函式(fetch / reverse / upsert / repivot / assert)。`run_reverse_pivot()` 一站式 runner | 給 5 個 reverse_pivot_*.py 呼叫,不直接跑 |
| `scripts/reverse_pivot_institutional.py` 🆕 v1.10 | institutional_daily → institutional_investors_tw(1 → 最多 5 法人列) | `python scripts/reverse_pivot_institutional.py --stocks 2330 --dry-run` |
| `scripts/reverse_pivot_valuation.py` 🆕 v1.10 | valuation_daily → valuation_per_tw(最簡 3 欄 1:1) | `python scripts/reverse_pivot_valuation.py` |
| `scripts/reverse_pivot_day_trading.py` 🆕 v1.10 | day_trading → day_trading_tw(2 stored + 2 detail unpack) | `python scripts/reverse_pivot_day_trading.py` |
| `scripts/reverse_pivot_margin.py` 🆕 v1.10 | margin_daily → margin_purchase_short_sale_tw(6 stored + 8 detail unpack) | `python scripts/reverse_pivot_margin.py` |
| `scripts/reverse_pivot_foreign_holding.py` 🆕 v1.10 | foreign_holding → foreign_investor_share_tw(2 stored + 9 detail unpack) | `python scripts/reverse_pivot_foreign_holding.py` |
| `scripts/verify_pr18_bronze.py` 🆕 v1.10 | PR #18 5 張 Bronze 反推聚合驗證,印 status table。push 前必跑 5/5 OK | `python scripts/verify_pr18_bronze.py` |
| `scripts/probe_finmind_sponsor_unused.py` 🆕 v3.13 | 從 422 enum parser 拉 FinMind 全 catalog → diff vs collector.toml unused → probe 看 row+sample;ASCII labels(cp950 console OK)| `python scripts/probe_finmind_sponsor_unused.py --max 5` |
| `scripts/verify_event_kind_rate.sql` 🆕 v3.14 | per-EventKind 觸發率 verify(對齊 v1.32 ≤ 12/yr/stock 標準),4 sections:per-stock cores / market-level cores / Round 7 verify / milestone 4 variants 顯式(v3.18 加)| `psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql` |
| `scripts/maintain_facts_stats.sql` 🆕 v3.19 | facts / indicator_values / structural_snapshots 三表 ANALYZE + VACUUM stats refresh;Round N DELETE+INSERT 後或 wall time 反常變慢時跑(便宜 ~50-200ms 換 query planner stats) | `psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql` |
| `scripts/diagnose_slow_tw_cores.sql` 🆕 v3.19 | tw_cores 跑期間另開 psql 取樣:pg_stat_activity / lock waits / dedup query plan / pool saturation 4 phase + 解讀指南 | `psql $env:DATABASE_URL -f scripts/diagnose_slow_tw_cores.sql`(tw_cores 開跑後 30s) |

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
python scripts\inspect_db.py 2330
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
- ⚠️ `scripts/inspect_db.py` v1.6 之前是 SQLite hardcode，v2.0 後仍未升級，**已不可用**；改用 `scripts/check_all_tables.py`（已 PG 版）+ `python src\main.py status`

---

## 下次 session 建議優先序

> **🎯 v3.18 收尾(2026-05-17)**:Round 8 calibration session **完整結算** ☕ —
> 6 個原始 over-fired EventKind = 5 校準到 target band + 1(LargeTransaction)
> accepted baseline(fat-tail Lo 2001 reality)。production verify 4/4 milestone
> variants in band(Low 10.06 / High 7.90 / LowAnn 5.10 / HighAnn 3.74)。
>
> alembic head 不變:`a6b7c8d9e0f1`(v3.15 → v3.18 純 Rust const tweak)。
> Rust workspace 35 crates / **426 tests passed / 0 failed**。
> Production state:1266 stocks × 35 cores / ~10 min wall / facts ~10M+。
>
> **m2 大重構 + Round 7/8 calibration + Aggregation Layer 4 Phase + Neely Core v1.0.1 P0 Gate** 全部已收尾,進入「沒有 calibration backlog」的乾淨起點。

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

**1d. wall time 微優化**(v3.18 run-all 695s vs v3.17 561s,+24%):
- 非 calibration 引起(params_hash 0 變動 → facts dedup 行為一致)
- 可能 PG contention / sqlx pool 大小 / WAL flush 時機
- 若想動:從 `cargo flamegraph` 看哪個 core hot path 變慢起
- 排序低,wall time 仍 < 12 分鐘可接受

### 2. 中期 backlog(non-blocking)

**2a. exhaustive compaction Round 3 邊界波 reevaluation**(留 V3):
- spec `m3Spec/neely_rules.md §Three Rounds` line 1198-1256
- 目前 Round 1-2 已落地(`compaction/three_rounds.rs`),Round 3 邊界波 m(+1)/m(-1)
  reevaluation 需要部分 Stage 3-4 rerun

**2b. Silver schema 假設待 user 驗**(目前不阻塞 production):

| 假設 | 影響 core | 處置 |
|---|---|---|
| `margin_daily_derived.margin_maintenance` 是否存在 | margin_core | 不存在 → MaintenanceLow 永遠不觸發 |
| `holding_shares_per_derived.detail` JSONB schema | shareholder_core | best-guess key(small_holders_count 等)|
| `fear_greed_index` 是否需 `_derived` 表 | fear_greed_core | 目前直讀 Bronze,§6.2 已登記架構例外 |
| `financial_statement_derived.detail` JSONB key | financial_statement_core | v3.14 改中文 origin_name + 17 levels iterate 已部分解 |

**2c. Neely P0 Gate 五檔實測校準**(估 ~1 天 + 校準):
- 0050 / 2330 / 3363 / 6547 / 1312
- 校準常數寫入 `docs/benchmarks/`:`forest_max_size` / `compaction_timeout_secs`
  / `BeamSearchFallback.k` / `REVERSAL_ATR_MULTIPLIER` / `BEAM_CAP_MULTIPLIER`
- 對齊 cores_overview §9.1「P0 完成後的 Gate」

**2d. Phase 4 真正的 incremental 優化** — 偵測「該股票無新除權息事件 → 跳過」每天 incremental 可省 ~6 分鐘

**2e. asyncio.gather 7a 平行優化** — 需先升 PostgresWriter 為 connection pool;perf gain ~ms 量級

### 3. m3Spec/ 後階段(若需新 cores)

- **P3 後階段 cores**(若 user 寫新 spec):Volume Profile / Anchored VWAP /
  Smart Money Concepts / Wyckoff 等(目前 35 cores 已涵蓋 NEELY + 主流 indicator
  + chip / fundamental / environment 完整集)
- **Wave Cores Phase 20+**(若 user 想拓展非 Elliott 派波浪理論)
- **跨資產配對交易**(pairs_trading core in `cross_cores/`)

### accepted baselines(超 12/yr/stock 但 user 拍版接受)

| EventKind | rate/yr | 接受理由 | 紀錄版本 |
|---|---|---|---|
| `institutional / DivergenceWithinInstitution` | 58.41 | production reality(高頻法人分歧)| v1.32 |
| `institutional / LargeTransaction` | 14.16 | fat-tail (Lo 2001),邊際效益遞減 | v3.17 |

之後 calibration 看到 ≤ 12/yr/stock 為方向性目標,以上兩個 EventKind 不需要再 tighten。

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
