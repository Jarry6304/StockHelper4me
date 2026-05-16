# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件下方「v1.X 大項總覽」開始的章節是跨 session 銜接的歷程紀錄（v1.5 → v1.33，最新 2026-05-13）。動工前先讀本段 Quick Reference，然後依任務性質往下讀對應 v1.X 段落。

---

## 專案概要

`tw-stock-collector` — 台股資料蒐集 + 計算 pipeline。FinMind API → Postgres 17。**v3.5 R3 後改 5 層架構**(Bronze / Silver per-stock / **Cross-Stock Cores 新層** / M3 Cores / MCP API)。Python 3.11+ + Rust workspace(Silver S1 後復權 + M3 Cores 全市場全核 dispatch + tw_cores monolith 拆 8 module)。schema v3.2 r1(`schema_metadata`),開發分支 `claude/continue-previous-work-xdKrl`,alembic head **`a6b7c8d9e0f1`**(v3.14 gov_bank Bronze 加 bank_name 維度)。collector.toml 34 entries 全 enabled(v3.14 gov_bank 開,FinMind sponsor tier)。

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

## v1.34 — P0 Gate v3/v4 production 校準雙波(2026-05-14)

接 v1.33 收尾後動 P0 Gate production calibration 雙波。0 alembic / 0 Python /
0 collector.toml,純 Rust 常數調整 4 個 indicator cores。

### Commits

| commit | 範圍 |
|---|---|
| `7d18b3f` | P0 Gate v2:`neely_core` forest_max_size 1000 → 200 |
| `919c0ee` | P0 Gate v3 follow-up report:missing_wave / emulation / reverse_logic 健康確認 |
| `518f2c8` | **v4 Cross spacing**(B 路線):KD/MA cross 10→15,MACD cross 10→15,zero cross 5→8 |
| `8312b5e` | **v4 MIN_PIVOT_DIST**(C 路線):kd/macd/rsi 20 → 12(讓步至 Murphy 1999 下限) |

### v4 Cross spacing 校準確認(本機已驗 ✅)

User 跑 DELETE Cross facts + 重跑 production 後確認 7/7 EventKind 全部命中目標:

| EventKind | v3 | v4 | 目標 |
|---|---|---|---|
| kd GoldenCross / DeathCross | 20.17 / 20.13 | **10.29 / 10.26** | 8-12/yr ✅ |
| ma MaBullishCross / MaBearishCross | 13.59 / 13.42 | **7.39 / 7.32** | 6-9/yr ✅ |
| macd HistogramZeroCross | 19.01 | **11.90** | ≤ 12/yr ✅ |
| macd GoldenCross / DeathCross | 9.58 / 9.42 | **6.79 / 6.68** | 5-7/yr ✅ |

### v4 MIN_PIVOT_DIST(C 路線)— **⚠️ 本機驗證未跑**

v3 facts 表混雜「過去 MIN_PIVOT_DIST=10」+「v1.33 後 MIN_PIVOT_DIST=20」資料。
DELETE + 重跑後揭露純 v1.33 行為:Divergence 0.27-0.36/yr(kd/macd) /
0.66-0.71/yr(rsi),低於 Murphy (1999) p.248 預期 1-4/yr 下限 3×。

| Core | const | v3 (混雜) | v4 純 (預期) | Murphy |
|---|---|---|---|---|
| kd_core | MIN_PIVOT_DIST 20→12 | 1.08-1.11 | **0.8-0.9/yr** | 下限 ✅ |
| macd_core | MIN_PIVOT_DIST 20→12 | 0.81-0.98 | **0.7-0.8/yr** | 接近下限 ✅ |
| rsi_core | MIN_PIVOT_DIST 20→12 | 0.66-0.71 | **1.6-1.8/yr** | 中段 ✅ |

讓步理由:
- 保留 spec §3.6「兩極值點距離 ≥ N」結構性要求(N ≥ 2 × PIVOT_N = 6,12 滿足)
- N=12 為 NEoWave 經驗值,12-bar ≈ 2.4 週,介於 v1.32(10)和 v1.33(20)之間
- spec §3.6 預設 20 為「保守值」,Murphy 1999 沒給明確下限

### v5 驗證(已收尾,2026-05-14 本機驗過 ✅)

v5 production run 後 §N Divergence 落入預期 — kd/macd/rsi BullishDivergence /
BearishDivergence 全 6 個 EventKind 命中 ≤ 4/yr 範圍。**P0 Gate 12 EventKinds
全部命中**(7 Cross + 6 Divergence 一輪收尾,留 v1.35 接續推進 P3/P2 cores)。

### 已知狀態(本段結束時)

- Rust workspace:24 crate / **290 tests passed** / 0 warnings
- v4 Cross spacing(B 路線)+ v4 MIN_PIVOT_DIST(C 路線)— 全 12 EventKind 命中 ✅
- v1.35 接續推進 Neely Phase 13-19 + P3/P2 indicator cores batch + agg layer 補強

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- 純常數值改變,4 個 indicator cores 各 1 const
- 修正方向 spec-defensible(C 路線 N=12 ≥ 2 × PIVOT_N 仍滿足 §3.6 結構性條件)
- Rollback:單 commit `git revert` 即可(分 v4 cross spacing / v4 pivot dist 兩段)

---

## v1.33 — P2 修正項目補完 spec alignment + 出處註解(2026-05-13)

接 v1.32 收尾 P2 後,user 指示「修正後的資料反寫回原本的 core 文件，並附上來源出處
供反查證」+「完整再確認一次程式碼跟規格有無對齊，若正確則更新文件後推上主幹，
若不正確則修正後更新文件推上主幹」。本 session 兩步走:

**Step 1 — 出處註解(commit `3d49cc7`)**:7 個 P2 修正過的 core 文件補完整出處體系:
- `Verification: scripts/p2_calibration_data.sql §X` 反查 SQL section
- 修前→修後觸發率對照(91.83→6-12 / 50.06→2-6 / 34.87→10-15 / 17→8-12 等)
- 學術文獻完整書名 + journal + page(Murphy 1999 / Lucas & LeBeau 1992 / Sheingold 1978 / Brown & Warner 1985 / Fama et al. 1969)
- `day_trading_core` ratio 閾值加 `m3Spec/chip_cores.md §7.3` 引用

**Step 2 — Spec alignment 校驗(commit 本 PR)**:逐 core 對 m3Spec/m2Spec audit,
揭露 1 個 spec 偏離:

| 偏離項 | spec 規定 | 原值 | 修正後 |
|---|---|---|---|
| rsi/kd/macd `MIN_PIVOT_DIST` | spec §3.6:「兩個價格極值點時間距離 ≥ N=20」 | 10(P5 重寫時誤用 Murphy 範圍下界) | **20** |

P5 commit `8d3288a` divergence rewrite 時用了 `MIN_PIVOT_DIST=10` 並標註
「Murphy 20-60 intervals 下界」,但實際 10 < Murphy 範圍下界(20)且偏離 spec
§3.6 預設 20。本 PR 對齊。

**對應測試更新**(rsi/kd/macd × bearish/bullish):
- pivot 間距從 idx 5/18(距離 13)→ idx 5/28(距離 23)
- 序列長度從 n=25 → n=35

### Spec 校驗其他項全綠

| Core | 校驗項 | 對齊 |
|---|---|---|
| `day_trading_core` | Params §7.3 / EventKind §7.5 / Fact §7.6 | ✅ 完全對齊 m3Spec/chip_cores.md |
| `kd_core` | Params §5.2 / Output §5.4 / Fact §5.5 | ✅ 對齊 spec r2 |
| `ma_core` | EventKind §7.6(ma_bullish/bearish/golden/death + above_ma_streak) | ✅ spec 未強制 cross_spacing,production-data-driven addition |
| `macd_core` | Params §3.2 / 5 種 EventKind §3.5 / 背離 §3.6 | ✅(MIN_PIVOT_DIST 修後對齊) |
| `rsi_core` | Params §4.2 / Output §4.4 / Fact §4.5 / FailureSwing §4.6 | ✅(MIN_PIVOT_DIST 修後對齊) |
| `institutional_core` | edge trigger 非 spec 規定(production calibration,Brown & Warner 1985 學術依據) | ✅ spec 允許 |
| `foreign_holding_core` | edge trigger + rolling z-score(Fama 1969 + Brown & Warner 1985) | ✅ spec 允許 |

### Commit 預計(本 PR)

| commit | 範圍 |
|---|---|
| `3d49cc7` | 7 cores 出處註解補完(Verification SQL + 學術文獻完整書名 + m3Spec ref) |
| 本 PR | MIN_PIVOT_DIST 10→20 修正 + 3 cores 測試對應更新 + macd_core header 更新 |

### Production 影響

修前 pivot 版觸發率 2–6/yr 🟢;修後 MIN_PIVOT_DIST=20 預期更稀疏,落 1–4/yr 🟢
(背離本質應為稀有訊號,Murphy 1999 p.248 原文「RSI 最重要 但也最少見」)。
User 跑 `tw_cores run-all --write` + `p2_calibration_data.sql §2` 可驗 Divergence
EventKind 觸發率變化。

### 已知狀態(下次 session 起點)

- alembic head:`x3y4z5a6b7c8`(不變,本 session 0 migration)
- Rust workspace:24 crate / **172 tests passed** / 0 warnings
- spec alignment 全綠;0 已知偏離
- 下個 session:**P3 neely 22 條 R4-R7 + Diagonal sub_kind**(等 user m3Spec/neely_core.md)

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- 純常數值改變 + 註解補完
- 修正方向:更嚴格(更少 false positive),不會生新 false negative
- Rollback:單 commit `git revert` 即可

---

## v1.32 — P2 收尾：5 個 🟠 噪音 EventKind 修正(2026-05-12 continuation)

接 v1.31 divergence pivot rewrite + edge trigger 落地後，user 指示「P2處理光」。
本 session 完成剩餘 5 個 🟠 noise EventKind 全部修正。

### Commit(`a678383`，branch `claude/review-todo-items-t9bbt`)

| Core | 修法 | 常數 |
|---|---|---|
| `day_trading_core` | RatioExtremeHigh/Low level trigger → edge trigger(進入 zone 才觸發) | — |
| `kd_core` | GoldenCross/DeathCross 加最小間距 | `MIN_KD_CROSS_SPACING=10` |
| `ma_core` | MaBullishCross/BearishCross + MaGoldenCross/DeathCross 加最小間距 | `MIN_MA_CROSS_SPACING=10` |
| `macd_core` | HistogramZeroCross 加最小間距 | `MIN_ZERO_CROSS_SPACING=5` |
| `macd_core` | GoldenCross/DeathCross 加最小間距（一致性） | `MIN_MACD_CROSS_SPACING=10` |
| `p2_calibration_data.sql` | §1 + §2 institutional EventKind CASE + day_trading RatioExtremeHigh/Low 識別 | SQL only |

### 預期修後觸發率

| EventKind | 修前 | 預期修後 |
|---|---|---|
| day_trading RatioExtremeHigh/Low | ~31/yr 🔴 | ~2–5/yr 🟢（zone entry） |
| KD GoldenCross/DeathCross | ~17/yr 🟠 | ~8–12/yr 🟢 |
| MA BullishCross/BearishCross | ~11.5/yr 🟠 | ~6–9/yr 🟢 |
| MACD HistogramZeroCross | ~15/yr 🟠 | ~8–12/yr 🟢 |
| MACD GoldenCross/DeathCross | ~7.5/yr 🟢 | ~5–7/yr 🟢（防衛性間距） |

### 已知狀態（下次 session 起點）

- alembic head：`x3y4z5a6b7c8`（不變，本 session 0 migration）
- Rust workspace：24 crate / **172 tests passed** / 0 warnings（從 168 → 172，+4 新測試）
- 下個 session 主要任務：**P3 neely 22 條 R4-R7 + Diagonal sub_kind**（等 user m3Spec/neely_core.md）
- 建議 user 跑：`tw_cores run-all --write` 重跑受影響 5 cores + `p2_calibration_data.sql` 驗證觸發率

### 風險

🟢 低：
- 0 alembic / 0 Python / 0 collector.toml
- 所有修改都是加常數 + 狀態追蹤，不改邏輯語意
- Rollback：`git revert a678383` 即可

---

## v1.31 — P2 阻塞 5(c) 校準 + P2 阻塞 6 噪音修正 + P5 Divergence 算法重寫(2026-05-12)

接 v1.30 P1/A1 收尾後,本 session 完成 3 項:
1. **P2 阻塞 5(c)**:C 類常數 production data driven 校準 — ma_core scaling fn +
   foreign_holding 雙時間窗口 + 文獻註解修正 + p2_calibration_data.sql
2. **P2 阻塞 6**:4 個 🔴噪音 EventKind 根因修正 — institutional edge trigger +
   foreign_holding edge trigger + rolling z-score
3. **P5 Divergence 重寫**:RSI/KD/MACD 三核心 divergence 從固定 20-bar window
   改為 pivot-based swing-point detection(Murphy 1999 p.248 學術定義)

### Commits(6 個,branch `claude/review-todo-items-t9bbt`)

| Commit | 範圍 |
|---|---|
| `252b790` | P2 阻塞 5(c):ma_core ABOVE_MA_STREAK_MIN scaling fn + foreign_holding 雙時間窗口 + 文獻註解修正 + p2_calibration_data.sql(新增) |
| `576b7b2` | C-2 修正:ma_core scaling fn `period/8` → `period*3/2`(production data 校準) |
| `e17d68c` | hotfix:p2_calibration_data.sql `years_span` 加 `::numeric` cast |
| `61a809e` | p2_calibration_data.sql §3/§5 scaling fn hint 同步新公式 |
| `2b1cbc7` | P2 阻塞 6:institutional edge trigger + foreign_holding edge trigger + rolling z-score(+4 regression test) |
| `8d3288a` | fix(rsi/kd/macd):pivot-based divergence detection 算法重寫(+11 tests) |

### P2 阻塞 5(c) — C 類常數校準決策

| 常數 | 修前 | 修後 | 根據 |
|---|---|---|---|
| `ABOVE_MA_STREAK_MIN` (ma_core) | 固定 30 | `(period*3/2).min(30).max(5)` | production data:MA20 舊 30d = 0.59/yr 🟢,保持;MA5/10 比例縮放降噪 |
| `EXPANSION_LOOKBACK` (atr_core) | 10 | 14(v1.30 已改) | Wilder ATR period=14 對齊 |
| Divergence `DIV_MIN_BARS` | 20(固定間距算法) | 算法重寫(pivot-based) | 根因:算法錯,不是 threshold 問題 |
| `MILESTONE_LOOKBACK` (foreign_holding) | 60d 單窗口 | 60d(季) + 252d(年)雙窗口 | George & Hwang (2004) JF 52 週高點;讓 user 對比後選擇 |
| `LOOKBACK_FOR_Z=60` / `LARGE_TRANSACTION_Z=2.0` | level trigger | edge trigger(算法改) | Brown & Warner (1985) 事件研究:事件=狀態轉變 |
| `STREAK_MIN_DAYS=3` (rsi/kd/day_trading) | 不動 | 不動(§2 SQL 驗後再決定) | — |
| `SQUEEZE_STREAK_MIN=5` (bollinger) | 不動 | 不動(§2 SQL 驗後再決定) | — |
| `STREAK_MIN_WEEKS=8` (shareholder) | 不動 | 不動(MOP 2012 跨領域,但量足夠) | — |

### P2 阻塞 6 — 4 個 🔴噪音 EventKind 修法

| EventKind | 修前 | 預期修後 | 修法分類 |
|---|---|---|---|
| RSI/KD/MACD Divergence | 20–33/yr 🔴 | 2–6/yr 🟢 | 算法重寫(pivot-based) |
| institutional LargeNetBuy/Sell | 91.83/yr 🔴 | 6–12/yr 🟢 | edge trigger(`prev_z_abs` state) |
| foreign_holding LimitNearAlert | 50.06/yr 🔴 | 2–6/yr 🟢 | edge trigger(`was_near_limit` state) |
| foreign_holding SignificantSingleDayChange | 34.87/yr 🔴 | 10–15/yr 🟢 | rolling z-score(固定 0.5% → 個股 2σ) |

### P5 Divergence 算法重寫

新 `detect_divergences()` 函式在 rsi_core / kd_core / macd_core 各自獨立 copy
(對齊 §十四 零耦合原則):

```rust
// PIVOT_N=3 (Lucas & LeBeau 1992), MIN_PIVOT_DIST=10 (Murphy「20-60 intervals」下界)
// Bearish: 比較連續兩個 swing high — price 新高但 indicator 未創新高
// Bullish: 比較連續兩個 swing low  — price 新低但 indicator 未創新低
// confirm_date = pivot_idx + PIVOT_N (確認完成當天)
// ind.abs() < 1e-12 skip warmup zeros
```

### 已知狀態(下次 session 起點)

- alembic head:`x3y4z5a6b7c8`(不變,本 session 0 migration)
- Rust workspace:24 crate / **168 tests passed** / 0 warnings
- Production run(2026-05-12):1263 stocks / 539.8s / 0 errors
  - 5 個修改 core 都已寫入 facts(kd 370K / macd 322K / rsi 109K / institutional 873K / foreign_holding 433K)
- **待 user 驗**:跑 `psql $env:DATABASE_URL -f scripts/p2_calibration_data.sql > p2_after.txt`
  確認 §2 每股每年觸發次數降到目標範圍(2–12/yr 視 EventKind)
- 下個 session 主要任務:**P3 neely 22 條 R4-R7 + Diagonal sub_kind**(等 user m3Spec/neely_core.md)

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml
- `ForeignHoldingParams` breaking change(刪 `change_threshold_pct`,加兩 field)—
  預設值 `Default::default()` 不影響 existing code
- Divergence pivot 實作:測試覆蓋「bearish fires once」/ 「bullish fires once」/ 「monotone = 0」
- Rollback:2 個 commit `git revert` 即可

---

## v1.30 — A1 Bronze financial_statement PK fix + P1/P2 收尾(2026-05-11)

接 v1.29 PR-9a 落地後,本 session 連續推進「下個 session 動工清單」三項:
P1 阻塞 1(Silver `_per` + ROE/ROA 元值)、P2 阻塞 5(c)(atr_core
EXPANSION_LOOKBACK)、小缺漏 A/B(day_trading historical_high + docs 修)。
過程中揭露 A1 Bronze PK 衝突,連帶修。

### Commits(7 個,branch `claude/review-todo-items-t9bbt`)

| Commit | 範圍 |
|---|---|
| `a372879` | P1 阻塞 1:Silver `_per` suffix + ROE/ROA 重啟;P2-minor:day_trading historical_high;docs §2.2/§3.2/§3.4 |
| `23a39b7` | P2 阻塞 5(c) 分析 SQL(scripts/p2_eventkinds.sql)|
| `a5a54b1` | P2:atr_core EXPANSION_LOOKBACK 10→14 對齊 Wilder period |
| `b5d8ab5` | P1 follow-up:ROE/ROA TTM 4-quarter sum(Buffett 15% 是 annual,FinMind 給 quarterly)|
| `2f0cbf9` + `a2a9df3` + `4484196` | A1:Bronze `financial_statement` PK 從 origin_name 改 type + 2 hotfix(動態 constraint name + legacy_v2 索引名衝突)|

### A1 根因(本 session 揭露)

P1 Silver `_per` fix 雖正確,但 user 跑全市場後揭露 2330 RoeHigh 仍只在
2019-2020 觸發。Diagnostic:
- Bronze `financial_statement` PK `(market, stock_id, date, event_type, origin_name)`
- FinMind TaiwanStockBalanceSheet 同 origin_name(資產總額)回 2 row(`TotalAssets`
  元值 + `TotalAssets_per` %)→ PK 衝突
- 對 2330 / 2357 / 2836 三檔,`_per` 被最後寫入 → 元值消失
- 其他 1074+ 檔元值原本就 survive(FinMind 多數情況元值寫在後)

修法:新 PK 用 `type`(FinMind 英文科目代碼)取代 `origin_name`,兩者共存。
alembic `x3y4z5a6b7c8`。詳見 `docs/m3_cores_spec_pending.md §15`。

### 兩個 migration hotfix 揭露的 schema 細節

1. **PR #R3 ALTER TABLE RENAME 不會 rename constraint**(a2a9df3):
   - 既有部署 PK 仍叫 `financial_statement_tw_pkey`(原 PR #18.5 命名)
   - alembic 改用 DO $$ ... `pg_constraint` 查 conrelid + contype='p' 動態取名

2. **PR #R2 legacy_v2 表佔用 `financial_statement_pkey` 索引名**(4484196):
   - 原 `financial_statement` table 被 rename 成 `financial_statement_legacy_v2`,
     PK constraint/index 名仍是 `financial_statement_pkey`
   - 新表 ADD CONSTRAINT 撞名失敗
   - 修法:先 rename legacy 表 PK → `financial_statement_legacy_v2_pkey`(對齊
     table name 慣例),釋出 `financial_statement_pkey` 給新表

### 3 檔受影響股票全修

| 股票 | 修前 RoeHigh facts | 修後 |
|---|---|---|
| 2330 TSMC | 6(僅 2019-2020)| **16+**(2019-2025,ROE 23-31%) |
| 2357 華碩 | 0 | 7(2021-2025) |
| 2836 高雄銀 | 0 | 0(金融業 ROE < 15%,符合預期) |

### 本 session 順手收的維護

- **ROE/ROA TTM 改 4-quarter sum**(b5d8ab5):FinMind 給 quarterly net_income,
  Buffett 15% 是 annual ROE。公式 `series[i-3..=i].iter().sum() / equity * 100`,
  前 3 季 fallback `× 4`。加 regression test `ttm_roe_uses_four_quarter_sum`
- **atr_core EXPANSION_LOOKBACK 14**(a5a54b1):對齊 Wilder ATR period=14 語意
- **day_trading_core historical_high**(a372879):metadata 加 `historical_high: bool`
  (對齊 margin_core 3617d84 同款設計)
- **docs/m3_cores_spec_pending.md**:§2.2 RSI FailureSwing / §3.2 margin historical
  high / §3.4 foreign_holding LimitNearAlert 改 ✅ 已實作
- **0 cargo warnings**(本 session 末段清理):neely_core classifier 4 個 unused
  imports + chrono::NaiveDate + classify_5wave unused candidate(改 `_candidate`)
  + tw_stock_compute `FwdDailyPrice.stock_id` field 加 `#[allow(dead_code)]`

### 已知狀態(下次 session 起點)

- alembic head:`x3y4z5a6b7c8`
- Rust workspace:24 crate / **158 tests passed**(從 155 +1 TTM test +2 RSI FS tests)
  / **0 warnings**
- A1 全市場套用:3 檔已修,1074+ 檔元值原本就 survive,無需大規模重抓
- 剩餘 m3Spec writing / PR-3c neely R4-R7 / PR-9b Workflow toml 同 v1.29 收尾,
  不變(詳見 `docs/m3_cores_spec_pending.md §11 / §13`)

### 風險

🟢 低:
- A1 alembic migration 已 idempotent(DO $$ 動態查 PK name + legacy 表 rename)
- 全部 commits user 本機 verify pass
- Silver builder + Rust core 158 tests passed / 0 warnings
- Production data 限縮到 3 檔影響,backfill 成本 < 1 分鐘
- 後續若想全市場套用,SQL 一句找出 0 元值 stocks,流程同單股(見 §15 重新套用流程)

---

## v1.29 — M3 PR-9a tw_cores 全市場全核 dispatch(2026-05-09)

接 v1.28 spec-comply rewrite + CLAUDE.md 收緊「下次 session 待作事項」後,
user 拍版「走 pr9a」全市場 × 全 22 cores production run。0 alembic、0 Python
邏輯、0 collector.toml,純 Rust(只動 `tw_cores/src/main.rs`)。

### 範圍

| 項目 | 內容 |
|---|---|
| `tw_cores` 加 `run-all` subcommand | 5 args:`--stocks` / `--limit` / `--timeframe` / `--skip-market` / `--skip-stock` / `--write` |
| 22 cores 硬編碼 dispatch | 5 environment market-level + 17 stock-level(1 Wave + 8 Indicator + 5 Chip + 3 Fundamental)|
| 寫表分流 | Wave (neely) → `structural_snapshots`;其他 21 cores → `indicator_values`(JSONB,本 PR 補 INSERT path);全部 → `facts`(per event,UPSERT ON CONFLICT DO NOTHING)|
| `params_hash` 真實 blake3 | 用 `fact_schema::params_hash()` 算各 core Params,寫進三表(對齊 cores_overview §7.4)|
| Stock list pull | `SELECT DISTINCT stock_id FROM price_daily_fwd WHERE market='TW' ORDER BY stock_id`,對齊 `silver/orchestrator._fetch_dirty_fwd_stocks` pattern |
| Per-core / per-stock 失敗不阻塞 batch | match arm 內 loader/compute err 走 `loader_err_summary` / `CoreRunSummary::err`,印 summary table 列出 |
| Output JSON metadata 抽取 | `extract_indicator_meta(output_json)` 從 `output.stock_id` / `output.timeframe` / `series[-1].date` 拿,ma_core 例外從 `series_by_spec[0].series` fallback |

### Generic dispatch helper(避免 600+ line 重複)

`dispatch_indicator<C: IndicatorCore>(pool, &core, &input, params, write)` 一行包:
compute → produce_facts → write_indicator_value + write_facts → return CoreRunSummary。
ma_core / shareholder_core 等 series shape 不同的 core 都 work,因為走 JSON-based
metadata 抽取(不直接拿 Output 欄位)。

22 core 各自的 loader call + Params::default() 構造仍寫 explicit match arm
(對齊 V2「禁止抽象」原則,§十四),但 dispatch+寫入邏輯共用 generic helper,
總 line count ~700 line(對比硬編碼 22 個獨立 fn 的 ~1400 line)。

### 關鍵設計決策

1. **series 整段 serialize 進單 row**(`(stock_id, value_date=last_date,
   timeframe, source_core, params_hash)`):避免 17M row 爆量,1700 stocks ×
   21 indicator-class cores ≈ 35K rows / `indicator_values`;query 跨日走
   JSONB array index
2. **不引入 ErasedCore trait** — 22 個 match arm 重複但可讀,新 core 上線只
   要加 1 個 arm。對齊 cores_overview §四「禁止抽象」+ §十四「P3 後考慮,V2
   不規劃」
3. **串列跑** — 對齊 v1.16 PostgresWriter thread-safety 限制(2 max_connections);
   並行優化(per-stock task spawn 共用 pool)留 PR-9b
4. **Workflow toml 不在本 PR** — orchestrator dispatch 留 PR-9b;本 PR 走
   hardcoded 全 22 cores
5. **`run` 既有 path 不動** — neely 單核單股(PR-7 落地)行為對齊,既有 user
   workflow 不受影響

### 驗證(沙箱已通)

```bash
cd rust_compute && cargo build --release -p tw_cores      # 1m 29s,0 errors
cargo test --workspace --release --no-fail-fast            # 145 passed / 0 failed
./target/release/tw_cores list-cores                       # 22 cores 全列出
./target/release/tw_cores run-all --help                   # 5 args 解析正確
```

### user 本機驗證流程(三階段)

```powershell
git pull
# 不需 alembic upgrade(本 PR 0 migration)
cd rust_compute && cargo build --release -p tw_cores

# Stage 1:dry-run smoke(先看 5 stocks 跑得通,~30 秒)
$env:DATABASE_URL = "postgresql://twstock:twstock@localhost:5432/twstock"
.\target\release\tw_cores.exe run-all --limit 5
# 預期:印 5 environment cores + 5 stocks × 17 stock-level cores = 90 條 summary
#       per-core elapsed_ms / events / status

# Stage 2:小範圍 write(P0 Gate 5 stocks)
.\target\release\tw_cores.exe run-all --stocks 0050,2330,3363,6547,1312 --write
psql $env:DATABASE_URL -c "SELECT source_core, COUNT(*) FROM indicator_values GROUP BY source_core ORDER BY 1"
psql $env:DATABASE_URL -c "SELECT source_core, COUNT(*) FROM facts GROUP BY source_core ORDER BY 1"
psql $env:DATABASE_URL -c "SELECT core_name, COUNT(*) FROM structural_snapshots GROUP BY 1"
# 預期:
#   indicator_values:21 source_core(5 environment + 16 stock-level)各 1 ~ 5 rows
#   structural_snapshots:1 core (neely_core) × 5 stocks = 5 rows
#   facts:不定數量(看 best-guess threshold 觸發頻率)

# Stage 3:全市場 production(預估 ~30 分鐘 串列)
.\target\release\tw_cores.exe run-all --write
# 預期:1700 stocks × 17 stock-level + 5 environment = ~28905 indicator_values rows
#       1700 structural_snapshots rows
#       facts:預估數萬~數十萬(視各 core threshold 觸發)
```

### 留 PR-9b(下個 session)

- **Workflow toml dispatch**:讀 `workflows/tw_stock_standard.toml` 動態決定
  跑哪些 cores(目前硬編碼全 22 cores)
- **sqlx pool 並行** per-stock(需從 `max_connections=2` 升 16 + per-stock
  task spawn,~10x 加速)
- **incremental dirty queue** 模式:只跑 `is_dirty=TRUE` 的 stock(目前全跑)
- **best-guess threshold 校準**:user 跑 Stage 3 後 visual review facts,
  feedback 進 m3Spec/ 後續 PR 改各 core thresholds 重跑

### 風險

🟢 低:
- 純 Rust,0 alembic / 0 Python / 0 collector.toml
- 既有 `tw_cores run --stock-id` neely path 完全不動
- Rollback:單 commit `git revert` 即可
- 沙箱 cargo build + cargo test + list-cores + run-all --help 全綠
- best-guess threshold 跑出來的 facts 多/少需校準 — user 拿 production data
  visual review 後寫進 m3Spec/

### user 本機 verify 過程 + 4 個 hotfix(2026-05-09 同 session)

User 跑 stage 1 → stage 3 揭露 4 個 issue,同 session 全修:

| commit | 範圍 |
|---|---|
| **2c0decc** | clap subcommand explicit `#[command(name = "...")]` annotation(PowerShell + Windows binary 雙端 kebab-case 命名一致)|
| **c37783e** | 4 個 loaders SQL 加 `::float8` cast 對齊 PG NUMERIC schema(price_*_fwd OHLCV / taiex / us_market / exchange_rate / market_margin / fear_greed / day_trading_ratio / foreign_holding_ratio / valuation per/pbr/yield/mvw / monthly_revenue revenue_yoy/mom)|
| **c37783e** | fear_greed_index `score` 欄 alias 成 `value`(PG 表欄名是 score,Rust struct field 是 value 對齊 spec §6.5)|
| **(本 commit)** | margin_core NULL skip:`MarginPoint.margin_maintenance` Option<f64>;compute() 對 margin_balance / short_balance 任一 NULL 整 row skip(避免 unwrap_or(0) → 「Margin balance down 100% on 假日」false positive)+ regression unit test |

### user 本機 stage 1-3 production run(已驗收)

| Stage | 範圍 | 結果 |
|---|---|---|
| 1 | dry-run 5 stocks(00632R/00673R/00674R/00676R/1101)| 22 cores 全綠 / 3.2 秒 / 0 error |
| 2 | P0 Gate 5 檔(0050/2330/3363/6547/1312)`--write` | 13.7 秒 / indicator_values 85 row / structural_snapshots 5 row / facts ~28K row |
| 3 | dev DB 全市場 30 stocks `--write` | 87.6 秒 / 22 cores × 30 = 660 個 compute() / facts 全部 ON CONFLICT DO NOTHING dedup,facts 表 ~140K 累計 |

dev DB 只有 30 distinct stocks(`SELECT COUNT(DISTINCT stock_id) FROM price_daily_fwd WHERE market='TW'`)— 對齊 v1.10 partial backfill,不是 SQL bug。**production scale 1700 stocks 等 user backfill 補齊後可直接跑同樣命令**(預估 ~80 分鐘串列;PR-9b 並行可降到 ~10 分鐘)。

### 3 個 known limitation 留 m3Spec/ 校準(stage 3 spot check 揭露)

1. **`shareholder_core` + `financial_statement_core` events = 0**:best-guess threshold / detail JSONB key 命名(英文假設不對齊真實 Bronze 欄名),需 user 寫 m3Spec/{chip,fundamental}_cores.md 完整版校準 — **2026-05-10 Round 1+2 fix 解決**(見下方)
2. **`neely_core` 22 條 Stage 4 規則 deferred**:`Wave structure: ... rules passed = 0, deferred = 22`(對齊 v1.28 PR-3b R1-R3 完整 + R4-R7/F/Z/T/W 22 條 Deferred),等 user 寫 m3Spec/neely_core.md 完整版後 PR-3c 補
3. **`0050` + `6547` snapshot_date = 1900-01-01 / forest_size = 0**:dev DB `price_daily_fwd` 沒這 2 stocks 的料(neely loader 載到空 series → compute() 仍 OK 但 forest 0,data_range fallback 1900-01-01 sentinel),不是 bug,user 補 backfill 即可

### Round 1-4 + PR-9b/c/d 後續(2026-05-10 同 session 連續推進)

接 v1.29 PR-9a 1263 stocks production run 收尾後,user 拍版往兩條路推:
**(A) 量級降量 + 加速 production**(PR-9b/c/d + Round 4)+ **(B) 對齊真實
schema 解 0-events 問題**(Round 1/2)。同 session 全推完。

| Commit | 範圍 | wall time / facts |
|---|---|---|
| **e368fc8 Round 1** | financial_statement / shareholder detail JSONB key 改中文 + 17 levels iterate | 解 2 cores 永久 0 events(從 0 → 27880 financial / 106157 shareholder) |
| **7eb9a96 Round 2** | financial_statement 全形括號 fallback chain + balance type 是 % common-size 處理(ROE/ROA 永久 0 避免 false positive) | 修 RoeHigh value=8478 億 false positive;GrossMarginRising/Falling/EpsTurn 等 5 EventKind 觸發 |
| **615a8eb PR-9b** | tw_cores Stage B `for_each_concurrent` 並行(default 16);`max_connections` 對齊 | 1263 stocks:**3666s → 1388s(↓62%)** |
| **d2e0594 PR-9c** | `write_facts` UNNEST array batch INSERT(取代 per-event loop;BATCH_SIZE=4000) | 1263 stocks:**1388s → 554s(↓60%)** |
| **91804df Round 4** | valuation/margin/bollinger 3 cores stay-in-zone EventKind 改 EnteredX/ExitedX transition pattern(對齊 fear_greed 範本)| facts:**9M → 4.4M(↓51%)**;valuation 2.0M → 189K(↓91%);margin 1.3M → 605K(↓54%);bollinger 466K → 457K(bouncy 本質,僅 ↓2%) |
| **ef855b8 PR-9d** | concurrency default 16 → 32 + `max_connections` 升 36 | wall time **不變**(PG IO 已是 hard ceiling;`per-core elapsed_s` 升但 contention 抵消) |

User 拍版核心原則:**「去耦合 + 減少抽象 + 重工 OK」**(對齊 cores_overview §四 / §十四)。

### Production verify milestone

| Stage | 配置 | wall time | facts 量級 |
|---|---|---|---|
| Stage 5(初始)| 串列 concurrency=1 | 3666s = 61min | ~9M(stay-in-zone)|
| PR-9b | concurrency=16 | 1388s = 23min | 同上 |
| PR-9c batch INSERT | concurrency=16 | 554s = 9.2min | 同上 |
| Round 4 transition | concurrency=16 | 535s = 8.9min | **4.4M(↓51%)**|
| PR-9d concurrency 32 | concurrency=32 | 535s | 同上 |

**起點 → 收尾比**:**~7× wall time 加速 + 51% facts 降量**

### 9 個阻塞點拍版收尾(2026-05-10 同 session)

對齊 production 1263 stocks × 22 cores × 4.4M facts state,user 拍版 9 個阻塞:

| # | 阻塞 | 狀態 | commit |
|---|---|---|---|
| 1 | `financial_statement_core` Silver builder origin_name 元值/% 覆蓋 bug | 🔴 **留下個 session** | n/a(P1 動工項)|
| 2 | `shareholder_core` 4-level (50/400/1000 張) + STREAK 8 + concentration unit-based | ✅ **完成** | 458a45a |
| 3 | `neely_core` 22 條 R4-R7/F/Z/T/W deferred | 🟡 跳過(user 拍「先跳過」) | n/a |
| 4 | Round 4 EnteredX/ExitedX bouncy 防衛 | ✅ **不動**(user 拍「不動」) | n/a |
| 5 | 100 個 threshold 校準 | ✅ **a+b 加 reference 註解 / c 留下個 session / d 不動** | 本 commit |
| 6 | `Timeframe::Quarterly` variant 加 | ✅ **完成** | 458a45a |
| 7 | `foreign_holding_core` foreign_limit_pct 從 detail JSONB 取(無需 alembic)| ✅ **完成** | 458a45a |
| 8 | `rsi_core` FailureSwing 4-step 邏輯(Wilder 1978 §7) | ✅ **完成** | 458a45a |
| 9 | `Diagonal` Leading vs Ending sub_kind | 🟡 跳過(user 拍「等 NEELY」) | n/a |

### 100 const reference 註解收尾(2026-05-10)

10 cores 加 `Reference(2026-05-10 加)` doc 註解,標明每個 const 的學術 / 監管出處:

| Core | 主要 reference |
|---|---|
| atr_core / rsi_core / adx_core | Wilder, J. Welles Jr. (1978). "New Concepts in Technical Trading Systems" Ch. 21 |
| macd_core | Appel, Gerald (1979). "The Moving Average Convergence Divergence Method" |
| bollinger_core | Bollinger, John (2002). "Bollinger on Bollinger Bands". McGraw-Hill |
| kd_core | Lane (1957) 原版 14;period=9 為 Asian convention(無 explicit 學術) |
| margin_core / market_margin_core | 證交所《有價證券借貸辦法》§39(維持率 145/130) |
| us_market_core | Whaley, R. E. (2000). "The Investor Fear Gauge". *Journal of Portfolio Management* 26(3), 12-17(VIX zone) |
| valuation_core | Graham, Benjamin (1949). "The Intelligent Investor" Ch. 14(yield 5%) |
| financial_statement_core | Buffett (1987) Berkshire letter + Cunningham (1997) The Essays of Warren Buffett(ROE 15%) |
| shareholder_core | Money 錢雜誌 50/400 + 凱基/集保 1000 張 + Moskowitz/Ooi/Pedersen (2012) JFE(streak 8) |

純註解,**不改 const value**(對齊 user 「進階資料不應動 const 預設值,先標 source 後 production data driven 校準」原則)。

### 已知狀態(下次 session 起點)

- alembic head:`w2x3y4z5a6b7`(user 已落地)
- Rust workspace:24 crate,0 errors / **155 tests passed**(對比 153 + shareholder synth + rsi bearish FS + rsi bullish FS - 1 = 155)
- production state:**4.4M facts / 9.2 分鐘 wall time / 1263 stocks(production scale 上限,340 stocks 是 empty 已退市)**
- 9 個阻塞:**4 動工完成 + 2 跳過(NEELY relate)+ 2 留下個 session(P1 阻塞 1 / P2 阻塞 5c)+ 1 完成 a+b**
- m3Spec/ 仍待寫:詳見 `docs/m3_cores_spec_pending.md`(14 段 spec writing + 9 阻塞拍版紀錄)
- 下個 session 動工清單(對齊 docs §13 + §14):
  1. **P1 阻塞 1**:`financial_statement_core` Silver builder `_per` suffix 修法
     + Rust ROE/ROA 改元值 keys + silver phase 7b full-rebuild + tw_cores 重跑(~2 小時)
  2. **P2 阻塞 5(c)**:production data driven 統計各 streak/lookback const 觸發率
     + user 拍版動態值(~半天)
  3. **P3 阻塞 3 / 9**:neely 22 條 R4-R7 + Diagonal sub_kind(等 user m3Spec/neely_core.md
     或 best-guess Frost-Prechter batch 補,~1-2 天)

---

## v1.28 — M3 Cores 動工(PR-1 → PR-CC1,2026-05-09 同 session 推到極限)

接 v1.27 m2 大重構 + Bronze 質量修收尾後,M3 Cores 階段正式動工。User 在
同 session 連續指示「繼續」/「先實踐以後再改」/「前進到不能前進為止,
然後繼續開其他 cores 實踐」,推到 9 段 PR + 第 1 個 chip core:

| PR | 範圍 |
|---|---|
| **PR-1** | Rust workspace 重構 + cores_shared/fact_schema + neely_core skeleton + tw_cores binary stub |
| **PR-2** | neely_core Stage 1-2:Pure Close monowave detector + Wilder ATR + Rule of Neutrality + Rule of Proportion(神股 ATR 比例 / 加權指數 0.5%)|
| **PR-3a** | neely_core Stage 3:Bottom-up Candidate Generator(wave_count ∈ {3,5} 滑窗 + alternation filter + beam_width × 10 上限)|
| **PR-3b** | neely_core Stage 4:Validator framework + R1/R2/R3 完整實作 + R4-R7/F/Z/T/W 22 條 Deferred(等 user m3Spec/ 寫完後 batch 補)|
| **PR-4** | neely_core Stage 5-7:Classifier(Impulse/Diagonal/Zigzag 基本)+ Post-Validator skeleton + Complexity Rule(差距 ≤ 1 級篩選)|
| **PR-5** | neely_core Stage 8:Compaction 簡化 pass-through + Forest 上限保護(BeamSearchFallback by power_rating)|
| **PR-6** | neely_core Stage 9-10:Missing Wave / Emulation skeleton + Power Rating 查表 + Fibonacci NEELY_FIB_RATIOS 寫死 + Triggers + facts.rs produce_facts |
| **PR-7** | alembic w2x3y4z5a6b7 落 indicator_values / structural_snapshots / facts 三表 + cores_shared/ohlcv_loader 讀 Silver price_*_fwd + tw_cores binary 接 PG(`run --stock-id 2330 --write`)|
| **PR-8** | cores_shared/core_registry inventory + neely_core 註冊 + workflows/tw_stock_standard.toml 範例 |
| **PR-CC1** | 第 1 個 chip core:day_trading_core(完整實作)+ chip_loader/DayTradingSeries + inventory 註冊(P2)|
| **PR-batch** | 剩餘 19 cores 極限推進一次到位(user「不確定的部分直接上 todo,後續一併討論一併檢討測試」):chip 4(institutional/margin/foreign_holding/shareholder)+ fundamental 3(revenue/valuation/financial_statement)+ environment 5(taiex/us_market/exchange_rate/fear_greed/market_margin)+ indicator 8(macd/rsi/kd/adx/ma/bollinger/atr/obv)+ 3 個 loaders(fundamental_loader / environment_loader / chip_loader 擴 4 種 Series)|

0 collector.toml、0 Python 邏輯改變(只 sync `rust_bridge.py` stale-check path)。
1 個 alembic migration(PR-7,三表新增)。

`alembic head:v1w2x3y4z5a6 → w2x3y4z5a6b7`(PR-7 落地時)。
`workspace 23 crate;cargo test 143 tests 全綠 0 failed;22 cores 全部 inventory 註冊`

### Spec 來源 — 暫時對 oldm2Spec/

`m3Spec/` 目前只有 user 既有的 `chip_cores.md`,其他 Cores spec(neely / fundamental
/ environment / indicator / cores_overview)**仍在 `m2Spec/oldm2Spec/` r2**。
Rust code 對 spec 的 `// 對齊 ...` 註解暫時 ref `m2Spec/oldm2Spec/`,等 user
逐份在 m3Spec/ 落最新版後,再批次同步 ref。**本 PR 不複製 spec 進 m3Spec/** —
那會凍結 r2 為 m3 版本,影響 user 寫最新 spec 的自由度。

### PR-1 範圍

| 項目 | 內容 |
|---|---|
| Rust workspace | `rust_compute/Cargo.toml` 從 [package] 改 [workspace] virtual root,既有 `tw_stock_compute` binary 搬進 member `silver_s1_adjustment/`(name + binary 名仍 `tw_stock_compute`,Python 端 path 不動)|
| 新 crate 1 | `cores_shared/fact_schema/` — `Fact` struct + `IndicatorCore` / `WaveCore` trait + `Timeframe` enum + `params_hash()`(blake3 + canonical JSON,對齊 cores_overview §7.4)|
| 新 crate 2 | `cores/wave/neely_core/` — WaveCore trait impl skeleton + 14 sub-modules(monowave / candidates / validator / classifier / post_validator / complexity / compaction / missing_wave / emulation / power_rating / fibonacci / triggers / degree / facts)+ `config.rs`(NeelyCoreParams + NeelyEngineConfig + OverflowStrategy)+ `output.rs`(完整 Scenario Forest 合約)|
| 新 binary | `cores/system/tw_cores/` — Cores 層 Monolithic Binary 入口(對齊 cores_overview §五)|
| Python sync | `src/rust_bridge.py:_check_binary_freshness` main.rs path 從 `rust_compute/src/main.rs` 改 `rust_compute/silver_s1_adjustment/src/main.rs`;4 處 `cargo build --release` hint 加 `-p tw_stock_compute`(避免 user 重編整個 workspace)|

### PR-2 範圍 — neely_core Stage 1-2

| 子模組 | 實作 |
|---|---|
| `monowave/pure_close.rs` | Wilder ATR(period)序列 + close-reversal monowave detector;反向 movement < 0.5 ATR 視為噪音不算反轉。寫死常數對齊 §4.4 / §6.6 |
| `monowave/neutrality.rs` | Rule of Neutrality:個股 `|magnitude| < ATR * 1.0` → Neutral;加權指數(`stock_id == "_index_taiex_"`)`|magnitude| / start_price * 100 < neutral_threshold_taiex` → Neutral(對齊 §10.4.1)|
| `monowave/proportion.rs` | Rule of Proportion metrics:magnitude / duration_bars / atr_relative / slope_vs_45deg。45° 參照「1 ATR/bar」寫死 |
| `monowave/mod.rs` | `classify_monowaves(bars, monowaves, stock_id, cfg)` 入口 + `ClassifiedMonowave` struct |
| `lib.rs::compute()` | 從 `unimplemented!` 改 partial impl:跑 Stage 1+2 → 回 NeelyCoreOutput,scenario_forest 暫空(Stage 8 才產出),monowave_series 已填,diagnostics.stage_elapsed_ms 含 stage_1_monowave / stage_2_classify;version bump 0.1.0 → 0.2.0 |
| `tw_cores` binary | banner 從「skeleton」改「skeleton + Stage 1-2 partial」並列出已實作 stage 與待做 stage |

### PR-3a 範圍 — neely_core Stage 3 Bottom-up Candidate Generator

| 子模組 | 實作 |
|---|---|
| `candidates/generator.rs` | `WaveCandidate` struct(id / monowave_indices / wave_count / initial_direction)+ `generate_candidates(classified, cfg)`:過濾 Neutral → 對 wave_count ∈ {3, 5} 滑動取窗 → 視窗 direction 必須交替 → push candidate;`BEAM_CAP_MULTIPLIER = 10`(候選上限 = `cfg.beam_width * 10`)|
| `candidates/mod.rs` | expose `generate_candidates` + `WaveCandidate` |
| `output.rs` | `MonowaveDirection` 加 `PartialEq + Eq` derive(generator 內部 direction 比對需要)|
| `lib.rs::compute()` | 加 Stage 3 dispatch + `diagnostics.candidate_count` + `stage_3_candidates` 耗時 key;version bump 0.2.0 → 0.3.0;test 加 `candidate_count == 1`(U-D-U 3 monowave 應生 1 個 candidate)|
| `tw_cores` binary | banner 「Stage 1-2 partial」改「Stage 1-3 partial」+ Stage 3 ✅ 進度條 |

**Stage 3 範圍邊界**:
- ✅ 純 enumeration:窮舉「可能是 wave structure 的視窗」(交替 Up/Down)
- ✅ 過濾 Neutral monowaves(Stage 2 已標)
- ✅ beam_width × 10 上限保護(避免 Stage 4 跑爆)
- ❌ **不**判 pattern_type(Impulse / Zigzag / ...)— 那是 Stage 5 Classifier
- ❌ **不**檢查 R1-R7 規則 — 那是 Stage 4 Validator
- ❌ **不**做 5-wave-of-3 嵌套(Combination 類型)— 留後續 PR
- ❌ **不**用 ProportionMetrics 預先剔除明顯不對稱視窗 — 留 P0 Gate 校準

**Stage 4 留 PR-3b**(25 條 validator 規則 R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2):
spec §10.1 寫「每條規則的具體內容(門檻、容差、書頁)在 P0 開發時逐條建檔於
`validator/*.rs` 註解中,並對照 Neely 書頁」。具體規則細節 oldm2Spec/neely_core.md
未完整列出,**等 user 在 m3Spec/ 寫最新 neely 版本後再開 PR-3b** — 避免 best-guess
的規則邏輯跟 user 後續 spec 衝突。

### PR-3b 範圍 — neely_core Stage 4 Validator Framework + R1-R3

> ⚠️ **user 指示「先實踐以後再改」**:m3Spec/ user 還沒寫最新 neely_core spec,
> 但仍動工 PR-3b 把框架搭起來。R1/R2/R3 是 Elliott Wave 教科書通用規則(跨派
> 系一致性高,Neely 派與 Frost-Prechter 派差異小),先 best-guess 實作。
> R4-R7 + F/Z/T/W 22 條規則全部 Deferred 留 PR-3c,等 user 在 m3Spec/ 寫最新
> spec 後 batch 補。

| 子模組 | 實作 |
|---|---|
| `validator/mod.rs` | `RuleResult` enum(Pass / Fail / Deferred / NotApplicable)+ `ValidationReport`(passed / failed / deferred / not_applicable / overall_pass)+ `validate_candidate` dispatcher + `validate_all` 批次入口 |
| `validator/core_rules.rs` | **R1 完整實作**(W2 不可完全回測 W1)+ **R2 完整實作**(W3 不可是 W1/W3/W5 中最短)+ **R3 完整實作**(W4 不可重疊 W1 區間,strict Impulse 版本,Diagonal 例外留 PR-4 Post-Validator)+ R4-R7 Deferred stub。11 unit test |
| `validator/flat_rules.rs` | F1-F2 全 Deferred + 1 unit test |
| `validator/zigzag_rules.rs` | Z1-Z4 全 Deferred + 1 unit test |
| `validator/triangle_rules.rs` | T1-T10 全 Deferred + 1 unit test |
| `validator/wave_rules.rs` | W1-W2 全 Deferred + 1 unit test |
| `lib.rs::compute()` | 加 Stage 4 dispatch + `diagnostics.validator_pass_count` / `validator_reject_count` / `rejections` 列表 + `stage_4_validator` 耗時 key;version bump 0.3.0 → 0.4.0 |
| `tw_cores` binary | banner 「Stage 1-3 partial」改「Stage 1-4 partial」+ Stage 4 🟡(部分實作標記) |

**R1-R3 best-guess 實作邏輯**(對齊 Elliott Wave 通用規則,**非嚴格 Neely 派系**):
- **R1**:W2 endpoint 不可跨過 W1 起點(上漲 W1 → W2.end >= W1.start;下跌反之)
- **R2**:wave_count == 5 時,W3 magnitude >= min(W1, W5) magnitude(W3 不是最短)
- **R3**:wave_count == 5 時,W4 終點不在 W1 區間內(strict Impulse;Diagonal 允許重疊,留 Post-Validator)

每條 Fail 紀錄 `RuleRejection`:`rule_id` / `expected` / `actual` / `gap%`(以 W1 magnitude 為基準的偏離百分比)/ `neely_page`(目前標「Elliott Wave 通用規則(具體 Neely 書頁待 m3Spec/ 校準)」)。

**Stage 4 範圍邊界**:
- ✅ Validator framework(RuleResult / ValidationReport / dispatcher)
- ✅ R1/R2/R3 完整實作 + 規則 fail 時記 RuleRejection(對齊 §18.1 拒絕原因紀錄)
- ✅ R4-R7 + F1-F2 + Z1-Z4 + T1-T10 + W1-W2 共 22 條 Deferred(框架 expose RuleId,規則邏輯留空)
- ❌ Diagonal exception for R3(留 PR-4 Post-Validator)
- ❌ Pattern-specific dispatch(目前對所有 candidate 跑全部 25 條,N/A 自動標,留 PR-4 Classifier 後做 dispatch 優化)
- ❌ Deferred rule 暫時通過機制(對齊 §10.3,目前 Deferred 不阻塞 overall_pass,但未來規則上線後會變嚴格)

**Stage 4 留 PR-3c**:R4-R7 + F1-F2 + Z1-Z4 + T1-T10 + W1-W2 共 22 條規則的具體
門檻 + Neely 書頁追溯,**等 user 在 m3Spec/ 寫最新 neely_core spec 後 batch 補**。

### Workspace 後新長相

```
rust_compute/
├── Cargo.toml                            # virtual workspace + workspace.dependencies
├── target/release/{tw_stock_compute,tw_cores}   # 雙 binary
├── silver_s1_adjustment/                 # 既有 Silver S1 後復權 binary
│   ├── Cargo.toml                        # name = "tw_stock_compute"(對齊 Python rust_bridge.py)
│   └── src/main.rs                       # 638 行,內容 verbatim from PR-1 之前
├── cores_shared/
│   └── fact_schema/                      # IndicatorCore / WaveCore trait + Fact + params_hash
│       ├── Cargo.toml
│       └── src/lib.rs                    # 含 2 unit test(params_hash + Timeframe)
└── cores/
    ├── system/tw_cores/                  # Monolithic Binary 入口
    │   ├── Cargo.toml
    │   └── src/main.rs                   # CLI + 印 linked cores + Stage 進度條
    └── wave/neely_core/                  # P0 Wave Core
        ├── Cargo.toml
        └── src/
            ├── lib.rs                    # NeelyCore + WaveCore trait impl + 4 unit test
            ├── config.rs                 # NeelyCoreParams + NeelyEngineConfig + 1 unit test
            ├── output.rs                 # Scenario Forest 合約(§八 §九 §十 完整定義)
            ├── facts.rs                  # 留 PR-6:Fact 產出規則
            ├── monowave/                 # ✅ PR-2 Stage 1-2 完整實作
            │   ├── mod.rs                #     classify_monowaves + ClassifiedMonowave + 3 test
            │   ├── pure_close.rs         #     Wilder ATR + monowave detector + 7 test
            │   ├── neutrality.rs         #     Rule of Neutrality + 7 test
            │   └── proportion.rs         #     Rule of Proportion + 6 test
            ├── candidates/                # ✅ PR-3a Stage 3 Bottom-up Candidate Generator
            │   ├── mod.rs                #     expose generate_candidates + WaveCandidate
            │   └── generator.rs          #     wave_count ∈ {3,5} 滑窗 + alternation filter + 8 test
            ├── validator/                 # 🟡 PR-3b Stage 4 framework + R1/R2/R3 完整(22 條 Deferred)
            │   ├── mod.rs                #     RuleResult / ValidationReport / validate_all + 2 test
            │   ├── core_rules.rs         #     R1/R2/R3 完整 + R4-R7 Deferred + 11 test
            │   ├── flat_rules.rs         #     F1-F2 Deferred + 1 test
            │   ├── zigzag_rules.rs       #     Z1-Z4 Deferred + 1 test
            │   ├── triangle_rules.rs     #     T1-T10 Deferred + 1 test
            │   └── wave_rules.rs         #     W1-W2 Deferred + 1 test
            ├── classifier/mod.rs         # Stage 5 留 PR-4
            ├── post_validator/mod.rs     # Stage 6 留 PR-4
            ├── complexity/mod.rs         # Stage 7 留 PR-4
            ├── compaction/mod.rs         # Stage 8(exhaustive + beam_search 子檔留 PR-5)
            ├── missing_wave/mod.rs       # Stage 9a 留 PR-6
            ├── emulation/mod.rs          # Stage 9b 留 PR-6
            ├── power_rating/{mod.rs,table.rs}    # Stage 10a 查表留 PR-6
            ├── fibonacci/mod.rs          # Stage 10b(ratios + projection 子檔留 PR-6)
            ├── triggers/mod.rs           # Stage 10c 留 PR-6
            └── degree/mod.rs             # Degree 詞彙留 PR-6
```

### 為什麼第 1 個實作的 Wave Core 選 neely 而非簡單 chip core

User 選擇 P0(對齊 cores_overview §九 開發優先級),理由:
- P0 是 Gate(完成 + 五檔股票實測校準後才能進 P1)
- WaveCore trait 與 IndicatorCore trait 簽章不同(Output 是 Scenario Forest),
  P0 結束前 trait 簽章可能微調,先把這層落地避免後續返工
- skeleton 階段只落 struct 合約 + trait 簽章,不寫 compute() 內部 Stage 1-10
  Pipeline,風險可控

### 編譯 + 測試驗證(沙箱已通)

```bash
cd rust_compute
cargo build --workspace                      # 4 crate 全綠(首次 ~56s,後續 incremental ~1.5s)
cargo test --workspace                       # 56/56 unit test 全綠(neely_core 54 + fact_schema 2)
                                             #   fact_schema: 2 (params_hash + Timeframe)
                                             #   neely_core:  54
                                             #     - config:               1
                                             #     - pure_close:           8 (ATR 暖機 / 反轉偵測 / 噪音過濾)
                                             #     - neutrality:           7 (個股 ATR 比例 vs 加權指數 % 比較)
                                             #     - proportion:           6 (45° 線 / 巨大斜率 / 邊界 0)
                                             #     - monowave/mod:         3 (end-to-end zigzag / TAIEX neutral)
                                             #     - candidates:           8 (alternation / Neutral 過濾 / beam cap)
                                             #     - validator/mod:        2 (5-wave Up impulse pass / batch process)
                                             #     - validator/core_rules: 11 (R1×4 / R2×3 / R3×3 / R4-R7 stub×1)
                                             #     - validator/flat:       1 (all Deferred)
                                             #     - validator/zigzag:     1 (all Deferred)
                                             #     - validator/triangle:   1 (all Deferred)
                                             #     - validator/wave:       1 (all Deferred)
                                             #     - lib(compute):         4 (warmup + version + 2 partial compute w/ Stage 4)
target/release/tw_cores                      # 印 linked cores 列表 + Stage 進度條
target/release/tw_stock_compute --help       # 對齊 PR-1 之前(0 行為改變)
```

### user 本機驗證流程

```powershell
git pull
# 不需 alembic upgrade(本版 0 migration)
cd rust_compute
cargo build --release --workspace
# 預期:既有 tw_stock_compute(.exe)仍在 target/release/,Python 端 silver phase 7c 跑不變
cargo test --workspace
# 預期:56/56 unit test 全綠(neely_core 54 + fact_schema 2)

# 跑既有 silver phase 7c 確認 Phase 4 後復權仍 OK(Rust binary path 不變)
python src/main.py silver phase 7c
# 預期:行為對齊 v1.27,無 stale warning,無 binary 不存在 error

# smoke run M3 cores binary
target/release/tw_cores
# 預期輸出:M3 cores binary(skeleton + Stage 1-4 partial)+ neely_core v0.4.0 + Stage 進度條(Stage 1/2/3 ✅,Stage 4 🟡)
```

### 設計關鍵約束(對齊 oldm2Spec/ 暫時 ref)

- **Forest 不選 primary**(neely §9.3):`scenario_forest: Vec<Scenario>`,
  順序不反映優先級,Aggregation Layer 可依 power_rating 提供 UI 篩選
- **不引入機率語意**(neely §9.4):移除 v1.1 `confidence` / `composite_score`,
  Trigger.on_trigger 移除 ReduceProbability → WeakenScenario
- **PowerRating enum**(neely §9.4):取代 v1.1 `i8`,避免 99 等無效值
- **Neely 規則寫死**(neely §4.4 / §6.6):Fibonacci 比率 / ±4% 容差 /
  Power Rating 查表 / ATR multiplier 0.5 / Neutral threshold 1.0 ATR /
  Stage 3 BEAM_CAP_MULTIPLIER 10 全部寫死 Rust 常數,**不可外部化**
- **加權指數 Neutral 例外**(neely §10.4.1):`stock_id == "_index_taiex_"`(cores_overview §6.2.1 保留字)走 `neutral_threshold_taiex`(預設 0.5%)而非 ATR 比例
- **trait `Input` 由各 Core 自宣告**(cores_overview §3.4):IndicatorCore /
  WaveCore 都不限定 OHLCV,各 Core 用對應 loader(`shared/ohlcv_loader/` 等)

### 留 PR 後續(M3 PR-3c+)

| PR | 範圍 | 估時 |
|---|---|---|
| M3 PR-3c | Stage 4 完整:R4-R7 + F1-F2 + Z1-Z4 + T1-T10 + W1-W2 共 22 條規則的具體門檻 + Neely 書頁追溯。**等 user 在 m3Spec/ 寫最新 neely_core spec 後 batch 補**(規則細節 oldm2Spec/ §10.1 沒列) | ~1 天 |
| M3 PR-4 | Stage 5-7:Classifier + Post-Constructive Validator + Complexity Rule(含 R3 Diagonal exception) | ~1 天 |
| M3 PR-5 | Stage 8:Compaction(exhaustive + beam_search fallback)+ Forest 上限保護 | ~1.5 天 |
| M3 PR-6 | Stage 9-10:Missing Wave + Emulation + Power Rating(查表)+ Fibonacci 投影 + Triggers + facts.rs produce_facts | ~1 天 |
| M3 PR-7 | `shared/ohlcv_loader/`(讀 Silver `price_*_fwd`)+ `tw_cores` binary 接 PG + 寫 `structural_snapshots` / `facts` 表 + alembic 落地三表 | ~1 天 |
| M3 PR-8 | inventory `CoreRegistration` + `CoreRegistry::discover` + Workflow toml | ~半天 |
| M3 P0 Gate | 五檔(0050 / 2330 / 3363 / 6547 / 1312)實測 + 校準 forest_max_size / compaction_timeout / BeamSearchFallback.k 預設值,寫入 `docs/benchmarks/` | ~1 天 + 校準 |

### 已知狀態(下次 session 起點)

- alembic head:`v1w2x3y4z5a6`(不變,本版 0 migration)
- Rust workspace:4 crate(silver_s1_adjustment / fact_schema / neely_core / tw_cores),
  56/56 unit test 全綠;neely_core v0.4.0(Stage 1-4 partial,R1/R2/R3 完整 + 22 條 Deferred)
- m3Spec/:1 份(`chip_cores.md`,user 既有);其他 cores spec 仍在 `m2Spec/oldm2Spec/` r2,
  待 user 在 m3Spec/ 寫最新版後 batch sync code ref
- `src/rust_bridge.py` stale-check path 同步 + 4 處 cargo build hint 加 `-p tw_stock_compute`
- 下個 session:**M3 PR-3c** Stage 4 補完(22 條 Deferred 規則細節)or **PR-4**
  Stage 5-7 Classifier(可能更務實 — Classifier 邏輯較對齊 spec,
  Validator 細節需 user 寫 m3Spec/ 後再開)

### 風險

🟢 低:
- 純 Rust,0 alembic / 0 Python 邏輯 / 0 collector.toml
- Python `rust_bridge.py` stale-check path 改 1 處(舊 path `rust_compute/src/main.rs`
  不存在 → defense-in-depth `return`,不洗 false warning)
- target/release/ 雙 binary 名字 + path 對齊 PR-1 之前,既有 silver phase 7c 不需動
- Stage 1-2 演算法為 best-effort 對齊 spec 文字描述,實際 Neely 書頁細節對照
  在 P0 Gate 五檔實測時校準(警示常數:`REVERSAL_ATR_MULTIPLIER=0.5` /
  `STOCK_NEUTRAL_ATR_MULTIPLIER=1.0` / Wilder ATR period 14)
- Rollback:每 PR `git revert` 單 commit 即可

---

## v1.27 — day_trading 對齊 spec + Bronze pae dedup(2026-05-09)

接 v1.26 nice-to-haves merge + docs api_pipeline_reference 重寫對齊
`m2Spec/layered_schema_post_refactor.md` 後,doc §6 揭露的 `day_trading` builder
deviation 修正,連帶踩出 Bronze data 質量問題並修。

### 範圍 2 PR

| PR | 主題 | scope |
|---|---|---|
| #35 | day_trading builder 改 LEFT JOIN price_daily_fwd(spec §6.4 + chip_cores §7.2)| 1 builder + 3 docs lines;0 alembic |
| #36 | pae dedup par_value_change + split duplicates(修 16 對 + 防衛 trigger)| 1 alembic v1w2x3y4z5a6 + schema_pg.sql sync;0 builder |

### PR #35:day_trading 改用 price_daily_fwd.volume

`src/silver/builders/day_trading.py`:`BRONZE_TABLES` + SQL JOIN target
從 `price_daily` 改 `price_daily_fwd`。對齊雙處 spec 明文要求:
- `m2Spec/layered_schema_post_refactor.md` §6.4 流向圖
- `m2Spec/oldm2Spec/chip_cores.md` §7.2(M3 Cores 接點規範)
- `m3Spec/chip_cores.md` §7.2(本輪 main merge 同樣規範)

實質影響:
- POST-event 日(無未來 stock_dividend / split):fwd.volume == raw.volume
  → ratio 不變
- PRE-event 日:fwd.volume = raw × cumulative_vf(scaled to post-event)
  → ratio 對齊「後復權 scale」,跨歷史 cross-comparison 一致

依賴:S1_adjustment(7c Rust)需先跑過才有 fwd 資料;初次 silver phase 7a 跑時
fwd 空 → ratio NULL(LEFT JOIN safe degradation)。Orchestrator 排程順序
(7c 先 / 7a 後)留 follow-up,目前 user 自己 invoke 順序即可。

### PR #36:Bronze pae dedup(根因發現 + 修)

PR #35 merge 後 user spot check 5278 fwd_volume 揭露 ×108 異常(預期 ×10.83)。
trace 發現 `price_adjustment_events` 對 16 個 (stock, date) 同時記錄 par_value_change
+ split 兩個 event_type(同 vf=0.1),Rust 累乘 0.01 → fwd_volume 多 ×10。

根因:FinMind `TaiwanStockParValueChange` + `TaiwanStockSplitPrice` 兩 dataset
同時報告同一個面額變更(e.g. 5278 在 2024-12-09 從面額 1000 NTD 改 100 NTD)
→ collector 兩條 path 都收 → Bronze 變兩 row。

dev DB query 揭露 16 對:2327 / 3093 / 4763 / 5278 / 5314 / 5536 / 6415 /
6531 / 6548 ×2 / 6613 / 6763 / 6919 / 8070 / 8476 / 8932(同股可能多次)。

#### alembic v1w2x3y4z5a6 三階段

1. **既有 16 dup cleanup**:DELETE split row WHERE 同 (market, stock_id, date,
   before_price, reference_price, vf) 有 par_value_change row。保留
   par_value_change 為 primary(命名更具體 + FinMind ParValueChange 是
   authoritative source)。
2. **Mark 4 fwd 表 is_dirty=TRUE**:UPDATE price_daily_fwd / price_weekly_fwd /
   price_monthly_fwd / price_limit_merge_events SET is_dirty=TRUE for stocks
   that ever had par_value_change event。
3. **CREATE 防衛 trigger** `trg_pae_dedup_par_value_split`:AFTER INSERT
   OR UPDATE on price_adjustment_events,WHEN event_type IN ('split',
   'par_value_change'),DELETE 同 (key, before/ref/vf) 的 split row(保留
   par_value_change)。涵蓋 INSERT + UPDATE(UPSERT case)+ 任意寫入順序。

#### user 本機驗證(已通)

```powershell
git pull
alembic upgrade head    # → v1w2x3y4z5a6

# dup cleanup 驗證:0 row
psql $env:DATABASE_URL -c "
  SELECT stock_id, date, COUNT(*) FROM price_adjustment_events
  WHERE event_type IN ('par_value_change','split')
  GROUP BY stock_id, date HAVING COUNT(*) > 1
"

# trigger 落地(預期 2 row,INSERT + UPDATE 兩 event_manipulation)
psql $env:DATABASE_URL -c "
  SELECT trigger_name FROM information_schema.triggers
  WHERE trigger_name = 'trg_pae_dedup_par_value_split'
"

# 重算 fwd
python src/main.py silver phase 7c    # 預期 dirty queue 拉 15 unique stocks(16 pair - 6548 重複)

# 5278 fwd_volume 驗證(從 ×108 修正到 ×10.83)
psql $env:DATABASE_URL -c "
  SELECT date, volume FROM price_daily_fwd
  WHERE stock_id='5278' ORDER BY date LIMIT 3
"
```

實際結果:
| date | 修前 | 修後 | 縮回 |
|---|---|---|---|
| 2019-01-02 | 6,927,633 | **692,763** | × 10 ✓ |
| 2019-01-03 | 4,875,105 | 487,511 | × 10 ✓ |
| 2019-01-04 | 2,056,641 | 205,664 | × 10 ✓ |

驗算:`64000 raw → 692,763 fwd` = ×10.824 ≈ `1 / (0.1 × 0.9663329597 × 0.9560229446)
= 10.825`。**精確對上**(差 < 0.01% float rounding)。

### 連動修正

- 5278 day_trading_ratio for 2019~2024-12 歷史日期(原本因分母多 ×10 → ratio 縮小 1/10)→ 自動修正
- 其他 14 股(2327/3093/4763/5314/5536/6415/6531/6548/6613/6763/6919/8070/8476/8932)同樣連動修正
- VWAP / OBV / day_trading_ratio 等讀 fwd.volume 的 Cores 計算全部受惠

### 已知狀態(下次 session 起點)

- alembic head:`v1w2x3y4z5a6_pae_dedup_par_value_split`(user 已落)
- m2 主動工 + nice-to-haves + Bronze 質量修 + docs 對齊 全部收尾
- R5 觀察期 21~60 天啟動;最早 2026-05-30 進 R6 永久 DROP `_legacy_v2`
- 下個 PR:**M3 Cores 動工**(`m3Spec/` 已有 chip_cores.md,缺 fundamental /
  environment / indicator / wave 4 個 cores)或 **#R5 觀察期 SLO telemetry**
  (純驗證,無 code change)

---

## v1.26 — nice-to-have 一輪收尾(F/E/A/D/B)(2026-05-09)

接 R4(PR #29)merge 後動工 nice-to-have 收尾。範圍純 collateral fix,**不動 schema、
不動 alembic head**(仍 `u0v1w2x3y4z5`),只 Python + Rust code 改良。

### 範圍 5 項

| ID | 項目 | 影響檔 | 風險 |
|---|---|---|---|
| F | Rust binary stale warning 從 `__init__` 移到 `run_phase4`(只在實際 dispatch 才檢) | `src/rust_bridge.py` | 🟢 低 |
| E | `verify_pr19c2_silver` docstring 加邊緣日期 1-row delta SLO 說明 | `scripts/verify_pr19c2_silver.py` | 🟢 低(純 docs) |
| A | Rust `resolve_stock_ids` fallback 從 `stock_sync_status.fwd_adj_valid=0` 改成 `price_daily_fwd.is_dirty=TRUE` | `rust_compute/src/main.rs` | 🟢 低(只改 fallback SQL,Python orchestrator path 不受影響) |
| D | Phase 4 `_run_phase4` 加 incremental dirty queue filter — 0 dirty → skip Rust dispatch | `src/bronze/phase_executor.py` | 🟡 中(改 dispatch 邏輯,backfill 維持原狀) |
| B | margin / market_margin builder 改 iterate UNION(主, 副) Bronze keys,消除 PR #20 trigger 留下的 stub row(9706 SBL stub + 10 total_margin stub) | `src/silver/builders/{margin,market_margin}.py` | 🟡 中(builder 邏輯改變,但 round-trip 仍對齊 v2.0 legacy_v2) |

C(asyncio.gather 7a 平行)**留 follow-up** — 需先升 PostgresWriter 為 connection pool,
~半天 refactor + 風險中,獨立 PR 處理。

### F:Rust stale warning relocate

```
__init__ 只保留 binary 不存在 → raise FileNotFoundError(早於 subprocess);
run_phase4 開頭 call _check_binary_freshness()(只在真要派 Rust 才檢)
```

incremental --phases 5(不派 Rust)不再洗 stale warning。

### A:Rust `resolve_stock_ids` fallback

```rust
// 舊
SELECT stock_id FROM stock_sync_status WHERE fwd_adj_valid = 0
// 新(v1.26)
SELECT DISTINCT stock_id FROM price_daily_fwd WHERE is_dirty = TRUE
```

對齊 silver/orchestrator._fetch_dirty_fwd_stocks 同款 SQL。Rust binary 直接被
manual ops invoke(無 --stocks 傳入)時,會走 PR #20 trigger 維護的 dirty queue,
而非 deprecated `stock_sync_status.fwd_adj_valid` flag。

### D:Phase 4 incremental dirty queue skip

`bronze/phase_executor._run_phase4` 加分支:

- `mode == "backfill"` → 全市場 dispatch(對齊 v1.25 之前)
- `mode == "incremental"` → query `price_daily_fwd.is_dirty=TRUE` distinct stock_id
  - 0 dirty → log skip,**不 dispatch Rust**(完整省 ~6 分鐘 / 1700 stocks × 200ms)
  - 否則只送 dirty stocks 給 Rust

對齊 silver/orchestrator._run_7c 的同款 dirty queue pattern。新加
`_fetch_dirty_fwd_stocks()` helper(對齊 orchestrator 名稱)。

### B:margin / market_margin UNION 升級

PR #20 trigger 設計:Bronze upsert 觸發 Silver dirty row insert(stub)。
v1.26 之前,builder 只 iterate 主 Bronze keys → 副 Bronze 的 dates/stocks 在 Silver
留下永久 stub row(margin 9706 stub from SBL trigger / market_margin 10 stub from
total_margin trigger)。

Fix:builder 改 iterate `set(主 keys) ∪ set(副 keys)`:

| Bronze 狀態 | Silver row 內容 |
|---|---|
| 主有 + 副有 | full row |
| 主有 + 副無 | 主 cols 正常 + 副衍生 cols = NULL |
| 主無 + 副有 | 主 cols = NULL + detail = `{}` + 副衍生 cols = 副 Bronze 值 |

避免 stub row 殘留;Silver row count 對齊真實 (主 ∪ 副) 集合。

沙箱合成資料 4 個 case 全綠(margin: 主+副 / 主 only / 副 only / mixed;
market_margin: 主+副 / 主 only / 副 only)。

### user 本機驗證(預期全綠)

```powershell
git pull
# 不需 alembic upgrade(本 PR 0 migration)
cd rust_compute && cargo build --release && cd ..    # A 改 Rust SQL,需重編

# F: incremental --phases 5 不再洗 Rust stale warning
python src/main.py incremental --phases 5 --stocks 2330
# 預期:不再見 "Rust binary 比 source 舊" warning(因為 phase 5 不派 Rust)

# D: incremental --phases 4 dirty queue skip(預期 skip,因 dirty 為 0)
python src/main.py incremental --phases 4
# 預期:[Phase 4] dirty queue 為空(無新除權息事件),skip Rust dispatch

# B: 跑 Silver phase 7a 全市場後 Silver row count 應 = 主 ∪ 副(不再有 stub)
python src/main.py silver phase 7a --full-rebuild
# 預期 log:[margin] read=X margin + Y sbl (union to Z rows) → wrote=Z
#         [market_margin] read=A margin + B total_margin (... union to C silver rows) → wrote=C

# E: verify_pr19c2 docstring 加 SLO 說明,跑起來行為不變
python scripts/verify_pr19c2_silver.py    # 仍應 ±1% SLO 內(可能仍有 1-row deltas)
```

### 風險

🟡 中:
- D dirty queue filter 在 incremental 模式生效,backfill 維持原狀
- B builder 改 UNION 後 Silver row count 可能微增(把過去 stub row 真實化);
  v2.0 legacy_v2 round-trip 仍應對齊(verifier 用 skip_silver_cols 處理 SBL/total_margin 衍生欄)
- A 只改 Rust binary fallback 路徑,Python orchestrator 路徑不變
- Rollback:單 commit revert(無 alembic 動作)

### 已知狀態(下次 session 起點)

- alembic head:`u0v1w2x3y4z5`(不變,本 PR 0 migration)
- v1.26 nice-to-haves:5/5 done(F/E/A/D/B);C 留 follow-up
- m2 大重構主動工(R1~R4)+ nice-to-haves 收尾,進入 R5 觀察期
- 下個 PR:**M3 indicator core 動工** 或 **C asyncio.gather 7a 平行優化**
  (需 PostgresWriter → connection pool refactor,獨立 PR)

---

## v1.25 — PR #R4 m2 大重構:v2.0 entry name 加 `_legacy` 後綴(2026-05-09)

接 R3 落地後動 R4(plan §6.3 簡化選項)。同 PR #28 收尾(沒切新分支)。

### 為什麼走 §6.3 簡化選項

plan §6.2 原方案要把 v3 entry name 從 `_v3` 收回主名(`holding_shares_per_v3`
→ `holding_shares_per`),但這會踩 `api_sync_progress.api_name` collision —
v2.0 entry 跨時間點同名不同 target 會混淆。

§6.3 推薦:**v3 entry 永遠保留 `_v3` 後綴作為「重抓 spec 來源」標籤**,
只把 v2.0 entry 加 `_legacy` 後綴 — 沒有跨時間點同名,api_sync_progress
clean UPDATE 即可。

範圍縮成「v2.0 entry 顯式 `_legacy` 後綴」,1~2h 完工。

### 範圍

5 個 v2.0 entry name rename(target_table 已在 R2 改 `_legacy_v2`,本 PR 不動):

| 舊 entry name | 新 entry name | target_table(R2 已落) |
|---|---|---|
| `holding_shares_per` | `holding_shares_per_legacy` | `holding_shares_per_legacy_v2` |
| `monthly_revenue` | `monthly_revenue_legacy` | `monthly_revenue_legacy_v2` |
| `financial_income` | `financial_income_legacy` | `financial_statement_legacy_v2` |
| `financial_balance` | `financial_balance_legacy` | `financial_statement_legacy_v2` |
| `financial_cashflow` | `financial_cashflow_legacy` | `financial_statement_legacy_v2` |

5 個 v3 entries(`*_v3`)**永遠不動**,主路徑 100% 不受影響。

### alembic `u0v1w2x3y4z5_pr_r4_rename_v2_entry_names_legacy`

5 條 idempotent UPDATE:

```sql
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM api_sync_progress WHERE api_name = 'holding_shares_per') THEN
        UPDATE api_sync_progress
           SET api_name = 'holding_shares_per_legacy'
         WHERE api_name = 'holding_shares_per';
    END IF;
END $$;
-- (其他 4 條同 pattern)
```

**無 collision 風險**:R4 前 5 個 `*_legacy` row count = 0(沒任何地方用過),
新名 row 由本 migration UPDATE 寫入。

### 為什麼必須遷移 api_sync_progress.api_name

`api_sync_progress` PK = (api_name, stock_id, segment_start)。entry name rename
而 api_name 不同步遷移 → incremental 找不到既有 segment status → 以為從未跑過,
踩 1700+ stocks × 21 年全市場重抓(~30-40h calendar-time)。

migration 強制 UPDATE 同步,既有 backfill 進度紀錄無痕跟到新 entry name。

### 配套改動

- `config/collector.toml`:5 個 v2.0 entry name 加 `_legacy` 後綴 + notes 更新
- `alembic/versions/2026_05_09_u0v1w2x3y4z5_pr_r4_rename_v2_entry_names_legacy.py`:
  本 migration(5 idempotent UPDATE)
- `scripts/verify_pr19c2_silver.py`:**R2 follow-up fix** — 3 個 `legacy_table` ref
  從 `holding_shares_per` / `monthly_revenue` / `financial_statement` 改成
  `*_legacy_v2`(R2 漏改,verifier 之前對 v2.0 legacy 比對抓不到表 → 全 FAIL,
  本 PR 順手收掉,讓 R5 觀察期 SLO 驗證能用)

### user 本機驗證流程

```powershell
git pull
alembic upgrade head                                    # → u0v1w2x3y4z5

# 5 個新 entry name 在 api_sync_progress 找得到(舊名 row count = 0)
psql $env:DATABASE_URL -c "
  SELECT api_name, COUNT(*) FROM api_sync_progress
  WHERE api_name IN (
    'holding_shares_per_legacy', 'monthly_revenue_legacy',
    'financial_income_legacy', 'financial_balance_legacy', 'financial_cashflow_legacy'
  )
  GROUP BY api_name ORDER BY api_name
"

# 舊 entry name 應該 row count = 0(全部遷移完)
psql $env:DATABASE_URL -c "
  SELECT api_name, COUNT(*) FROM api_sync_progress
  WHERE api_name IN (
    'holding_shares_per', 'monthly_revenue',
    'financial_income', 'financial_balance', 'financial_cashflow'
  )
  GROUP BY api_name
"
# 預期 0 rows(沒任何 row 用舊名)

# validate config(會看到 5 個 *_legacy entry)
python src/main.py validate

# incremental smoke — 跑 phase 5,既有進度紀錄應跟新 entry name 對齊不重抓
python src/main.py incremental --phases 5 --stocks 2330
# 預期:5 個 *_legacy entry 各跑 1 segment(2330 已 fully backfilled,
# incremental 只查最近一段;若新名找不到舊紀錄 → 會踩全市場重抓 36 個 segment)

# verify_pr19c2 R2 follow-up fix 後應 3/3 OK(對 v2.0 legacy_v2 比對等值)
python scripts/verify_pr19c2_silver.py

# rollback smoke
alembic downgrade -1 && alembic upgrade head
```

### 風險

🟡 中:
- migration 落地時若有 active backfill in-flight 跑到 phase 5,舊名 row UPDATE
  後該 backfill 會繼續完成寫入但用新 entry name — 推薦落地時不要有 active backfill
- v3 entry 完全不動,主路徑寫入 100% 不受影響
- collector.toml 5 個 entry name 改名:`python src/main.py validate` 會看到
  10 個 phase 5 entry(5 v2.0 _legacy + 5 v3),pre-R4 是 5+5 = 10 同數,只有名字差
- Rollback:downgrade UPDATE 反向(新名 → 舊名)

### 已知狀態(下次 session 起點)

- alembic head:`u0v1w2x3y4z5`(待 user 本機 `alembic upgrade head` 落地)
- PR #R3 + #R4:同 PR #28(R3 R4 合一,user verify 一輪)
- 下個 PR:**#R5** 觀察期 21~60 天(無 code change,純驗證 SLO)+
  **#R6** DROP 3 張 `_legacy_v2`(永久 DROP,需 backup 後執行)
- 觀察期 SLO(plan §7.2):
  - Silver builder 持續每日 12/12 OK
  - api_sync_progress.status='failed' = 0
  - 3 張 `_legacy_v2` row count 與主名表 ±1%
- 完整 m2 大重構結束後可進 M3 indicator core(spec/m2Spec/)

---

## v1.24 — PR #R3 m2 大重構:`_tw` Bronze 升格主名(2026-05-09)

接 R2 落地後動 R3(plan §五)。3 張 `_tw` Bronze 表去 `_tw` 後綴升格成主名,
完成 m2 大重構 schema 升格的最後一步;collector.toml v3 entry name 收回主名留 R4。

### 範圍

| 舊名 | 新名 |
|---|---|
| `holding_shares_per_tw` | `holding_shares_per` |
| `financial_statement_tw` | `financial_statement` |
| `monthly_revenue_tw` | `monthly_revenue` |

連帶 rename 3 個 explicit index(去 `_tw` 後綴):
- `idx_holding_shares_per_tw_stock_date_desc` → `idx_holding_shares_per_stock_date_desc`
- `idx_financial_statement_tw_stock_date_desc` → `idx_financial_statement_stock_date_desc`
- `idx_monthly_revenue_tw_stock_date_desc` → `idx_monthly_revenue_stock_date_desc`

(PK 由 PG 自動跟著表 rename。)

### Trigger 不需手動 DROP+重建(2026-05-09 sandbox 驗證)

PG trigger 透過 OID 綁 table,不是綁名字 — ALTER TABLE RENAME 後 trigger 自動
跟著表走,`information_schema.triggers.event_object_table` 自動更新到新名。

驗證:`_smoke_t` → `_smoke_t2` rename 後 `_smoke_trg.event_object_table = _smoke_t2` ✓
(本機 user PowerShell + here-string + psql 跑過,結果回 `_smoke_t2`)。

3 個受影響的 dirty trigger 全部自動跟到主名,**migration 不動 trigger DDL**:
- `mark_holding_shares_per_derived_dirty`
- `mark_financial_stmt_derived_dirty`
- `mark_monthly_revenue_derived_dirty`

→ `src/schema_pg.sql` 仍要把 CREATE TRIGGER ... ON `*_tw` 改成主名(給 fresh DB
   走 schema_pg.sql 初始化用),這部分 PR #R3 同步落地。

### alembic `t9u0v1w2x3y4_pr_r3_promote_tw_bronze_3_tables`

idempotent 設計(`DO $$ ... IF EXISTS old AND NOT EXISTS new ... THEN ALTER TABLE ... RENAME ... END $$`):
- 既有 DB(舊 `_tw`)→ rename 走起來
- fresh DB(schema_pg.sql 已是主名)→ no-op pass through

下面條件一起檢查:`IF EXISTS old AND NOT EXISTS new`,**只做安全 rename,不會
誤踩已 rename 的表**。

### 配套改動

- `src/schema_pg.sql`:3 個 CREATE TABLE 改主名 + 3 個 INDEX rename + 3 個 CREATE
  TRIGGER ON 表名同步 + comment 標 PR #R3
- `config/collector.toml`:5 個 v3 entry 的 `target_table` 從 `*_tw` 改主名:

  | entry | target_table |
  |---|---|
  | `holding_shares_per_v3` | `holding_shares_per` |
  | `financial_income_v3` | `financial_statement` |
  | `financial_balance_v3` | `financial_statement` |
  | `financial_cashflow_v3` | `financial_statement` |
  | `monthly_revenue_v3` | `monthly_revenue` |

  Entry name 仍留 `_v3` 後綴,等 R4 收回主名(plan §6.3 推薦簡化選項:留 `_v3`
  作為「重抓 spec 來源」標籤永久,避免 api_sync_progress 遷移)。

  5 個 v2.0 entries(target=`_legacy_v2`)**不動** — R2 已落地。

- `src/silver/builders/{holding_shares_per,financial_statement,monthly_revenue}.py`:
  `BRONZE_TABLES` + `fetch_bronze()` 表名改主名 + docstring 標 PR #R3

- `scripts/inspect_db.py`:Bronze 區段 3 表名同步主名

- `scripts/verify_pr20_triggers.py`:trigger spec 表名同步主名(holding_shares_per
  generic spec / financial_statement special spec / monthly_revenue generic spec)

### user 本機驗證流程

```powershell
git pull
alembic upgrade head                                    # → t9u0v1w2x3y4

# 3 張主名表存在 + 既有資料完整
psql $env:DATABASE_URL -c "
  SELECT 'holding_shares_per'  AS t, COUNT(*) FROM holding_shares_per
  UNION ALL SELECT 'financial_statement', COUNT(*) FROM financial_statement
  UNION ALL SELECT 'monthly_revenue',     COUNT(*) FROM monthly_revenue
"

# 舊 _tw 名應全部消失
psql $env:DATABASE_URL -c "SELECT 1 FROM holding_shares_per_tw LIMIT 1"   # ERROR: relation does not exist
psql $env:DATABASE_URL -c "SELECT 1 FROM financial_statement_tw LIMIT 1"  # 同
psql $env:DATABASE_URL -c "SELECT 1 FROM monthly_revenue_tw LIMIT 1"      # 同

# 3 個 dirty trigger 自動跟著表 rename(無需 DROP+重建)
psql $env:DATABASE_URL -c "
  SELECT trigger_name, event_object_table FROM information_schema.triggers
  WHERE trigger_name IN (
    'mark_holding_shares_per_derived_dirty',
    'mark_financial_stmt_derived_dirty',
    'mark_monthly_revenue_derived_dirty'
  )
"
# 預期 event_object_table 全部 = 主名(holding_shares_per / financial_statement / monthly_revenue)

# Silver builders 讀新名 OK
python src/main.py silver phase 7a --full-rebuild --stocks 2330
python src/main.py silver phase 7b --full-rebuild --stocks 2330

# dual-write 仍正常(主路徑寫主名 / legacy 路徑寫 _legacy_v2)
python src/main.py validate
python src/main.py incremental --phases 5 --stocks 2330
psql $env:DATABASE_URL -c "
  SELECT MAX(date) FROM holding_shares_per WHERE stock_id='2330'
"

# rollback smoke
alembic downgrade -1 && alembic upgrade head
```

### 風險

🟡 中(R3 是 m2 大重構最複雜的 PR):
- collector.toml 5 個 v3 entries 的 target_table 必須跟 alembic rename 同步,
  否則 dual-write 會 INSERT 進不存在的舊名 → upsert 炸
- 已驗:5 v3 entries 全部改主名(grep 確認)
- 3 個 Silver builder BRONZE_TABLES + fetch_bronze() 必須同步主名(已驗)
- v2.0 entries(target=`_legacy_v2`)**不動** — R2 已落地
- PR 順序強約束:必須 R2 → R3,否則 R3 rename 會撞既有 v2.0 表 PK 衝突
  (R2 已落,空出主名,R3 才能升格)
- Rollback:downgrade rename 反向(主名 → `*_tw`,索引同步反向)

### 已知狀態(下次 session 起點)

- alembic head:`t9u0v1w2x3y4`(待 user 本機 `alembic upgrade head` 落地)
- PR #R3:本 PR(待 user verify)
- 下個 PR:**#R4**(plan §六)— collector.toml v3 entry name 從 `_v3` 後綴
  收回主名(`holding_shares_per_v3` → `holding_shares_per` 等)。**改 entry name
  會改 `api_sync_progress.api_name`**,要小心 backfill 進度紀錄遷移
  (用 SQL UPDATE 同步改名;plan §6.2 有寫流程)。
  推薦走 plan §6.3 簡化選項:**保留 `_v3` 後綴永久作為「重抓 spec 來源」標籤**,
  避免 api_sync_progress 遷移複雜度,只把 v2.0 entry 改 `_legacy` 後綴 + target
  改 `_legacy_v2`(R2 已做)
- R5 觀察期 21~60 天後,PR #R6 才會 DROP `_legacy_v2`

---

## v1.23 — PR #R2 m2 大重構:v2.0 舊 3 表 rename `_legacy_v2`(2026-05-09)

接 R1 落地後動 R2(plan §四)。3 張 v2.0 舊 Bronze 表 rename `_legacy_v2`
進入 PR #R5 觀察期(21~60 天),PR #R6 後永久 DROP。

### 範圍

| 舊名 | 新名 |
|---|---|
| `holding_shares_per` | `holding_shares_per_legacy_v2` |
| `financial_statement` | `financial_statement_legacy_v2` |
| `monthly_revenue` | `monthly_revenue_legacy_v2` |

連帶 rename `financial_statement` 的 2 個 explicit index:
- `idx_financial_type_date` → `idx_financial_legacy_type_date`
- `idx_financial_detail_gin` → `idx_financial_legacy_detail_gin`

(holding_shares_per / monthly_revenue 無 explicit index;PK 由 PG 自動跟著表 rename。)

### alembic `s8t9u0v1w2x3_pr_r2_rename_v2_legacy_3_tables`

idempotent 設計(`DO $$ ... IF EXISTS ... THEN ALTER TABLE ... RENAME ... END $$`):
- 既有 DB(舊名)→ rename 走起來
- fresh DB(schema_pg.sql 已是 _legacy_v2)→ no-op pass through

下面條件:`IF EXISTS old AND NOT EXISTS new`,**只做安全 rename,不會誤踩已 rename 的表**。

### collector.toml dual-write 5 entries 同步

5 個 v2.0 entries 的 target_table 改 `_legacy_v2`(維持 dual-write 行為,只是寫到 legacy 表):

| entry | target_table |
|---|---|
| `holding_shares_per` | `holding_shares_per_legacy_v2` |
| `monthly_revenue` | `monthly_revenue_legacy_v2` |
| `financial_income` | `financial_statement_legacy_v2` |
| `financial_balance` | `financial_statement_legacy_v2` |
| `financial_cashflow` | `financial_statement_legacy_v2` |

5 個 PR #18.5 v3 entries(`*_v3`)不動,主路徑仍寫 `*_tw`。

### 配套改動

- `src/schema_pg.sql`:3 個 CREATE TABLE 改名 `_legacy_v2` + 2 個 INDEX rename + comment 標 PR #R2/#R6
- `scripts/check_all_tables.py`:Phase 5 區段表名對齊 `_legacy_v2`
- `scripts/inspect_db.py`:Legacy v2.0 group label + 表名同步
- `scripts/test_db.py`:Test 9 用 `financial_statement_legacy_v2`
- 0 Silver builder 改動(builders 讀 `_tw` 不讀 v2.0 legacy)
- 0 trigger 改動(trigger 都是 ON `_tw` Bronze,不在 v2.0 legacy 上)

### user 本機驗證流程

```powershell
git pull
alembic upgrade head                                    # → s8t9u0v1w2x3

# 3 張 _legacy_v2 表存在 + 既有資料完整
psql $env:DATABASE_URL -c "
  SELECT 'holding_shares_per_legacy_v2'  AS t, COUNT(*) FROM holding_shares_per_legacy_v2
  UNION ALL SELECT 'financial_statement_legacy_v2', COUNT(*) FROM financial_statement_legacy_v2
  UNION ALL SELECT 'monthly_revenue_legacy_v2',     COUNT(*) FROM monthly_revenue_legacy_v2
"

# 舊名應全部消失
psql $env:DATABASE_URL -c "SELECT 1 FROM holding_shares_per LIMIT 1"   # ERROR: relation does not exist
psql $env:DATABASE_URL -c "SELECT 1 FROM financial_statement LIMIT 1"  # 同
psql $env:DATABASE_URL -c "SELECT 1 FROM monthly_revenue LIMIT 1"      # 同

# 跑 incremental backfill 驗 dual-write 仍正常寫入新名
python src/main.py validate
python src/main.py incremental --phases 5 --stocks 2330
psql $env:DATABASE_URL -c "
  SELECT MAX(date) FROM holding_shares_per_legacy_v2 WHERE stock_id='2330'
"

# rollback smoke
alembic downgrade -1 && alembic upgrade head
```

### 風險

🟡 中:
- collector.toml v2.0 entries 必須跟 alembic rename 同步,否則 dual-write 會 INSERT 進不存在的舊名 → upsert 炸
- 已驗:5 v2.0 entries 全部改 `_legacy_v2`(grep 確認)
- Silver pipeline 不受影響(讀 `_tw` 不讀 legacy)
- Rollback:downgrade rename 反向

### 已知狀態(下次 session 起點)

- alembic head:`s8t9u0v1w2x3`(待 user 本機 `alembic upgrade head` 落地)
- PR #R2:本 PR(待 user verify)
- 下個 PR:**#R3** — 3 張 `_tw` Bronze 升格 rename(去 `_tw` 後綴成主名),
  trigger / collector.toml v3 entries / Silver builder BRONZE_TABLES 同步遷移

---

## v1.22 — PR #R1 m2 大重構:source 欄補回 3 張 _tw Bronze(2026-05-09)

接 PR #21 deprecated path cleanup + PR #23 m2Spec/oldm2Spec/ 歸檔後,m2 大重構
正式動工。R1 是 plan §三 第 1 個 PR,範圍純 schema additive。

### 範圍

3 張 Bronze 表 ALTER ADD COLUMN `source TEXT NOT NULL DEFAULT 'finmind'`:

| Bronze 表 | spec ref | 補上原因 |
|---|---|---|
| `holding_shares_per_tw` | spec §3.5 line 430 | 明文「**PR #R1 補回**」標記 |
| `financial_statement_tw` | spec §3.6 漏寫 | Bronze 全表 source 一致原則(見 plan §1.2 註 2) |
| `monthly_revenue_tw` | spec §3.6 漏寫 | 同上 |

### alembic `r7s8t9u0v1w2_pr_r1_add_source_to_3_tw`

```sql
ALTER TABLE holding_shares_per_tw   ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'finmind';
ALTER TABLE financial_statement_tw  ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'finmind';
ALTER TABLE monthly_revenue_tw      ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'finmind';
```

PG 11+ 對 ALTER ADD COLUMN with DEFAULT 是 instant operation(不掃 row,
不阻塞)。既有資料(holding_shares_per_tw 7M+ rows / financial_statement_tw
~14M+ rows / monthly_revenue_tw 数十萬 rows)default 自動填 'finmind'。

### 配套改動

- `src/schema_pg.sql`:3 個 CREATE TABLE 同步加 source 欄 + comment 標 PR #R1
- 既有 collector.toml 5 個 v3 entries 沒指定 source,db.upsert 不過濾此欄,
  schema default 接管 — **0 collector.toml 改動**
- 既有 Silver builders(holding_shares_per / financial_statement / monthly_revenue)
  不讀 source 欄 — **0 builder 改動**

### user 本機驗證流程

```powershell
git pull
alembic upgrade head                                    # r7s8t9u0v1w2

# 3 表 source 欄存在 + 既有資料 default = 'finmind'
psql $env:DATABASE_URL -c "
  SELECT 'holding_shares_per_tw' AS t, source, COUNT(*) FROM holding_shares_per_tw GROUP BY source
  UNION ALL SELECT 'financial_statement_tw', source, COUNT(*) FROM financial_statement_tw GROUP BY source
  UNION ALL SELECT 'monthly_revenue_tw', source, COUNT(*) FROM monthly_revenue_tw GROUP BY source
"

# rollback smoke
alembic downgrade -1 && alembic upgrade head

# 既有 Silver pipeline 不受影響(round-trip 仍 5/5)
python scripts/verify_pr19b_silver.py
```

### 風險

🟢 低:
- ALTER ADD COLUMN with DEFAULT instant operation
- 0 collector.toml / 0 builder 改動
- Rollback:downgrade DROP COLUMN

### 已知狀態(下次 session 起點)

- alembic head:`r7s8t9u0v1w2`(待 user 本機 `alembic upgrade head` 落地)
- PR #24:m2 大重構 plan(`m2Spec/data_refactor_plan.md`)— **§1.2 wording fix
  pushed**(2026-05-09)
- PR #R1:本 PR(待 user verify)
- 下個 PR:**#R2** v2.0 舊 3 表(`holding_shares_per` / `financial_statement` /
  `monthly_revenue`)rename `_legacy_v2` + collector.toml v2.0 entry target_table
  同步

---

## v1.21 — PR #21 deprecated path 全砍(2026-05-09)

接 PR #22 收尾後動工 PR #21 — 砍 §5.6 短期補丁路徑,讓 PR #20 trigger
成為唯一真相來源。

### 移除範圍

| 路徑 | 處理 |
|---|---|
| `src/bronze/dirty_marker.py` 整檔 | **DELETE**(無 caller,deprecated stub 自 PR #20)|
| `post_process.invalidate_fwd_cache` 函式本體 | **DELETE**(無 caller,deprecated 自 PR #20)|
| `post_process.dividend_policy_merge` 內已砍 call 留下的歷史 comment | clean |
| `bronze/phase_executor._run_api` 內 deprecated comment | clean |
| `bronze/__init__.py` docstring 提 `dirty_marker.py` | rewrite |
| `post_process.py` 頂端 docstring 改用過去式描述 | rewrite |

剩 1 個歷史 reference(`silver/orchestrator.py:209-210` docstring 提
`stock_sync_status.fwd_adj_valid=0`)留著 — 是說明 orchestrator 不再依賴
這 flag 的歷史脈絡,沒呼叫對應 deprecated 函式。

### Rust binary 自接 dirty queue 留 follow-up

per spec 「兩端任一條 path work 就夠」— orchestrator path 已接
(`silver/orchestrator._run_7c` 從 `price_daily_fwd.is_dirty=TRUE` pull),
Rust binary 還在讀 `stock_sync_status.fwd_adj_valid=0`(`rust_compute/src/main.rs`)。
這個改造非 critical,留下個 session 動。當前生產環境用 orchestrator path 完整 work。

### 沙箱已驗

- AST parse:`post_process.py` / `bronze/phase_executor.py` ✓
- import:`post_process` ✓ / `bronze.dirty_marker` 預期 ModuleNotFoundError ✓
- `dividend_policy_merge` 仍可呼叫 ✓
- `invalidate_fwd_cache` import 失敗(已移除)✓
- repo grep 0 active python references to deprecated 函式名

### 已知狀態(下次 session 起點)

- alembic head:`q6r7s8t9u0v1`(不變,純 Python 程式碼移除)
- v3.2 r1 PR sequencing:#17~#22 + #21 deprecated cleanup **全綠** ✅
- 進入 M3 indicator core 之前的 v3.2 r1 collector + Silver pipeline 完整收尾

下個 session 建議:
1. **margin / market_margin builder UNION 升級**(可選 nice-to-have,~9716 stub row)
2. **Rust binary 自接 dirty queue**(讀 `price_daily_fwd.is_dirty` 取代
   `stock_sync_status.fwd_adj_valid`)— 收尾用,非 critical
3. **m2 milestone 完整收尾** — Silver views + legacy_v2 rename + M3 prep

---

## v1.20 — PR #22 / B-1/B-2 TAIEX/TPEx daily OHLCV(2026-05-09)

接 PR #21-B 完整收尾後動工 PR #22 — `taiex_index_derived` 一直 read=0 wrote=0
是因為 Bronze `market_ohlcv_tw`(PR #11/B-1/B-2 已建 schema)從未被 populate
(collector.toml 沒對應 entry,blueprint 註明「multi-source merge 邏輯留 PR #17
重構 phase_executor 時實作」是當時 spec 的假設)。

### A. 找對 dataset 的曲折歷程

spec §2.3 寫 `market_ohlcv_tw` 來源 = `TaiwanStockTotalReturnIndex` +
`TaiwanVariousIndicators5Seconds`,本 session probe(`scripts/probe_finmind_taiex_ohlcv.py`)
揭露這 2 個 dataset 互不可 merge:

| dataset | 內容 | 數值 |
|---|---|---|
| `TaiwanStockTotalReturnIndex` | 報酬指數(含股利再投資)daily price only | 50486 |
| `TaiwanVariousIndicators5Seconds` | 加權指數 5-sec ticks(只 TAIEX 一檔) | 22832 |
| `TaiwanStockEvery5SecondsIndex` | 加權指數 5-sec ticks(支援 data_id=TAIEX/TPEx) | 22832 |

兩種指數**物理意義不同**(報酬 vs 加權),不能合成 OHLCV。原 spec 假設錯。

走過 4 條死路:
1. `USStockPrice + ^TWII / ^TWOII / ^TWO`:200 OK 但 0 rows(Yahoo ticker 認得
   但 FinMind 沒 cover TAIEX)
2. `TaiwanStockMarketIndex` / `TaiwanStockOHLC` / 其他 4 個 candidate:422 不存在
3. 5-sec aggregate 路(commit b5ad596):寫 `aggregate_5sec_to_daily_ohlc`,
   collector.toml `market_ohlcv_v3` 用 5Seconds + segment_days=3,~2h backfill,
   volume 永遠 NULL — **可行但不漂亮**
4. /datalist endpoint 只回 6 個國家名(`Canda`/`China`/`Euro`/`Japan`/`Taiwan`/`UK`),
   廢話 endpoint;改從 422 error message 撈 91 個 backer-tier dataset enum 才
   找到 `TaiwanStockEvery5SecondsIndex` 等候選名

User 提示「應該有別的表能拿到 OHL」→ probe `TaiwanStockPrice + data_id=TAIEX/TPEx`:

```
fields: ['Trading_Volume', 'Trading_money', 'Trading_turnover',
         'close', 'date', 'max', 'min', 'open', 'spread', 'stock_id']
TAIEX 2025-01-02:open=22975.71 max=23038.08 min=22713.63 close=22832.06
                  Trading_Volume=6,901,476,303(真有 volume!)
```

🎯 **直接命中** — FinMind 把 TAIEX/TPEx 當股票 expose 給 `TaiwanStockPrice`(同
既有 `price_daily` entry 用的 dataset),完整 OHLCV + volume,加權指數收盤跟
5Seconds tick 一致,0 工程量解決。

### B. 落地實作(commit aa1b094)

(1) collector.toml `market_ohlcv_v3` entry — 對齊既有 `price_daily` pattern:

```toml
[[api]]
name         = "market_ohlcv_v3"
dataset      = "TaiwanStockPrice"
param_mode   = "per_stock_fixed"
target_table = "market_ohlcv_tw"
phase        = 1
enabled      = true
is_backer    = true
segment_days = 365
fixed_ids    = ["TAIEX", "TPEx"]
field_rename = {
    "max"               = "high",
    "min"               = "low",
    "Trading_Volume"    = "volume",
    "Trading_money"     = "_trading_money",
    "Trading_turnover"  = "_trading_turnover",
    "spread"            = "_spread",
}
detail_fields = ["_trading_money", "_trading_turnover", "_spread"]
```

注意 `market_ohlcv_tw` schema 沒 `turnover` stored col(price_daily 有),
`Trading_turnover` 改進 detail JSONB(跟 price_daily 略不同)。

(2) Revert 前一 commit(b5ad596)的 5-sec aggregator 路:
- 砍 `aggregate_5sec_to_daily_ohlc` from aggregators.py
- 砍 apply_aggregation dispatcher key='aggregate_5sec_ohlc'
- YAGNI — 未來若需要 intraday 微結構分析可從 git 撈回

(3) Bronze schema(`market_ohlcv_tw` alembic h7i8j9k0l1m2)不動,既有 PR #20
trigger `mark_taiex_index_derived_dirty` 也不動。

### C. User 本機驗證(2026-05-09)

```
[Phase 1][market_ohlcv_v3] TAIEX 8 segments × ~245 rows = 1781 rows
[Phase 1][market_ohlcv_v3] TPEx  8 segments × ~245 rows = 1781 rows
elapsed = 33 秒(預估 ~2 分鐘,per_stock_fixed × 2 indices × 8 segments × ~3s)

[taiex_index] read=3562 → wrote=3562  ← 從 0 變 3562 ✅
```

Spot check Bronze TAIEX 2026-05-08:

```
date=2026-05-08 open=41886.03 high=42038.60 low=41132.25 close=41603.94
volume=15,497,124,214  detail={spread:-329.84, trading_money:1.3T, trading_turnover:7.4M}
```

`spread = today.close - yesterday.close` 內部一致(5/8 close - 5/7 close
= 41603.94 - 41933.78 = -329.84,對得上 detail spread)。

### 已知狀態(下次 session 起點)

- alembic head:`q6r7s8t9u0v1`(不變,PR #22 純 collector.toml 改不需 migration)
- `market_ohlcv_tw` Bronze:1781 rows × 2 indices(TAIEX + TPEx)
- `taiex_index_derived` Silver:3562 rows(從 0 → full)
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️
  → #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → #20 ✅ → #21-A ✅ → #21-B ✅
  → **#22 ✅(TAIEX/TPEx daily OHLCV 含 volume)** → #23(待定)

下個 session 建議:
1. **PR #21 完整收尾** — 觀察 1~2 sprint 後砍 §5.6 deprecated 路徑
   (`post_process.invalidate_fwd_cache` + `bronze/dirty_marker.mark_silver_dirty`)
2. **margin / market_margin builder UNION 升級**(可選)
3. **m2 PR #20 / #21 完整 milestone** — Silver views + legacy_v2 rename + M3 prep

---

## v1.19 — PR #21-B 3 條新 Bronze + 5 衍生欄補完(2026-05-05)

接 PR #21-A 完整收尾後動工 PR #21-B。原 plan 寫「~1 天 + backfill」、3 條
新 Bronze 表。本 session 完成全部 code 動作,留 30~40h backfill 給 user 排日曆
(走 PR #18.5 同 dual-write pattern)。

### A. 5 個衍生欄 vs 3 個新 Bronze 對映

| 衍生欄 | 所屬 Silver | 來源 Bronze(PR #21-B 落地) | FinMind dataset |
|---|---|---|---|
| `institutional.gov_bank_net` | institutional_daily_derived | government_bank_buy_sell_tw | TaiwanStockGovernmentBankBuySell |
| `market_margin.total_margin_purchase_balance` | market_margin_maintenance_derived | total_margin_purchase_short_sale_tw | TaiwanStockTotalMarginPurchaseShortSale |
| `market_margin.total_short_sale_balance` | 同上 | 同上 | 同上 |
| `margin.sbl_short_sales_short_sales` | margin_daily_derived | short_sale_securities_lending_tw | **TaiwanStockShortSaleSecuritiesLending(候選名)** |
| `margin.sbl_short_sales_returns` | 同上 | 同上 | 同上 |
| `margin.sbl_short_sales_current_day_balance` | 同上 | 同上 | 同上 |

⚠️ SBL 那條 dataset 名是候選名(spec §2.6.1 line 577 寫「SecuritiesLending」
泛稱,實際 FinMind 命名 user 首跑 backfill 才能驗;若 404,collector.toml 改名
重跑,同 PR #18.5 流程)。SBL 與既有 `securities_lending_tw` 來源
`TaiwanStockSecuritiesLending`(借券成交明細,trade-level)是不同 dataset:
後者是「借入交易」per-trade,前者是「借券賣出」daily aggregate 含 short_sales /
returns / current_day_balance,兩者並存,trigger 名稱也不同
(`mark_margin_derived_from_sbl_dirty` vs `mark_margin_derived_from_short_sale_dirty`)。

### B. alembic `p5q6r7s8t9u0_pr21_b_bronze3_derived5`

3 張新 Bronze raw 表 + 索引 + 3 個 Bronze→Silver dirty trigger:

```sql
CREATE TABLE government_bank_buy_sell_tw (
    market    TEXT NOT NULL,
    stock_id  TEXT NOT NULL,
    date      DATE NOT NULL,
    buy       BIGINT,
    sell      BIGINT,
    PRIMARY KEY (market, stock_id, date)
);

CREATE TABLE total_margin_purchase_short_sale_tw (
    market                          TEXT NOT NULL,
    date                            DATE NOT NULL,
    total_margin_purchase_balance   BIGINT,
    total_short_sale_balance        BIGINT,
    PRIMARY KEY (market, date)         -- market-level,無 stock_id
);

CREATE TABLE short_sale_securities_lending_tw (
    market                TEXT NOT NULL,
    stock_id              TEXT NOT NULL,
    date                  DATE NOT NULL,
    short_sales           BIGINT,
    returns               BIGINT,
    current_day_balance   BIGINT,
    PRIMARY KEY (market, stock_id, date)
);
```

3 個 trigger:

| trigger 名 | 來源 Bronze | 模式 | 目標 Silver |
|---|---|---|---|
| mark_institutional_derived_from_gov_bank_dirty | government_bank_buy_sell_tw | generic `trg_mark_silver_dirty('institutional_daily_derived')` | institutional_daily_derived |
| mark_market_margin_derived_from_total_dirty | total_margin_purchase_short_sale_tw | reuse 既有 `trg_mark_market_margin_dirty()`(2-col PK 函式 body 一致,可服務多 source Bronze) | market_margin_maintenance_derived |
| mark_margin_derived_from_short_sale_dirty | short_sale_securities_lending_tw | generic `trg_mark_silver_dirty('margin_daily_derived')` | margin_daily_derived |

設計重點:**0 新 trigger function**,2 generic + 1 reuse 既有。reuse 是因為
`trg_mark_market_margin_dirty()` 函式 body 只讀 `NEW.market` / `NEW.date`,
2 個 source Bronze 都有這 2 個欄,服務任一 source 都正確。

### C. collector.toml 3 個 dual-write entry

對映 alembic 落地的 3 張 Bronze。phase 分配:

| entry | param_mode | phase | segment_days | enabled |
|---|---|---|---|---|
| government_bank_buy_sell_v3 | per_stock | 5 | 365 | true |
| total_margin_purchase_short_sale_v3 | all_market | 6 | 365 | true |
| short_sale_securities_lending_v3 | per_stock | 5 | 365 | true |

`field_rename` best-guess 對齊 FinMind 常見命名(e.g.
`TotalMarginPurchaseTodayBalance` → `total_margin_purchase_balance`,
`ShortSales` → `short_sales`);user 首跑 smoke test 對齊真實欄名,必要時調整。

⚠️ 首次 backfill ~30-40h calendar-time(2 個 per_stock × 1700+ stocks × 21 年
+ 1 個 all_market × 21 年 @ 1600 reqs/h)。user 想推遲:把 3 個 enabled 改 false,
等準備好再切回 true。

### D. 3 個 Silver builder LEFT JOIN 補欄

3 個 builder 改成讀 2 張 Bronze,以 (market, stock_id, date) 或 (market, date)
LEFT JOIN 將新 Bronze 補進衍生欄;新 Bronze 缺 row → 衍生欄 NULL,不影響其他
stocks/dates 的 pivot/合成。

| builder | 主 Bronze | 副 Bronze(LEFT JOIN) | LEFT JOIN key | 補進衍生欄 |
|---|---|---|---|---|
| institutional | institutional_investors_tw | government_bank_buy_sell_tw | (market, stock_id, date) | gov_bank_net = buy - sell(任一 NULL → NULL) |
| market_margin | market_margin_maintenance | total_margin_purchase_short_sale_tw | (market, date) | total_margin_purchase_balance / total_short_sale_balance(1:1) |
| margin | margin_purchase_short_sale_tw | short_sale_securities_lending_tw | (market, stock_id, date) | sbl_short_sales_short_sales / _returns / _current_day_balance(1:1) |

`institutional._gov_bank_net(buy, sell)` 邊界處理(per spec §2.6.2「buy/sell 二
擇一,留 net」):任一 NULL → None。資料完整時才算 net,避免 0 視為缺失。

### E. verifier 更新(comments only,skip_silver_cols 結構不動)

`verify_pr19b_silver.py` + `verify_pr19c_silver.py` 的 `skip_silver_cols` 結構
**不動**,因為這 5 衍生欄是 Silver-only,legacy v2.0 表沒對應欄位 — round-trip
驗證仍要 skip,改的只是 comment 標明欄位來源(從「PR #19c 待補」改「PR #21-B
從 X Bronze fill,legacy 無對應欄」)。

(本 PR 不加新 verifier;user 直接 `psql` spot check Silver 衍生欄是否 NOT NULL
+ 對 Bronze raw 值即可,對齊 PR #18.5 / PR #21-A user-facing 流程。)

### F. 沙箱已驗

- alembic migration AST 解析 ✓ + chain `o4p5q6r7s8t9 → p5q6r7s8t9u0` ✓
- collector.toml `python -c "import tomllib; tomllib.load(...)"` ✓ + 3 entry 結構 OK
- 3 個 builder import ✓ + BRONZE_TABLES 各含 2 張(主 + 副)
- 3 個 builder transform 邏輯合成資料 smoke test 全綠:
  - institutional._gov_bank_net 6 個 case(normal / negative / 任一 NULL / 兩 NULL / 0)
  - institutional._pivot LEFT JOIN(2330 補 net=300,8888 缺 lookup → NULL)
  - market_margin._build_silver_rows LEFT JOIN(2026-05-01 補,2026-05-02 缺 → NULL)
  - margin._build_silver_rows LEFT JOIN(2330 補 SBL,8888 缺 → 3 SBL NULL)

### G. user 本機驗證流程

```powershell
# 1. 拉 + 落 schema
git pull
alembic upgrade head                        # p5q6r7s8t9u0(3 Bronze + 3 trigger)

# 2. validate config
python src/main.py validate
psql $env:DATABASE_URL -c "\dt *_tw"        # 應看到 11 張 Bronze(8 + 3 新)

# 3. smoke test 單股(5~10 分鐘,驗 dataset 名 + 欄位語意)
python src/main.py backfill --phases 5,6 --stocks 2330
# 注意觀察:
#   - government_bank_buy_sell_v3 / short_sale_securities_lending_v3 dataset 是否 200 OK
#   - 若 short_sale_securities_lending dataset 404:改 collector.toml dataset 名再跑
#   - 若 row count 為 0:檢查 field_rename 是否需調

# 4. 全市場 backfill(預期 30~40h calendar-time)
python src/main.py backfill --phases 5,6

# 5. 跑 Silver builder + spot check 5 衍生欄
python src/main.py silver phase 7a --stocks 2330 --full-rebuild
python src/main.py silver phase 7c                  # market-level builder

psql $env:DATABASE_URL -c "
SELECT stock_id, date, gov_bank_net
FROM institutional_daily_derived
WHERE stock_id='2330' ORDER BY date DESC LIMIT 5
"
psql $env:DATABASE_URL -c "
SELECT date, ratio, total_margin_purchase_balance, total_short_sale_balance
FROM market_margin_maintenance_derived ORDER BY date DESC LIMIT 5
"
psql $env:DATABASE_URL -c "
SELECT stock_id, date,
       sbl_short_sales_short_sales, sbl_short_sales_returns, sbl_short_sales_current_day_balance
FROM margin_daily_derived
WHERE stock_id='2330' ORDER BY date DESC LIMIT 5
"

# 6. 既有 round-trip 驗證仍應 5/5 OK + 5/5 OK(skip 5 衍生欄)
python scripts/verify_pr19b_silver.py
python scripts/verify_pr19c_silver.py
```

### H. 已知設計風險(首跑 smoke test 該驗的)

1. **gov_bank Bronze 假設 1 row/(stock,date)**:若 FinMind 回 8 家行庫各自一筆
   (`bank_name` 維度),會踩 PK 衝突。修法:加 `bank_name` 進 PK + builder 加
   aggregate(SUM(buy) / SUM(sell))。
2. **short_sale_securities_lending dataset 名候選**:`TaiwanStockShortSaleSecuritiesLending`
   是依 FinMind 命名慣例 best-guess。若 404,常見替代名:
   - `TaiwanStockShortSaleBalance`(spec hint 提過)
   - `TaiwanStockShortSale`
   - `TaiwanDailyShortSaleBalances`
3. **3 個 entry field_rename 都是 best-guess**:user 跑 smoke test 看
   `api_sync_progress.status`(預期 `completed`)+ Bronze 表 row count 是否 > 0,
   不對就調 field_rename。

### I. user 本機 smoke test 揭露 2 個 hotfix(2026-05-06)

User 跑 `alembic upgrade head` + `backfill --phases 5,6 --stocks 2330` 揭露:

| Bronze | 結果 | 原因 |
|---|---|---|
| `total_margin_purchase_short_sale_tw` | ✅ 1778 rows | dataset 名 + field_rename 都對 |
| `government_bank_buy_sell_tw` | ❌ HTTP 400 × 8 | FinMind tier 限制 |
| `short_sale_securities_lending_tw` | ❌ HTTP 422 × 8 | candidate dataset 名不存在 |

寫 `scripts/probe_finmind_datasets.py` 探 FinMind `/datalist` + 候選名,揭露:

**1. gov_bank 需 sponsor tier**:
```
"Your level is backer. Please update your user level"
```
`TaiwanStockGovernmentBankBuySell` dataset 名 valid(從別個 422 enum error 也能看到列在
allowed datasets 裡),但 user 是 `backer` tier,該 dataset 需 `sponsor` 訂閱。

**Hotfix**:collector.toml `government_bank_buy_sell_v3` 設 `enabled = false` + 註明
原因。Bronze schema + trigger 已落,等 user 升 FinMind tier 後切回 true 即可。
Silver `institutional_daily_derived.gov_bank_net` 維持 NULL(builder LEFT JOIN
缺 row → NULL,行為對齊 PR #21-B 之前)。

**2. SBL 真實 dataset 是 `TaiwanDailyShortSaleBalances`**:
回 15 個欄位(Margin + SBL 兩組),我們要的 3 個 SBL 欄是:
- `SBLShortSalesShortSales` → `short_sales`
- `SBLShortSalesReturns` → `returns`
- `SBLShortSalesCurrentDayBalance` → `current_day_balance`

額外 4 個 SBL 欄(PreviousDayBalance / Quota / ShortCovering / Adjustments)spec
§2.6.1 明文要砍,db.upsert PRAGMA filter 自動 drop 不入 Bronze。

**Hotfix**:collector.toml `short_sale_securities_lending_v3` 改:
- `dataset` = `TaiwanStockShortSaleSecuritiesLending`(404)→ `TaiwanDailyShortSaleBalances`
- `field_rename` = 3 個 SBL\* → 3 個 Silver 欄名

api_sync_progress 既有 8 segment failed 紀錄,user 下次跑 backfill 自動 retry
(phase_executor 對 `failed` status 會重試)。Bronze schema / trigger / Silver builder
不動。

**3. backfill 規模調整**:30~40h → ~22h
原估 3 dataset × 1700+ stocks × 21 年。實際:
- gov_bank disabled — 0
- total_margin all_market — 已完成
- SBL per_stock — 1700 × 21 ≈ 35700 reqs @ 2.25s/req ≈ 22h

### J. user 跑 ~17.5h SBL 全市場 backfill + 4 個後續 hotfix(2026-05-08)

User SBL 全市場 backfill 跑完(actual 17.5h vs 預估 22h),`short_sale_securities_lending_tw`
1,848,375 rows。spot check 揭露 3 個衍生欄(sbl_short_sales_*)在 Bronze 有資料的
範圍 fill rate 99.21%,但 total_*_balance(market_margin)0%、且 PR #18 5 張
Bronze 中 4 張(margin / foreign_holding / day_trading / valuation)沒被全市場
反推過(只跑過 prototype),Silver vs legacy round-trip 4 張 FAIL。

4 個 hotfix 同 session 收(commit 順序):

**1. (`f36838c`)total_margin Bronze schema 重建 — pivot-by-row**
   FinMind probe(`scripts/probe_finmind_datasets.py`)揭露 `TaiwanStockTotalMarginPurchaseShortSale`
   真實格式不是預期的 wide row,而是 pivoted-by-row:
   ```
   {date, name='MarginPurchase', TodayBalance, YesBalance, buy, sell, Return}
   {date, name='ShortSale',      TodayBalance, ...}
   ```
   alembic `q6r7s8t9u0v1` DROP+重建 Bronze:
   - PK 從 (market, date) → (market, date, name)
   - 砍 total_margin_purchase_balance / total_short_sale_balance(這 2 個是 Silver 衍生欄)
   - 加 today_balance / yes_balance / buy / sell / return_amount(`Return` SQL 保留字 → return_amount)
   - 重建 mark_market_margin_derived_from_total_dirty trigger(reuse 既有 function)
   builder market_margin 加 `_build_total_margin_lookup` pivot:
   `MarginPurchase.today_balance → total_margin_purchase_balance`,
   `ShortSale.today_balance → total_short_sale_balance`。

**2. (`d183201`)MarginPurchaseMoney silent skip + builder docstring 補述 PR #20 stub 行為**
   FinMind 自 2026-04-29 起新增第 3 個 name='MarginPurchaseMoney'(融資金額 NTD),
   spec §2.6.3 不需。`KNOWN_SKIP_NAMES = {'MarginPurchaseMoney'}` silently skip,
   未知 name 仍走 warning(防衛 FinMind 之後再加新 metric 不被 silently 吃掉)。

**3. (`3e3eb61`)reverse_pivot foreign_holding declare_date sanitize**
   user 跑 `reverse_pivot_foreign_holding.py` 全市場炸 InvalidDatetimeFormat —
   legacy detail JSONB 對未申報 stock 把 `RecentlyDeclareDate` 存 `'0'`(FinMind
   missing 占位),反推進 Bronze DATE 欄收不下。`_reverse_detail_unpack()` 對
   `DATE_DETAIL_KEYS = {'declare_date'}` 走 `_sanitize_date()`:None / '0' / '' /
   non-ISO 字串 → None。

**4. (`295ab70`)+(`973fd8b`)round-trip / verify_pr19b semantics 修正**
   - 295ab70:`_normalize_detail` 對 DATE_DETAIL_KEYS 內的 key 把 '0' / '' 視為
     等價 None(配合 sanitize lossy 設計);verify_pr18_bronze foreign_holding
     不再 6916 value_diffs FAIL。
   - 973fd8b:verify_pr19b `match` 條件砍 `extra_in_silver`(只看 missing + value_diffs)。
     原因:legacy v2.0 deprecated 不每日 dual-write,Silver 可能比 legacy 新;且
     PR #20 trigger 從 SBL Bronze 建 stub Silver row(margin_* NULL,sbl_* 填),
     legacy 自然沒這些 PK。9706 extra in silver 全是 5/4-5/7 dates 的 SBL-only stub。

User 順手把 4 張 Bronze 反推全跑(PR #18 follow-up 收尾):
```powershell
python scripts/reverse_pivot_margin.py           # 1840528 rows
python scripts/reverse_pivot_foreign_holding.py  # 1933954 rows
python scripts/reverse_pivot_day_trading.py      # 1728615 rows
python scripts/reverse_pivot_valuation.py        # 1728675 rows
```

### 最終驗證結果(2026-05-08 收尾)

| 驗證項 | 結果 |
|---|---|
| **PR #18 reverse-pivot round-trip**(`verify_pr18_bronze.py`)| **5/5 OK** ✅ |
| **Silver builders Phase 7a/7b/7c**(全市場 full-rebuild)| **12 + 1 + 1 OK** ✅ |
| **PR #19b Silver vs legacy round-trip**(`verify_pr19b_silver.py`)| **5/5 OK** ✅(margin +9706 extra 屬 stub,不算 FAIL)|
| **PR #21-B 5 衍生欄 fill rate** | **4/5 ~99%** ✅ + 1 blocked |
|   `market_margin.total_margin_purchase_balance` | **99.44%**(1771/1781)|
|   `market_margin.total_short_sale_balance` | **99.44%**(同上)|
|   `margin.sbl_short_sales_short_sales` | **99.21%**(對 Bronze 有資料範圍)|
|   `margin.sbl_short_sales_returns` | **99.21%** |
|   `margin.sbl_short_sales_current_day_balance` | **99.21%** |
|   `institutional.gov_bank_net` | 🔒 0%(blocked on FinMind sponsor tier — user 不升)|

**Silver 表最終 row count**:
- foreign_holding_derived: 1,933,954(從 8,957 → 全市場)
- margin_daily_derived: 1,850,234(從 1,850,169 微增 — SBL trigger 加 stub)
- day_trading_derived: 1,728,615(從 8,690 → 全市場)
- valuation_daily_derived: 1,728,675(從 8,881 → 全市場)
- institutional_daily_derived: 1,825,884
- market_margin_maintenance_derived: 1,781

### 已知狀態(下次 session 起點)

- alembic head:`q6r7s8t9u0v1`(user 已落)
- PR #21-B + PR #18 follow-up:**完整收尾** ✅
- 5 衍生欄缺口:1/5 永久 N/A(gov_bank_net on backer tier);其他 4/5 ~99% fill
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅(回頭補完 4 張)→ #19a ✅ → #19b ✅
  → #18.5 ⚠️(smoke ✓)→ #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → #20 ✅(15/15)
  → #21-A ✅ → **#21-B ✅(4/5 衍生欄收尾,gov_bank 永久 N/A)** → #22

下個 session 建議:
1. **PR #22 / B-1/B-2** market_ohlcv_tw dual-source merge
   (`TaiwanStockTotalReturnIndex` + `TaiwanVariousIndicators5Seconds` → daily OHLCV);
   完成後 `taiex_index_derived` 才有真資料(目前 read=0 wrote=0 是 source-empty)
2. **PR #21 完整收尾** — 1~2 sprint 後砍 §5.6 deprecated 路徑
   (`post_process.invalidate_fwd_cache` + `bronze/dirty_marker.mark_silver_dirty`)
3. **margin / market_margin builder UNION 升級**(可選 nice-to-have)
   讓 builder iterate(主 Bronze ∪ 副 Bronze)keys,避免 Silver 永遠有 stub row。
   現狀的 9706 stub row 不影響 4/5 衍生欄 fill rate,只是 Silver row count 偏多。

---

## v1.18 — PR #21-A 兩個衍生欄補齊 + 雜項收尾(2026-05-04 後續)

接 PR #20 user 本機 `verify_pr20_triggers.py` **15/15 OK**(commit `57b2a6c` 補
`db.create_writer` load_dotenv 同步落地)後動工 PR #21。完整 PR #21 scope 切
兩段:

| 切片 | 範圍 | 估時 | 阻塞 |
|---|---|---|---|
| **PR #21-A 本 session** ✅ | 2 個 builder-only 衍生欄(`market_value_weight` + `day_trading_ratio`)+ B-1/B-2 雜項收尾(SCHEMA_VERSION drift / CLAUDE.md 下次 session 段落 / db.create_writer load_dotenv) | ~半天 | 低 |
| PR #21-B 下 session | 3 個需新 Bronze 的衍生欄(`gov_bank_net` / `total_*_balance` / SBL `sbl_short_sales_*`)+ 30~40h calendar-time backfill;走 PR #18.5 dual-write pattern | ~1 天 + backfill | 中 |

### A. `valuation.market_value_weight`(spec §2.6.4)

公式:`(close × total_issued) / SUM_market_date(close × total_issued)`,範圍
[0, 1]。`close` 取 `price_daily`,`total_issued` 取 `foreign_investor_share_tw`。
INNER JOIN 分母,LEFT JOIN 分子(stock 沒 close 或沒 total_issued → mv = NULL
→ weight = NULL,不貢獻分母)。

關鍵設計:**分母永遠對全市場聚合**(不受 `--stocks` 過濾影響),這樣 partial
backfill 也能算出正確的 weight。實作走 2 query 拼接:

```sql
-- query A:全市場 total per (market, date)— 永遠不過濾 stock_id
SELECT v.market, v.date, SUM(pd.close * fis.total_issued) AS total_mv
FROM valuation_per_tw v
JOIN price_daily pd USING (market, stock_id, date)
JOIN foreign_investor_share_tw fis USING (market, stock_id, date)
GROUP BY v.market, v.date

-- query B:per-stock(可過濾 stock_id)
SELECT v.market, v.stock_id, v.date, v.per, v.pbr, v.dividend_yield,
       (pd.close * fis.total_issued) AS mv
FROM valuation_per_tw v
LEFT JOIN price_daily pd ON ...
LEFT JOIN foreign_investor_share_tw fis ON ...
WHERE v.stock_id IN (...)   -- 只在 stock_ids 給的時候加
```

Python 端 `_build_silver_rows(per_stock, market_totals)` stitch:
`weight = mv / market_totals[(market, date)]`,total > 0 才算。

### B. `day_trading.day_trading_ratio`(spec §7.4)

公式:`(buy + sell) × 100 / volume`,單位 %。volume 取 `day_trading_tw.volume`
(Bronze raw FinMind 已含,不必跨表 join `price_daily.volume`)— 確認過兩邊
語意一致。

實作純 Python `_compute_ratio(buy, sell, volume)`:
- 任一 NULL → None
- volume <= 0 → None
- 其他 → `(buy + sell) * 100 / volume`

### C. alembic `o4p5q6r7s8t9_pr21_a_day_trading_ratio`

PR #19a silver14 schema 漏了 `day_trading_derived.day_trading_ratio` column
(其他 4 個衍生欄如 `gov_bank_net` / `market_value_weight` / `total_*_balance` /
SBL 6 都有先放佔位 column,只是 PR #19b/c 寫 NULL)。本 migration 補:

```sql
ALTER TABLE day_trading_derived
    ADD COLUMN IF NOT EXISTS day_trading_ratio NUMERIC(10, 4);
```

`schema_pg.sql` 同步 inline 加進 day_trading_derived DDL。

### D. verifier 更新

`scripts/verify_pr19b_silver.py` 的 day_trading VerifySpec 加
`skip_silver_cols=("day_trading_ratio",)`(legacy v2.0 表無對應欄)。
valuation 那組已有 `skip_silver_cols=("market_value_weight",)`,不變。

### E. B-1 + B-2 雜項收尾(同 PR)

- **`SCHEMA_VERSION` drift**:`src/db.py:54` 一直留在 `"2.0"`,但 PG 端早被
  alembic `c2d3e4f5g6h7` bump 到 `"3.2"`(rust_bridge `EXPECTED_SCHEMA_VERSION`
  也是 `"3.2"`)。本 commit 把 db.py 那行對齊 + 同步更新 rust_bridge.py:133
  docstring example 的 stale `"2.0"`。
- **CLAUDE.md「下次 session 建議優先序」重寫**:原段落是 PR #19 收尾時寫的
  狀態,推 PR #20 為下個任務 + 1267~1284 行有上次 edit 的重複 bullet 殘留。
  全段重寫對齊 v1.18 後事實。
- **prelude `verify_pr20_triggers.py` 預期 16/16 → 15/15**:fwd 是 1 個 subtest
  不是 2 個,我寫 PR #20 v1.17 段時多算。

### F. 沙箱已驗

- builder AST 解析 ✓ + 兩個 _compute_* helper smoke test 全綠:
  - `_compute_ratio(100, 200, 1000) == 30.0` 等 6 個 case ✓
  - `_build_silver_rows` stitch 正確算出 weight = 0.5 / 0.25 / None 三 case ✓
- alembic chain `n3o4p5q6r7s8 → o4p5q6r7s8t9` ✓

User 本機驗證流程:

```powershell
git pull
alembic upgrade head                                  # o4p5q6r7s8t9
python src/main.py silver phase 7a --stocks 2330 --full-rebuild
# 預期:valuation / day_trading 兩個 builder OK,Silver 表新欄位有值

# spot-check 數值合理性
psql $env:DATABASE_URL -c "
SELECT stock_id, date, per, pbr, market_value_weight
FROM valuation_daily_derived
WHERE market='TW' AND stock_id='2330'
ORDER BY date DESC LIMIT 5
"
psql $env:DATABASE_URL -c "
SELECT stock_id, date, day_trading_buy, day_trading_sell, day_trading_ratio
FROM day_trading_derived
WHERE market='TW' AND stock_id='2330'
ORDER BY date DESC LIMIT 5
"
python scripts/verify_pr19b_silver.py                # 仍 5/5 OK(skip 新欄)
```

### G. PR #21-B 留 follow-up(下 session 動工)

3 個衍生欄需新 Bronze 表 + collector.toml dual-write entry + 30~40h backfill。
走 PR #18.5 同 pattern:

| 衍生欄 | 新 Bronze 表 | FinMind dataset |
|---|---|---|
| `institutional.gov_bank_net` | `government_bank_buy_sell_tw` | `TaiwanStockGovernmentBankBuySell`(候選名,需確認) |
| `market_margin.total_margin_purchase_balance` / `total_short_sale_balance` | `total_margin_purchase_short_sale_tw` | `TaiwanStockTotalMarginPurchaseShortSale` |
| `margin.sbl_short_sales_*`(3 欄) | (待研究)| 現 `securities_lending_tw` 是 trade-level transaction,缺 daily 累計;可能要新 `TaiwanStockShortSaleBalance` 之類 |

User 本機需排 30~40h 跑首次 backfill,流程同 v1.13 PR #18.5。

### H. user 本機驗收 + day_trading_ratio hotfix(2026-05-05)

User 跑完三段流程,verify 都通,但 spot-check 揭露兩問題:

1. **`day_trading_ratio` 對 2330 = 436758%**(明顯爆表)
   - Root cause:builder 公式 `(buy + sell) × 100 / volume` 把**金額(NTD)** 除以
     **股數**。FinMind `TaiwanStockDayTrading` 欄位語意:
     - `day_trading_buy / sell` = `BuyAmount / SellAmount`(NTD 金額)
     - `day_trading_tw.volume` = `Volume`(當沖股數)
   - Spec §7.4 真正的公式是 `day_trade_volume / total_volume × 100`,
     分子是「當沖股數」(`day_trading_tw.volume`),分母是「全日股數」
     (`price_daily.volume`)。
   - **Hotfix**(本次 commit):builder 改加 LEFT JOIN `price_daily`,
     `_compute_ratio(dt_volume, pd_volume)` = `dt_volume × 100 / pd_volume`。
     BRONZE_TABLES 從 `["day_trading_tw"]` → `["day_trading_tw", "price_daily"]`。
   - 修完 smoke test 通(`12M / 30M = 40%` 合理範圍)。

2. **`market_value_weight` 對 2330 = 0.995**(預期 ~25-30%)
   - Root cause:**不是 code bug**,是 dev env 的 `valuation_per_tw` 只反推填了
     ~5 檔(8881 row / 1776 date ≈ 5 stocks)。SUM 分母只算這 5 檔,2330
     在這 5 檔裡的市值佔比就是 99.5%。production 全市場 backfill 後 2330
     在 1700+ 檔裡的佔比才會回到 ~25-30%。
   - **Doc-only**:builder docstring 加 dev env caveat 段落,提醒「分母是
     valuation_per_tw 內所有 stock 聚合,partial dev backfill 會使分母偏小」。
   - 不改 code 邏輯(production 行為正確)。

修完 user 重跑驗收(2026-05-05):

```
2330 2026-04-29  buy=26260025000 sell=26356290000 ratio=24.5120 ✓
2330 2026-04-28  buy=21707825000 sell=21706385000 ratio=16.8533 ✓
2330 2026-04-27  buy=47410225000 sell=47564645000 ratio=25.8767 ✓
2330 2026-04-24  buy=25458850000 sell=25601855000 ratio=22.6390 ✓
2330 2026-04-23  buy=33146195000 sell=33188310000 ratio=28.8539 ✓
```

ratio 落在 16~28% 合理區間(2330 normal day 當沖率)。**PR #21-A 完整 verify pass**。

### I. 兩個候補 backlog 同 PR 收(2026-05-05)

User verify pass 後趁手感熱把「下次 session 建議優先序」中的兩個小項一起做掉:

**1. `bronze/phase_executor.py` 拆段**(blueprint §三 結構工)
- `git mv src/phase_executor.py src/bronze/phase_executor.py`
- `src/main.py:40` import 從 `from phase_executor import PhaseExecutor`
  改成 `from bronze.phase_executor import PhaseExecutor`
- 內部 import `aggregators` / `api_client` / `config_loader` 等 `src/` root
  modules 不變(pyproject.toml editable install 已把 `src/` 加進 sys.path,
  從 `src/bronze/` 也能正常 resolve)
- 對齊 `src/silver/orchestrator.py` 的 Phase 7 排程結構;phase 1-6 屬 bronze,
  phase 7 屬 silver,各自一個檔
- 沙箱已驗:`from bronze.phase_executor import PhaseExecutor` 解析 ✓
  (後續 aiohttp ImportError 是沙箱沒裝 3rd-party,不是 code bug)
- 全 repo grep 無 stale 引用(`grep -rn "from phase_executor\|import phase_executor"`)

**2. `scripts/inspect_db.py` 升 PG 版**
- 從 v1.6 之前的 SQLite hardcode 改用 `create_writer()`(走 .env 自動 load_dotenv)
- 完整重寫 441 → 290 行,砍掉:
  - 後復權正確性驗證段(`adjustment_factor` 欄已在 PR #17 砍掉,改 `scripts/av3_spot_check.sql` 做完整驗證)
  - SQLite-specific `sqlite_master` query
  - 各種 v2.0-only printout 細節
- 加 `TABLE_GROUPS` dict 分 5 組:Reference / Bronze / Silver / Legacy v2.0 / System
- 加 schema_version 印在開頭
- 加 Silver `*_derived` 主要表 latest row spot-check
- `_fmt_date` / `_fmt_num` helpers 處理 psycopg `datetime.date` / `Decimal` → str
- 沙箱已驗:`_fmt_*` helpers 7/7 OK + 模組 import OK + `TABLE_GROUPS` 5 組共 55 表

### 已知狀態(下次 session 起點)

- alembic head:`o4p5q6r7s8t9`(user 已落)
- PR #21-A:**code 已 user verify pass**,`day_trading_ratio` hotfix + 2 候補
  backlog(bronze/phase_executor 拆段 + inspect_db.py 升 PG 版)同 PR 收尾。
- 5 衍生欄缺口:2 補(market_value_weight / day_trading_ratio + 1 hotfix)
  + 3 留 PR #21-B
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️(smoke ✓)
  → #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → #20 ✅(15/15)→ **#21-A ✅(完整收尾)**
  → #21-B → #22

下個 session 建議:
1. **PR #21-B** 動工:3 條新 Bronze + 30~40h backfill 計畫(需 user 排日曆時間)
2. **B-1/B-2 收尾** market_ohlcv_tw dual-source merge(下次主要結構工)
3. (可選)拿 production 全市場 backfill 後資料 spot-check `market_value_weight`
   對 2330 確認 ~25-30%(不擋 PR sequencing)

---

## v1.17 — PR #20 Bronze→Silver dirty trigger ENABLE(2026-05-04)

接 PR #19c-3 後動工 PR #20(blueprint v3.2 r1 §5.5 + §5.7)。
PR #19a 落了 14 張 Silver `*_derived` schema + dirty 欄位 + partial index 但
**不啟用 trigger**(避免 Bronze 雙寫期間每筆 upsert 都觸發級聯)。本 PR 把
Bronze→Silver dirty trigger 接上,讓 dirty queue 真正生效。

### A. alembic migration `n3o4p5q6r7s8_pr20_silver_dirty_triggers`

6 個 trigger function + 15 個 CREATE TRIGGER。Bronze 表 PK shape 不齊,4 個變體
+ fwd 全段歷史 mark 各自一個 function。

| function | 涵蓋 | 形狀 |
|---|---|---|
| `trg_mark_silver_dirty(silver_table)` | 10 generic 3-col | (market, stock_id, date) UPSERT 進 silver_table |
| `trg_mark_financial_stmt_dirty()` | financial_statement | 4-col PK,Bronze.event_type ↔ Silver.type |
| `trg_mark_exchange_rate_dirty()` | exchange_rate | (market, date, currency)無 stock_id |
| `trg_mark_market_margin_dirty()` | market_margin_maintenance | (market, date)2-col |
| `trg_mark_business_indicator_dirty()` | business_indicator | Bronze 2-col → Silver 注 sentinel `'_market_'` |
| `trg_mark_fwd_silver_dirty()` | price_adjustment_events | UPDATE 4 fwd 表整檔 dirty(全段歷史 mark) |

15 個 trigger:10 generic + 5 special。pae 1:4 fanout 處理「multiplier 倒推設計,
新除權息會回頭改全段歷史值」的硬約束。

### B. §5.6 短期補丁路徑 deprecate + cut

- `post_process.invalidate_fwd_cache`:加 `DeprecationWarning`(寫入仍照舊以
  避免 emergency manual ops 直接斷掉),PR #21 完全移除。
- `phase_executor._run_phase`:price_adjustment_events 寫入後**移除**
  `invalidate_fwd_cache(stock_id)` call(trigger 接管,call 是 redundant)。
- `post_process.dividend_policy_merge`:同樣移除 `invalidate_fwd_cache(db, stock_id)`
  call(trigger 接管)。
- `bronze/dirty_marker.mark_silver_dirty`:由 stub no-op 改為 deprecated 路徑,
  emit `DeprecationWarning`,PR #21 移除。

短期補丁路徑由 trigger 接管的證明:av3 揭露的 staleness production bug
(3363 / 1312 stock_dividend 事件 fwd 沒處理)在 PR #20 後由
`trg_mark_fwd_silver_dirty` 直接處理 — pae INSERT/UPDATE → 4 fwd 表整檔 dirty
→ Phase 7c orchestrator 從 `price_daily_fwd.is_dirty=TRUE` 拉清單派 Rust。

### C. orchestrator 7c 改走 dirty queue

`silver/orchestrator.SilverOrchestrator._run_7c` 行為變更:

| stock_ids | full_rebuild | 行為 |
|---|---|---|
| 明確傳 list | any | pass through(manual ops / 開發測試) |
| None | False | `SELECT DISTINCT stock_id FROM price_daily_fwd WHERE is_dirty=TRUE` 拉清單派 Rust |
| None | True | `SELECT DISTINCT stock_id FROM price_daily_fwd` 全部派 Rust(全市場重算) |

dirty queue 為空 → skip Rust dispatch + log,不 raise。

PR #19c-3 的 `_run_7c(stock_ids)` 現在多收 `full_rebuild` 參數;run() 多帶一條
傳遞路徑。

### D. 整合測試 `scripts/verify_pr20_triggers.py`

15 個 subtest 對映 15 個 trigger:
- 10 generic 3-col Bronze → 同 PK Silver
- financial_statement(event_type → type)
- exchange_rate(currency PK)
- market_margin(2-col PK)
- business_indicator(注 sentinel '_market_')
- price_adjustment_events → 4 fwd 表整檔 dirty(pre-INSERT 8 row × 4 表,驗 trigger 後全部 dirty)

Sentinel PK 慣例:`market="TW"`, `stock_id="__PR20__"`, `date="1900-01-01"`,
fwd 用 `"__PR20_FWD__"`;date 早於 FinMind 起算 1990 不衝突真實資料。
每 subtest 跑完先清 Silver 再清 Bronze(只有 INSERT/UPDATE 觸發 trigger,
DELETE 不觸發,cleanup 不會回頭再 mark)。

### E. schema_pg.sql sync

trigger DDL(6 function + 15 CREATE TRIGGER)同步 append 到 `src/schema_pg.sql`
尾段。docker compose 啟動新 PG 17 instance 時 `01-schema.sql` 會直接帶到。

### F. 沙箱限制 + user 本機驗證

沙箱無 PG instance,無法跑 `alembic upgrade head` 或 verifier。已驗:
- alembic migration AST 解析 ✓
- migration `revision` / `down_revision` chain 正確(`m2n3o4p5q6r7 → n3o4p5q6r7s8`)✓
- 6 functions / 10+5 triggers / 15 unique bronze tables / 15 unique trigger names ✓
- 4 個觸碰的 Python 檔(orchestrator / post_process / phase_executor / dirty_marker)AST 解析 ✓

User 本機驗證流程:

```powershell
git pull
alembic upgrade head                                # n3o4p5q6r7s8
psql $env:DATABASE_URL -c "
SELECT trigger_name, event_object_table FROM information_schema.triggers
WHERE trigger_name LIKE 'mark_%' ORDER BY trigger_name
"   # 應看到 15 個 trigger
psql $env:DATABASE_URL -c "
SELECT routine_name FROM information_schema.routines
WHERE routine_name LIKE 'trg_mark_%' ORDER BY routine_name
"   # 應看到 6 個 function
python scripts/verify_pr20_triggers.py              # 預期 15/15 OK(10 generic + 4 special + fwd 全段歷史)
alembic downgrade -1 && alembic upgrade head       # rollback smoke
```

### 已知狀態(下次 session 起點)

- alembic head:`n3o4p5q6r7s8`(待 user 本機 `alembic upgrade head`)
- 15 個 Bronze→Silver dirty trigger 上線;dirty queue 接管 §5.6 短期補丁路徑
- `invalidate_fwd_cache` / `mark_silver_dirty` deprecated 但保留 1~2 sprint
- orchestrator 7c 改走 dirty queue pull
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️(smoke ✓)→ #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → **#20 ⏳ 待 user verify** → #21 next

user 本機驗收結果(2026-05-04):**verify_pr20_triggers.py 15/15 OK** ✅
(alembic upgrade head 跑了 m2n3o4p5q6r7 → n3o4p5q6r7s8;一同 commit `57b2a6c`
補了 `db.create_writer` load_dotenv,verify_*.py 入口免再手動 `$env:DATABASE_URL`)。

下個 session 建議:**PR #21**:衍生欄補齊 + market_ohlcv_tw dual-source merge
+ bronze/phase_executor 拆段 + `invalidate_fwd_cache` / `mark_silver_dirty` 完全
移除(PR #20 觀察 1~2 sprint 後);詳見「下次 session 建議優先序」段。

---

## 分支狀態

> 2026-05-02 剪枝後僅留 4 條 remote(從 19 → 4)。所有已 merge / 已被 supersede 的 claude/* 工作分支已從 origin 刪除,歷史保留在 `claude/initial-setup-RhLKU` 的 commit log。

- **開發分支**:`claude/initial-setup-RhLKU`(**v1.9.1 active**)— 從 v1.5 到 v1.9.1 全部歷史 commit 都在這條,v1.9 (PR #17 主體) 是從 tblnC merge 進來的
- **目標分支**:`m1/postgres-migration`(v1.7 baseline,stale)
- **m2 spec 源**:`m2/neo-pipeline-spec-origin`(原始 m2 spec 上傳源,歷史保留)
- **GitHub default**:`main`(內容極舊,僅初版 spec)
- **已刪 remote 分支**(歷史 commit 都已 merge 進 initial-setup-RhLKU):
  - `claude/review-todo-list-tblnC`(v1.9 main session)
  - `claude/restructure-collector-architecture-t9ScN`(v1.8 收尾,PR #9)
  - `claude/m2-architecture-design-3Q3Fd`(user 主分支,Easy 階段 PR #10~#16)
  - `claude/m2-pr2-schema-bump-3.2` ~ `claude/m2-pr6-b6-businessindicator`(PR #10 ~ #14)
  - `claude/hotfix-b6-leading-keyword`(PR #15)
  - `claude/review-collector-dependencies-n03rE`(v1.7 PR)
  - `claude/collector-schema-mapping-2YF5U` / `claude/continue-work-dvkRv` / `claude/setup-agent-review-mcp-berOR`(v1.5/v1.6 探勘)
  - `claude/review-collector-spec-Gktcf`(早期 review 分支)
  - `collector`(早期 PR #4)
- **PR**:v1.9 + v1.9.1 + v1.10 + v1.11 + v1.12 + v1.13 + v1.14 + v1.15 + v1.16 PR 開於 initial-setup-RhLKU 分支

---

## v1.16 — PR #19c-3 orchestrator + Phase 7 CLI(cherry-pick from build-data-builders-4BwpT,2026-05-04)

意外發現 user 本機原本另有平行 Claude session 在 `claude/build-data-builders-4BwpT`
分支做 PR #19c part 1,**已經寫好我留給 PR #19c-3 的 orchestrator 真實邏輯 + main.py
silver phase 子命令**。差異盤點:

- 他們做的:5 個 market-level builder(版本不同)+ orchestrator + CLI
- 我做的:5 個 market-level builder(PR #19c-1)+ 3 個 PR #18.5 依賴 builder(PR #19c-2)+ PR #18.5 alembic + dual-write entries
- 沒重疊的:他們做了 orchestrator + CLI(我留 PR #19c-3),我做了 PR #19c-2 + PR #18.5(他們沒做)

**整合策略**:cherry-pick 他們的 orchestrator + main.py CLI(品質高,設計穩),
保留我所有 builder + PR #19c-2 + PR #18.5。CLAUDE.md 我自己寫(他們的 v1.14 版
跟我的 v1.13~v1.15 衝突)。

### A. src/silver/orchestrator.py 真實邏輯(從 stub 升)

`SilverOrchestrator.run(phases, stock_ids, full_rebuild)`:

- **串列跑 builder**(不是 asyncio.gather)— PostgresWriter 持單一 connection,
  concurrent thread access 踩 psycopg thread-safety 限制。要平行跑需先升 db
  connection pool,perf gain 在這層實際很小(每 builder ~ms 量級),**先求正確**,
  平行優化留後續 PR
- **NotImplementedError → status='skipped'** 不中斷其他 builder(防衛性,雖然
  13 個 builder 全實作)
- **Exception → status='failed'** + reason 紀錄,**也不中斷其他**(對齊
  cores_overview §7.5 dirty 契約:失敗 builder 不 reset is_dirty 留下次重試)
- **7c 派 rust_bridge.run_phase4** 給 tw_market_core 系列(price_*_fwd +
  price_limit_merge_events)
- 結構化回傳 dict for status table

### B. src/main.py silver 子命令

`python src/main.py silver phase 7a/7b/7c [--stocks ...] [--full-rebuild]`

- argparse 加 silver subparser + silver_phase_parser
- `_run_silver()` 函式獨立於 _run_collector,7c 才需要 RustBridge instance
- 印 status table(builder × status × read × wrote × ms),總計 ok/skipped/failed

### C. 沙箱整合驗證

- orchestrator + builders 套件 import 通(`from silver.orchestrator import SilverOrchestrator`)
- PHASE_GROUPS 對齊 BUILDERS 註冊表(7a 12 個 + 7b 1 個 = 13 builders 全部 covered)
- `builders_in_phase('7a' / '7b' / '7c')` classmethod 工作正常
- async run() 對 mock db 跑 7b phase 成功 dispatch 到 financial_statement builder
  → status='ok',rows_read=0(空 mock),不中斷

### D. 用戶本機驗證

```powershell
git pull
python src/main.py silver phase 7a --stocks 2330 --full-rebuild
# 預期:12 個 7a builder 全 ok 跑完(對齊 PR #19c-1 / PR #19c-2 已驗的邏輯),
# 印出 status table 含 builder name / status / rows_read / rows_written / ms

python src/main.py silver phase 7b --stocks 2330 --full-rebuild
# 預期:financial_statement builder 跑完 status=ok

# 7c(需 Rust binary)
python src/main.py silver phase 7c --stocks 2330
# 預期:派 rust_bridge.run_phase4 給 Rust binary,跑後復權系列
```

### E. 平行分支留下的東西沒撈過來

`origin/claude/build-data-builders-4BwpT` 還在 origin,但不再需要:
- 他們版本的 5 個 market-level builder(我自己版本已驗 PR #19c-1)
- 他們版本的 verify_pr19c_silver_5.py 空 Bronze 改 skip(orchestrator 已用同樣
  pattern handle skipped/failed,verify 改善是純 UX 沒急迫性,留 follow-up)
- 他們的 CLAUDE.md v1.14(跟我這邊 v1.13~v1.15 太多衝突,不撈)

PR #19c 主要工作完成,留給後續 PR 的:
- 5 個衍生欄補齊(SBL 6 / gov_bank_net / market_value_weight / day_trading_ratio
  / market_margin total_*_balance)
- bronze/phase_executor.py 從 src/phase_executor.py 拆出
- verify scripts 統一空 Bronze 處理(skip vs abort 的 UX)
- asyncio.gather 7a 平行優化(需 db connection pool 升級)

### 已知狀態(下次 session 起點)

- silver/orchestrator.py 真實邏輯落地 ✓
- silver phase 7a/7b/7c CLI 落地 ✓
- 13 個 builder 全部可被 orchestrator dispatch ✓
- **本機 verify 結果(stock 2330)**:Phase 7a 12/12 OK,Phase 7b 1/1 OK,Phase 7c rust_bridge 1 stock 處理完無 error
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5(smoke ✓)→ #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → **#20 ⏳ next**

### F. 已知 follow-up issue(non-blocking)

**taiex_index_derived 對 stock 2330 read=0 wrote=0** — 不是 builder bug,**Bronze
`market_ohlcv_tw` 從來沒被 populate**(collector.toml 沒對應 API entry,只有
v2.0 `market_index_tw` entry 寫到 legacy 表)。Blueprint §四 注解寫:

> market_ohlcv_tw 來源:TaiwanStockTotalReturnIndex(close)+
> TaiwanVariousIndicators5Seconds(intraday 5-sec aggregate to daily OHLCV)
> **multi-source merge 邏輯留待 PR #17 重構 phase_executor 時實作**

屬於 B-1/B-2(PR #11 era)沒收尾的 known incomplete,不是 PR #19 引入。
TAIEX OHLCV 暫時沒 downstream consumer,**不阻塞 PR #20**。真要做時走「加
collector.toml dual-write entry + phase_executor 雙源 merge」一次到位。

builder 行為已正確:`taiex_index._build_silver_rows(empty_bronze)` 回 [] →
upsert 0 rows。Silver 表為空是 source-empty 的正確反映。

---

## v1.15 — PR #19c-2 Silver 3 個 PR #18.5 依賴 builder(2026-05-04)

接 PR #19c-1 後動工 PR #19c-2。原計畫切片 scope 仍太大,本 session 進一步縮:
**只完成 3 個 PR #18.5 依賴 builder + verifier**。orchestrator 真實邏輯 + CLI
整合 + bronze/phase_executor 拆段 + 衍生欄補齊留 PR #19c-3。

### A. 3 個 builder 從 stub 升實作

| builder | Silver 寫入 | Bronze 來源 | 邏輯 |
|---|---|---|---|
| holding_shares_per | holding_shares_per_derived | holding_shares_per_tw | Bronze N rows/level → Silver 1 row/(stock,date)+ detail JSONB pack levels(對齊 v2.0 aggregate_holding_shares) |
| monthly_revenue | monthly_revenue_derived | monthly_revenue_tw | Bronze raw FinMind 欄名 → Silver:revenue_year → revenue_yoy / revenue_month → revenue_mom rename;country / create_time(TEXT)pack 進 detail JSONB |
| financial_statement | financial_statement_derived | financial_statement_tw | Bronze N rows/(stock,date,event_type,origin_name)→ Silver 1 row/(stock,date,type)+ detail JSONB pack origin_name → value(對齊 v2.0 aggregate_financial)。Bronze.event_type → Silver.type |

### B. 完成 13 個 builder 全部從 stub 升實作

PR sequencing milestone:
- PR #19a:14 張 Silver schema + 13 builder stubs
- PR #19b:5 個 simple stock-level builder(institutional / margin / foreign_holding / day_trading / valuation)
- PR #19c-1:5 個 market-level builder(taiex / us / exchange_rate / market_margin / business_indicator)
- **PR #19c-2(本 session)**:3 個 PR #18.5 依賴 builder ✓

13 個 silver/builders/*.py 全部從 raise NotImplementedError 升實作。

### C. 驗證器 scripts/verify_pr19c2_silver.py

對齊 verify_pr19b_silver.py 模式:對 v2.0 legacy 表逐 PK 等值比對。

預設 stocks=`["1101","2317","2330"]`(對齊 PR #18.5 user smoke test 已 backfill
範圍)。Bronze 空表 sanity check 在跑 builder 前先過濾,點明該跑哪個 backfill。

### D. 沙箱合成資料測試

3 個 builder transform 邏輯通過:
- holding_shares_per:2 levels × 2 dates → pack into detail JSONB 正確
- monthly_revenue:revenue_year → revenue_yoy rename + 空字串 create_time pass-through
- financial_statement:income + balance 分開 group + origin_name 集合進 detail

### E. 用戶本機驗證(預期全綠)

```powershell
git pull
python scripts/verify_pr19c2_silver.py    # 預設 1101,2317,2330 — 預期 3/3 OK
```

### 已知狀態(下次 session 起點)

- 13 個 builder 全部實作完成 ✓
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️ (smoke test) → #19c-1 ✅ → **#19c-2 ⏳ 待 user verify** → #19c-3 → #20

PR #19c-3 留:
- silver/orchestrator.py 真實邏輯(asyncio.gather 7a 平行 + 7b 序列 + 7c rust_bridge)
- src/main.py 加 `silver phase 7a/7b/7c` 子命令
- bronze/phase_executor.py 從 src/phase_executor.py 拆出
- PR #19b/#19c-1 暫不填的衍生欄(institutional.gov_bank_net /
  margin SBL 6 / valuation.market_value_weight / day_trading_ratio /
  market_margin total_*_balance)— 部分需新 Bronze table + alembic migration
  (GovernmentBankBuySell / TotalMarginPurchaseShortSale)

---

## v1.14 — PR #19c-1 Silver 5 market-level builder(2026-05-04)

接 PR #18.5 schema + smoke test 後動工 PR #19c。完整 PR #19c 太大切兩段:

| 切片 | 範圍 | 估時 |
|---|---|---|
| **PR #19c-1 本 session** ✅ | 5 個 market-level builder(taiex_index / us_market_index / exchange_rate / market_margin / business_indicator)+ verify_pr19c_silver.py + fetch_bronze 補 order_by 參數 | ~半天 |
| PR #19c-2 下 session | 3 個 PR #18.5 依賴 builder(holding_shares_per / monthly_revenue / financial_statement)+ orchestrator 真實邏輯 + CLI 整合 + bronze/phase_executor 拆段 + PR #19b 衍生欄補齊(gov_bank_net / SBL 6 / market_value_weight / day_trading_ratio) | ~1 天 |

### A. 5 個 Silver builder 實作

| builder | Silver 寫入 | Bronze 來源 | 邏輯 |
|---|---|---|---|
| taiex_index | taiex_index_derived | market_ohlcv_tw | OHLCV 1:1 + detail JSONB 直拷 |
| us_market_index | us_market_index_derived | market_index_us | OHLCV 1:1(v2.0 legacy 表名,v3.2 後可能 rename us_market_index_tw) |
| exchange_rate | exchange_rate_derived | exchange_rate(legacy)| PK 含 currency 維度 (market, date, currency);rate + detail 1:1 |
| market_margin | market_margin_maintenance_derived | market_margin_maintenance | ratio 1:1;`total_margin_purchase_balance` / `total_short_sale_balance` 衍生欄 = NULL(留 PR #19c-2 接 TaiwanStockTotalMarginPurchaseShortSale Bronze) |
| business_indicator | business_indicator_derived | business_indicator_tw | 5 stored 1:1(`leading_indicator` 等避 PG 保留字後綴 PR #19a hotfix);PK 從 (market, date) → (market, '_market_', date)注入 sentinel stock_id |

### B. fetch_bronze 加 order_by 參數

`silver/_common.py:fetch_bronze` 原本 ORDER BY 寫死 `market, stock_id, date`,對 market-level 表(無 stock_id 欄)會炸。新增 order_by kwarg 預設保留舊行為,market-level 三 builder 明確傳 `"market, date"` 或 `"market, date, currency"` 覆蓋。

### C. 驗證器 scripts/verify_pr19c_silver.py

對齊 PR #19b verifier 模式,但比對對象從 v2.0 legacy 表改成 Bronze(因為 5 個 market-level Silver 是 1:1 直拷 Bronze,無 pivot/pack 過程):

- taiex_index / us_market_index / exchange_rate:OHLCV / rate + detail JSONB 等值
- market_margin:ratio 等值;skip 2 個 PR #19c-1 暫不填的衍生欄
- business_indicator:Bronze (market, date) ←→ Silver (market, '_market_', date),透過 `silver_stock_id_const = "_market_"` 在比對時加 sentinel 對齊

加 Bronze 空表 sanity check(對齊 verify_pr19b 同 trap)— 各表來源不同(market_ohlcv_tw 在 Phase 1 / market_index_us 在 Phase 6),空表時直接點明該跑哪個 Phase。

### D. 沙箱合成資料測試

5 個 builder transform 邏輯通過合成資料測試:
- taiex_index / us_market_index OHLCV pass-through ✓
- exchange_rate PK 含 currency ✓
- market_margin 2 衍生欄 = None ✓
- business_indicator stock_id = '_market_' sentinel + Decimal value pass-through ✓

### E. 用戶本機驗證(預期全綠)

```powershell
git pull
pip install -e .                                       # 已落地,no-op
python scripts/verify_pr19c_silver.py                  # 5/5 OK
```

### 已知狀態(下次 session 起點)

- 5 個 market-level Silver 表寫入 ✓
- 3 個 PR #18.5 依賴 builder + orchestrator 真實邏輯 + CLI + 衍生欄補齊留 PR #19c-2
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️ schema OK / smoke test 通 → **#19c-1 ⏳ 待 user verify** → #19c-2 → #20

---

## v1.13 — PR #18.5 Bronze refetch 3 張 schema + dual-write entries(2026-05-02 後續)

接 PR #19b 後處理 Option A 重抓的 3 張表(blueprint §八.1 + CLAUDE.md v1.10 §E follow-up)。
原因 detail JSONB unpack 不可逆:
- holding_shares_per:HoldingSharesLevel taxonomy 在 v2.0 detail 內,反推不知 level 完整集合
- financial_statement:中→英 origin_name 對應在 pack 過程丟失
- monthly_revenue:FinMind 1 row/股/月 不回更細粒度(其實可解 pivot 但保守歸 Option A)

### A. alembic migration `l1m2n3o4p5q6_pr18_5_bronze3_refetch`

3 張 Bronze raw 表:

| Bronze | PK | 邏輯 |
|---|---|---|
| `holding_shares_per_tw` | (market, stock_id, date, holding_shares_level) | 1 row per level(field_mapper 直拷,無 aggregation) |
| `financial_statement_tw` | (market, stock_id, date, event_type, origin_name) | event_type ∈ {income, balance, cashflow}— reuse pae convention,3 個 FinMind dataset 統一進這張 |
| `monthly_revenue_tw` | (market, stock_id, date) | raw FinMind 欄名(revenue_year / revenue_month — 不在 Bronze 改名,Silver builder PR #19c 才 rename → revenue_yoy / revenue_mom) |

每張加 `idx_<table>_stock_date_desc(stock_id, date DESC)` 給 PR #19c Silver builder。
schema_pg.sql 同步附 DDL;coexist 模式,legacy v2.0 表 T0+21 後砍。

### B. collector.toml dual-write 5 個新 entries

| name | dataset | target_table | event_type | 備註 |
|---|---|---|---|---|
| holding_shares_per_v3 | TaiwanStockHoldingSharesPer | holding_shares_per_tw | — | 1 row/level |
| monthly_revenue_v3 | TaiwanStockMonthRevenue | monthly_revenue_tw | — | raw FinMind 欄名 |
| financial_income_v3 | TaiwanStockFinancialStatements | financial_statement_tw | `income` | 走 field_mapper 既有 event_type 注入機制 |
| financial_balance_v3 | TaiwanStockBalanceSheet | financial_statement_tw | `balance` | 同上 |
| financial_cashflow_v3 | TaiwanStockCashFlowsStatement | financial_statement_tw | `cashflow` | 同上 |

per blueprint §八.2 dual-write 設計:v2.0 entries(holding_shares_per / monthly_revenue / financial_{income,balance,cashflow})保留 `enabled = true`,user 跑 `backfill --phases 5` 兩條 path 同時填,T0+21 後砍 v2.0。

### C. ⚠️ 首次 backfill ~30-40h calendar-time

新 5 個 entries 共 1700+ stocks × 21 年 segments @ 1600 reqs/h ≈ 30-40h。**user 規劃日曆時間**再跑首次 backfill。

延後選項:把 5 個新 entries 的 `enabled` 改 false,等準備好再 true。

### D. User 操作流程(本機)

```powershell
git pull
alembic upgrade head                                     # l1m2n3o4p5q6 — 3 張 Bronze
psql $env:DATABASE_URL -c "\dt *_tw"                     # 看到加 3 張(8 張總共)

# 規劃 30-40h 後跑(也可先 --stocks 2330 smoke test 30 分鐘):
python src/main.py backfill --phases 5 --stocks 2330    # 單股 smoke
python src/main.py backfill --phases 5                   # 全市場(30-40h)

# 驗證 row count:
psql $env:DATABASE_URL -c "
SELECT 'holding_shares_per_tw' AS t, COUNT(*) FROM holding_shares_per_tw
UNION ALL SELECT 'financial_statement_tw', COUNT(*) FROM financial_statement_tw
UNION ALL SELECT 'monthly_revenue_tw', COUNT(*) FROM monthly_revenue_tw
"
```

### E. 沙箱限制

- 沙箱無 FinMind 連線,無法跑驗證(pip install -e . + alembic upgrade 可在沙箱驗,但 backfill 必須本機)
- alembic migration syntax 沙箱已驗 OK
- collector.toml entries 結構對齊既有 v2.0 entries 的格式,user 本機跑 `python src/main.py validate` 應通

### 已知狀態(下次 session 起點)

- alembic head:`l1m2n3o4p5q6`(3 張 Bronze schema 已落,資料待 user 30-40h 重抓)
- collector.toml dual-write 5 entries 上線(enabled=true)
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → **#18.5 ⏳ schema 落地待 user 重抓** → #19c → #20

---

## v1.12 — PR #19b Silver 5 builder + pyproject.toml(2026-05-02 後續)

接 PR #19a scaffolding 後動工 PR #19b:5 個簡單 builder 從 stub 升實作,
並一次解掉 src layout 的 import friction(pyproject.toml + pip install -e .)。

### A. pyproject.toml(setuptools src layout)

```
[tool.setuptools]
package-dir = {"" = "src"}

[tool.setuptools.packages.find]
where   = ["src"]
include = ["silver*", "bronze*"]
```

`pip install -e .` 後:
- silver / silver.builders / bronze 套件 importable
- src/ 內 loose modules(api_client / db / main / phase_executor / ...)
  也 importable(setuptools editable .pth 把 src/ 加進 sys.path)
- 沙箱 + 用戶本機從 repo root 之外跑 `python -c "from silver ..."` 直接通,
  不再需要 `$env:PYTHONPATH = "src"`

alembic.ini `prepend_sys_path = .` 保留(讓 alembic env.py 仍可從 root 跑)。
Console script entry point 暫不加(`python src/main.py` 仍是 CLI 入口),
留待後續 PR 評估是否升級為 `tw-stock-collector` 全域 command。

### B. 5 個 Silver builder 實作(institutional / margin / foreign_holding / day_trading / valuation)

每個 builder 對應 PR #18 落地的 Bronze 表(已有真資料可驗 round-trip):

| builder | Silver 寫入 | Bronze 來源 | 邏輯 |
|---|---|---|---|
| institutional | institutional_daily_derived | institutional_investors_tw | pivot 5 投資人 row → 1 寬 row(10 buy/sell);gov_bank_net=NULL(PR #19c) |
| margin | margin_daily_derived | margin_purchase_short_sale_tw | 6 stored + detail JSONB 重 pack(8 keys)+ 3 margin_short_sales_* 別名 = short_*;3 SBL 欄 NULL(PR #19c 接 securities_lending_tw) |
| foreign_holding | foreign_holding_derived | foreign_investor_share_tw | 2 stored + detail JSONB 重 pack(9 keys) |
| day_trading | day_trading_derived | day_trading_tw | 2 stored + detail JSONB 重 pack(2 keys);day_trading_ratio 衍生欄留 PR #19c 7b |
| valuation | valuation_daily_derived | valuation_per_tw | 3 stored 1:1;market_value_weight=NULL(PR #19c 跨表 join) |

### C. silver/_common.py 補 4 個 helper(builder 共用)

- `get_trading_dates(db)` — 一次讀 trading_calendar(institutional 過濾鬼資料用)
- `fetch_bronze(db, table, stock_ids=, where=)` — 統一 SELECT Bronze
- `upsert_silver(db, table, rows, pk_cols)` — 批次 UPSERT 包 is_dirty=FALSE / dirty_at=NULL
- `reset_dirty(db, table, pks, pk_cols)` — 顯式 reset(備用,trigger 路徑會用)

### D. 驗證器 `scripts/verify_pr19b_silver.py`

5 個 builder 跑完(full_rebuild=True),對 v2.0 legacy 表逐 PK 比對:
- stored cols 數值 1e-9 容差
- detail JSONB normalize 後等值(reuse `_reverse_pivot_lib._values_equal`)
- 排除 PR #19b 暫不填的 Silver 專屬欄(institutional.gov_bank_net /
  valuation.market_value_weight / margin SBL 6 欄)

預期 5/5 OK(對 v2.0 legacy 等值)。

### E. 沙箱合成資料測試已通

5 個 builder transform 邏輯通過合成資料測試:
- institutional 4 row → 2 wide row(2 個 date,各 1 wide);
- margin 16 cols 含 3 alias 對齊 short_*;
- foreign_holding / day_trading detail JSONB 正確 pack;
- valuation 3 cols + market_value_weight=NULL。

### F. 用戶本機驗證(預期全綠)

```powershell
git pull
pip install -e .                                       # 一次性,後續無需 PYTHONPATH
alembic upgrade head                                   # k0 已落,no-op
python scripts/verify_pr19b_silver.py                  # 5/5 OK 對 v2.0 legacy 等值
psql $env:DATABASE_URL -c "SELECT COUNT(*) FROM institutional_daily_derived"
```

### 已知狀態(下次 session 起點)

- 5 個 Silver 表寫入(對 v2.0 legacy 等值);8 個 builder + dirty queue + Phase 7 留 PR #19c
- pyproject.toml 落地,sys.path 不再卡 src layout
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → **#19b ⏳ 待 user verify** → #19c → #20

---

## v1.11 — PR #19a Silver 14 表 scaffolding(2026-05-02 後續)

接 v1.10 PR #18 後動工 PR #19(blueprint v3.2 r1 §五 §5.5 + §十 PR #8)。完整 PR #19 scope 估 3 天太大塞不進一個 session,本 session 切 PR #19a 純 scaffolding,後續切兩段:

| 切片 | 範圍 | 估時 | 風險 |
|---|---|---|---|
| **PR #19a 本 session** ✅ | 14 張 Silver `*_derived` schema + 3 張 fwd ALTER 加 dirty 欄位 + silver/ 套件骨架 + 13 個 builder stub + bronze/dirty_marker stub | 半天 | 低(全 additive,builder 全 raise NotImplementedError) |
| PR #19b 下 session | 5 個簡單 builder(institutional / valuation / day_trading / margin / foreign_holding,因 Bronze 已 PR #18 落地有真資料可驗) | ~1 天 | 中 |
| PR #19c 再下 session | 剩 8 個 builder + orchestrator 真實邏輯 + Phase 7a/7b/7c CLI + bronze/phase_executor 拆段 | ~1.5 天 | 高(部分依賴 PR #18.5 Bronze 重抓) |

### A. alembic migration `k0l1m2n3o4p5_silver14_dirty_scaffolding`

單一 migration 同時建 14 張 Silver `*_derived` 表 + 14 個 partial index `WHERE is_dirty = TRUE` + 3 張 fwd 表 ALTER ADD COLUMN(dirty 欄位 + 對應 index)。schema_pg.sql 同步附 DDL 在尾段。

14 張 Silver 對映 spec §2.3 canonical 清單:
1. `price_limit_merge_events`(Rust 計算,schema TBD per PR #20)
2. `monthly_revenue_derived`
3. `valuation_daily_derived`(+market_value_weight)
4. `financial_statement_derived`(PK 含 type)
5. `institutional_daily_derived`(+gov_bank_net)
6. `margin_daily_derived`(+SBL 6 欄)
7. `foreign_holding_derived`
8. `holding_shares_per_derived`
9. `day_trading_derived`
10. `taiex_index_derived`
11. `us_market_index_derived`
12. `exchange_rate_derived`(PK 含 currency,不是 stock_id)
13. `market_margin_maintenance_derived`(PK 含 market+date,+2 欄)
14. `business_indicator_derived`(NEW per spec §6.3)

### B. silver/ 套件骨架(`src/silver/`)

```
src/silver/
├── __init__.py
├── _common.py             # filter_to_trading_days(從 aggregators.py 搬)+ SilverBuilder protocol
├── orchestrator.py        # Phase 7a/7b/7c 排程 skeleton(run() raise NotImplementedError)
└── builders/
    ├── __init__.py        # BUILDERS dict 註冊 13 個 builder
    ├── institutional.py   ← PR #19b
    ├── margin.py          ← PR #19b
    ├── foreign_holding.py ← PR #19b
    ├── day_trading.py     ← PR #19b
    ├── valuation.py       ← PR #19b
    ├── holding_shares_per.py  ← PR #19c(依賴 PR #18.5 重抓)
    ├── monthly_revenue.py     ← PR #19c(同上)
    ├── financial_statement.py ← PR #19c(7b 階段,同上)
    ├── taiex_index.py     ← PR #19c
    ├── us_market_index.py ← PR #19c
    ├── exchange_rate.py   ← PR #19c
    ├── market_margin.py   ← PR #19c
    └── business_indicator.py  ← PR #19c
```

每個 builder stub expose `NAME / SILVER_TABLE / BRONZE_TABLES / run()`,run() 全 raise `NotImplementedError(f"{NAME} builder 留 PR #19b/c 動工。...")`。orchestrator `BUILDERS` dict 統一註冊,動工時直接 import + replace stub。

### C. bronze/dirty_marker.py(短期路徑 stub)

`BRONZE_TO_SILVER` dict 對映 14 + 1(price_adjustment_events → 4 張 fwd 一起 dirty)= 15 entries。`mark_silver_dirty(db, bronze_table, rows)` API surface 定下,PR #19a 階段 no-op return 0。PR #19b/#19c 補實際 INSERT/UPDATE 邏輯;PR #20 trigger 上線後改 deprecated no-op。

### D. 不啟用 trigger(PR #20 才 enable)

per blueprint §5.7 step-1 vs step-2 設計:本 PR 只建 schema,Bronze→Silver trigger DDL 留 PR #20 一起 CREATE + ENABLE,避免 Bronze 雙寫期間每筆 upsert 都觸發級聯。

### E. 驗證(用戶本機)

```powershell
git pull
alembic upgrade head                                         # k0l1m2n3o4p5
psql $env:DATABASE_URL -c "\dt *_derived"                    # 13 張 *_derived
psql $env:DATABASE_URL -c "\d institutional_daily_derived"   # 確認 dirty 欄位 + gov_bank_net
psql $env:DATABASE_URL -c "\d price_daily_fwd"               # 確認新加 is_dirty/dirty_at

# pyproject.toml 已落地(v1.12),只要跑過一次 pip install -e . 之後永久 importable
pip install -e .                                            # 一次性,後續無需設 PYTHONPATH
python -c "from silver import orchestrator; print(orchestrator.PHASE_7A_BUILDERS)"
python -c "from silver.builders import BUILDERS; print(sorted(BUILDERS))"
python -c "from bronze.dirty_marker import BRONZE_TO_SILVER; print(len(BRONZE_TO_SILVER))"

alembic downgrade -1 && alembic upgrade head                # rollback smoke
```

### 已知狀態(下次 session 起點)

- alembic head:`k0l1m2n3o4p5`
- 14 張 Silver 表 schema 落地;13 builder stub + orchestrator skeleton 在 src/silver/
- bronze/dirty_marker.py API surface 定;Bronze→Silver trigger 留 PR #20
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → **#19a ✅ → #19b ⏳ next** → #19c → #20

---

## v1.10 — PR #18 Bronze 5 reverse-pivot 落地(2026-05-02 後續)

接 v1.9.1 後動工 PR #18(blueprint v3.2 r1 §六 #11 / §十 PR #5)。本 session 完成 5 張 v2.0 pivot/pack 表 → v3.2 Bronze raw 反推 + alembic 落地 + round-trip 驗證器。

### A. 共用 helper:`scripts/_reverse_pivot_lib.py`

`SPECS` dict + `ReversePivotSpec` dataclass + 5 公開函式:

| function | 用途 |
|---|---|
| `fetch_legacy_pivot` | 從 legacy 表 SELECT(自動 strip `source` 等 control 欄) |
| `reverse_pivot_rows` | legacy 寬列 → Bronze 瘦/平列(兩 mode) |
| `upsert_bronze` | 批次 UPSERT 到 Bronze(走 db.upsert + bronze_pk) |
| `repivot_for_verify` | Bronze → legacy 寬列(round-trip 驗證用,mirror aggregators) |
| `assert_round_trip` | NULL-aware + 1e-9 容差 + dict normalize 比對,回 diff report |

加 `run_reverse_pivot()` 一站式 runner,5 個 script 都是 thin wrapper(~25 行)。lib 邏輯通過 7 個合成資料邊界測試:Decimal vs float、NULL vs all-None dict、partial detail dict、空 dict 等。

### B. 5 張 Bronze 反推契約

| legacy | bronze | mode | 預期 row 比 |
|---|---|---|---|
| institutional_daily | institutional_investors_tw | investor_pivot | 1 → 最多 5(每法人 1 列) |
| margin_daily | margin_purchase_short_sale_tw | detail_unpack | 1 → 1(8 detail key 攤平成欄) |
| foreign_holding | foreign_investor_share_tw | detail_unpack | 1 → 1(9 detail key 攤平) |
| day_trading | day_trading_tw | detail_unpack | 1 → 1(2 detail key 攤平) |
| valuation_daily | valuation_per_tw | detail_unpack | 1 → 1(無 detail) |

institutional 反推已由用戶本機 prototype 驗證 1775 ↔ 8875 ↔ 1775 100% round-trip(v1.9.1 結束時驗的)。本 session lib 化後 4 張延伸表待用戶本機跑全市場驗證。

### C. alembic migration `j9k0l1m2n3o4`

單一 migration `2026_05_02_j9k0l1m2n3o4_b_pr18_bronze5_reverse_pivot.py` 同時建 5 張 Bronze + 5 個 `idx_<table>_stock_date_desc` 索引(給 PR #19 Silver builder reads)。Coexist 模式:legacy v2.0 表保留;`_legacy_v2` rename 留到 T0+21(blueprint §八.2,後續 PR #21+)。`schema_pg.sql` 同步附 5 張 DDL 在尾段。

### D. 驗證器 `scripts/verify_pr18_bronze.py`

5 張一次跑完印 status table,任一 FAIL → exit 1 + 印各表前 5 筆 diff(missing / extra / value_diffs)。push 前必跑 5/5 OK。

### E. PR #18.5 留 follow-up(不阻塞 PR #18 close)

3 張表 (`holding_shares_per` / `financial_statement` / `monthly_revenue`) 因 detail JSONB unpack 不可逆(level taxonomy 未知 / 中→英 origin_name 對應丟失 / FinMind 月營收 1 row/股/月)走 Option A 全量重抓(~30-40h calendar-time @ 1600 reqs/hr)。獨立 PR 異步處理。

### 已知狀態(下次 session 起點)

- alembic head:`j9k0l1m2n3o4`(待用戶本機 `alembic upgrade head` 落地)
- 5 張 Bronze schema 已寫(scripts + migration + schema_pg.sql 三邊對齊)
- institutional 反推用戶本機驗過;4 張延伸待用戶 `python scripts/verify_pr18_bronze.py` 全市場跑
- Silver 14 張 + dirty queue + Bronze→Silver trigger 留 PR #19 動工
- v3.2 r1 PR sequencing:#17 ✅ → **#18 ⏳ 本 session 待 user verify** → #18.5 → #19 → #20 → #21

---

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
| `scripts/verify_event_kind_rate.sql` 🆕 v3.14 | per-EventKind 觸發率 verify(對齊 v1.32 ≤ 12/yr/stock 標準),3 sections:per-stock cores / market-level cores(events/yr 評估)/ Round-specific verify | `psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql` |

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

> **🎯 v1.35 收尾(2026-05-14)**:Neely Core v1.0.1 P0 Gate 通過 + P3/P2
> indicator cores batch 11 個落地 + Round 6 calibration per-EventKind ≤ 12/yr
> 全部命中 + Aggregation Layer 4 Phase 全完整(spec → lib → dashboard → MCP)
> + 本 session agg 補強(health_check + 內建 look-ahead + input validation)。
>
> Rust workspace 35 crates / **384 tests passed / 0 failed**;agg tests
> **30 passed / 1 skipped(pandas)**;mcp_server data tests **9 passed**。
>
> alembic head 不變:`x3y4z5a6b7c8`(本 session 全部 0 migration)。
> Production state:1263 stocks × 34 cores / 0 errors / ~10 min wall。
>
> ⚠️ PR #51 待 user merge → main(我無 merge 權限)。

### 0. 立即可動工(無 blocker)

**0a. user merge PR #51 到 main**(blocking,user 端):
本 session 累積 22+ commits 在 `claude/continue-previous-work-xdKrl` 分支,
涵蓋 neely Phase 13-19 v1.0.x、P3/P2 cores batch、Round 5/6 calibration、
agg layer 補強。merge 後 branch close。

**0b. Aggregation Layer 升級選項**(可拆 sub-PR):
- **Phase B-4 FastAPI thin wrap**(估 ~2-3h):`agg_api/main.py` + Pydantic
  schemas + `/as_of/{stock_id}` + `/health_check` endpoint;不含 auth /
  rate limit / deploy(屬 Phase B-5 網站工程,獨立規格)
- ~~**per-timeframe lookback fold-forward**~~ → ✅ **v3.8 落地**(PR #63,
  `as_of()` 加 `lookback_days_monthly` / `lookback_days_quarterly` 參數 +
  `_filter_by_timeframe_lookback` helper + spec r3)
- ~~**structural_snapshots schema partition observation**~~ → ✅ **v3.9 完成**
  (`docs/structural_snapshots_partition_observation.md`)。結論:🟢 目前
  **不需要** partition;5 年內最多 ~6.4M rows 遠低於門檻;主 query
  `WHERE stock_id = ?` 走 index seek ~5 ms。預警閾值(p95 > 100 ms /
  total > 10M rows / daily batch > 5min / disk > 50GB)觸發時再評估

### 1. 立即可動工(無 blocker)— 收尾 M3 後階段

**1a. user 本機 smoke + 全市場 production**(blocking,user 端):
```powershell
git pull
# 不需 alembic upgrade(本 PR 0 migration)
cd rust_compute && cargo build --release -p tw_cores
$env:DATABASE_URL = "postgresql://twstock:twstock@localhost:5432/twstock"

# Stage 1:dry-run smoke 5 stocks(~30 秒)
.\target\release\tw_cores.exe run-all --limit 5
# 預期:5 environment + 5 × 17 = 90 條 summary

# Stage 2:小範圍 write(P0 Gate 5 stocks)
.\target\release\tw_cores.exe run-all --stocks 0050,2330,3363,6547,1312 --write
psql $env:DATABASE_URL -c "SELECT source_core, COUNT(*) FROM indicator_values GROUP BY 1 ORDER BY 1"
psql $env:DATABASE_URL -c "SELECT core_name, COUNT(*) FROM structural_snapshots GROUP BY 1"
psql $env:DATABASE_URL -c "SELECT source_core, COUNT(*) FROM facts GROUP BY 1 ORDER BY 1"

# Stage 3:全市場 production(~30 分鐘 串列)
.\target\release\tw_cores.exe run-all --write
```

**1b. PR-9b 工程進階**(✅ **全部已落地**;v3.9 audit 確認):
- ~~**Workflow toml dispatch**~~ → ✅ **已落地**(`tw_cores/src/workflow.rs`)
  `CoreFilter::from_workflow_toml` + walk-up cwd resolve + 35 cores 全部接
  `filter.is_enabled()` check;`--workflow workflows/tw_stock_standard.toml` 用法
- ~~**sqlx pool 並行**~~ → ✅ **已落地**(v1.29 PR-9b commit 615a8eb):
  `for_each_concurrent` 並行 Stage B per-stock(default 32);production 1263 stocks
  從 3666s → 535s(7× 加速)
- ~~**incremental dirty queue**~~ → ✅ **已落地**(`tw_cores run-all --dirty`)
  對齊 `silver/orchestrator.py:_fetch_dirty_fwd_stocks` pattern
- ~~**ErasedCore trait wrapper**~~ → 🟡 不做(對齊 cores_overview §十四「禁止抽象」,
  V2 不規劃;workflow filter 用 hardcoded match arm + `is_enabled()` check 即可)

**1c. RSI Failure Swing**(估 ~半天):
- spec §4.6 四步全成立才產出(RSI 進超買 → 退出 → 折返但未再進 → 跌破前低)
- 框架 `RsiEventKind::FailureSwing` 已存在,只需補 detect 邏輯

### 2. Code follow-up + production calibration(**v3.7 update — spec 不缺**)

> **v3.7(2026-05-16)reframe**:原 §2「等 user m3Spec/ 寫最新 spec 後做(spec-blocked)」段
> 大幅清理 — neely R4-R7 / F1-F2 / Z1-Z2 / T1-T3 / W1-W2 / Diagonal Leading/Ending / R3 Diagonal
> exception / Power Rating 查表 / Fibonacci 接 monowave 全部已實作。spec_pending.md §1.1+§1.3
> 同步 v3.7。本段現只剩 **production data calibration**(spec 不缺,純校準) — 對齊
> `m3Spec/neely_core_architecture.md` 與 `m3Spec/neely_rules.md` 既有規格。

**2a. exhaustive compaction 真窮舉**(v3.7 Phase B 動工):
- spec `m3Spec/neely_rules.md §Three Rounds` line 1198-1256 已完整描述 Round 1-3 流程
- code `compaction/exhaustive.rs` 目前 pass-through,需依 spec 跑 Figure 4-3 五大序列 + Similarity & Balance 過濾 + 邊界波 retracement reevaluation
- **spec 不缺**,屬純 Rust 工程跟進

**2b. Indicator/Chip/Fundamental/Environment 各規則閾值校準**(production data driven):
- 各 core 內 `// TODO` / `best-guess` 註解標的常數
- `financial_statement_core.detail` JSONB key 命名(目前英文 best-guess)
- ATR/Bollinger/OBV lookback 常數(1y / 6m / 20-day)
- ma_core Dema/Tema/Hma 公式精確性
- Divergence min bars(目前寫死 20,對齊 spec §3.6)

### 3. P0 Gate(Neely 校準,需 user 本機 PG 跑)

**3a. 五檔股票實測**(估 ~1 天 + 校準):
- 0050 / 2330 / 3363 / 6547 / 1312
- 校準常數寫入 `docs/benchmarks/`:
  - `forest_max_size`(目前 1000)
  - `compaction_timeout_secs`(目前 60)
  - `BeamSearchFallback.k`(目前 100)
  - `REVERSAL_ATR_MULTIPLIER`(目前 0.5)
  - `STOCK_NEUTRAL_ATR_MULTIPLIER`(目前 1.0)
  - `BEAM_CAP_MULTIPLIER`(目前 10)
- 對齊 cores_overview §9.1「P0 完成後的 Gate」

**3b. 測試策略**:
- Indicator golden test 對 TA-Lib / pandas-ta 比對
- Integration test 走 PG real data(沙箱無 PG,留 user 本機)
- 各 core「best-guess 閾值」對 user 預期行為 visual review

### 4. Silver schema 假設待 user 驗(可能需 alembic 補欄)

| 假設 | 影響 core | 處理方式 |
|---|---|---|
| `margin_daily_derived.margin_maintenance` 是否存在 | margin_core | 不存在 → MaintenanceLow event 永遠不觸發 |
| `foreign_holding_derived.foreign_limit_pct` stored col | foreign_holding_core | 目前 NULL placeholder,LimitNearAlert 不觸發 |
| `holding_shares_per_derived.detail` JSONB schema | shareholder_core | 目前 best-guess key(small_holders_count 等) |
| `market_margin_maintenance_derived` 完整欄位 | market_margin_core | 目前 ratio + total_*_balance 三欄 |
| `fear_greed_index` 是否需 `_derived` 表 | fear_greed_core | 目前直讀 Bronze,§6.2 已登記架構例外 |
| `financial_statement_derived.detail` JSONB key | financial_statement_core | 英文 key 假設(EPS/Revenue/GrossProfitMargin 等) |

### 5. m2 收尾(R5/R6 觀察期)— ✅ **v3.10(2026-05-16)完整收尾**

- ~~**R5 觀察期 21~60 天**~~ → ✅ user 拍版「直接 DROP」提前結束(2026-05-09 →
  2026-05-16,7 天觀察期);v3.7+v3.8+v3.9 連續 4 sprint 無觀察到 regression
- ~~**R6 DROP 3 張 `_legacy_v2`**~~ → ✅ **v3.10 完成**(PR #65):
  - alembic `z5a6b7c8d9e0_pr_r6_drop_legacy_v2_3_tables.py`
    DROP `holding_shares_per_legacy_v2` / `financial_statement_legacy_v2` /
    `monthly_revenue_legacy_v2`(CASCADE,downgrade no-op)
  - collector.toml 5 個 `*_legacy` entry 全部移除(剩 27 entry)
  - schema_pg.sql 3 CREATE TABLE + 2 INDEX DDL 移除
  - check_all_tables.py / inspect_db.py 同步移除 references
  - verify_pr19c2_silver.py 標 🪦 DEPRECATED(legacy 表已 DROP,verifier 不可執行)

### 6. nice-to-have(可平行)

- shared/timeframe_resampler / data_ref / degree_taxonomy 等 utility crate
  (cores_overview §四 列出但未做)
- asyncio.gather 7a 平行優化(需 PostgresWriter connection pool;perf gain ~ms)
- ~~tw_cores `run-all`~~ ✅ v1.29 PR-9a 已落地(全市場 × 全 22 cores hardcoded dispatch)

### ⚠️ V2 階段禁止做(spec 已明文)

- **Indicator kernel 共用化** → cores_overview §十四「P3 後考慮,V2 不規劃」。
  2026-05-09 嘗試過抽出(commit 5abca8d),user 退板,revert(commit 6f05fb9)。
  8 個 indicator cores 保持各自獨立 ema/sma/wma/wilder_atr/wilder_rsi 實作,
  符合 §四 零耦合原則。
- **跨指標訊號獨立 Core**(TTM Squeeze / `chip_concentration_core` 等)
  → cores_overview §十一 / chip_cores §八「不在 Core 層整合」。
- **`financial_statement_core` 拆分**(損益/資產負債/現金流獨立 Core)
  → cores_overview §十四「V3 議題,V2 不規劃」。

m2 收尾(可平行,**不阻塞 M3**):

7. **PR #R5** 觀察期 21~60 天(2026-05-09 啟動,最早 2026-05-30 進 R6)
   - Silver builder 持續每日 12/12 OK
   - api_sync_progress.status='failed' = 0
   - 3 張 `_legacy_v2` row count 與主名表 ±1%
8. **PR #R6** DROP 3 張 `_legacy_v2`(永久 DROP,需 backup 後執行 — 不可 rollback)
   + 對應 5 個 v2.0 `_legacy` entry 從 collector.toml 移除

剩 nice-to-have(可平行):

9. **asyncio.gather 7a 平行優化** — 需先升 PostgresWriter 為 connection pool;
   perf gain ~ms 量級,排序低
10. **Orchestrator 7c-first 排程修正** — 目前 user 自己 invoke `silver phase 7c`
    再跑 `silver phase 7a` 即正確;orchestrator 自動 chain 留 follow-up

### 中期 backlog(non-blocking)

4. **`asyncio.gather` 7a 平行優化** — 需先升 PostgresWriter 為 connection pool;
   perf gain ~ms 量級,排序低
5. **Phase 4 真正的 incremental 優化** — 偵測「該股票無新除權息事件 → 跳過」
   每天 incremental 可省 ~6 分鐘
6. ~~**`inspect_db.py` 升 PG 版**~~ ✅ v1.18 §I 完成(441 → 290 行重寫,
   砍掉 SQLite-only 後復權驗證段,加 v3.2 TABLE_GROUPS + Silver derived spot-check)
7. ~~**bronze/phase_executor.py 拆段**~~ ✅ v1.18 §I 完成
   (`git mv src/phase_executor.py src/bronze/`,main.py import 1 行同步)
8. **CLAUDE.md 章節重組** — ~~v1.4 → v1.7 詳解搬 `docs/claude_history.md`~~ ✅ v1.18 reorg 已完成(v1.5-v1.9.1 全部搬到 history;主檔從 1500+ → ~1260 行)
9. **agent-review-mcp 支線**(v1.4 spec,自 v1.6 懸而未決)
10. **PR review + merge** — `claude/initial-setup-RhLKU` 累積 v1.10 → v1.18 ~60+ commit
    待 maintainer 整合
11. **m2 PR #20 / #21 完整 milestone** — orchestrator go-live + Silver views(spec §2.5)
    + legacy_v2 rename(blueprint §八.2)+ M3 prep,blueprint §十 PR 切法
