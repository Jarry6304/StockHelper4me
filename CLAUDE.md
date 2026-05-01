# CLAUDE.md — tw-stock-collector Session 銜接文件

> 這份文件記錄本專案的完整實作歷程與架構決策，供下次 session 自動載入後直接銜接，無需重新閱讀 git log。
> 最後更新：2026-05-02（**v1.8**）

---

## 分支狀態

- **開發分支**：`claude/restructure-collector-architecture-t9ScN`（**v1.8 active**；m2 重構 + r3 spec audit + av3 實機驗證 + 完整修復）
- **過去分支**：`claude/review-collector-dependencies-n03rE`（v1.7 收尾）
- **目標分支**：`m1/postgres-migration`（v1.7 PR 已合）
- **PR**：v1.8 PR 開於 t9ScN 分支,涵蓋以下大項

---

## v1.8 大項總覽（2026-05-01 ~ 2026-05-02）

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

## 過去版本沿革（v1.7 以下保留為歷史記錄）

### v1.7 分支狀態(已 close)

- **開發分支**：`claude/review-collector-dependencies-n03rE`（已合）
- **目標分支**：`m1/postgres-migration`（review #2 結尾的 PG 基礎；CLAUDE.md v1.6 提的 `collector` 分支已不存在於 remote，被 m1 branch 取代）
- **base**：`9890294`（review #2 結尾，fix(rust_bridge): schema_version hard-fail）→ 本 branch 累積 8 個 commit
- **PR**：v1.7 已開（review #3 + #4），已合

### v1.5 commits（已驗證 Phase 1~6 全通過）

| SHA | 訊息 |
|-----|------|
| `e464c1f` | dividend_policy 補 source 欄位 rename |
| `f9348d4` | inspect_db.py 加印 Phase 3 驗證資訊 |
| `e67bf96` | SchemaValidation：把 DB target_table 欄位納入 known_keys 豁免 |
| `8dfa9f8` | incremental 子命令補 --phases 參數 |
| `9b1a2cf` | Phase 4 兩個炸點修正：Windows .exe 路徑 + 缺少 stock_ids 傳遞 |
| `0afec6a` | inspect_db.py 加印 Phase 4 驗證資訊（fwd 三張表） |
| `e302050` | inspect_db.py 加後復權正確性驗證（raw vs fwd 對照） |
| `536962e` | **修正 Rust 後復權 bug：除息日當日 AF 重複計算** |
| `acc7b1f` | **擴充 institutional_daily schema：5 類法人各自獨立記錄** |
| `68269d5` | Phase 6 預先修正：institutional_market 同 5 類法人擴充 |
| `00c5dae` | 新增 scripts/drop_table.py：單表 schema 變更後快速 drop |
| `60b2937` | 靜默略過 institutional API 的 'total' 合計列 |
| `6ad5302` | 更新 CLAUDE.md 為 v1.5 銜接文件 |

### v1.6 commits（detail / source warning 清理）

| SHA | 訊息 |
|-----|------|
| `(本次)` | 8 張表加 `detail TEXT` 欄位 + `_dividend_policy_staging` 加 `source TEXT`；移除 dividend_policy 的 source rename |

### v1.7 review #3 commits（M1 第 3 輪 review，user 本機 PG 17 驗證全綠）

| SHA | 訊息 | 性質 |
|-----|------|------|
| `e056a25` | fix(schema): 擴充 api_sync_progress CHECK 涵蓋 empty / schema_mismatch | 🔴 critical |
| `ffc5778` | fix(phase_executor): Phase 4 mode 從 CLI runtime 傳，不從 config 讀 | 🟡 medium |
| `5ffa94f` | fix(db): SqliteWriter 寫入 / init_schema 停用，明確 raise | 🟡 medium |

### v1.7 review #4 commits（LOW 級項目 + #7 衍生 fix）

| SHA | 訊息 | 性質 |
|-----|------|------|
| `4b42506` | docs(rust): 註解 Phase 4 全量重算的設計意圖 | 文檔 |
| `9f22008` | fix(post_process): 4 處 SELECT 補 market filter，對齊 schema PK | 一致性 |
| `35f41e0` | refactor(db): 把 PK 查詢從 phase_executor 硬編碼移到 DBWriter._table_pks | DRY |
| `1557cbf` | fix(collector): stock_info API date 改 pack 進 detail，updated_at 交給 NOW() | 語意修正 |
| `fd727a2` | fix(db): upsert UPDATE 路徑統一刷新 updated_at = NOW() | #7 補丁 |

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

## 本 session 重要修正詳解

### 1. Rust 後復權 bug（commit `536962e`）— 重要！

**問題**：除息日當日 fwd_close 多乘了一次 AF。

**原版邏輯（錯）**：
```rust
for price in raw_prices.iter().rev() {
    if let Some(&af) = event_af.get(&price.date) {
        multiplier *= af;            // ← 先更新
    }
    result.push(... close: price.close * multiplier ...);  // ← 當日已含當日 AF
}
```

**正確邏輯**：
```rust
for price in raw_prices.iter().rev() {
    result.push(... close: price.close * multiplier ...);  // ← 先用當前 multiplier
    if let Some(&af) = event_af.get(&price.date) {
        multiplier *= af;            // ← 再更新給更早的日子
    }
}
```

理由：除息日當日 raw 已是「除息後價格」，不應再乘該日 AF（會重複）。

### 2. institutional schema 5 類法人（commit `acc7b1f` + `68269d5`）

FinMind 實際回傳 5 類法人：
- `Foreign_Investor`（外資不含自營商）
- `Foreign_Dealer_Self`（外資自營商）
- `Investment_Trust`（投信）
- `Dealer_self`（自營商自行買賣）
- `Dealer_Hedging`（自營商避險）

DB 表 `institutional_daily` 跟 `institutional_market_daily` 從 6 欄擴充為 10 欄，分別記錄。

### 3. Phase 4 鏈路修正（commit `9b1a2cf`）

兩個炸點：
- Windows 上 `asyncio.create_subprocess_exec` 不會自動補 `.exe` → `rust_bridge.__init__` 偵測 `sys.platform=="win32"` 自動補
- `phase_executor._run_phase4` 沒傳 `stock_ids` 給 Rust → Rust 從 `stock_sync_status` 取，但這表沒人寫入 → 永遠 0 筆。改為從 `self._stock_list` 直接傳。

### 4. SchemaValidation 用 DB schema 豁免（commit `e67bf96`）

`field_mapper._validate_schema` 的 `known_keys` 只包含 `field_rename` 來源欄位 + 通用豁免。但 API 也會回「與 DB 同名直接入庫」的核心欄位（如 `open/close/limit_up`）→ 被誤判 novel。修法：把 `db._table_columns(target_table)` 加進 known_keys。

### 5. INSTITUTIONAL_NAME_IGNORED 名單（commit `60b2937`）

FinMind `TaiwanStockTotalInstitutionalInvestors` 多回 `total` 合計列。可從 5 類自行加總，不需要。加進「靜默略過」名單，避免 noisy warning。

### 6. detail TEXT 欄位群補齊（v1.6 本次）

8 張表 schema 在 v1.5 之前都缺 `detail TEXT` 欄位，導致 toml 定義的 `_xxx` rename 經 field_mapper pack 成 detail JSON 後被 PRAGMA 過濾掉，每筆都印 warning + 次要欄位資料完全丟失。本次補齊：

| 表 | 補進 detail 的欄位（來自 toml `detail_fields`） |
|----|---------------------------------------------|
| `price_daily` | `_trading_money`, `_spread` |
| `price_limit` | `_reference_price` |
| `margin_daily` | 8 個（_margin_cash_repay, _margin_prev_balance, _margin_limit, _short_cash_repay, _short_prev_balance, _short_limit, _offset_loan_short, _note）|
| `foreign_holding` | 9 個（_remaining_shares, _remain_ratio, _upper_limit_ratio, _cn_upper_limit, _total_issued, _declare_date, _intl_code, _stock_name, _note）|
| `day_trading` | `_day_trading_flag`, `_volume` |
| `index_weight_daily` | `_rank`, `_stock_name`, `_index_type` |
| `monthly_revenue` | `_country`, `_create_time` |
| `market_index_us` | `_adj_close` |

修法只動 `db.py` schema DDL，PRAGMA 結果有 cache（DBWriter._col_cache）→ 必須**整個 process 重啟**才生效；舊資料沒 detail JSON，所以**這 8 張表需要 drop 後重跑**才會有 detail 欄位實際值（光改 schema 不會回填歷史）。

### 7. dividend_policy source warning 終結（v1.6 本次）

兩條老 warning：
```
[WARNING] field_mapper: expected fields missing from API response: {'source'}
[WARNING] db: upsert → _dividend_policy_staging: 略過不存在的欄位 {'source'}
```

修法兩段：
- `collector.toml` 拿掉 `"source" = "_source"` rename → schema validation 不再期待 API 回 `source` → warning 1 消失
- `_dividend_policy_staging` schema 加 `source TEXT DEFAULT 'finmind'` → field_mapper 步驟 5 自動填 `row["source"] = "finmind"` 不再被 PRAGMA 過濾 → warning 2 消失

### 8. drop_table.py 連帶清 api_sync_progress（v1.6 本次）

**踩坑**：v1.6 第一次重跑時，drop 完目標表 + 重跑 backfill，所有 segment 都顯示 `Skipped (completed)`，0 秒結束 → 表是空的但進度說已完成。原因：`api_sync_progress` 表用 `api_name` 為 key 記錄已完成的 segment，drop 目標表不會清掉這些進度記錄。

**修法**：`scripts/drop_table.py` 改成讀 `collector.toml` 反查 `target_table → api_name` 映射，drop 目標表的同時 `DELETE FROM api_sync_progress WHERE api_name IN (對應的 api 們)`。並改成「表不存在也照樣清 progress」，方便已踩坑的使用者直接重跑同一個 drop 指令救回。

---

## v1.7 / review #3 + #4 重要修正詳解

### review #1（e056a25）— api_sync_progress CHECK 擴 5 種 status — 🔴 critical

**問題**：baseline 的 `chk_progress_status` 只允許 `('pending', 'completed', 'failed')`，但 `sync_tracker.py` + `phase_executor.py` 實際會寫 `empty`（API 回空陣列，例如 dividend_result 對沒發股利的股票）跟 `schema_mismatch`（FieldMapper 偵測到欄位漂移）。沒擴 CHECK 的話空段 upsert 直接被 PG 拒絕、segment 永遠標不起來、斷點續傳卡死。

**修法**：因 baseline 已上線，**不能直接改 baseline**，必須走 alembic incremental migration。新加 `2026_04_30_a1b2c3d4e5f6_progress_status_check_expand.py`：DROP 舊 CHECK + ADD 5 種 status 的新 CHECK。同步把 `src/schema_pg.sql` baseline 也改成 5 種，讓 fresh-install fallback 路徑也對齊。

### review #2（ffc5778）— Phase 4 mode 從 CLI runtime 傳

**問題**：`main.py` 用 CLI command 算出 runtime mode、傳給 `executor.run(mode)`、再傳到 `_run_phase`，但 `_run_phase4` 跳開不收參數、改從 `self.config.execution.mode` 讀。`python main.py incremental` 時 Rust 會收到 toml 寫死的 `"backfill"`。

**現況不會出錯的原因**：Rust `process_stock(_mode: &str)` 變數名 `_` 前綴 → 沒消費 mode。但這個對齊裂縫遲早會踩。

**修法**：`_run_phase4(mode)` 收 runtime 參數、由 `run()` 傳入；對應 docstring 也補一段。

### review #3（5ffa94f）— SqliteWriter 寫入 / init_schema 停用

**問題**：v2.0 (PG) 之後 SqliteWriter 已實質不可用：
- `init_schema()` 依賴的 `db_legacy_sqlite_ddl` 模組根本不在 repo
- `update()` 用 `?` 佔位符，但 phase_executor / post_process / stock_resolver 已全面改 PG 的 `%s`，SQLite 模式跑這些 SQL 會語法錯
- schema 演進不再回填 SQLite 端

之前是「半殘但表面看起來能跑」狀態，使用者啟用 `TWSTOCK_USE_SQLITE=1` 會在實際呼叫 init_schema 時才炸 ImportError，誤導性高。

**修法**：upsert / insert / update / init_schema 全 raise NotImplementedError 帶清楚提示。query / query_one 保留供讀取舊 dump（CI 環境驗證舊資料仍可用）。v2.1 規劃完全砍除整個 class。

### review #6（9f22008）— post_process 4 處 SELECT 補 market filter

**問題**：`post_process.py` 4 個 SELECT 都只 filter `stock_id`，但 schema PK 是 `(market, stock_id, ...)`。同檔的 UPDATE / INSERT 已正確帶 `market='TW'`（L104 / L186），SELECT 不對齊就是表面安全的潛在漏洞，未來多市場（HK / US 等）這層會直接撈錯股票。

**修法**：4 處 SELECT 一致改用參數化 `market = %s`（params 帶 `'TW'`）：
- `_patch_mixed_dividend` mixed_events 查詢
- `_patch_mixed_dividend` policy 查詢
- `_detect_capital_increase` capital_increases 查詢
- `_detect_capital_increase` existing 查詢

### review #7（1557cbf + fd727a2）— stock_info.updated_at 兩段修法

**問題**：API `TaiwanStockInfo` 回的 `date` 欄位語意是「資料更新日」，但 `field_rename` 直接把它塞進 TIMESTAMPTZ 的 `updated_at`：
- ISO date string `"YYYY-MM-DD"` 進 TIMESTAMPTZ，PG 用 server timezone 補成 `YYYY-MM-DD 00:00:00+TZ`
- 跟 `_merge_delist_date` 的 `SET updated_at = NOW()` 語意衝突（一邊是 FinMind 標日、一邊是真的同步時刻）

**第一段修法（1557cbf）— collector.toml field_rename 改**：把 `"date" = "updated_at"` 改成 `"date" = "_data_update_date"`。`_` 前綴讓 `field_mapper.transform()` 自動把它丟進 detail JSON（key = `data_update_date`）。`updated_at` 沒 rename 進來，schema `DEFAULT NOW()` 接手 INSERT 路徑。stock_info 表早就有 `detail JSONB` 欄位（baseline schema_pg.sql:55 從 commit b77ac52 就有），不需要 schema migration。

**踩坑**：第一段修完，user 重跑 `python src\main.py backfill --phases 1` 後查 2330：
```
updated_at | 2026-04-29 00:00:00+08    ← 還是舊的
api_date   | 2026-04-30                 ← 對的
```

`api_date` 出現了 ✅，但 `updated_at` 沒變。原因是 PostgresWriter.upsert 的 `ON CONFLICT DO UPDATE SET ...` 用 EXCLUDED 覆蓋；但因為 row dict 不再包含 `updated_at`（已改進 detail）→ `update_cols` 沒有 `updated_at` → UPDATE 子句完全不碰它 → PG 保留舊值。**`DEFAULT NOW()` 只對 INSERT 生效，UPSERT 走 UPDATE 路徑沒效**。

**第二段修法（fd727a2）— upsert UPDATE 強制 updated_at = NOW()**：在 `db.py` 組 SQL 時偵測 `updated_at` 欄位，UPDATE 子句強制 `SET updated_at = NOW()`，兩種情境都對齊：
- row dict 有 updated_at（api_sync_progress：sync_tracker 傳 `datetime.now()`）→ UPDATE 用 NOW() 而不是 EXCLUDED.updated_at
- row dict 沒 updated_at 但 schema 有（stock_info）→ 補一條 `"updated_at" = NOW()` 到 update_clause

對沒有 updated_at 欄位的表（stock_sync_status、fear_greed_index 等）無影響（valid_cols 篩過）。

**驗證**：第二段修完 user 再清 progress + 重跑：
```
updated_at | 2026-04-30 12:18:11.134528+08    ← 現在這刻 ✅
api_date   | 2026-04-30                        ← FinMind 標日 ✅
```
兩個欄位語意分清。

### review #8（35f41e0）— DBWriter._table_pks 動態查 information_schema

**問題**：`phase_executor.py:237-247` 硬編碼 9 個表的 PK 對照、`sync_tracker.py:146` 也獨立硬編碼一份。schema migration 改 PK 時要記得改三處，違反 DRY；schema 應該是 single source of truth。

**修法**：`PostgresWriter` 加 `_table_pks(table) -> list[str]`，用 `pg_index + pg_attribute` 查 PRIMARY KEY 欄位（依 ordinal_position 排序），仿 `_table_column_types` cache pattern；`_invalidate_cache` 同步清 `_pk_cache`。`DBWriter` Protocol 加同名簽名、`SqliteWriter` 一致 raise NotImplementedError。phase_executor / sync_tracker 改用 `self.db._table_pks(table)`。

**副作用**：對沒 PK 的表（不存在）會直接 raise，比之前的「fallback 到 `['market', 'stock_id', 'date']`」靜默吃掉好。

### review #9（4b42506）— Phase 4 全量重算的設計意圖註解

**問題**：Rust `process_stock(_mode: &str)` 變數名 `_` 前綴暗示忽略，且每次 `DELETE FROM price_daily_fwd WHERE market AND stock_id` 整段重刪重建。但業務理由（後復權 multiplier 從尾端倒推 → 新除權息事件會回頭改全段 fwd 歷史值 → partial 邏輯上錯誤）沒寫在 code，下次有人改 Phase 4 容易誤判「為什麼不做 incremental」就動手錯誤優化。

**修法**：純註解 — `rust_compute/src/main.rs:383` 加 doc comment 說明設計決策與未來真正想做 incremental 必須在 Python 層偵測「該股票自從上次 Phase 4 以後有沒有新除權息事件」，沒有的話跳過呼叫；有的話仍須全量。`src/phase_executor.py` 的 `_run_phase4` docstring 補一段對齊 Rust 端說明。

---

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

### 🟡 待研究：CLAUDE.md 章節重組

每輪 review 都加段落，文件愈來愈長（v1.7 已 ~400 行）。下次可考慮把 v1.4 / v1.5 / v1.6 的 commits 表格與詳解搬到附錄或單獨 docs/ 目錄，主文只保留「最新 v1.X 銜接資訊 + 不變的關鍵架構決策」。

---

## helper 腳本清單

| 腳本 | 用途 | 範例 |
|------|------|------|
| `scripts/inspect_db.py` | 檢視 db 各表筆數 + 特定股票詳細內容 + Phase 6 全市場資料 + 後復權驗證 | `python scripts/inspect_db.py 2330` |
| `scripts/drop_table.py` | schema 變更後 drop 指定表（避免重灌全套） | `python scripts/drop_table.py institutional_market_daily` |
| `scripts/test_28_apis.py` | 28 支 API 連線健檢（urllib + tomllib，零依賴） | 需要 token |

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

1. **v1.8 PR review + merge**(t9ScN 分支累積 8+ commit,涵蓋 m2 重構藍圖 / r3 spec audit / av3 實機驗證 / 完整修復)
2. **跑 `scripts/fix_p1_17_stock_dividend_vf.sql`** + 全市場 Phase 4 重跑 + av3_spot_check 重跑驗證 P1-17 修正後 stock_dividend vol_ratio 確實 < 1.0
3. **Backfill 含 split / par_value 的股票**(如 6505 等)讓 av3 Test 4 有資料能驗證 split 事件 volume 行為
4. **m2 collector 重構動工**(blueprint v3.2 r1 第 1 個 PR:K-1 + schema_metadata bump 到 3.2 + alembic migration 入口,~0.5 天)
5. **agent-review-mcp 支線開始**(spec 在最早的訊息,從 v1.6 起就懸而未決)
6. **Phase 4 真正的 incremental 優化**(現在 staleness 補丁是「全部 reset 0」,長期該做 dirty-detection 只跑變動股票)
7. **CLAUDE.md 章節重組**(本檔已超過 500 行,v1.4-v1.6 詳解可搬 docs/ 子目錄)
8. **inspect_db.py 升 PG 版**(v2.0 後該腳本已不可用)
