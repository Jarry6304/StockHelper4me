## 四、Rate Limiter

### 4.1 Token Bucket 實作

```python
class RateLimiter:
    """
    全域共用，所有 API call 共享同一個 bucket。
    不分 per-API 限額——FinMind 的 rate limit 是帳號級別。
    """
    def __init__(self, calls_per_hour: int, burst_size: int, min_interval_ms: int):
        self.capacity = burst_size
        self.tokens = burst_size          # 啟動時有 burst 額度
        self.refill_rate = calls_per_hour / 3600.0  # tokens per second
        self.min_interval = min_interval_ms / 1000.0
        self.last_call_time = 0.0
        self.last_refill_time = time.monotonic()

    async def acquire(self):
        """等待直到有 token 可用，回傳等待的秒數"""
        self._refill()
        while self.tokens < 1:
            wait = (1 - self.tokens) / self.refill_rate
            await asyncio.sleep(wait)
            self._refill()

        # 確保最小間隔
        elapsed = time.monotonic() - self.last_call_time
        if elapsed < self.min_interval:
            await asyncio.sleep(self.min_interval - elapsed)

        self.tokens -= 1
        self.last_call_time = time.monotonic()

    def cooldown(self, seconds: int):
        """收到 429 時手動冷卻，清空 token"""
        self.tokens = 0
        self.last_call_time = time.monotonic() + seconds

    def _refill(self):
        now = time.monotonic()
        elapsed = now - self.last_refill_time
        self.tokens = min(self.capacity, self.tokens + elapsed * self.refill_rate)
        self.last_refill_time = now
```

### 4.2 呼叫量估算（初始全量回補）

| Phase | API 數量 | 模式 | 估算呼叫次數（1800 檔 × 2019-2026） |
|-------|---------|------|--------------------------------------|
| 1 | 4 | all_market | ~30（少量分段） |
| 2 | 5 | per_stock / all | ~1,800 × 4 + 少量 = ~7,500 |
| 3 | 2 | per_stock | ~1,800 × 2 × 7段 = ~25,200 |
| 5 | 11 | per_stock | ~1,800 × 11 × 7段 = ~138,600 |
| 6 | 5 | all_market / fixed | ~40 |
| **合計** | | | **~171,370** |

以 1,600 次/小時計算：**~107 小時 ≈ 4.5 天**（不含 Phase 4 本地計算時間）。

**建議執行策略**：
- Phase 1 + 2 + 3 先跑，約 32,700 次 ≈ 20 小時
- Phase 4 本地計算：~1,800 檔 × 7 年 ≈ 數分鐘
- Phase 5 + 6 後跑，約 138,640 次 ≈ 87 小時
- 波浪分析只依賴 Phase 1-4，所以 Phase 1-4 完成即可開始引擎開發

---

## 五、Phase Executor

### 5.1 執行模式

```toml
# collector.toml 追加
[execution]
mode = "backfill"                     # "backfill" | "incremental"

[execution.backfill]
start_date = "2019-01-01"            # 覆蓋 global.backfill_start_date（可選）
phases = [1, 2, 3, 4, 5, 6]         # 指定要跑的 Phase（可部分跑）
resume = true                        # 從上次中斷處繼續

[execution.incremental]
phases = [1, 2, 3, 4, 5, 6]
# incremental 模式自動從 stock_sync_status.last_incr_sync 起算
```

### 5.2 Phase Executor 邏輯

```python
class PhaseExecutor:
    async def run(self, mode: str):
        phases_to_run = self.config.execution[mode].phases

        for phase_num in sorted(phases_to_run):
            if phase_num == 4:
                # Phase 4 特殊處理：呼叫 Rust binary
                await self.run_rust_compute()
                continue

            apis = self.get_apis_for_phase(phase_num)

            for api_config in apis:
                if not api_config.enabled:
                    continue

                stock_ids = self.resolve_stock_ids(api_config)
                date_segments = self.build_date_segments(api_config, mode)

                for stock_id in stock_ids:
                    for (start, end) in date_segments:
                        # 斷點續傳檢查
                        if self.sync_tracker.is_completed(api_config.name, stock_id, start, end):
                            continue

                        await self.rate_limiter.acquire()
                        result = await self.api_client.fetch(api_config, stock_id, start, end)
                        self.process_result(api_config, result)
                        self.sync_tracker.mark_progress(api_config.name, stock_id, start, end)

            # Phase 1 完成後刷新股票清單
            if phase_num == 1:
                self.stock_list = self.stock_resolver.resolve()

    def resolve_stock_ids(self, api_config) -> list[str]:
        """依 param_mode 決定迭代清單"""
        if api_config.param_mode in ("all_market", "all_market_no_id"):
            return ["__ALL__"]  # sentinel，api_client 不送 data_id
        if hasattr(api_config, "fixed_stock_ids"):
            return api_config.fixed_stock_ids
        return self.stock_list
```

### 5.3 Date Segmenter

```python
class DateSegmenter:
    def segments(self, api_config, mode: str, stock_id: str) -> list[tuple[str, str]]:
        """
        回傳 (start_date, end_date) 的 list。

        - segment_days = 0 → 單段 [(backfill_start, today)]
        - segment_days = 365 → 切成年段 [(2019-01-01, 2019-12-31), (2020-01-01, 2020-12-31), ...]
        - incremental 模式 → 單段 [(last_sync + 1, today)]
        """
        if mode == "incremental":
            last = self.sync_tracker.get_last_sync(api_config.name, stock_id)
            start = (last + timedelta(days=1)) if last else self.config.backfill_start_date
            return [(start.isoformat(), date.today().isoformat())]

        if api_config.segment_days == 0:
            return [(self.config.backfill_start_date, date.today().isoformat())]

        # 按 segment_days 切段
        segments = []
        cursor = date.fromisoformat(self.config.backfill_start_date)
        today = date.today()
        while cursor <= today:
            seg_end = min(cursor + timedelta(days=api_config.segment_days - 1), today)
            segments.append((cursor.isoformat(), seg_end.isoformat()))
            cursor = seg_end + timedelta(days=1)
        return segments
```

---

## 六、API Client

### 6.1 統一 HTTP 介面

```python
class FinMindClient:
    BASE_URL = "https://api.finmindtrade.com/api/v4/data"

    def __init__(self, token: str, rate_limiter: RateLimiter, retry_config: dict):
        self.token = token
        self.rate_limiter = rate_limiter
        self.retry = retry_config
        self.session: aiohttp.ClientSession | None = None

    # ── v1.1 新增：Session 生命週期管理 ──
    async def __aenter__(self):
        self.session = aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=30),
        )
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.session and not self.session.closed:
            await self.session.close()
        return False
    # ── 呼叫端用法：async with FinMindClient(...) as client: ──

    async def fetch(self, api_config, stock_id: str, start: str, end: str) -> list[dict]:
        params = {
            "dataset": api_config.dataset,
            "start_date": start,
            "token": self.token,
        }

        # 依 param_mode 組裝參數
        if stock_id != "__ALL__":
            params["data_id"] = stock_id

        if api_config.param_mode in ("per_stock", "all_market", "all_market_no_id"):
            if end:
                params["end_date"] = end

        # 帶重試的 HTTP GET
        for attempt in range(self.retry["max_attempts"]):
            try:
                async with self.session.get(self.BASE_URL, params=params) as resp:
                    if resp.status == 200:
                        body = await resp.json()
                        if body.get("status") == 200:
                            return body.get("data", [])
                        else:
                            raise APIError(f"FinMind error: {body.get('msg')}")

                    if resp.status in self.retry["retry_on_status"]:
                        if resp.status == 429:
                            self.rate_limiter.cooldown(self.retry.get("cooldown_on_429_sec", 120))
                        wait = min(
                            self.retry["backoff_base_sec"] * (2 ** attempt),
                            self.retry["backoff_max_sec"]
                        )
                        logger.warning(f"HTTP {resp.status}, retry {attempt+1} in {wait}s")
                        await asyncio.sleep(wait)
                        continue

                    raise APIError(f"HTTP {resp.status}")

            except (aiohttp.ClientError, asyncio.TimeoutError) as e:
                if attempt < self.retry["max_attempts"] - 1:
                    wait = self.retry["backoff_base_sec"] * (2 ** attempt)
                    await asyncio.sleep(wait)
                    continue
                raise

        raise APIError(f"Max retries exceeded for {api_config.dataset}/{stock_id}")
```

---

## 七、Field Mapper + DB Writer

### 7.1 通用欄位映射

```python
class FieldMapper:
    def transform(self, api_config, raw_records: list[dict]) -> list[dict]:
        """
        0. (v1.1 新增) Schema Validation — 檢查 API 回傳欄位是否與預期一致
        1. 套用 field_rename（原始欄位名 → DB 欄位名）
        2. 以 _ 開頭的欄位收集進 detail JSON
        3. 附加 event_type（若有定義）
        4. 執行 computed_fields 計算
        """
        if not raw_records:
            return []

        # ── v1.1: Schema Validation ──
        self._validate_schema(api_config, raw_records[0])

        results = []
        for record in raw_records:
            row = {}
            detail = {}

            for src_key, value in record.items():
                dest_key = api_config.field_rename.get(src_key, src_key)
                if dest_key.startswith("_"):
                    detail[dest_key.lstrip("_")] = value
                else:
                    row[dest_key] = value

            if detail:
                row["detail"] = json.dumps(detail, ensure_ascii=False)

            if hasattr(api_config, "event_type"):
                row["event_type"] = api_config.event_type

            if hasattr(api_config, "computed_fields"):
                self._compute(api_config, row)

            row["market"] = "TW"
            row["source"] = "finmind"
            results.append(row)

        return results

    def _compute(self, api_config, row: dict):
        """
        依 event_type 計算衍生欄位。
        規則來自 tw_stock_architecture_review_v1.1 §3.4
        """
        if "adjustment_factor" in api_config.computed_fields:
            bp = row.get("before_price")
            ap = row.get("after_price")
            if bp and ap and ap != 0:
                row["adjustment_factor"] = bp / ap
            else:
                row["adjustment_factor"] = 1.0
                logger.warning(f"Cannot compute AF: before={bp}, after={ap}")

        if "volume_factor" in api_config.computed_fields:
            et = row.get("event_type", "")
            if et == "dividend":
                row["volume_factor"] = 1.0
            else:
                bp = row.get("before_price", 0)
                ap = row.get("after_price", 0)
                row["volume_factor"] = (ap / bp) if bp != 0 else 1.0

        if "cash_dividend" in api_config.computed_fields:
            # 見 v1.1 §3.4.1 拆分邏輯
            self._split_dividend(row)

    def _split_dividend(self, row: dict):
        """TaiwanStockDividendResult 的 cash/stock 拆分"""
        detail = json.loads(row.get("detail", "{}"))
        subtype = detail.get("event_subtype", "")
        combined = detail.get("combined_dividend", 0.0)

        if subtype in ("除息", "息"):
            row["cash_dividend"] = combined
            row["stock_dividend"] = 0.0
        elif subtype in ("除權", "權"):
            row["cash_dividend"] = 0.0
            row["stock_dividend"] = combined
        elif subtype == "權息":
            # 需查 _dividend_policy_staging 拆分，此處先留 NULL
            row["cash_dividend"] = None
            row["stock_dividend"] = None
        else:
            row["cash_dividend"] = None
            row["stock_dividend"] = None

    # ── v1.1 新增 ──
    def _validate_schema(self, api_config, sample_record: dict):
        """
        以第一筆回傳資料的 key set 比對 field_rename 中定義的來源欄位。
        若出現未知欄位或缺少必要欄位，記錄 WARNING 供人工檢查。

        ⚠️ 不 raise exception：API 新增欄位不應阻斷入庫，但缺少必要欄位需標記。
        """
        actual_keys = set(sample_record.keys())
        expected_src_keys = set(api_config.field_rename.keys()) if api_config.field_rename else set()

        # 缺少的來源欄位 → 可能導致 rename 失敗或 computed_fields 計算錯誤
        missing = expected_src_keys - actual_keys
        if missing:
            logger.warning(
                f"[SchemaValidation] {api_config.name}: "
                f"expected fields missing from API response: {missing}. "
                f"Data will be ingested but computed_fields may be incorrect."
            )

        # API 新增未知欄位 → 純資訊記錄，不影響流程
        known_keys = actual_keys & (expected_src_keys | {"date", "stock_id", "stock_name"})
        novel = actual_keys - known_keys - expected_src_keys
        if novel:
            logger.info(
                f"[SchemaValidation] {api_config.name}: "
                f"novel fields detected in API response: {novel}"
            )
```

### 7.2 DB Writer

```python
class DBWriter:
    def __init__(self, db_path: str):
        self.conn = sqlite3.connect(db_path)
        # ── v1.1 新增：SQLite 安全性 PRAGMA ──
        self.conn.execute("PRAGMA journal_mode=WAL;")       # 寫入不阻塞讀取，長時間回補更穩定
        self.conn.execute("PRAGMA busy_timeout=5000;")       # 等待鎖最多 5 秒，避免立即噴 locked
        self.conn.execute("PRAGMA foreign_keys=ON;")

    def upsert(self, table: str, rows: list[dict], primary_keys: list[str]):
        """
        SQLite UPSERT (INSERT OR REPLACE)。
        primary_keys 用於衝突檢測。
        """
        if not rows:
            return

        columns = list(rows[0].keys())
        placeholders = ", ".join(["?"] * len(columns))
        col_str = ", ".join(columns)

        sql = f"INSERT OR REPLACE INTO {table} ({col_str}) VALUES ({placeholders})"

        values = [tuple(row.get(c) for c in columns) for row in rows]

        with self.conn:
            self.conn.executemany(sql, values)
```

---

## 八、Rust Bridge（Phase 4）

### 8.1 介面定義

Phase 4 由 Python 呼叫 Rust binary 執行，透過 CLI 參數和 SQLite 檔案交換資料。

```python
class RustBridge:
    def __init__(self, binary_path: str, db_path: str):
        self.binary = binary_path
        self.db = db_path

    async def run_phase4(self, stock_ids: list[str] | None = None, mode: str = "backfill"):
        """
        呼叫 Rust binary 執行後復權 + 週K/月K 聚合。

        Rust binary 直接讀寫同一個 SQLite 檔。
        """
        cmd = [
            self.binary,
            "--db", self.db,
            "--mode", mode,     # "backfill" | "incremental"
        ]

        if stock_ids:
            cmd.extend(["--stocks", ",".join(stock_ids)])
        # 不指定 --stocks 時，Rust binary 自動處理所有 stock_sync_status 中待計算的股票

        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        # ── v1.1 新增：Signal Handling ──
        # 攔截 CancelledError（Ctrl+C 觸發），優雅地終止 Rust 子進程
        try:
            stdout, stderr = await process.communicate()
        except asyncio.CancelledError:
            logger.warning("Phase 4 cancelled, sending SIGTERM to Rust binary...")
            process.terminate()                         # SIGTERM → Rust 端應 commit/rollback 當前 transaction
            try:
                await asyncio.wait_for(process.wait(), timeout=10)
            except asyncio.TimeoutError:
                logger.error("Rust binary did not exit in 10s, sending SIGKILL")
                process.kill()
            raise                                       # 重新拋出 CancelledError，讓上層處理

        if process.returncode != 0:
            raise RustComputeError(f"Phase 4 failed: {stderr.decode()}")

        # 解析 stdout（JSON 格式的計算摘要）
        result = json.loads(stdout.decode())

        # ── v1.1 新增：Schema Version 驗證 ──
        rust_schema = result.get("schema_version")
        if rust_schema and rust_schema != EXPECTED_SCHEMA_VERSION:
            logger.warning(
                f"Rust binary schema_version={rust_schema}, "
                f"expected={EXPECTED_SCHEMA_VERSION}. "
                f"Consider rebuilding: cargo build --release"
            )

        return result
```

### 8.2 Rust Binary CLI 規格

```
tw_stock_compute --db <path> --mode <backfill|incremental> [--stocks <id1,id2,...>]

行為：
  1. 讀取 price_daily + price_adjustment_events
  1.5 (v1.1 新增) 補算 capital_increase 事件的 adjustment_factor：
     - 篩選 event_type='capital_increase' 且 adjustment_factor=1.0 的待驗證事件
     - 從 price_daily 撈取 ex_date 前一交易日的收盤價作為 before_price
     - 從 detail JSON 讀取 subscription_price + subscription_rate_raw
     - 計算 AF 並更新 price_adjustment_events（含 before_price / after_price）
     - 無法計算的事件（如 price_daily 尚無資料）保留 AF=1.0 並記入 errors
  2. 計算 price_daily_fwd（後復權 OHLCV）
  3. 聚合 price_weekly_fwd（依 trading_calendar 的 ISO week）
  4. 聚合 price_monthly_fwd（依 year-month）
  5. 更新 stock_sync_status.fwd_adj_valid = 1
  6. stdout 輸出 JSON 摘要：
     {
       "schema_version": "1.1",
       "processed": 1800,
       "skipped": 12,
       "errors": [{"stock_id": "XXXX", "reason": "..."}],
       "af_patched": 3,
       "elapsed_ms": 45000
     }

演算法：直接引用 tw_stock_architecture_review_v1.1 §四（compute_forward_adjusted）
```

---
