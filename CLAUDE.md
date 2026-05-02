# CLAUDE.md — tw-stock-collector Session 銜接文件

> 這份文件記錄本專案的完整實作歷程與架構決策，供下次 session 自動載入後直接銜接，無需重新閱讀 git log。
> 最後更新：2026-05-02（**v1.9.1**）

---

## 分支狀態

- **開發分支**：`claude/initial-setup-RhLKU`(**v1.9.1 active**;tblnC 已 merge 進來,加 24 檔 split/par_value backfill 驗 av3 Test 4 完整覆蓋 + discover SQL + fix_p1_17 deprecate)
- **過去分支**:
  - `claude/review-todo-list-tblnC`(v1.9 main session,PR #17 (B-3) + R-1 漏改 + P1 dividend AF + PowerShell 亂碼戰役;已 merge 進 initial-setup-RhLKU)
  - `claude/restructure-collector-architecture-t9ScN`(v1.8 收尾,m2 重構藍圖 + r3 spec audit + av3)
  - `claude/m2-architecture-design-3Q3Fd`(user 主分支,Easy 階段 PR #10~#16 已合 + B-6 LEADING hotfix)
- **目標分支**:`m1/postgres-migration`(v1.7 PR 已合)
- **PR**:v1.9 + v1.9.1 PR 開於 initial-setup-RhLKU 分支

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

> **🎯 user 已指定下個 session 直接動工 #1**:PR #18 Bronze 6 raw 拆,~2 天 work。
> v1.9 PR (#16) 已開,maintainer review 平行進行不阻塞。

1. **🎯 PR #18 動工**(blueprint §六 #11:Bronze 6 raw 拆 + Option B reverse-pivot prototype),~2 天 work
   - 對應 blueprint §8.1 兩條路線:
     - **Option B(優先)**:institutional / margin / foreign_holding / day_trading / valuation 5 張用 reverse-pivot 從 v2.0 legacy 表反推 raw byte
     - **Option A**:holding_shares_per / financial_statement / monthly_revenue 3 張重抓 FinMind raw(pack JSONB unpack 困難)
   - 動工前先 prototype 1 張 reverse-pivot(建議 `institutional_daily`,因為 pivot 邏輯最透明,在 `src/aggregators.py:pivot_institutional`)驗證可行,再展全 5 張
   - 寫 `scripts/reverse_pivot_institutional.py`:對 stock 2330 SELECT pivot 後資料 → 反推 5 row × N 日 → INSERT 到 v3.2 Bronze `institutional_investors_tw` → 對得上原 pivot 即驗證通過
2. **v1.9 + v1.9.1 PR review + merge**(initial-setup-RhLKU 分支累積 ~24 commit,涵蓋 PR #17 (B-3) + R-1 漏改 + P1 dividend AF + PowerShell 亂碼戰役 + m2 blueprint Hard 階段 amend + 24 檔 backfill verify)。等 maintainer review,平行不阻塞 PR #18 動工。
3. ~~**stock list 補完**~~ ✅ v1.9.1 已處理(24 檔 split/par_value backfill + av3 Test 4 完整通過,詳見 §「v1.9.1 補丁」§A)
4. **agent-review-mcp 支線開始**(spec 在最早的訊息,從 v1.6 起就懸而未決)
5. **Phase 4 真正的 incremental 優化**(現在 staleness 補丁是「全部 reset 0」,長期該做 dirty-detection 只跑變動股票)
6. **CLAUDE.md 章節重組**(本檔已超過 600 行,v1.4-v1.7 詳解可繼續搬 docs/claude_history.md)
7. **inspect_db.py 升 PG 版**(v2.0 後該腳本已不可用)
8. **m2 PR #19 / #20 / #21**(Silver 14 + dirty + orchestrator + M3 prep,blueprint §十 PR 切法)
