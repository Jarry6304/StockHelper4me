# CLAUDE.md — tw-stock-collector Session 銜接文件

> 這份文件記錄本專案的完整實作歷程與架構決策，供下次 session 自動載入後直接銜接，無需重新閱讀 git log。
> 最後更新：2026-05-02（**v1.9.1**）

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
- v3.2 r1 PR sequencing:#17 ✅ → #18 ✅ → #19a ✅ → #19b ✅ → #18.5(smoke ✓)→ #19c-1 ✅ → #19c-2 ✅ → **#19c-3 ⏳ 待 user verify** → #20

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

## v1.9 大項總覽(2026-05-02)

本 session 主要做了 5 件事:

### 1. m2 Hard 階段動工前 spec 審查 + 3 處 amend

對 blueprint v3.2 r1 Hard 階段(PR #17~#21)在 av3/r3.1/P0-11/P1-17 修法後做 spec staleness audit,結論:不需重設,但動工前需補 3 處 amend。一次落地(commit `f46d50d`):
- **§3.1**:加 silver builder 入口/出口契約表 + 3 條紀律
- **§5.2**:同步 ALTER `price_daily_fwd` 加 4 欄 DDL(per §4.4 r3.1)
- **§5.5**:Bronze→Silver dirty trigger DDL 範例 + 後復權 trigger
- **§六**:加 5.7 row 描述長期 dirty queue 上線排程

### 2. PR #17 (B-3) 主體:events 砍 3 + fwd 加 4 + Rust 拆 multiplier

Hard 階段第 1 個 PR,把 av3 / r3.1 / P0-11 修正後的事實落地到 schema + production code:

| commit | 內容 |
|---|---|
| `f215d5b` | rust_bridge `EXPECTED_SCHEMA_VERSION` `2.0`→`3.2`(`db1a7f6` schema bump 漏改 Python 端 1 行) |
| `4eddd1c` | **PR #17 主體**:events 砍 `adjustment_factor`/`after_price`/`source` + fwd 加 `cumulative_adjustment_factor`/`cumulative_volume_factor`/`is_adjusted`/`adjustment_factor` 4 欄 + Rust 拆 multiplier + alembic migration `i8j9k0l1m2n3` |
| `7db9c42` | **R-1 漏改修補**:R-1 PR (`05b9101`) 只改了 alembic + schema_pg.sql,Rust binary `load_trading_dates`(原 `load_trading_calendar`)沒同步,user 跑 Phase 4 撞「relation trading_calendar does not exist」 |
| `d2b081f` | Merge user 主分支(R-1/R-2/B-4/B-5/B-6 + B-6 LEADING hotfix)進 review-todo-list-tblnC,兩端工作互補無衝突 |
| `ac7c980` | follow-up:`config_loader.py` 規則 5 改成要求 `volume_factor`(原強制 `adjustment_factor` 已砍欄);av3 SQL 4 處 `pae.adjustment_factor` 改用 `f.adjustment_factor` |
| `ccfe13e` | av3 verdict 段對齊 r3.1 + PR #17 + P0-11 + P1-17 後事實 |

### 3. P1 dividend AF 修補(reference_price 偷懶)

PR #17 後 av3 Test 3 揭露:純股票股利 events(`cash=0`, `stock>0`)的 `af_in_fwd = 1.0`,close_ratio 接近 1.0(該事件沒被套進 multiplier)。

**Root cause(SQL diagnostic 揭露)**:FinMind `TaiwanStockDividendResult` 對純股票股利**直接把 `reference_price` 設成 `= before_price`**(沒做真除權計算),Rust Priority 1 算 `af = bp/rp = 78.8/78.8 = 1.0` 數學上對但語意錯。

3 輪修法:

| commit | 內容 |
|---|---|
| `a5e089f` | v1:加 dividend Priority 2 fallback 公式,但條件 `Some(bp)` 沒生效(誤以為 before_price NULL) |
| `1974fa9` | v2:before_price 改從 raw_prices lookup,但 Priority 1 仍先觸發 af=1.0 |
| `c8367f8` | **v3 主修**:Priority 1 加 sanity check — `event_type='dividend' AND stock>0 AND cash=0 AND bp==rp` → fallthrough Priority 2 用 `af = 1 + stock/10` 公式 |

**驗證**(av3 Test 3 重跑後):
- 3363 2026-01-20: cash=0 stock=7.61 → af = **1.7610** ✓
- 3363 2023-10-17: cash=0 stock=2.64 → af = **1.2640** ✓
- 1312 2023-11-28: cash=0 stock=0.42 → af = **1.0420** ✓
- 8932/5278 等混合 dividend Priority 1 維持(cash>0 不觸發 sanity check)

`vf_in_pae * af_in_fwd = 1.0` 倒數守恆驗證:0.5679 × 1.7610 = 1.0000 ✓

### 4. PowerShell 中文亂碼戰役(5 輪修法)

User 在 zh-TW Windows 11 (cp950 ACP) PowerShell 5.1 跑 av3 SQL,中文 verdict / 章節標題 / `(N 筆資料)` 全部亂碼。經過 5 輪攻防:

| 嘗試 | 結果 |
|---|---|
| chcp 65001 + Console.OutputEncoding=UTF8 | SELECT verdict 中文 OK,`\echo` 中文亂 |
| `\echo` 全換 `COPY (SELECT '...') TO STDOUT;` | 仍亂(PS 5.x 對 native command stdout pipe 大 byte stream encoding bug) |
| Get-Content -Encoding UTF8 pipe 給 psql stdin | 仍亂(同 PS pipe bug) |
| **`psql -o tempFile` + Get-Content -Encoding UTF8 讀檔顯示** | ✅ 99% 對(只剩 `(N 筆資料)` 亂) |
| **`$env:LC_MESSAGES = "C"` 強制 psql 用英文 message** | ✅ **100% 對**(`(N rows)` 純 ASCII) |

**Byte-level diagnostic 證實**(commit `6222834`):psql `-o file` 寫的是純 UTF-8 byte(byte 81 = `E5 B7 B2 = 已`)。問題在 PS 5.x 對 native command stdout pipe 的 encoding handling,不是 psql.exe transcode bug。

最終 wrapper(commit `3c3d8a0`):`scripts/run_av3.ps1`,試圖三層 console UTF-8 + LC_MESSAGES=C + temp file roundtrip,完整 finally 區塊還原 user shell。

### 5. av3 結論段對齊 r3.1 + a0a5ddf SQL transform

`scripts/av3_spot_check.sql` 75 處 `\echo` 一次性轉 `COPY (SELECT '...') TO STDOUT`(`a0a5ddf`),雖然後續發現 `\echo` 在新 wrapper 下也 work,但 COPY 形式保留(對 stdin/file 兩條 path 都 work,更 portable)。

判讀指南(commit `ccfe13e`)整段重寫對齊 r3.1 + PR #17 + P0-11 + P1-17 落地版,砍掉過時的 `P0-8/C1` / 「Test 6 sanity FAIL」等錯誤判讀。

---

## v1.9.1 補丁(2026-05-02 後續 session)

接續 v1.9 main session,在 `claude/initial-setup-RhLKU` 分支補完 av3 Test 4 完整覆蓋驗證,並把 tblnC merge 進來統一分支。

### A. 24 檔 split / par_value backfill 完成(解 v1.9 todo #3「stock list 補完」)

之前 av3 Test 4 只 join 到 10 個事件(7 split + 1 cap_red + 2 cap_inc),揭露 16 / 31 個 par_value / split 事件對應股票不在 user `stock_info_ref` 收錄。

本 session 跑 `scripts/discover_split_candidates.sql` 列出 24 缺檔:
- **par_value**: 2327 / 6919 / 4763 / 8476 / 3093 / 5536 / 6613 / 6415 / 6531 / 8070
- **split**: 3086 / 8937 / 7780 / 8422 / 00715L / 0052 / 00674R / 00631L / 00706L / 00673R / 0050 / 00663L / 00676R / 00632R

User 一次 `python src\main.py backfill --stocks <24 ids> --phases 1,2,3,4` 跑完。

**驗證**(av3 Test 4 重跑):

| event_type | events 變化 |
|---|---|
| split | 7 → 17 |
| capital_increase | 2 → 3 |
| capital_reduction | 1 → 3 |
| par_value_change | 6 → 16 |

**數學核對**(精確匹配 P1-17 公式 + cumulative vf 設計):
- 2327 2024-08-15 stock_div=121.48(超大案)→ vf_in_pae=0.0760 ≈ 1/12.148 ✓,subsequent split+par_value 同日 vf=0.25 各一 → vol_ratio=16 ✓
- 4763 2024-09-12 stock_div=0.15 → vf=0.9852 ✓,subsequent vf=0.1 各一 → vol_ratio=100 ✓
- 8476 2024-07-09 stock_div=0.02 → vf=0.9980 ✓,subsequent vf=0.5 各一 → vol_ratio=4 ✓
- 8932 2025-09-08 cash=0.28 stock=0.04 → vf=0.9958 ✓,subsequent split vf=0.5 → vol_ratio=2.0 ✓

### B. 新工具 `scripts/discover_split_candidates.sql`(commit `0d650c0` + `b88a882`)

盤點 av3 Test 4 backfill 候選 SQL:6 步驟列 pae 各 event_type 統計、與 price_daily / price_daily_fwd join 涵蓋率、缺檔 stock_id 清單(LIMIT 50)、6505 對照組驗證。`b88a882` 修兩個 schema 落差 bug:
- `stock_info` → `stock_info_ref`(R-2 後表名,PR #11)
- `pae.adjustment_factor` → 砍掉(PR #17 後該欄在 pae 已不存在)

### C. `scripts/fix_p1_17_stock_dividend_vf.sql` deprecated(commit `b88a882`)

UPDATE 0 row 證明 post_process `_recompute_stock_dividend_vf` 路徑早已自動修對既存資料。檔案保留 + 加 DEPRECATED header 供事件考古,不再使用。

### D. 分支整合 + 清掉 log dump

- `f83adf9` Merge tblnC 22 commits 進 initial-setup-RhLKU(0 衝突,merge base = `0d650c0`,兩邊改檔完全沒交集)
- 清掉 tblnC merge 進來的 10 個 log .txt(`av3_*.txt` / `discover.txt` / `fix_p1_17_log.txt` / `p1_17_result.txt` / `test.txt`)
- `.gitignore` 補 root-level `/av3_*.txt` 等規則防未來再進

---

## v1.8 大項總覽(2026-05-01 ~ 2026-05-02)

本 session 主要做了 4 件事：

### 1. m2 collector 重構藍圖（藍圖 v3.2 r1）

依 `m2Spec/collector_schema_consolidated_spec_v3_2.md` 對齊 4 層 Medallion(Bronze/Reference/Silver/M3),產出 `m2Spec/collector_rust_restructure_blueprint_v3_2.md`：盤點現行 v2.0 collector + rust_compute,提供模組拆分、Schema 異動、Phase 0 動工順序、Migration 雙寫策略、PR 切法。

### 2. Cores spec 系列審查（r1 → r2 → r2.1 → r3 → r3.1）

針對 11 篇 Core spec 三輪迭代審查整合報告 `m2Spec/unified_alignment_review_r2.md`：
- **r1→r2**：13 處 r1 邏輯/引用/數量修正
- **r2→r2.1**：12 處事實/流程錯誤(7 A 系列 + 5 B 系列),基於 11 篇 spec 原文 spot-check
- **r2.1→r3**：C 系列 10 條漏抓 gap promote 進 P0(3)/P1(3)/P2(4)
- **r3→r3.1**：av3 實機驗證後新增 P0-11(Rust split volume bug)+ P1-17(field_mapper stock_dividend bug)

### 3. A-V3 spot-check 實機驗證（P0-2 阻塞解除）

`scripts/av3_spot_check.sql` 6 個 test 揭露：
- ✅ 現金 dividend：Rust 派 dollar_vol 守恆(spec 假設成立)
- 🔴 stock_dividend / split / par_value：Rust 算錯方向(P0-11 production bug)
- 🔴 staleness production 證據：3363 / 1312 stock_dividend 事件 fwd 沒處理(P0-7)
- ✅ 既有 collector field_mapper 寫對的 `volume_factor` 但 Rust 完全不讀

### 4. 完整修復(8 個 commit)

| commit | 任務 | 內容 |
|---|---|---|
| `9dd2da5` | A-V3 SQL 創建 | scripts/av3_spot_check.sql + .md |
| `f44fc0d` | F | Test 2 CASE 順序修正(後續 commit a2c94c2 改用 dollar_vol invariant 重作) |
| `5a05cff` | A + B | r3 → r3.1 整合 av3 結論 + P0-11 / P1-17 新增 |
| `e051216` | D 補丁 | post_process.invalidate_fwd_cache + phase_executor 寫 events 後 reset fwd_adj_valid |
| `c71d422` | **C(主修)** | Rust compute_forward_adjusted 拆 price_multiplier / volume_multiplier(用 vf 不用 AF)|
| `a2c94c2` | F 重作 | Test 2 CASE 改用 dollar_vol invariant 判派系 |
| `608d275` | P1-17 | post_process._recompute_stock_dividend_vf + scripts/fix_p1_17_stock_dividend_vf.sql |
| `d029be3` | overview | §7.5 dirty queue 契約 + §10.0 Core 邊界三原則(P0-7 + Core 邊界落地 spec 端) |

**Convention 切換**(動工後重大語意變化):
- 對現金 dividend：fwd_volume = raw_volume(不再 / AF)→ dollar_vol 不再守恆,但反映實際 share 流動性
- 對 split / par_value：fwd_volume = raw_volume / vf(post-event equivalent shares)
- 對 stock_dividend：vf = 1 / (1 + stock_dividend / 10) 由 post_process 修正(commit 608d275)

User 已 cargo build + 全市場 1348 檔 Phase 4 重跑驗證(av3 重跑 Test 1 vol_ratio 從 0.924 → 1.0 全部對齊預測)。

---

## 過去版本沿革（v1.5 ~ v1.7）

> v1.5 / v1.6 / v1.7 的 commits 表 + 逐輪修正詳解(共 ~210 行)已搬到 [`docs/claude_history.md`](docs/claude_history.md)。
> 主檔保留:v1.7 收尾 PR 已合到 `m1/postgres-migration`,base sha `9890294`。

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

> **🎯 PR #19c orchestrator + CLI 已落地(PR #19c-3,cherry-pick 自平行分支)— user verify 後接 PR #20**(trigger ENABLE)或衍生欄補齊。

1. **🎯 PR #19c-3 本機驗證 + push**(本 session 完成 v1.16 整合):
   - `python src/main.py silver phase 7a --stocks 2330 --full-rebuild`(12 個 7a builder)
   - `python src/main.py silver phase 7b --stocks 2330 --full-rebuild`(financial_statement)
   - `git push`(本 session 已 commit + push 到 init-project-setup-yGset)
2. **PR #20 動工**(Bronze→Silver trigger CREATE + ENABLE + 1:4 fanout 整合測試)
   - blueprint §5.5 DDL trg_mark_silver_dirty + 14 個 CREATE TRIGGER
   - 砍 §5.6 短期補丁 post_process.invalidate_fwd_cache(dirty queue 接管)
   - 整合測試:INSERT INTO price_adjustment_events → 4 fwd 表 is_dirty=TRUE
3. **衍生欄補齊**(可與 PR #20 平行,部分需新 Bronze + alembic):
   - institutional.gov_bank_net(新 GovernmentBankBuySell Bronze)
   - margin SBL 6 cols(integrate securities_lending_tw,Bronze 已存在)
   - valuation.market_value_weight(join price_daily + stock_info_ref)
   - day_trading_ratio(join price_daily volume)
   - market_margin total_*_balance(新 TotalMarginPurchaseShortSale Bronze)
4. **bronze/phase_executor.py 從 src/phase_executor.py 拆出**(blueprint §三 結構)
5. **asyncio.gather 7a 平行優化**(需先升 db connection pool)
3. **PR #19c 動工**(剩 8 個 builder + orchestrator 真實邏輯 + Phase 7a/7b/7c CLI)
   - holding_shares_per / monthly_revenue / financial_statement(依 PR #18.5)
   - taiex_index / us_market_index / exchange_rate / market_margin / business_indicator
   - margin / valuation / institutional / day_trading 補 PR #19b 暫不填的衍生欄
     (gov_bank_net / market_value_weight / SBL 6 欄 / day_trading_ratio)
   - `silver/orchestrator.py` 補 asyncio.gather 7a 平行 + 7b 序列 + 7c 走 rust_bridge
   - `src/main.py` 加 `silver phase 7a/7b/7c` 子命令
   - `bronze/phase_executor.py` 從 src/phase_executor.py 拆出
4. **PR #20 動工**(Bronze→Silver trigger CREATE + ENABLE + price_adjustment_events 1:4 fanout 整合測試)
   - blueprint §5.5 DDL trg_mark_silver_dirty + 14 個 CREATE TRIGGER
   - 砍 §5.6 短期補丁 post_process.invalidate_fwd_cache(PR #19c 起 dirty queue 接管)
   - 整合測試:INSERT INTO price_adjustment_events → 4 fwd 表 is_dirty=TRUE
5. **v1.9~v1.12 PR review + merge**(initial-setup-RhLKU 分支累積 ~40+ commit)。等 maintainer review,平行進行。
5. **agent-review-mcp 支線開始**(spec 在最早的訊息,從 v1.6 起就懸而未決)
6. **Phase 4 真正的 incremental 優化**(現在 staleness 補丁是「全部 reset 0」,長期該做 dirty-detection 只跑變動股票)
7. **CLAUDE.md 章節重組**(本檔已超過 700 行,v1.4-v1.7 詳解可繼續搬 docs/claude_history.md)
8. **inspect_db.py 升 PG 版**(v2.0 後該腳本已不可用)
9. **m2 PR #20 / #21**(orchestrator go-live + Silver views + legacy_v2 rename + M3 prep,blueprint §十 PR 切法)
