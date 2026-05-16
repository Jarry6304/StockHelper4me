"""
api_client.py
--------------
FinMind HTTP Client 模組。

設計要點：
- 實作為 async context manager，確保 aiohttp.ClientSession 正確關閉
- 統一出入口，所有 API call 都經過此模組
- 自動重試（指數退避）：支援 429 / 5xx 錯誤
- 429 觸發 RateLimiter.cooldown()，確保全域限流
- 依 param_mode 自動組裝 FinMind API 參數
"""

import asyncio
import logging
from typing import Any

import aiohttp

from config_loader import ApiConfig, RetryConfig
from rate_limiter import RateLimiter

logger = logging.getLogger("collector.api_client")

# FinMind v4 API 基礎 URL
FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"

# all_market 模式使用的 sentinel stock_id（api_client 看到此值不送 data_id）
ALL_MARKET_SENTINEL = "__ALL__"

# 網路 / HTTP retry backoff sequence(秒)
# 對 DNS / 網路層短暫錯誤,給較長 wait 等待恢復(2026-05-10 修:原 exp 5/10/20 太短)
# attempt 0 失敗等 60s(1 分鐘)→ attempt 1 失敗等 300s(5 分鐘)→ attempt 2 失敗 raise
# 取代 RetryConfig.backoff_base_sec / backoff_max_sec(留 collector.toml 欄位不影響 schema,api_client 不再讀)
RETRY_BACKOFF_SEC = [60, 300]


def _retry_wait_sec(attempt: int) -> int:
    """attempt 索引對應的 wait 秒數;超出索引則用最後一個值"""
    if attempt < len(RETRY_BACKOFF_SEC):
        return RETRY_BACKOFF_SEC[attempt]
    return RETRY_BACKOFF_SEC[-1]


class APIError(Exception):
    """FinMind API 回傳非預期結果時拋出。

    含 `status_code` 屬性(若是 HTTP 錯誤,值為 HTTP status;若是業務錯誤如 FinMind
    回 msg 但 status=200,值為 None)。供 caller(如 phase_executor short-circuit
    機制)區分 dataset-level error(403/404/422)vs 個別股可 retry error。
    """

    def __init__(self, message: str, *, status_code: int | None = None):
        super().__init__(message)
        self.status_code = status_code


class FinMindClient:
    """
    FinMind API 非同步 HTTP Client。

    使用方式（async context manager）：
        async with FinMindClient(token, rate_limiter, retry_cfg) as client:
            data = await client.fetch(api_config, "2330", "2023-01-01", "2023-12-31")
    """

    def __init__(
        self,
        token: str,
        rate_limiter: RateLimiter,
        retry_config: RetryConfig,
    ):
        """
        Args:
            token:        FinMind API Token
            rate_limiter: 共用的 Token Bucket 限流器
            retry_config: 重試策略設定
        """
        self.token        = token
        self.rate_limiter = rate_limiter
        self.retry        = retry_config
        self._session: aiohttp.ClientSession | None = None

    # =========================================================================
    # Context Manager 生命週期
    # =========================================================================

    async def __aenter__(self) -> "FinMindClient":
        """建立 aiohttp Session（每個 session 共用 TCP 連線池）"""
        self._session = aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=30),
        )
        logger.debug("FinMindClient session 建立")
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> bool:
        """確保 Session 正確關閉，避免長時間運行時資源洩漏"""
        if self._session and not self._session.closed:
            await self._session.close()
            logger.info("FinMindClient session closed.")
        return False  # 不吞掉例外

    # =========================================================================
    # 主要 fetch 方法
    # =========================================================================

    async def fetch(
        self,
        api_config: ApiConfig,
        stock_id: str,
        start: str,
        end: str,
    ) -> list[dict[str, Any]]:
        """
        向 FinMind API 發送 GET 請求並回傳資料列表。
        包含 Rate Limit 控制與指數退避重試。

        Args:
            api_config: API 設定（dataset、param_mode 等）
            stock_id:   股票代碼；ALL_MARKET_SENTINEL 表示不送 data_id
            start:      開始日期（YYYY-MM-DD）
            end:        結束日期（YYYY-MM-DD）

        Returns:
            FinMind 回傳的 data 陣列（list of dict）

        Raises:
            APIError:   FinMind 回傳業務錯誤或達最大重試次數
        """
        if self._session is None:
            raise RuntimeError("請在 async with 區塊內使用 FinMindClient")

        # 等待 Rate Limiter 放行
        await self.rate_limiter.acquire()

        # 組裝 HTTP 查詢參數
        params = self._build_params(api_config, stock_id, start, end)

        # 帶重試的 HTTP GET
        for attempt in range(self.retry.max_attempts):
            try:
                async with self._session.get(FINMIND_BASE_URL, params=params) as resp:
                    if resp.status == 200:
                        body = await resp.json()

                        # FinMind 自定義業務狀態碼
                        if body.get("status") == 200:
                            return body.get("data", [])
                        else:
                            raise APIError(
                                f"FinMind API error: msg={body.get('msg')}, "
                                f"dataset={api_config.dataset}, stock={stock_id}",
                                status_code=None,
                            )

                    # 需要重試的 HTTP 狀態碼
                    if resp.status in self.retry.retry_on_status:
                        if resp.status == 429:
                            # 觸發全域冷卻，冷卻秒數從 rate_limiter 讀取（來自 RateLimitConfig）
                            self.rate_limiter.cooldown(self.rate_limiter.cooldown_on_429_sec)

                        wait = _retry_wait_sec(attempt)
                        logger.warning(
                            f"HTTP {resp.status}, retry {attempt + 1}/{self.retry.max_attempts} "
                            f"in {wait}s. dataset={api_config.dataset}, stock={stock_id}"
                        )
                        await asyncio.sleep(wait)
                        continue

                    # 不在重試清單的錯誤，直接拋出
                    raise APIError(
                        f"HTTP {resp.status}，dataset={api_config.dataset}, stock={stock_id}",
                        status_code=resp.status,
                    )

            except (aiohttp.ClientError, asyncio.TimeoutError) as e:
                # 網路層錯誤(DNS / connection reset 等),用 RETRY_BACKOFF_SEC 較長 wait 等待恢復
                if attempt < self.retry.max_attempts - 1:
                    wait = _retry_wait_sec(attempt)
                    logger.warning(
                        f"網路錯誤 {type(e).__name__}，retry {attempt + 1}/{self.retry.max_attempts} "
                        f"in {wait}s. dataset={api_config.dataset}, stock={stock_id}"
                    )
                    await asyncio.sleep(wait)
                    continue
                raise

        raise APIError(
            f"達最大重試次數（{self.retry.max_attempts}）。"
            f"dataset={api_config.dataset}, stock={stock_id}, segment={start}~{end}",
            status_code=None,
        )

    # =========================================================================
    # 私有輔助方法
    # =========================================================================

    def _build_params(
        self,
        api_config: ApiConfig,
        stock_id: str,
        start: str,
        end: str,
    ) -> dict[str, str]:
        """
        依 param_mode 組裝 FinMind v4 HTTP API 查詢參數。

        param_mode 說明：
          all_market        → dataset + start_date + end_date（無 data_id）
          all_market_no_id  → 同上，語意明示「無 data_id」
          all_market_no_end → dataset + start_date（無 data_id 也無 end_date)
                              v3.14:FinMind 對 size-too-large dataset(e.g.
                              gov_bank)的限制,「only send one day data, so
                              end_date parameter need be none」
          per_stock         → dataset + data_id + start_date + end_date
          per_stock_no_end  → dataset + data_id + start_date（無 end_date）
          per_stock_fixed   → 同 per_stock，但 data_id 來自 fixed_ids（如 SPY、TAIEX）

        ⚠️ FinMind v4 HTTP API 統一使用 data_id 作為股票識別參數。
           SDK 函式參數 stock_id 為 Pythonic 命名，內部會 mapping 到 data_id 才送出。
        """
        params: dict[str, str] = {
            "dataset":    api_config.dataset,
            "start_date": start,
            "token":      self.token,
        }

        # 需要 data_id 的模式（per_stock 系列）
        per_stock_modes = ("per_stock", "per_stock_no_end", "per_stock_fixed")
        if api_config.param_mode in per_stock_modes and stock_id != ALL_MARKET_SENTINEL:
            params["data_id"] = stock_id

        # 需要 end_date 的模式（per_stock_no_end 例外不送）
        end_date_modes = ("per_stock", "per_stock_fixed", "all_market", "all_market_no_id")
        if api_config.param_mode in end_date_modes and end:
            params["end_date"] = end

        return params
