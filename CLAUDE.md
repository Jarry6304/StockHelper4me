# CLAUDE.md — tw-stock-collector Session 銜接文件

> 這份文件記錄本專案的完整實作歷程與架構決策，供下次 session 自動載入後直接銜接，無需重新閱讀 git log。
> 最後更新：2026-04-27（v1.6）

---

## 分支狀態

- **開發分支**：`claude/continue-work-dvkRv`（已推到 origin；前身 `claude/setup-agent-review-mcp-berOR` 的 v1.5 commits 已併入）
- **目標分支**：`collector`（v1.4 PR #2 已合併，本 branch 累積 v1.5 + v1.6）
- **base**：`37ca6d0`（v1.4 結尾）→ 本 branch 累積 14 個 commit

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

---

## 已知問題清單（下次 session todo）

按優先序排列，每項都標明影響範圍與建議修法：

> v1.6 已處理：~~detail warning 群~~、~~dividend_policy 雙 source warning~~（仍待 user 在本機 drop 受影響表 + 重跑 phase 才能驗證）

### 🔴 待 user 驗證：v1.6 schema 變更後的重跑

修改了 9 張表 schema（8 加 detail + 1 加 source）。由於 `CREATE TABLE IF NOT EXISTS` 不更新已存在表，user 需先 drop 受影響表再重跑：

```powershell
# 在 project 根目錄
python scripts\drop_table.py price_daily
python scripts\drop_table.py price_limit
python scripts\drop_table.py margin_daily
python scripts\drop_table.py foreign_holding
python scripts\drop_table.py day_trading
python scripts\drop_table.py index_weight_daily
python scripts\drop_table.py monthly_revenue
python scripts\drop_table.py market_index_us
python scripts\drop_table.py _dividend_policy_staging

# 重跑相關 phases
python src\main.py backfill --stocks 2330,2317 --phases 2  # _dividend_policy_staging
python src\main.py backfill --stocks 2330,2317 --phases 3  # price_daily, price_limit
python src\main.py backfill --stocks 2330 --phases 4       # 後復權需重跑（依賴 price_daily）
python src\main.py backfill --stocks 2330 --phases 5       # margin/foreign_holding/day_trading/index_weight/monthly_revenue
python src\main.py backfill --stocks 2330 --phases 6       # market_index_us
```

驗證點：
- 跑完不應再看到 `略過不存在的欄位 {'detail'}` 或 `略過不存在的欄位 {'source'}`
- `inspect_db.py 2330` 各表 detail 欄位應該有 JSON 內容（用 SQL `SELECT detail FROM price_daily WHERE stock_id='2330' LIMIT 1` 確認）

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

CLAUDE.md v1.4 第 6 點提到「要不要切支線開始建 agent-review-mcp（spec 在最早的訊息）」這件事還沒開始。原本想用的 branch 名稱 `claude/setup-agent-review-mcp-berOR` 已被 v1.5 的 collector 工作佔用，現在 v1.6 又延續到 `claude/continue-work-dvkRv`。下次 session 處理。

### 🟢 待做：PR 合併

本 branch 14 commit 累積（v1.5 + v1.6），等 user 在本機重跑驗證 v1.6 schema 變更無誤後即可 merge 到 collector branch。

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

## 資料庫 Schema（25 張表，v1.5 變更標 ⚠️、v1.6 變更標 🆕）

| 資料表 | PK | 備註 |
|--------|----|----|
| `stock_info` | market, stock_id | |
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
| `api_sync_progress` | api_name, stock_id, segment_start | |
| `stock_sync_status` | market, stock_id | ⚠️ `update_stock_sync_status` 沒人呼叫，這表永遠 0 筆 |

---

## 環境細節（沿用 v1.4）

- Python 3.11+（需 tomllib）
- aiohttp 已裝
- DB schema 變更不能用 ALTER（CREATE IF NOT EXISTS 不更新已存在表），可用 `scripts/drop_table.py` 單表 drop
- PowerShell 對 `python -c "..."` 的 nested quotes 處理很差，所有查詢用 `scripts/inspect_db.py`，**不要寫 inline SQL**
- User token 環境變數 `$env:FINMIND_TOKEN`，禁止寫進 collector.toml
- Sandbox 環境連不到 finmindtrade.com，所有 API 實測都得 user 本機跑

---

## 下次 session 建議優先序

1. **user 在本機驗證 v1.6 schema 變更**（drop 9 表 → 重跑 phase 2/3/4/5/6 → 確認無 detail/source warning）
2. **PR 合併準備**（user 驗證通過後即可）
3. **agent-review-mcp 支線開始**（spec 在最早的訊息）
4. **exchange_rate 19 筆限制研究**（API 層面研究）
5. **institutional_daily 多 2 筆查證**（小議題）
