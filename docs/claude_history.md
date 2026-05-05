# CLAUDE.md 歷史銜接記錄（v1.5 ~ v1.7）

> 從 CLAUDE.md v1.8 reorg 抽出 — 主檔只保留 v1.8 + 不變的關鍵架構決策 + 待辦,
> 歷史 commits 表跟逐輪修正詳解搬到本檔。
> 
> **時間範圍**:2026-04-29 ~ 2026-04-30(v1.5/v1.6/v1.7 三輪)
> **對應主檔版本**:CLAUDE.md v1.8 之後

---

## 過去版本沿革（v1.5 ~ v1.7）

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

# v1.8 / v1.9 / v1.9.1 大項總覽（2026-05-01 ~ 2026-05-02）

> 從 CLAUDE.md v1.18 reorg 抽出 — 主檔只保留 v1.10+(PR #18 起的 m2 collector 重構落地)。
> 時間範圍:2026-05-01 ~ 2026-05-02(v1.8 m2 blueprint + Cores 審查 + av3;
> v1.9 PR #17 主體 + dividend AF 修補 + PowerShell 中文亂碼戰役;
> v1.9.1 24 檔 split/par_value backfill + tblnC 分支整合)。
> 對應主檔版本:CLAUDE.md v1.18 之後。

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
- 🔴 stock_dividend / split / par_value:Rust 算錯方向(P0-11 production bug)
- 🔴 staleness production 證據:3363 / 1312 stock_dividend 事件 fwd 沒處理(P0-7)
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
- 對現金 dividend:fwd_volume = raw_volume(不再 / AF)→ dollar_vol 不再守恆,但反映實際 share 流動性
- 對 split / par_value:fwd_volume = raw_volume / vf(post-event equivalent shares)
- 對 stock_dividend:vf = 1 / (1 + stock_dividend / 10) 由 post_process 修正(commit 608d275)

User 已 cargo build + 全市場 1348 檔 Phase 4 重跑驗證(av3 重跑 Test 1 vol_ratio 從 0.924 → 1.0 全部對齊預測)。

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
