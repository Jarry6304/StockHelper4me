# CLAUDE.md — tw-stock-collector Session 銜接文件

> 這份文件記錄本專案的完整實作歷程與架構決策，供下次 session 自動載入後直接銜接，無需重新閱讀 git log。
> 最後更新：2026-04-15

---

## 分支狀態

- **開發分支**：`claude/review-collector-spec-Gktcf`
- **目標分支**：`collector`
- **PR**：PR #2（非 draft，待 review）
- **Base**：`302c9d7 Initial commit` → `1d255a8 搜集器初版規格添加` → 以下 7 個 commit

| SHA | 訊息 |
|-----|------|
| `b031d1d` | 實作 tw-stock-collector 完整骨架（Phase A-D） |
| `80ae049` | 實作 Phase E：籌碼 / 基本面 / 總經聚合層 |
| `13514d5` | 修正 4 個實作缺漏（audit 結果） |
| `57253cc` | 更新 README：補全 Phase E 完成狀態與 Schema 一覽表 |
| `667e672` | 修正 main.py argparse 結構：執行選項移至子命令層 |
| `f1a3db5` | 修正 collector.toml：所有多行 inline table 改為單行格式 |
| `c622a59` | 修正 stock_info 欄位映射 + db.upsert() 加欄位過濾防炸 |

---

## 目前可正常執行

```bash
python src/main.py validate
# ✓ collector.toml 格式正確
# ✓ stock_list.toml 格式正確

python src/main.py backfill --stocks 2330,2317,2442 --phases 1
# Phase 1 stock_info 已可正確寫入 market_type / industry

python src/main.py --verbose backfill --stocks 2330 --dry-run
```

---

## 各 Commit 核心變更

### `b031d1d` — Phase A-D 骨架（所有模組初版）

新增所有 Python 模組 + Rust binary + 設定檔：

| 模組 | 功能 |
|------|------|
| `src/logger_setup.py` | 日誌初始化，每日輪替，寫入 `logs/` |
| `src/config_loader.py` | TOML 解析 + 7 條驗證規則，DataClass 強型別 |
| `src/db.py` | SQLite WAL + 17 張 DDL + upsert/query/update |
| `src/rate_limiter.py` | Token Bucket，支援 burst + min_interval + 429 冷卻 |
| `src/api_client.py` | FinMindClient async context manager + 指數退避 |
| `src/phase_executor.py` | Phase 1-6 排程引擎（per stock × per segment） |
| `src/field_mapper.py` | 欄位映射 + schema 驗證 + computed_fields |
| `src/stock_resolver.py` | db/file/both 三種清單模式 |
| `src/date_segmenter.py` | backfill 年度分段 + incremental 從上次同步日起算 |
| `src/sync_tracker.py` | `api_sync_progress` 5 種狀態（pending/completed/empty/failed/schema_mismatch） |
| `src/post_process.py` | `dividend_policy_merge`：權息/減資/分割拆分 + 現增偵測 |
| `src/rust_bridge.py` | SIGTERM/SIGKILL subprocess + schema_version 驗證 |
| `src/main.py` | CLI：backfill/incremental/phase/status/validate |
| `rust_compute/src/main.rs` | 後復權 + 週K/月K + capital_increase AF |
| `config/collector.toml` | 全 28 API entry（Phase 1-3, 5-6） |
| `config/stock_list.toml` | db/file/both + dev mode |

---

### `80ae049` — Phase E（籌碼 / 基本面 / 總經聚合）

新增 `src/aggregators.py`，4 種聚合策略：

```python
# 三大法人：3 筆/日 → pivot 1 筆
aggregate_institutional(rows) -> list[dict]
aggregate_institutional_market(rows) -> list[dict]

# 財報：N 科目/日 → detail JSON
aggregate_financial(rows, stmt_type) -> list[dict]

# 股權分散：N 級距/日 → detail JSON
aggregate_holding_shares(rows) -> list[dict]

# 分派入口
apply_aggregation(strategy, rows, stmt_type=None)
```

- `src/config_loader.py`：`ApiConfig` 新增 `aggregation: str | None`、`stmt_type: str | None`
- `src/phase_executor.py`：`_run_api()` 在 `field_mapper.transform()` 後接入 `apply_aggregation`
- `config/collector.toml` Phase 5/6：補全 `field_rename` + `aggregation` + `stmt_type`
- `src/db.py`：`exchange_rate` PK 加 `currency`；`fear_greed_index` 加 `detail` 欄位

---

### `13514d5` — Audit 修正（4 個缺漏）

| # | 問題 | 修正位置 |
|---|------|---------|
| 1 | `DateSegmenter(config)` → incremental 永遠從頭拉 | `phase_executor.py:70`：改為 `DateSegmenter(config, sync_tracker)` |
| 2 | `cooldown_on_429_sec` 恆為 120（讀錯設定源） | `rate_limiter.py` 加 param；`api_client.py` 讀 `rate_limiter.cooldown_on_429_sec`；`main.py` 傳入 config 值 |
| 3 | `updated_at` 存入字串 `"datetime('now')"` 而非真實時間 | `sync_tracker.py`：改為 `datetime.now().isoformat(timespec="seconds")` |
| 4 | `schema_mismatch` 只記 log，未寫入 sync_tracker | `field_mapper.transform()` 改回傳 `(rows, bool)`；`phase_executor` 解包後呼叫 `mark_schema_mismatch()` |

---

### `57253cc` — README 更新

- 開發進度表：Phase E 標記完成，加缺漏修正列
- 新增資料庫 Schema 章節（25 張表）
- 專案結構加 `aggregators.py` / `post_process.py`

---

### `667e672` — argparse 修正

**問題**：`--stocks`、`--dry-run` 是全域參數，必須放在子命令前；
`python src/main.py backfill --stocks 2330` 報 `unrecognized arguments` 錯誤。

**修正方式**（`src/main.py`）：
```python
# 建立共用 parent parser（add_help=False 避免衝突）
_exec_parent = argparse.ArgumentParser(add_help=False)
_exec_parent.add_argument("--stocks", ...)
_exec_parent.add_argument("--dry-run", action="store_true", ...)

# 透過 parents= 注入各子命令
subparsers.add_parser("backfill",     parents=[_exec_parent], ...)
subparsers.add_parser("incremental",  parents=[_exec_parent], ...)
subparsers.add_parser("phase",        parents=[_exec_parent], ...)
```

真正全域選項（影響 config 載入）：`--config`、`--stock-list`、`--verbose`（仍在主 parser）。

---

### `f1a3db5` — TOML 格式修正

**問題**：Python `tomllib`（TOML v1.0）不允許 inline table `{ }` 跨行；
13 處多行 `field_rename = {\n  ...\n}` 導致 `Invalid statement` 錯誤。

```toml
# 錯誤（TOML v1.0 不允許）：
field_rename = {
    "type" = "market_type",
    "industry_category" = "industry"
}

# 正確（單行）：
field_rename = {"type" = "market_type", "industry_category" = "industry"}
```

修正：`config/collector.toml` 所有 13 處多行 field_rename 壓縮為單行。

---

### `c622a59` — stock_info 欄位映射 + db 防禦修正

**問題**：`TaiwanStockInfo` API 回傳 `type`/`industry_category`，但 DB 欄位為 `market_type`/`industry`。
沒有 field_rename 導致 `sqlite3.OperationalError: table stock_info has no column named industry_category`。

**修正 1**（`config/collector.toml`）：
```toml
# stock_info section
field_rename = {"type" = "market_type", "industry_category" = "industry"}
```

**修正 2**（`src/db.py`）— PRAGMA 欄位過濾防禦機制：
```python
def upsert(self, table, rows, primary_keys=None) -> int:
    valid_cols = self._table_columns(table)          # PRAGMA table_info 查詢（有快取）
    columns = [c for c in rows[0].keys() if c in valid_cols]
    if not columns:
        logger.warning(f"upsert → {table}: 所有欄位都不在 schema 中，略過")
        return 0
    dropped = set(rows[0].keys()) - set(columns)
    if dropped:
        logger.warning(f"upsert → {table}: 略過不存在的欄位 {dropped}")
    # ... 後續 INSERT OR REPLACE ...

def _table_columns(self, table: str) -> set[str]:
    if not hasattr(self, "_col_cache"):
        self._col_cache: dict[str, set[str]] = {}
    if table not in self._col_cache:
        rows = self.query(f"PRAGMA table_info({table})")
        self._col_cache[table] = {row["name"] for row in rows}
    return self._col_cache[table]
```

---

## 關鍵架構決策（不要改）

| 決策 | 原因 |
|------|------|
| `field_mapper.transform()` 回傳 `(rows, schema_mismatch: bool)` tuple | phase_executor 需要知道是否要呼叫 mark_schema_mismatch |
| `db.upsert()` 有 PRAGMA 欄位過濾 | 防禦性設計：API 新增欄位不會炸掉整個 sync |
| TOML inline table 必須單行 | `tomllib` TOML v1.0 限制，array `[]` 可跨行但 table `{}` 不行 |
| `--stocks`、`--dry-run` 是子命令選項 | 放在子命令後才符合使用者直覺 |
| `--config`、`--verbose` 是全域選項 | 影響 config 載入與日誌初始化，必須在子命令前解析 |
| `cooldown_on_429_sec` 存在 `RateLimiter` 實例上 | `RetryConfig` 沒有這個欄位；`api_client` 從 `rate_limiter.cooldown_on_429_sec` 讀取 |
| `DateSegmenter(config, sync_tracker)` | incremental 模式需要 sync_tracker 查詢上次同步日期 |
| `updated_at = datetime.now().isoformat()` | SQLite function 字串透過 parameterized query 不會被執行 |

---

## 資料庫 Schema（25 張表）

| 資料表 | PK |
|--------|----|
| `stock_info` | market, stock_id |
| `trading_calendar` | market, date |
| `market_index_tw` | market, date |
| `price_adjustment_events` | market, stock_id, date, event_type |
| `price_daily` | market, stock_id, date |
| `price_limit` | market, stock_id, date |
| `price_daily_fwd` | market, stock_id, date |
| `price_weekly_fwd` | market, stock_id, year, week |
| `price_monthly_fwd` | market, stock_id, year, month |
| `institutional_daily` | market, stock_id, date |
| `margin_daily` | market, stock_id, date |
| `foreign_holding` | market, stock_id, date |
| `holding_shares_per` | market, stock_id, date |
| `valuation_daily` | market, stock_id, date |
| `day_trading` | market, stock_id, date |
| `index_weight_daily` | market, stock_id, date |
| `monthly_revenue` | market, stock_id, date |
| `financial_statement` | market, stock_id, date, type |
| `market_index_us` | market, stock_id, date |
| `exchange_rate` | market, date, currency |
| `institutional_market_daily` | market, date |
| `market_margin_maintenance` | market, date |
| `fear_greed_index` | market, date |
| `api_sync_progress` | api_name, stock_id, segment_start |
| `stock_sync_status` | market, stock_id |

---

## 下次 Session 建議工作項目

1. **Phase 2 欄位映射驗證**：執行 `python src/main.py backfill --stocks 2330 --phases 2` 確認 `price_adjustment_events` 寫入無誤
2. **Phase 3 測試**：`--phases 3` 確認日K + 漲跌停資料正確
3. **Phase 4 Rust binary**：需先在本機 `cd rust_compute && cargo build --release`，binary 輸出在 `rust_compute/target/release/tw_stock_compute`，對應 `config/collector.toml` 的 `rust_binary_path`
4. **stock_list `source_mode = "db"`**：首次 Phase 1 執行後 `stock_info` 有資料，可切換為 db 模式
5. **合併 PR**：測試穩定後 merge `claude/review-collector-spec-Gktcf` → `collector`
6. **Phase 5-6 欄位映射驗證**：三大法人 pivot、財報 pack、股權分散 pack 的實際 API 回傳欄位名稱可能需要微調

---

## 常見錯誤排查

| 錯誤訊息 | 原因 | 解法 |
|----------|------|------|
| `Invalid statement (at line N)` | collector.toml 有多行 inline table `{}` | 壓縮為單行 |
| `table X has no column named Y` | field_rename 未設定 API → DB 欄位映射 | 在 collector.toml 對應 API section 加 `field_rename = {"api_col" = "db_col"}` |
| `unrecognized arguments: --stocks` | `--stocks` 放在子命令前 | 改為放在子命令後：`backfill --stocks 2330` |
| `cooldown_on_429_sec` 不生效 | 舊版讀錯 config 源 | 確認 `api_client.py` 讀的是 `rate_limiter.cooldown_on_429_sec` |
| incremental 從頭拉 | `DateSegmenter` 沒傳 `sync_tracker` | `DateSegmenter(config, sync_tracker)` |
