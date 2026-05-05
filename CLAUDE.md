# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件下方「v1.X 大項總覽」開始的章節是跨 session 銜接的歷程紀錄（v1.5 → v1.18，最新 2026-05-04）。動工前先讀本段 Quick Reference，然後依任務性質往下讀對應 v1.X 段落。

---

## 專案概要

`tw-stock-collector` — 台股資料蒐集 + 計算 pipeline。FinMind API → Postgres 17，採 4 層 Medallion 架構（Bronze raw / Reference / Silver derived / M3）。Python 3.11+ + Rust（Phase 4 後復權 / Phase 7c K 線聚合）。schema v3.2 r1（`schema_metadata`），開發分支 `claude/initial-setup-RhLKU`，alembic head `o4p5q6r7s8t9_pr21_a_day_trading_ratio`（2026-05-04）。

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

### Silver 計算（Phase 7 dirty-driven）

```bash
python src/main.py silver phase 7a [--stocks 2330] [--full-rebuild]   # 12 個獨立 builder
python src/main.py silver phase 7b [--stocks 2330] [--full-rebuild]   # 跨表依賴(financial_statement)
python src/main.py silver phase 7c [--stocks 2330]                    # tw_market_core Rust(rust_bridge.run_phase4)
```

`--full-rebuild` 目前是唯一支援的模式;dirty queue pull 留 PR #20+。

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

### Medallion 層級（v3.2 r1）

| 層 | 內容 | 寫入 path | 主要 module |
|---|---|---|---|
| Bronze | FinMind raw 資料(8 張 `*_tw` 表 + 5 個 PR #18.5 dual-write entries) | Phase 1-6 collector | `phase_executor.py` + `field_mapper.py` + `aggregators.py` |
| Reference | `stock_info_ref` / `trading_date_ref` 等不變維度 | Phase 1 | 同上 |
| Silver | 14 張 `*_derived`(13 個 Python builder + `price_limit_merge_events` Rust)+ 4 張 `price_*_fwd`(Rust) | Phase 7a/7b/7c dirty-driven | `silver/orchestrator.py` + `silver/builders/*.py` + Rust |
| M3 | 下游波浪 / indicator core(規格在 `m2Spec/`,未動工) | — | — |

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

### Phase 7（Silver 計算）

```
Phase 7a  12 個獨立 builder           — 串列(PostgresWriter 單 connection,thread-safety 限制)
Phase 7b  跨表依賴 builder             — financial_statement(對齊 monthly_revenue)
Phase 7c  tw_market_core Rust 系列    — price_*_fwd + price_limit_merge_events(走 rust_bridge)
```

`SilverOrchestrator.run(phases, stock_ids, full_rebuild)` 行為：
- `NotImplementedError` → `status="skipped"`，不中斷其他 builder
- 一般 `Exception` → `status="failed"` + reason，**也不中斷**（對齊 `cores_overview §7.5` dirty 契約：失敗 builder 不 reset `is_dirty`，留下次重試）

### 模組地圖（`src/`）

| 模組 | 職責 |
|---|---|
| `main.py` | CLI（argparse subparsers + asyncio dispatch；`_run_collector` vs `_run_silver` 分流） |
| `config_loader.py` | TOML 解析 + validation（規則 5 要求 `volume_factor`） |
| `phase_executor.py` | Phase 1-6 排程；mode 從 CLI runtime 傳入（不從 `config.execution.mode` 讀） |
| `api_client.py` + `rate_limiter.py` | aiohttp FinMind client + token bucket（含 429 cooldown） |
| `field_mapper.py` | API → schema 映射 + detail JSONB pack；回 `(rows, schema_mismatch)` tuple |
| `db.py` | `DBWriter` Protocol + `PostgresWriter`（生產）/ `SqliteWriter`（過渡，`TWSTOCK_USE_SQLITE=1`） |
| `aggregators.py` | Phase 5/6 聚合：法人 pivot、財報 pack、`_filter_to_trading_days()` |
| `post_process.py` | 除權息事件衍生（`_recompute_stock_dividend_vf` SQL 修 P1-17）+ Phase 4 staleness 短期補丁 |
| `rust_bridge.py` | 派 Phase 4/7c 給 Rust binary；assert `schema_version="3.2"` |
| `silver/orchestrator.py` + `silver/builders/` | Phase 7 dirty-driven Silver 計算（13 builders） |
| `silver/_common.py` | builder 共用：`fetch_bronze` / `upsert_silver` / `reset_dirty` / `get_trading_dates` |
| `bronze/dirty_marker.py` | Bronze→Silver dirty 標記 API surface（PR #20 trigger 上線後 deprecated） |

### Rust（`rust_compute/`，sqlx + Postgres）

- Binary `tw_stock_compute`，入口 `src/main.rs`，呼叫端 `src/rust_bridge.py`
- 後復權迴圈核心：**先 push 再更新 multiplier**（除息日當日 raw 已是除息後，不可再乘該日 AF）
- 拆兩個 multiplier（v1.8）：`price_multiplier`（從 AF）+ `volume_multiplier`（從 vf）
- Phase 4 永遠全量重算（multiplier 倒推，partial 邏輯上錯）；Python `_mode` 對 Rust 端是 no-op

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
| `m2Spec/collector_schema_consolidated_spec_v3_2.md` | v3.2 r1 schema 整合規格（4 層 Medallion + 14 Silver） |
| `m2Spec/collector_rust_restructure_blueprint_v3_2.md` | Rust + collector 重構藍圖（PR #17 → #21 切法） |
| `m2Spec/cores_overview.md` | M3 計算層總覽（§7.5 dirty queue 契約 / §10.0 Core 邊界三原則） |
| `m2Spec/{tw_market,traditional,neely,fundamental,chip,environment}_core.md` | 各 core 計算規格 |
| `m2Spec/indicator_cores_{momentum,pattern,volatility,volume}.md` | indicator 計算規格 |
| `m2Spec/unified_alignment_review_r2.md` | 11 篇 core spec 審查整合（r1 → r3.1，含 av3 結論） |
| `docs/schema_reference.md` / `docs/collectors.md` | DB schema 與 collector 細節 |
| `docs/claude_history.md` | v1.4 → v1.7 歷史細節（已從本文件搬出） |
| `docs/MILESTONE_1_HANDOVER.md` | M1 milestone handover |

當前 PR sequencing：`#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️(smoke ✓) → #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → #20 ✅(15/15 OK) → #21-A ⏳ 待 user verify → #21-B`。下個 session 主任務見「下次 session 建議優先序」段。

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

### 已知狀態(下次 session 起點)

- alembic head:`o4p5q6r7s8t9`(待 user 本機 `alembic upgrade head`)
- 5 衍生欄缺口:2 補(market_value_weight / day_trading_ratio)+ 3 留 PR #21-B
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5 ⚠️(smoke ✓)→ #19c-1 ✅ → #19c-2 ✅ → #19c-3 ✅ → #20 ✅(15/15)→ **#21-A ⏳ 待 user verify** → #21-B → #22

下個 session 建議:
1. PR #21-A user 本機驗證 — `silver phase 7a` + spot-check 兩欄數值合理性
2. **PR #21-B** 動工:3 條新 Bronze + 30~40h backfill 計畫(需 user 排日曆時間)
3. 平行可動:bronze/phase_executor.py 拆段 / B-1/B-2 收尾 market_ohlcv_tw dual-source

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

**v1.8 進化**:`compute_forward_adjusted` 拆兩個獨立 multiplier(`price_multiplier` 從 AF / `volume_multiplier` 從 vf);詳見 commit `c71d422` + `m2Spec/unified_alignment_review_r2.md` r3.1 P0-11 段。


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

### 🟡 待研究：Phase 4 真正的 incremental 優化（v1.7 新提）

Rust `process_stock` 全量重算是必要設計（multiplier 倒推），但 Python 層可加「該股票自從上次 Phase 4 以後沒新除權息事件就跳過」的偵測。目前每天 incremental 跑 1700+ 檔都全炒，效能優化空間大（每檔約 200ms × 1700 = ~6 分鐘可省）。實作要點：在 phase_executor 跑 Phase 4 前查 `price_adjustment_events.date > stock_sync_status.last_phase4_at`，只把 dirty 股票傳給 Rust。

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

> **🎯 v1.18 PR #21-A 已 land(2026-05-04)**:`market_value_weight` +
> `day_trading_ratio` 兩個 builder-only 衍生欄補完;3 條需新 Bronze 的衍生欄
> 留 PR #21-B。
> 下階段主軸:**PR #21-B 新 Bronze 補完剩 3 條衍生欄**,需 user 排 30~40h
> backfill 計畫(走 PR #18.5 同 pattern)。

### 阻塞性排序

1. **🎯 PR #21-A user 本機驗證**(~30 分鐘)
   - `alembic upgrade head` → `o4p5q6r7s8t9`
   - `python src/main.py silver phase 7a --stocks 2330 --full-rebuild`
   - psql spot-check `valuation_daily_derived.market_value_weight` 範圍 [0,1]
     合理 + `day_trading_derived.day_trading_ratio` 約 0~50% 區間
   - `python scripts/verify_pr19b_silver.py` 仍 5/5 OK

2. **🎯 PR #21-B — 3 條新 Bronze + 衍生欄補齊**(下個 session 主任務,~1 天 + backfill)
   - `institutional.gov_bank_net` ← `TaiwanStockGovernmentBankBuySell`(候選名)
   - `market_margin.total_margin_purchase_balance` / `total_short_sale_balance`
     ← `TaiwanStockTotalMarginPurchaseShortSale`
   - `margin.sbl_short_sales_*`(3 欄)— 需研究 FinMind 哪個 dataset 提供
     daily 借券累計(現 `securities_lending_tw` 是 trade-level)
   - 三條都需新 Bronze 表 + alembic + collector.toml dual-write + builder 修
   - 規模:3 entries × 1700+ stocks × 21 年 ≈ 30~40h calendar-time @ 1600 reqs/h
     (對齊 v1.13 PR #18.5 流程)

3. **PR #21 收尾** — 砍 §5.6 deprecated 路徑
   - 觀察 1~2 sprint dirty queue 無歧義後,砍 `post_process.invalidate_fwd_cache`
     函式本體 + `bronze/dirty_marker.mark_silver_dirty` no-op
   - Rust binary 改讀 `price_daily_fwd.is_dirty=TRUE` 取代 `stock_sync_status.fwd_adj_valid=0`
     (orchestrator path 已接,Rust 自接是收尾用 — 兩端任一條 path work 就夠)

4. **bronze/phase_executor.py 從 src/phase_executor.py 拆出**(blueprint §三
   結構工 — phase 1-6 屬 bronze,phase 7 屬 silver,目前都擠在 src/ 根)

5. **B-1/B-2 收尾** — `market_ohlcv_tw` dual-source merge(`TaiwanStockTotalReturnIndex` +
   `TaiwanVariousIndicators5Seconds` → daily OHLCV);完成後 `taiex_index_derived`
   才有真資料(目前 PR #19c-1 verifier 對 2330 read=0 wrote=0 是 source-empty,非 builder bug)

### 中期 backlog(non-blocking)

6. **`asyncio.gather` 7a 平行優化** — 需先升 PostgresWriter 為 connection pool;
   perf gain ~ms 量級,排序低
7. **Phase 4 真正的 incremental 優化** — 偵測「該股票無新除權息事件 → 跳過」
   每天 incremental 可省 ~6 分鐘
8. **`inspect_db.py` 升 PG 版** — v2.0 後該腳本是 SQLite hardcode 不可用
9. **CLAUDE.md 章節重組** — ~~v1.4 → v1.7 詳解搬 `docs/claude_history.md`~~ ✅ v1.18 reorg 已完成(v1.5-v1.9.1 全部搬到 history;主檔從 1500+ → ~1260 行)
10. **agent-review-mcp 支線**(v1.4 spec,自 v1.6 懸而未決)
11. **PR review + merge** — `claude/initial-setup-RhLKU` 累積 v1.10 → v1.18 ~60+ commit
    待 maintainer 整合
12. **m2 PR #20 / #21 完整 milestone** — orchestrator go-live + Silver views(spec §2.5)
    + legacy_v2 rename(blueprint §八.2)+ M3 prep,blueprint §十 PR 切法
