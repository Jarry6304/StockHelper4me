# tw-stock-collector / StockHelper4me

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本檔是**入口檔**(操作手冊 + 架構不變量 + 索引),控制在 ≤400 行。
> 版本歷程一律在 [`docs/changelog/`](docs/changelog/INDEX.md)(v3.5 → v4.38 帶狀分檔);
> v1.x 沿革與過期段在 [`docs/claude_history.md`](docs/claude_history.md)。
> 動工前讀完本檔,再依任務從「歷程索引」與「常見任務」往下鑽。

---

## 專案概述

台股資料蒐集 + 計算 pipeline:FinMind API → Postgres 17,六層架構
(Bronze 收集 / Silver per-stock / Cross-Stock Cores / M3 Cores / Golden L3 物化 / MCP + Web API 對外)。
雙波浪引擎並排不整合:`neely_core`(NEoWave)與 `traditional_core`(Frost & Prechter EWP,自有 loader + `traditional_snapshots`)。
現行版本 **v4.38**(2026-06-06,Web 前端原型);版本狀態詳見下方「最近版本摘要」。

| 層 | 內容 | 寫入 path | 主要 module |
|---|---|---|---|
| Bronze(L1) | FinMind raw(`*_tw` 表) | Phase 1-6 collector | `bronze/phase_executor.py` + `segment_runner.py` + `aggregators/` |
| Silver(L2) | per-stock `*_derived` + `price_*_fwd`(Rust S1 後復權) | Phase 7a/7b/7c dirty-driven | `silver/orchestrator.py` + `builders/` |
| Cross-Stock(L2.5) | 跨股 ranking `*_ranked_derived`(12 builders) | Phase 8(全市場重算 latest) | `cross_cores/` |
| M3 Cores(L3) | Wave / Indicator / Chip / Fundamental / Environment(41 cores dispatch) | Rust `tw_cores run-all` | `rust_compute/cores/` + `cores_shared/` |
| Golden L3 | fusion 物化 levels / resonance / climate → `structural_snapshots` | `golden fusion` CLI | `src/fusion/materialize/` |
| 對外(L4) | MCP 14 tools / 唯讀 FastAPI / Streamlit / SvelteKit 前端 | on-demand | `mcp_server/` + `src/web_api/` + `dashboards/` + `frontend/` |

## 技術棧

- Python 3.11+(tomllib;aiohttp + psycopg3 + alembic + numpy;`pip install -e ".[dev,mcp,web]"`)
- Rust workspace **50 crates**(sqlx + Postgres;兩個 binary:`tw_stock_compute` S1 後復權 / `tw_cores` M3 全核)
- PostgreSQL 17(本機 service;schema 演進走 alembic,head:`j6k7l8m9n0o1`)
- FastMCP(stdio MCP server,14 public tools)+ FastAPI(唯讀 Web API)+ Streamlit dashboards
- SvelteKit 2 + Vite + TypeScript + Plotly.js(`frontend/`,契約由 ts-rs + pydantic2ts codegen)
- FinMind sponsor tier(`$env:FINMIND_TOKEN`;`config/collector.toml` 39 entries)

## 目錄結構

```text
src/
├── main.py                 # CLI 入口(backfill / incremental / silver / cross_cores / refresh / forecast / golden)
├── bronze/                 # L1 收集(phase_executor / segment_runner / aggregators / post_process_dividend)
├── silver/                 # L2 per-stock builders(orchestrator + builders/ + _common)
├── cross_cores/            # L2.5 跨股 ranking(12 builders + orchestrator)
├── fusion/                 # 讀法層 + Golden L3(raw/ 查詢、dual_track/、materialize/、_picker 等)
├── forecast/               # 區間預測 spine(backtest / conformalize / fuse / settle)
├── pit/                    # point-in-time 重建 helpers
├── web_api/                # 唯讀 FastAPI(passthrough + brotli;uvicorn web_api.app:app)
├── api_client.py / rate_limiter.py / field_mapper.py / db.py / rust_bridge.py ...
└── schema_pg.sql           # fresh-init 全量 DDL(與 alembic head 同步)
mcp_server/                 # FastMCP server(server.py 註冊 14 tools;tools/ + _*.py helpers)
dashboards/                 # Streamlit(aggregation.py + charts/)
frontend/                   # SvelteKit 原型(src/contracts/ 為 codegen 產物)
rust_compute/
├── cores/                  # wave / indicator / chip / fundamental / environment / system(tw_cores)
├── cores_shared/           # fact_schema / ohlcv_loader / 各 loader
└── silver_s1_adjustment/   # S1 後復權 binary
alembic/                    # migrations(head j6k7l8m9n0o1)
config/                     # collector.toml(39 entries)+ stock_list.toml(twse+tpex,~2172 檔)
m3Spec/ + m2Spec/           # 現行計算規格 / schema 規格
docs/changelog/             # 版本歷程帶檔 + INDEX.md(本檔禁寫版本段,見「禁止事項」)
scripts/                    # helper 腳本(見下方清單)+ workflows/
```

## 命令清單

```bash
# 環境
pip install -e ".[dev]"                   # editable install + pytest
docker compose up -d                      # 本地 Postgres 17(或 OS service)
cp .env.example .env                      # FINMIND_TOKEN + DATABASE_URL
alembic upgrade head
cd rust_compute && cargo build --release  # 兩個 binary

# Bronze(Phase 1-6)
python src/main.py validate | status | incremental
python src/main.py backfill [--phases 1,2,3,4] [--stocks 2330,2317] [--dry-run]
python src/main.py phase 4                               # 單一 Phase(0-6)

# Silver(Phase 7)/ Cross-Stock(Phase 8)
python src/main.py silver phase 7a|7b|7c [--stocks 2330] [--full-rebuild] [--builder NAME]
python src/main.py cross_cores phase 8 [--builder magic_formula] [--full-rebuild] [--lookback-days 60]

# 一鍵 refresh chain:Bronze incremental → 7c → 7a → 7b → Phase 8 → tw_cores run-all
python src/main.py refresh [--stocks 2330] [--skip-cores] [--skip-bronze]
.\scripts\install_refresh_task.ps1 [-At 19:30] [-NoCores]   # Windows 每日排程(wrapper: refresh_daily.ps1)
.\scripts\refresh_full.ps1                                  # 完整補完(7a/7b/8 全 --full-rebuild)

# M3 Cores / Golden / Forecast
rust_compute/target/release/tw_cores run-all --write [--dirty] [--concurrency 8] [--workflow workflows/*.toml]
python src/main.py golden fusion                            # Golden L3 物化(levels/resonance/climate)
python src/main.py forecast backtest|settle|conformalize|fuse|score [--parallelism N]
.\scripts\recalibrate_kalman.ps1 -Incremental               # Phase 3b 週排程(全量 seed 已做)

# 測試 / 驗證(push 前)
pytest                                                      # Python 全套
cd rust_compute && cargo test --workspace                   # Rust 全套
python scripts/verify_pr20_triggers.py                      # Bronze→Silver dirty trigger 整合測試
python scripts/verify_mcp_toolkit_v4_29.py                  # MCP toolkit 全覆蓋健康度(退碼 0/1)
python scripts/check_all_tables.py                          # 全表筆數體檢
.\scripts\test_pipeline.ps1                                 # 5-phase 測試流水線(.sh 同款)
```

## 架構不變量(不要改)

Bronze / collector:

| 決策 | 原因 |
|------|------|
| `FieldMapper(db=db)` 一定要帶 db | schema 補欄位豁免名單,避免「與 DB 同名直接入庫」誤報 novel |
| `field_mapper.transform()` 回 `(rows, schema_mismatch: bool)` tuple | 上層用來 mark_schema_mismatch |
| `db.upsert()` 自帶欄位過濾 | API 新增欄位不炸;Silver 寫入走 `silver/_common.upsert_silver()`(包 `is_dirty=FALSE`) |
| `_table_pks` 動態查 `information_schema` | schema 是 single source of truth,不硬編碼 PK 對照表 |
| `api_sync_progress.status` 5 種 | `pending / completed / failed / empty / schema_mismatch` |
| `stock_info.updated_at` 走 schema `DEFAULT NOW()` + upsert UPDATE 強制 NOW() | 兩條 path 行為對齊 |
| Phase 4 mode 從 CLI runtime 傳 | 避免 toml 寫死 backfill 但 CLI 跑 incremental 時錯位 |
| Phase 4 必須傳 `stock_ids` | `stock_sync_status` 沒人寫入,Rust 取不到清單 |
| `cooldown_on_429_sec` 存在 `RateLimiter` 實例上 | api_client 從這裡讀,不從 config 重讀 |
| `detail_fields` 在 toml 是「文件用」 | runtime 沒消費,純註記哪些欄位進 detail JSON |
| 5 類法人各自獨立欄位(不累加) | 外資/自營商「自行 vs 避險/自營」量化策略上有差別 |
| TOML inline table 必須單行 | `tomllib` TOML v1.0 限制 |

Rust / 計算層:

| 決策 | 原因 |
|------|------|
| 後復權迴圈:**先 push 再更新 multiplier** | 除息日當日 raw 已是除息後,不可再乘該日 AF |
| 後復權拆兩個 multiplier(v1.8) | `price_multiplier`(從 AF)+ `volume_multiplier`(從 vf);現金 dividend volume 不動(vf=1.0) |
| stock_dividend vf = 1/(1 + stock_div/10) | 由 `post_process._recompute_stock_dividend_vf` SQL UPDATE 修正 |
| Rust `process_stock` 永遠全量重算 | multiplier 從尾端倒推,partial 邏輯上錯;Python `_mode` 對 Rust 是 no-op |
| `EXPECTED_SCHEMA_VERSION = "3.2"`(`rust_bridge.py`) | schema 升版時 Rust + Python 兩端一起改 |
| Windows binary path 由 `rust_bridge.py` 自動補 `.exe` | `asyncio.create_subprocess_exec` 不像 shell 自動補 |
| `tw_cores` dispatch 保留 hardcoded match arm | 對齊 cores_overview §十四「禁止抽象」,V3 才考慮 generic dispatcher |
| `neely_core` 與 `traditional_core` 完全解耦 | 並排不整合;traditional 自有 loader 直讀 Silver,`cargo tree` 零 neely dep |
| PostgresWriter pool + 並行度 = `max(1, DB_POOL_SIZE - 1)` | v4.36 五層並行化;留 1 conn 給查詢路徑 |
| magic_formula `is_top_30`(語意旗標)全凍 | 實體欄已改 `is_top_n`(v4.35);`DualTrackResult.is_top_30` / 前端契約改 = breaking API |

## 禁止事項

文件:

| ❌ | ✅ |
|---|---|
| 在 CLAUDE.md 新增 `## vX.Y` 版本段 | 寫 `docs/changelog/<帶檔>.md` + INDEX.md 加一列 |
| 「最近版本摘要」累積超過 2 版 | 新版進來時把最舊一版移走 |
| 段內引用「見上方 §v4.32」 | 「見 docs/changelog/v4.30-v4.38.md §v4.32」 |
| docstring 寫死會漂移的數字(`23 cores`、`13 tools`) | 「以 `list_cores()` / server.py 註冊區為準」 |
| 註解引用 repo 外路徑(本機 plans 目錄等) | 引 repo 內檔案(規格 / changelog / README) |
| 註解描述未來計畫(「留 Step 3」) | 完成即刪;未完成寫進「已知陷阱」而非散落 docstring |
| 引用 `docs/archive/` 內容作為現行規格依據 | 現行規格只認 m2Spec(非 old)/ m3Spec / docs/schema_master.md |

程式(V2 階段,spec 已明文):

- **Indicator kernel 共用化** → cores_overview §十四「P3 後考慮,V2 不規劃」(2026-05-09 試過,user 退板 revert)
- **跨指標訊號獨立 Core**(TTM Squeeze 等)→ cores_overview §十一「不在 Core 層整合」
- **`financial_statement_core` 拆分** → V3 議題
- **ErasedCore trait wrapper** → V2 不規劃;workflow filter 用 hardcoded match arm + `is_enabled()`

## 常見任務

| 任務 | 入口 | 查閱規格 |
|---|---|---|
| 加 FinMind dataset | `config/collector.toml` + alembic Bronze 表 | docs/changelog/v3.20-v3.29.md §v3.20 pattern |
| 加 Silver builder | `src/silver/builders/` + `PHASE_GROUPS` | `m2Spec/layered_schema_post_refactor.md` |
| 加 cross_cores builder | `src/cross_cores/` + orchestrator BUILDERS | 同上 §1.5 + `_shared.py` helpers |
| 加 M3 core(Rust) | `rust_compute/cores/<類>/` 新 crate + `tw_cores` dispatch 3 處 | `m3Spec/` 對應 spec(best-guess 不上 Rust) |
| 加 MCP tool | `mcp_server/tools/` + `server.py` 註冊 | `mcp_server/README.md` |
| 調 EventKind 觸發率 | 對應 core 的 const + DELETE facts 重跑 | `scripts/verify_event_kind_rate.sql`(≤12/yr/stock) |
| Web API 端點 | `src/web_api/routers/` | docs/changelog/v4.30-v4.38.md §v4.32 |
| 前端視圖 | `frontend/src/routes/` | docs/changelog/v4.30-v4.38.md §v4.38(鐵則 L1-L8 / CL1-CL6) |

## 已知陷阱

- Sandbox 連不到 finmindtrade.com,API 實測都得 user 本機跑;DB-bound 測試(psql / production verify)同理
- PowerShell nested quotes 很差:inline SQL 走 `psql $env:DATABASE_URL -c "..."`,不用 `python -c`
- `python src/main.py refresh` 的 Silver 7c 強制 full-rebuild(v4.30 Option B;dirty queue 對 price_daily 新 close 不觸發)
- weekly snapshots production 累積比 daily 慢 → cross_tf 類欄位短期 0 屬累積動態,非 bug(v4.28 retract 教訓)
- neely 對急漲股無有效 scenario 是 spec 合法行為(§7.2);看 `quality_caveat.is_usable`,不要硬改引擎
- accepted baselines(超 12/yr 但拍版接受,不需再 tighten):`institutional/DivergenceWithinInstitution` 58.41、
  `institutional/LargeTransaction` 14.16、`exchange_rate/SignificantSingleDayMove` 14.8、`commodity_macro/CommoditySpike` 12.2
- Silver schema 假設待驗(不阻塞 production):`margin_daily_derived.margin_maintenance` 是否存在(margin_core)/
  `holding_shares_per_derived.detail` JSONB key(shareholder_core)/ `fear_greed_index` 直讀 Bronze(已登記例外)

## 最近版本摘要

**v4.38(2026-06-06)— Web 前端原型**:`frontend/` SvelteKit + Vite + TS + Plotly.js,
2 視圖(`/stocks/[id]` V1 個股 WAVE 卡 + `/screens/[toolkit]` V2 跨股因子排行);
消費既有 Golden L3 唯讀 API,前端零 compute;後端加 `CORSMiddleware`(`WEB_API_CORS_ORIGINS` env 覆寫);
V2 WAVE 欄是 placeholder(spec CL4),真實端點留 production 拍版 (a)/(b)/(c)。
詳見 docs/changelog/v4.30-v4.38.md §v4.38。

**v4.37(2026-06-06)— Traditional Core production 收尾**:compaction 改 `Rc` 共享子樹殺深拷貝
(單股 135-250s → 7.7s);`monowave_epsilon` 預設 0.03 + 4 旋鈕 env(`TRAD_*`);run-all 預設 concurrency 8;
全市場 2171 stocks × 40 cores ~2.5h,P0-Gate forest max 69/70/58 ≪ 200 全過。
詳見 docs/changelog/v4.30-v4.38.md §v4.37。

當前狀態:alembic head `j6k7l8m9n0o1`;Rust 50 crates / `cargo test --workspace` 668 passed(2026-08-26 實測);
Python `pytest tests/` 526 passed / 1 skipped(2026-06-10 實測);MCP 14 public tools;universe ~2172 stocks × 41 cores;
collector.toml 39 entries;`config/stock_list.toml` market_type `["twse","tpex"]`。

## helper 腳本清單

| 腳本 | 用途 |
|------|------|
| `scripts/check_all_tables.py` | 全表筆數體檢(PG 版) |
| `scripts/drop_table.py` | schema 變更後 drop 指定表 |
| `scripts/test_28_apis.py` | API 連線健檢(需 FINMIND_TOKEN) |
| `scripts/probe_finmind_sponsor_unused.py` | FinMind catalog diff + probe unused datasets |
| `scripts/verify_event_kind_rate.sql` | per-EventKind 觸發率 verify(≤12/yr/stock 標準) |
| `scripts/maintain_facts_stats.sql` | 三大表 ANALYZE + VACUUM(Round N DELETE+INSERT 後跑) |
| `scripts/diagnose_slow_tw_cores.sql` | tw_cores 跑期間取樣 lock / pool saturation |
| `scripts/verify_mcp_kalman_neely.py` / `.sql` | Kalman + Neely production 出值健康度 |
| `scripts/test_pipeline.ps1` / `.sh` | 5-phase 測試流水線(Environment / Sandbox / Schema / Production / MCP) |
| `scripts/verify_mcp_toolkit_v4_29.py` | 全覆蓋 public MCP tool 健康度(payload budget,退碼 0/1) |
| `scripts/verify_traditional_forest.py` | Traditional P0-Gate forest 分布 + 覆蓋驗收(免 psql) |
| `scripts/verify_golden_l3_v4_32.ps1` | Golden L3 物化 + serving 驗證流水線 |
| `scripts/recalibrate_kalman.ps1` + `install_recalibrate_task.ps1` | Phase 3b Kalman 校準(週排程增量) |
| `scripts/split_claude_md.py` | 一次性:CLAUDE.md → docs/changelog/ 拆分(P0-1,已執行) |

## 環境細節

- PostgreSQL 17 本機 service;`.env` 內 `DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock`
- schema 變動走 alembic incremental migration;fresh-init 可走 `psql -f src/schema_pg.sql` + `alembic stamp head`
- `$env:FINMIND_TOKEN` 環境變數,禁止寫進 collector.toml
- Windows console 中文:`chcp 65001` + UTF-8 OutputEncoding + `PGCLIENTENCODING=UTF8`(test_pipeline.ps1 已內建)

## 歷程索引

| 要找什麼 | 去哪 |
|---|---|
| 任一版本段(v3.5 → v4.38)細節 | [`docs/changelog/INDEX.md`](docs/changelog/INDEX.md) → 對應帶檔 |
| Traditional Core v2/v3 完整歷程 | `docs/changelog/traditional-core.md` |
| verify chain / 下班後流水線 / backlog triage | `docs/changelog/process-logs.md` |
| v1.x 沿革 + 過期 schema 描述 | `docs/claude_history.md`(過期段已標 ⚠️) |
| collector 程式規格 | `collectorSpec/tw_stock_collector_program_spec_v1.2_p{1,2,3}.md` |
| 現行 schema 規格 | `m2Spec/layered_schema_post_refactor.md` + `docs/schema_master.md` / `docs/schema_reference.md` |
| 計算層規格(cores / 波浪 / indicator) | `m3Spec/`(neely / traditional / chip / environment / fusion / dual_track …) |
| 舊版規格(v3.2 era,僅考古,不作現行依據) | `docs/archive/oldm2Spec/`(cores_overview §7.5 dirty 契約 / §10.0 Core 邊界三原則仍常被引用) |
| M1 handover / collector 細節 | `docs/MILESTONE_1_HANDOVER.md` / `docs/collectors.md` |

## 下次 session 優先序

1. **neely Compaction v2(G2.x 系列,進行中)**:規格 `m3Spec/neely_compaction_v2.md`(r3);
   G2.0 止血 + G2.1 tiling-round shadow 引擎已收案(含 2026-08-26 六檔 gate 實測
   I1–I6 零違反),歷程與 G2.2 設計輸入(branch cap 視窗偏置 / W4 round-1 語意差)
   見 `docs/changelog/neely-compaction-v2.md`。當前:G2.2(W5 端點泛化 + W6 分岔
   判別 + Q3 六檔雙軌實驗);TAIEX 於 `price_daily_fwd` 無供料另列 backlog。
2. **待辦 backlog(2026-06-04 拍版)**:① 對外 API 擴充(等 user 給範圍)— 詳見
   `docs/changelog/process-logs.md` §待辦 backlog。② V2 WAVE 欄已拍版 (a) 並落地
   (2026-06-11,`GET /waves/summary`;見 docs/changelog/v2-wave-endpoint.md),
   production verify 留本機 runbook。③ **漸進收攏 spec 拍版**(波浪引擎歷史段定型 +
   尾端錨定;討論稿 `m3Spec/proposal_progressive_settlement.md`,拍版前不動 Rust;
   表現層止血已落地,見 docs/changelog/wave-view-tuning.md)。
3. **gov_bank_net Core 消費**(需先寫 EventKind 規格;best-guess 不上 Rust)。
4. **wall time / PG contention 觀察**:run-all 全市場 ~37 min(tpex universe 後);爆了先跑
   `scripts/maintain_facts_stats.sql` 再用 `diagnose_slow_tw_cores.sql` 取證
   (2330 單股 smoke 已見 chip 表查詢 1-6.4s slow statement)。

> 架構整備 P0-P2 已全收案(2026-06-10,含本機 production verify);
> 紀錄與驗證結果見 `docs/changelog/architecture-p0-p2.md`。
