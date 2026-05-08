# Data Refactor Plan：Bronze + Silver m2 重構（PR #R1~#R6）

> **版本**：v1.0（draft for user review）
> **日期**：2026-05-09
> **配套文件**：`layered_schema_post_refactor.md`（重構目標 schema）、`adr/0001_tw_market_handling.md`
> **適用範圍**：tw-stock-collector / StockHelper4me — 從 v3.2 r1 完成狀態（main HEAD `6ee2d94`，alembic head `q6r7s8t9u0v1`）推進到 spec 的最終目標 schema
> **目的**：明確 R1~R6 6 個 PR 的範圍 / commit 順序 / 風險評估 / 觀察期 / 驗證流程，作為動工前 single source of truth

---

## 目錄

1. [現況盤點與 Delta](#一現況盤點與-delta)
2. [PR sequencing 總覽](#二pr-sequencing-總覽)
3. [PR #R1：補回 source 欄位](#三pr-r1補回-source-欄位)
4. [PR #R2：v2.0 舊表 rename `_legacy_v2`](#四pr-r2v20-舊表-rename-_legacy_v2)
5. [PR #R3：`_tw` Bronze 升格 rename](#五pr-r3_tw-bronze-升格-rename)
6. [PR #R4：collector.toml entry name 收主名](#六pr-r4collectortoml-entry-name-收主名)
7. [PR #R5：觀察期 21~60 天](#七pr-r5觀察期-21-60-天)
8. [PR #R6：DROP `_legacy_v2`](#八pr-r6drop-_legacy_v2)
9. [風險清單與 Rollback 策略](#九風險清單與-rollback-策略)
10. [後續退場（spec §7.3）](#十後續退場spec-73)

---

## 一、現況盤點與 Delta

### 1.1 已對齊（v3.2 r1 完成度 ~80%）

`layered_schema_post_refactor.md` 列的 Bronze 21 張、Silver 14 張，**已落地** 19+14 = 33 張。完整對齊：

- **Bronze 19 張**：`trading_date_ref` / `stock_info_ref` / `market_index_tw` / `market_ohlcv_tw` / `price_adjustment_events` / `stock_suspension_events` / `_dividend_policy_staging` / `price_daily` / `price_limit` / `institutional_investors_tw` / `margin_purchase_short_sale_tw` / `securities_lending_tw` / `foreign_investor_share_tw` / `day_trading_tw` / `valuation_per_tw` / `market_index_us` / `exchange_rate` / `institutional_market_daily` / `market_margin_maintenance` / `fear_greed_index` / `business_indicator_tw` / `government_bank_buy_sell_tw` / `total_margin_purchase_short_sale_tw` / `short_sale_securities_lending_tw`（PR #21-B）
- **Silver 14 張**：12 個 builder + financial_statement + 4 個 fwd 表

### 1.2 Delta（要動的 7 點）

| # | 點 | 細節 | 對映 PR |
|---|---|---|---|
| 1 | `holding_shares_per_tw` 缺 `source` 欄位 | spec §3.5 明文「PR #R1 補回」 | #R1 |
| 2 | `financial_statement_tw` 缺 `source` 欄位 | spec §3.6 同 | #R1 |
| 3 | `monthly_revenue_tw` 缺 `source` 欄位 | spec §3.6 同 | #R1 |
| 4 | v2.0 舊 `holding_shares_per` 仍存（dual-write target） | rename 成 `_legacy_v2` 進入觀察期 | #R2 |
| 5 | v2.0 舊 `financial_statement` / `monthly_revenue` 同 4 | 同 | #R2 |
| 6 | 3 張 `_tw` Bronze 要去 suffix 升格成主名 | spec §3.5 / §3.6 註明「PR #R4 後升格」 | #R3 |
| 7 | collector.toml 對應 `_v3` entry 要改 `target_table` 對齊 + 將 entry name 收回主名 | spec §五 註明「PR #R4 後 `api_name` 將從 `_v3` 收回主名」 | #R3 + #R4 |

> 註：spec 把「升格 rename」標記在「PR #R4 後」，我這份 plan 把實際 schema rename 動作放 PR #R3 執行（rename 必須先做，後續 PR 才能引用新名）。spec 的 PR #R4 標記是「升格『生效後』」的時序描述，不是 PR 編號上的順序。

### 1.3 退場時序

```
T0     PR #R1 / #R2 / #R3 / #R4 連續落地（同 sprint）
                              │
                              │ ←─── PR #R5 觀察期開始
                              │
T0+21  最早可進 PR #R6（spec §7.1 觀察期 21~60 天）
T0+60  最晚應進 PR #R6（過久觀察徒占空間）
```

---

## 二、PR sequencing 總覽

| PR | 目標 | 估時 | 風險 | 可 rollback? |
|---|---|---|---|---|
| **#R1** | 3 張 `_tw` Bronze 加回 `source` 欄 | 1h | 🟢 低 | 是（DROP COLUMN）|
| **#R2** | 3 張 v2.0 舊表 rename `_legacy_v2` + collector.toml v2.0 entry `target_table` 同步 | 半天 | 🟡 中 | 是（rename 反向）|
| **#R3** | 3 張 `_tw` Bronze rename 去 suffix 升格 + builder/dirty trigger/collector.toml v3 entry 同步 | 半天 | 🟡 中 | 是（rename 反向 + builder revert）|
| **#R4** | collector.toml v3 entry name 收主名（`holding_shares_per_v3` → `holding_shares_per` 等）+ api_sync_progress 同步遷移 | 1~2h | 🟡 中 | 是（entry name 反向）|
| **#R5** | 觀察期，無 code change | 21~60d | — | — |
| **#R6** | DROP 3 張 `_legacy_v2` | 1~2h | 🟢 低 | **不可**（永久刪表）|

---

## 三、PR #R1：補回 source 欄位

### 3.1 範圍

3 張 Bronze 表加回 `source TEXT NOT NULL DEFAULT 'finmind'`：

- `holding_shares_per_tw`
- `financial_statement_tw`
- `monthly_revenue_tw`

### 3.2 改動清單

```
alembic migration rN1_..._add_source_col_to_3_bronze.py
  - ALTER TABLE holding_shares_per_tw   ADD COLUMN source TEXT NOT NULL DEFAULT 'finmind';
  - ALTER TABLE financial_statement_tw  ADD COLUMN source TEXT NOT NULL DEFAULT 'finmind';
  - ALTER TABLE monthly_revenue_tw      ADD COLUMN source TEXT NOT NULL DEFAULT 'finmind';

src/schema_pg.sql
  - 3 個 CREATE TABLE 同步加 source 欄

CLAUDE.md
  - alembic head 更新
```

### 3.3 驗證

```sql
-- 確認 3 表都有 source 欄
\d+ holding_shares_per_tw
\d+ financial_statement_tw
\d+ monthly_revenue_tw

-- 既有資料 default 自動填 'finmind'
SELECT source, COUNT(*) FROM holding_shares_per_tw GROUP BY source;
-- 預期:source='finmind' 7M rows
```

### 3.4 風險

- 🟢 低：純 ALTER ADD COLUMN，default 值不影響既有資料；既有 collector.toml entries 沒指定 `source` 但 schema default 自動填，db.upsert 不會炸。
- Rollback：alembic downgrade（DROP COLUMN）

---

## 四、PR #R2：v2.0 舊表 rename `_legacy_v2`

### 4.1 範圍

3 張 v2.0 舊表 rename 進入觀察期：

| 舊名 | 新名 |
|---|---|
| `holding_shares_per` | `holding_shares_per_legacy_v2` |
| `financial_statement` | `financial_statement_legacy_v2` |
| `monthly_revenue` | `monthly_revenue_legacy_v2` |

### 4.2 改動清單

```
alembic migration rN2_..._rename_v2_legacy_3_tables.py
  - ALTER TABLE holding_shares_per   RENAME TO holding_shares_per_legacy_v2;
  - ALTER TABLE financial_statement  RENAME TO financial_statement_legacy_v2;
  - ALTER TABLE monthly_revenue      RENAME TO monthly_revenue_legacy_v2;

config/collector.toml
  - 3 個 v2.0 entry(holding_shares_per / financial_income / financial_balance /
    financial_cashflow / monthly_revenue)的 target_table 改成 *_legacy_v2

src/schema_pg.sql
  - 3 表 CREATE TABLE 改名(legacy_v2)

scripts/verify_pr18_bronze.py
  - SPECS legacy_table 名稱對齊(reverse_pivot 仍從 legacy 反推到 _tw,只是名稱改)
  ⚠️ 但 PR #18 reverse-pivot 對 financial_statement / holding_shares_per /
     monthly_revenue 走的是 PR #18.5 Option A FinMind 重抓,不是 reverse-pivot
     → 不需要動 verify_pr18

CLAUDE.md
  - 「資料庫 Schema」表 3 表名稱更新
  - alembic head 更新

src/silver/builders/*.py
  - holding_shares_per builder / financial_statement builder / monthly_revenue
    builder 的 BRONZE_TABLES 不動(它們本來就讀 _tw,不讀 v2.0 legacy)
  ✅ 0 builder 改動
```

### 4.3 驗證

```sql
-- 3 個 _legacy_v2 表存在 + 既有資料完整
SELECT 'holding_shares_per_legacy_v2' AS t, COUNT(*) FROM holding_shares_per_legacy_v2
UNION ALL SELECT 'financial_statement_legacy_v2', COUNT(*) FROM financial_statement_legacy_v2
UNION ALL SELECT 'monthly_revenue_legacy_v2',   COUNT(*) FROM monthly_revenue_legacy_v2;

-- 舊名表已不存在
SELECT 'holding_shares_per' AS t, COUNT(*) FROM holding_shares_per;  -- ERROR: relation does not exist
```

跑 incremental backfill 驗證 dual-write entries 仍正常寫入新 _legacy_v2：

```powershell
python src/main.py incremental --phases 5 --stocks 2330
psql $env:DATABASE_URL -c "
  SELECT MAX(date) FROM holding_shares_per_legacy_v2 WHERE stock_id='2330'
"
```

### 4.4 風險

- 🟡 中：rename 期間 dual-write 寫入路徑改變，若 collector.toml `target_table` 沒同步改會 INSERT 到不存在的表 → schema_mismatch error。
- 已 active 的 Silver builder 都讀 `_tw` 不讀 v2.0 legacy，**Silver pipeline 不受影響**。
- Rollback：alembic downgrade（rename 反向）

---

## 五、PR #R3：`_tw` Bronze 升格 rename

### 5.1 範圍

3 張 `_tw` Bronze 去 suffix 升格成主名：

| 舊名 | 新名 |
|---|---|
| `holding_shares_per_tw` | `holding_shares_per` |
| `financial_statement_tw` | `financial_statement` |
| `monthly_revenue_tw` | `monthly_revenue` |

### 5.2 改動清單（複雜度最高）

```
alembic migration rN3_..._promote_tw_bronze_3_tables.py
  - ALTER TABLE holding_shares_per_tw   RENAME TO holding_shares_per;
  - ALTER TABLE financial_statement_tw  RENAME TO financial_statement;
  - ALTER TABLE monthly_revenue_tw      RENAME TO monthly_revenue;
  - DROP + 重建 對應 dirty trigger:
    - mark_holding_shares_per_derived_dirty  ON holding_shares_per_tw
                                          → ON holding_shares_per
    - mark_financial_stmt_derived_dirty      ON financial_statement_tw
                                          → ON financial_statement(同名其實 trigger 自動跟著表)
    - mark_monthly_revenue_derived_dirty     ON monthly_revenue_tw
                                          → ON monthly_revenue

src/schema_pg.sql
  - 3 個 CREATE TABLE 改名(去 _tw)
  - 3 個 CREATE TRIGGER ON 對應表名同步

config/collector.toml
  - 3 個 v3 entry(holding_shares_per_v3 / financial_*_v3 / monthly_revenue_v3)
    的 target_table 從 *_tw 改成主名
  - entry name 暫不改(留 PR #R4)

src/silver/builders/*.py
  - holding_shares_per_derived 的 BRONZE_TABLES 從 ['holding_shares_per_tw']
    → ['holding_shares_per']
  - financial_statement 的 BRONZE_TABLES 從 ['financial_statement_tw']
    → ['financial_statement']
  - monthly_revenue 的 BRONZE_TABLES 同上

src/bronze/dirty_marker(已刪) — 不影響

CLAUDE.md
  - 多處 mention `_tw` 的 reference 更新
  - alembic head 更新
```

### 5.3 驗證

```sql
-- 3 個主名表存在 + 資料完整
SELECT 'holding_shares_per' AS t, COUNT(*) FROM holding_shares_per
UNION ALL SELECT 'financial_statement', COUNT(*) FROM financial_statement
UNION ALL SELECT 'monthly_revenue', COUNT(*) FROM monthly_revenue;

-- 3 個 trigger 重綁到主名
SELECT trigger_name, event_object_table FROM information_schema.triggers
WHERE trigger_name LIKE 'mark_%' ORDER BY trigger_name;

-- 跑 Silver builder 確認讀新名 OK
python src/main.py silver phase 7a --full-rebuild --stocks 2330
python src/main.py silver phase 7b --full-rebuild
```

### 5.4 風險

- 🟡 中：rename 期間如果 trigger 沒重綁，dirty queue 會斷；`_legacy_v2`(R2)和 `*_tw → 主名`(R3)同期進行，命名衝突需要小心 — 主名空出來後才能 R3 升格
- **PR 順序強約束**：必須 R2 → R3，否則 R3 rename 會撞 v2.0 舊表 PK 衝突
- Rollback：alembic downgrade（rename 反向 + trigger 重綁）

---

## 六、PR #R4：collector.toml entry name 收主名

### 6.1 範圍

collector.toml 4 個 dual-write entry name 從 `_v3` 後綴收回主名：

| 舊 entry name | 新 entry name |
|---|---|
| `holding_shares_per_v3` | `holding_shares_per` |
| `financial_income_v3` | `financial_income` |
| `financial_balance_v3` | `financial_balance` |
| `financial_cashflow_v3` | `financial_cashflow` |
| `monthly_revenue_v3` | `monthly_revenue` |

⚠️ 同時 v2.0 舊 entry `holding_shares_per` / `financial_income` / 等已在 R2 階段 target 改成 `_legacy_v2`，但 entry name 還是主名 — R4 會與 v2.0 entry name 衝突。

### 6.2 改動清單

```
config/collector.toml
  - 5 個 v3 entry 改名:
    holding_shares_per_v3   → holding_shares_per_main(暫名)
    financial_income_v3     → financial_income_main
    financial_balance_v3    → financial_balance_main
    financial_cashflow_v3   → financial_cashflow_main
    monthly_revenue_v3      → monthly_revenue_main

  - v2.0 entry 改名:
    holding_shares_per      → holding_shares_per_legacy
    financial_income        → financial_income_legacy
    financial_balance       → financial_balance_legacy
    financial_cashflow      → financial_cashflow_legacy
    monthly_revenue         → monthly_revenue_legacy

  - 將 _main 同步收回主名:
    holding_shares_per_main → holding_shares_per
    ...

api_sync_progress 遷移
  - UPDATE api_sync_progress SET api_name = 'holding_shares_per'
    WHERE api_name = 'holding_shares_per_v3'
  - UPDATE api_sync_progress SET api_name = 'holding_shares_per_legacy'
    WHERE api_name = 'holding_shares_per' AND
          stock_id IN (SELECT DISTINCT stock_id FROM api_sync_progress
                        WHERE api_name = 'holding_shares_per' AND
                              segment_start < <PR_R2_DATE>)
  ⚠️ 這段會混淆,實際做法另議:可考慮用一張新欄位區分

CLAUDE.md
  - collector.toml entry naming 慣例段更新
```

### 6.3 簡化選項（推薦）

R4 真實複雜度高在於 api_sync_progress 不能簡單 UPDATE — 同名 entry 跨時間點不同 target 會混淆。**推薦簡化**：

- ❌ 不收 v3 entry name 回主名 — 留 `_v3` 後綴永久作為「重抓 spec 來源」標籤
- ✅ 只把 v2.0 entry 改 `_legacy` 後綴 + target_table 改 `_legacy_v2`
- 結果：v3 spec entry 永久叫 `_v3`（OK 因為命名一致），v2.0 entry 顯式 `_legacy`

→ R4 範圍縮成「v2.0 entry 顯式 `_legacy` 後綴」，避免 api_sync_progress 遷移複雜度。

### 6.4 風險

- 🟡 中：api_sync_progress entry name change 會打斷既有 segment status 追蹤；若 R4 做完 backfill 走重抓 path，會踩 conflicts
- Rollback：collector.toml + api_sync_progress 反向 UPDATE

---

## 七、PR #R5：觀察期 21~60 天

### 7.1 範圍

純驗證 + 觀察，無 code change。期望從 R4 落地後算起 21 天最少觀察期：

- ✅ 確認所有 Silver builders 讀新主名 OK
- ✅ 確認 dual-write 仍同步寫入 `_legacy_v2`
- ✅ 確認 verifier `verify_pr19b_silver.py` 仍 5/5 OK
- ✅ 確認 incremental backfill 順暢

### 7.2 觀察 SLO

| 指標 | 目標 |
|---|---|
| Silver builder 12/12 OK | 持續每日 |
| api_sync_progress.status='failed' | 0 |
| 3 張 `_legacy_v2` row count | 與主名表 row count 比對 ±1% |

---

## 八、PR #R6：DROP `_legacy_v2`

### 8.1 範圍

確認觀察期 SLO 達標後永久 DROP：

```
alembic migration rN6_..._drop_legacy_v2_3_tables.py
  - DROP TABLE holding_shares_per_legacy_v2;
  - DROP TABLE financial_statement_legacy_v2;
  - DROP TABLE monthly_revenue_legacy_v2;

config/collector.toml
  - 對應的 5 個 v2.0 _legacy entry DELETE
    (holding_shares_per_legacy / financial_*_legacy / monthly_revenue_legacy)

src/schema_pg.sql
  - 3 個 CREATE TABLE 移除

CLAUDE.md
  - 「資料庫 Schema」表 3 個 _legacy_v2 row 移除
  - alembic head 更新
```

### 8.2 驗證

```sql
-- 3 個 _legacy_v2 不存在
SELECT 'holding_shares_per_legacy_v2' AS t, COUNT(*) FROM holding_shares_per_legacy_v2;
-- ERROR: relation does not exist

-- 主名表 row count 不變
SELECT 'holding_shares_per' AS t, COUNT(*) FROM holding_shares_per;
```

### 8.3 風險

- 🟢 低：DROP TABLE 對主流程無影響（觀察期 SLO 已驗證）
- ⚠️ **不可 rollback**：永久刪表，資料喪失。確認 backup 後執行。

---

## 九、風險清單與 Rollback 策略

| PR | 主要風險 | Rollback 方法 |
|---|---|---|
| #R1 | ALTER ADD COLUMN 鎖表 | alembic downgrade（DROP COLUMN）|
| #R2 | v2.0 dual-write 路徑斷 | alembic downgrade（rename 反向）+ collector.toml revert |
| #R3 | trigger 沒重綁 → dirty queue 斷 | alembic downgrade（rename + trigger 反向）+ builder revert |
| #R4 | api_sync_progress 命名衝突 | UPDATE 反向 + collector.toml revert |
| #R5 | 無 |
| #R6 | **不可 rollback** | 跑 R6 前先 `pg_dump` backup |

---

## 十、後續退場（spec §7.3）

R6 完成後另起一輪退場 PR（編號 #R7~ 或別組），DROP v2.0 籌碼舊表：

| 退場候選 | 替代品 | 估時 |
|---|---|---|
| `institutional_daily` | `institutional_investors_tw` | 1h |
| `margin_daily` | `margin_purchase_short_sale_tw` | 1h |
| `foreign_holding` | `foreign_investor_share_tw` | 1h |
| `day_trading` | `day_trading_tw` | 1h |
| `valuation_daily` | `valuation_per_tw` | 1h |
| `index_weight_daily` | （無下游使用，可直接 DROP） | 1h |

每個 PR 對應 1 表 DROP + collector.toml v2.0 entry remove + 對應驗證。也可合併為 1 個大 PR。**不在本份 plan 範圍**。

---

## 附錄 A：當前 main 狀態（2026-05-09）

- alembic head：`q6r7s8t9u0v1`
- main HEAD：`6ee2d94 Merge pull request #23` (m2 spec rename)
- v3.2 r1 PR sequencing：#17 → #22 + #21 cleanup 全綠
- 4/5 衍生欄 ~99% fill；1/5（gov_bank_net）blocked on FinMind sponsor tier

## 附錄 B：開發分支命名建議

- PR #R1: `claude/m2-r1-source-cols`
- PR #R2: `claude/m2-r2-legacy-v2-rename`
- PR #R3: `claude/m2-r3-tw-promote`
- PR #R4: `claude/m2-r4-collector-naming`
- PR #R6: `claude/m2-r6-drop-legacy`
