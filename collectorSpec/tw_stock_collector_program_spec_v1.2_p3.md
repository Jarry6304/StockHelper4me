
## 九、Sync Tracker（斷點續傳）

### 9.1 追蹤粒度

在 `stock_sync_status` 之外，新增一張 **API 級別** 的進度追蹤表，用於 backfill 的精細斷點續傳：

```sql
CREATE TABLE IF NOT EXISTS api_sync_progress (
    api_name        TEXT    NOT NULL,    -- collector.toml 中的 api.name
    stock_id        TEXT    NOT NULL,    -- "__ALL__" for all_market APIs
    segment_start   TEXT    NOT NULL,    -- 日期段起始
    segment_end     TEXT    NOT NULL,    -- 日期段結束
    status          TEXT    NOT NULL DEFAULT 'pending',
        -- 'pending'          尚未開始
        -- 'completed'        成功入庫
        -- 'failed'           失敗（含錯誤訊息）
        -- 'empty'            API 回傳空資料（正常，某些股票無此類事件）
        -- 'schema_mismatch'  (v1.1) API 回傳欄位與預期不符，已入庫但需人工檢查
    record_count    INTEGER DEFAULT 0,   -- 本段入庫筆數
    error_message   TEXT,
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (api_name, stock_id, segment_start)
);
```

### 9.2 斷點續傳行為

```
backfill 模式啟動時：
  1. 載入 api_sync_progress
  2. 對每個 (api_name, stock_id, segment) 組合：
     - status = 'completed' 或 'empty' → 跳過
     - status = 'failed' → 重試（受 max_attempts 限制）
     - status = 'pending' → 正常執行
  3. 所有 segment 完成後，更新 stock_sync_status 的對應狀態
```

---

## 十、Post-Process：TaiwanStockDividend 合併邏輯

### 10.1 目的

`TaiwanStockDividend`（股利政策表）有兩個用途：

1. **拆分「權息」混合事件**：提供 cash_dividend 和 stock_dividend 的明細
2. **偵測純現金增資事件**：`TaiwanStockDividendResult` 可能無對應紀錄時的 fallback

### 10.2 執行時機

Phase 2 中，`dividend_result` 和 `dividend_policy` 都完成後，執行 `dividend_policy_merge` post-process。

### 10.3 邏輯

```python
def dividend_policy_merge(db, stock_id: str):
    """
    Step 1: 修補「權息」混合事件的 cash/stock 拆分
    """
    mixed_events = db.query("""
        SELECT * FROM price_adjustment_events
        WHERE stock_id = ? AND event_type = 'dividend'
          AND cash_dividend IS NULL AND stock_dividend IS NULL
    """, [stock_id])

    for event in mixed_events:
        policy = db.query("""
            SELECT * FROM _dividend_policy_staging
            WHERE stock_id = ?
              AND (CashExDividendTradingDate = ? OR StockExDividendTradingDate = ?)
        """, [stock_id, event["date"], event["date"]])

        if policy:
            cash = policy["CashEarningsDistribution"] + policy["CashStatutorySurplus"]
            stock_rate = (policy["StockEarningsDistribution"] + policy["StockStatutorySurplus"]) / 10
            db.update("""
                UPDATE price_adjustment_events
                SET cash_dividend = ?, stock_dividend = ?
                WHERE market = 'TW' AND stock_id = ? AND date = ? AND event_type = 'dividend'
            """, [cash, stock_rate, stock_id, event["date"]])

    """
    Step 2: 偵測純現增事件
    尋找 _dividend_policy_staging 中有 CashIncreaseSubscriptionRate > 0
    但 price_adjustment_events 中無對應日期的紀錄
    """
    capital_increases = db.query("""
        SELECT * FROM _dividend_policy_staging
        WHERE stock_id = ? AND CashIncreaseSubscriptionpRrice > 0
    """, [stock_id])

    for ci in capital_increases:
        ex_date = ci.get("StockExDividendTradingDate") or ci.get("CashExDividendTradingDate")
        if not ex_date:
            continue

        existing = db.query("""
            SELECT 1 FROM price_adjustment_events
            WHERE stock_id = ? AND date = ?
        """, [stock_id, ex_date])

        if not existing:
            # 純現增事件，TaiwanStockDividendResult 中無對應紀錄
            # ── v1.1 修訂 ──
            # ⚠️ AF 計算完全移交 Rust Phase 4 處理。
            # 原因：Phase 2 執行時 price_daily（Phase 3）可能尚未入庫，
            #       無法取得 ex_date 前一日收盤價計算 AF。
            # Python 只負責存入原始 subscription_price/rate，
            # Rust 在 Phase 4 step 1.5 動態從 price_daily 撈資料補算。
            logger.warning(
                f"Pure capital increase detected: {stock_id} on {ex_date}, "
                f"subscription_price={ci['CashIncreaseSubscriptionpRrice']}, "
                f"subscription_rate={ci['CashIncreaseSubscriptionRate']}. "
                f"AF deferred to Rust Phase 4 (step 1.5)."
            )
            db.insert("price_adjustment_events", {
                "market": "TW",
                "stock_id": stock_id,
                "date": ex_date,
                "event_type": "capital_increase",
                "before_price": None,       # 需從 price_daily 補查前一日收盤
                "after_price": None,        # 需計算
                "adjustment_factor": 1.0,   # 暫用 1.0，待驗證後修正
                "volume_factor": 1.0,       # 暫用 1.0
                "detail": json.dumps({
                    "subscription_price": ci["CashIncreaseSubscriptionpRrice"],
                    "subscription_rate_raw": ci["CashIncreaseSubscriptionRate"],
                    "total_new_shares": ci["TotalNumberOfCashCapitalIncrease"],
                    "total_participating_shares": ci["ParticipateDistributionOfTotalShares"],
                    "source": "TaiwanStockDividend",
                    "status": "pending_rust_phase4",
                }),
                "source": "finmind",
            })
```

---

## 十一、Logging（v1.2 新增）

### 11.1 設計原則

- 每次執行產生一個日誌檔，路徑為 `logs/collector_YYYYMMDD.log`
- 同一天多次執行則**追加**（append）至同一檔案，不覆蓋
- 日誌級別由 `collector.toml` 的 `global.log_level` 控制
- **同時輸出至 stdout**（便於即時觀察），不影響日誌檔
- 使用 Python 標準 `logging` 模組，無額外依賴

### 11.2 初始化實作

```python
# src/logger_setup.py

import logging
import logging.handlers
from pathlib import Path
from datetime import date

def setup_logger(log_dir: str, log_level: str) -> logging.Logger:
    """
    初始化 Collector 全域 Logger。
    呼叫一次，後續各模組直接 logging.getLogger(__name__) 取用。
    """
    log_path = Path(log_dir)
    log_path.mkdir(parents=True, exist_ok=True)

    log_file = log_path / f"collector_{date.today().strftime('%Y%m%d')}.log"

    level = getattr(logging, log_level.upper(), logging.INFO)

    formatter = logging.Formatter(
        fmt="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S"
    )

    root_logger = logging.getLogger("collector")
    root_logger.setLevel(level)

    # 清除舊 handler，避免重複初始化
    root_logger.handlers.clear()

    # Handler 1：寫入日誌檔（追加模式）
    file_handler = logging.FileHandler(log_file, encoding="utf-8", mode="a")
    file_handler.setFormatter(formatter)
    root_logger.addHandler(file_handler)

    # Handler 2：輸出至 stdout
    stream_handler = logging.StreamHandler()
    stream_handler.setFormatter(formatter)
    root_logger.addHandler(stream_handler)

    return root_logger
```

### 11.3 各模組取用方式

```python
# 各模組頂層宣告，不重複初始化
import logging
logger = logging.getLogger("collector.<module_name>")

# 範例：
# src/phase_executor.py  → logging.getLogger("collector.phase_executor")
# src/api_client.py      → logging.getLogger("collector.api_client")
# src/field_mapper.py    → logging.getLogger("collector.field_mapper")
# src/sync_tracker.py    → logging.getLogger("collector.sync_tracker")
# src/rust_bridge.py     → logging.getLogger("collector.rust_bridge")
```

### 11.4 必要日誌事件清單

以下為各模組**必須**記錄的事件，確保流程可追蹤與錯誤可定位：

#### main.py（啟動 / 結束）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| 程式啟動 | INFO | `Collector started. command=backfill, phases=[1,2,3,4]` |
| 程式正常結束 | INFO | `Collector finished. elapsed=3612s` |
| 程式異常結束 | ERROR | `Collector aborted. reason=<exception>` |
| Config 載入成功 | INFO | `Config loaded. apis=28, stocks_mode=db` |
| Config 驗證失敗 | ERROR | `Config validation failed: <reason>` |

#### phase_executor.py（Phase 流程）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| Phase 開始 | INFO | `[Phase 3] Started. apis=2` |
| Phase 完成 | INFO | `[Phase 3] Completed. elapsed=1800s` |
| API 任務開始 | INFO | `[Phase 3][price_daily] Start stock=2330, segment=2023-01-01~2023-12-31` |
| API 任務完成 | INFO | `[Phase 3][price_daily] Done stock=2330, segment=2023-01-01~2023-12-31, records=248` |
| API 任務跳過（已完成） | INFO | `[Phase 3][price_daily] Skipped stock=2330, segment=... (completed)` |
| Phase 1 後刷新股票清單 | INFO | `StockList refreshed from DB. total=1823` |

#### api_client.py（HTTP 通訊）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| HTTP 重試 | WARNING | `HTTP 429, retry 1/3 in 10s. dataset=TaiwanStockPrice, stock=2330` |
| 429 冷卻啟動 | WARNING | `Rate limit hit. Cooling down 120s.` |
| 達最大重試次數 | ERROR | `Max retries exceeded. dataset=TaiwanStockPrice, stock=2330, segment=...` |
| FinMind API 回傳錯誤 | ERROR | `FinMind API error: msg=<msg>, dataset=..., stock=...` |
| Session 關閉 | INFO | `FinMindClient session closed.` |

#### field_mapper.py（Schema 驗證）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| Schema 欄位缺漏 | WARNING | `[SchemaValidation] dividend_result: missing fields={...}` |
| Schema 新增欄位 | INFO | `[SchemaValidation] dividend_result: novel fields={...}` |
| AF 無法計算 | WARNING | `Cannot compute AF: before=None, after=None. stock=2330, date=2023-06-20` |

#### sync_tracker.py（斷點續傳）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| 進度更新 | INFO | `Progress: api=price_daily, stock=2330, segment=..., status=completed, records=248` |
| 標記失敗 | WARNING | `Progress: api=price_daily, stock=2330, segment=..., status=failed, error=<msg>` |

#### rust_bridge.py（Phase 4）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| Phase 4 啟動 | INFO | `[Phase 4] Rust binary started. mode=backfill, stocks=all` |
| Phase 4 完成 | INFO | `[Phase 4] Rust binary finished. processed=1800, skipped=12, af_patched=3, elapsed=45000ms` |
| Phase 4 有錯誤股票 | WARNING | `[Phase 4] Errors: [{stock_id: XXXX, reason: ...}]` |
| Phase 4 執行失敗 | ERROR | `[Phase 4] Rust binary failed. returncode=1, stderr=<msg>` |
| SIGTERM 送出 | WARNING | `Phase 4 cancelled, sending SIGTERM to Rust binary...` |
| SIGKILL 送出 | ERROR | `Rust binary did not exit in 10s, sending SIGKILL` |
| Schema version 不符 | WARNING | `Rust binary schema_version=1.0, expected=1.1. Consider rebuilding.` |

#### post_process（dividend_policy_merge）

| 事件 | 級別 | 格式範例 |
|------|------|----------|
| 純現增事件偵測 | WARNING | `Pure capital increase detected: stock=2442, date=2023-05-10, subscription_price=..., AF deferred to Rust Phase 4.` |

### 11.5 日誌格式範例

實際日誌檔內容範例：

```
2026-04-15 09:00:01 [INFO] collector.main: Collector started. command=backfill, phases=[1,2,3,4]
2026-04-15 09:00:01 [INFO] collector.main: Config loaded. apis=28, stocks_mode=db
2026-04-15 09:00:02 [INFO] collector.phase_executor: [Phase 1] Started. apis=4
2026-04-15 09:00:03 [INFO] collector.phase_executor: [Phase 1][stock_info] Start stock=__ALL__, segment=2019-01-01~2026-04-15
2026-04-15 09:00:05 [INFO] collector.phase_executor: [Phase 1][stock_info] Done stock=__ALL__, segment=..., records=1823
2026-04-15 09:00:06 [INFO] collector.phase_executor: StockList refreshed from DB. total=1823
2026-04-15 09:00:06 [INFO] collector.phase_executor: [Phase 1] Completed. elapsed=4s
2026-04-15 09:00:07 [INFO] collector.phase_executor: [Phase 3] Started. apis=2
2026-04-15 09:00:07 [INFO] collector.phase_executor: [Phase 3][price_daily] Start stock=2330, segment=2023-01-01~2023-12-31
2026-04-15 09:00:10 [WARNING] collector.api_client: HTTP 429, retry 1/3 in 10s. dataset=TaiwanStockPrice, stock=2330
2026-04-15 09:00:20 [INFO] collector.phase_executor: [Phase 3][price_daily] Done stock=2330, segment=..., records=248
2026-04-15 10:30:00 [INFO] collector.rust_bridge: [Phase 4] Rust binary started. mode=backfill, stocks=all
2026-04-15 10:30:45 [INFO] collector.rust_bridge: [Phase 4] Rust binary finished. processed=1800, skipped=12, af_patched=3, elapsed=45000ms
2026-04-15 10:30:45 [INFO] collector.main: Collector finished. elapsed=5444s
```

---

## 十二、CLI 介面

```
usage: python src/main.py <command> [options]

commands:
  backfill        全量歷史回補
  incremental     增量同步（日常排程用）
  phase <N>       只跑指定 Phase
  status          顯示同步進度摘要
  validate        驗證 config 格式

options:
  --config <path>       指定 collector.toml 路徑（預設 config/collector.toml）
  --stock-list <path>   指定 stock_list.toml 路徑（預設 config/stock_list.toml）
  --stocks <id1,id2>    覆蓋股票清單（開發用，忽略 stock_list.toml）
  --dry-run             只印出計劃，不實際呼叫 API
  --verbose             DEBUG 級別日誌

examples:
  # 首次全量回補
  python src/main.py backfill

  # 只跑 Phase 1-4（拿到可分析的資料即可）
  python src/main.py backfill --phases 1,2,3,4

  # 只跑特定股票測試
  python src/main.py backfill --stocks 2330,2317,2442

  # 日常增量
  python src/main.py incremental

  # 查看進度
  python src/main.py status

  # 驗證設定檔
  python src/main.py validate
```

---

## 十三、開發順序建議

```
Phase A（基礎骨架）：
  1. config_loader — TOML 解析 + 驗證
  2. db — SQLite 連線 + schema 初始化（用 v1.1 的 CREATE TABLE 語句）
  3. rate_limiter — Token Bucket
  4. api_client — HTTP GET + retry
  5. logger_setup — 日誌初始化（v1.2 新增，應於 Phase A 完成）

Phase B（跑通 Phase 1）：
  6. phase_executor — 最簡版，只跑 all_market APIs
  7. field_mapper — 通用 rename + upsert
  8. stock_resolver — 從 stock_info 撈清單
  → 里程碑：stock_info + trading_calendar 入庫

Phase C（跑通 Phase 2-3）：
  9. date_segmenter — 日期分段
  10. sync_tracker — api_sync_progress 斷點續傳
  11. computed_fields — adjustment_factor / volume_factor
  → 里程碑：price_daily + price_adjustment_events 入庫

Phase D（Phase 4 — Rust）：
  12. rust_compute binary — 後復權 + 週K/月K
  13. rust_bridge — Python 呼叫 Rust
  → 里程碑：price_daily_fwd 可用，波浪引擎可開始開發

Phase E（Phase 5-6，低優先）：
  14. 法人籌碼、基本面等 API 接入
  → 里程碑：完整資料集
```

---

*文件版本: v1.2 | 產出日期: 2026-04-15*

---

## Changelog

### v1.2 (2026-04-15) — Logging 規格新增

| 修補項 | 影響模組 | 說明 |
|--------|---------|------|
| Logging 規格 | `logger_setup.py`（新增）、所有模組 | 新增 §十一，定義日誌初始化、格式、各模組必要事件清單與日誌範例。日誌輸出至 `logs/collector_YYYYMMDD.log`，同時輸出 stdout，使用標準 `logging` 模組。 |
| 開發順序 | §十三 | `logger_setup` 提升至 Phase A（第 5 步），確保從骨架階段即有日誌能力。 |

### v1.1 (2026-04-15) — 交叉驗證修補

基於外部 Code Review 的交叉驗證結果，修補以下確認缺漏：

| 修補項 | 影響模組 | 說明 |
|--------|---------|------|
| SQLite WAL 模式 | `db.py` §7.2 | 初始化加入 `PRAGMA journal_mode=WAL` + `busy_timeout=5000`，防禦性提升寫入穩定性 |
| Schema Validation | `field_mapper.py` §7.1 | `transform()` 前置 `_validate_schema()` 檢查 API 回傳欄位，缺漏時 WARNING 不中斷、新增欄位時 INFO 記錄 |
| Session 生命週期 | `api_client.py` §6.1 | `FinMindClient` 改為 async context manager，確保 `ClientSession` 正確 close，避免長時間運行資源洩漏 |
| Signal Handling | `rust_bridge.py` §8.1 | `run_phase4` 攔截 `CancelledError`，先 SIGTERM 再 wait(10s)，確保 Rust binary 可 commit/rollback 當前 transaction |
| Rust Schema Version | `rust_bridge.py` §8.1 / §8.2 | Rust JSON 輸出加入 `schema_version`，Python 端驗證版本一致性 |
| 現增 AF → Rust | §8.2 step 1.5 / §10.3 | 現金增資的 `adjustment_factor` 計算完全移交 Rust Phase 4，解決 Phase 2 無法取得 Phase 3 price_daily 的跨 Phase 依賴問題 |
| Sync 狀態擴充 | §9.1 | `api_sync_progress.status` 新增 `'schema_mismatch'` 狀態 |
