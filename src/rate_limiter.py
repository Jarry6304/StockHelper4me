"""
rate_limiter.py
----------------
Token Bucket 流量控制模組。

設計要點：
- 全域共用，所有 API call 共享同一個 bucket（FinMind 限額是帳號級別）
- 不分 per-API 限額
- 支援 cooldown()：收到 429 時手動清空 token 並暫停
- min_interval：確保兩次呼叫的最小時間間隔，防止短時間 burst 觸發限流
"""

import asyncio
import logging
import time

logger = logging.getLogger("collector.rate_limiter")


class RateLimiter:
    """
    Token Bucket Rate Limiter（非同步版本）。

    容量（burst_size）= 允許短時間連續發送的最大次數。
    補充速率（refill_rate）= calls_per_hour / 3600 tokens/秒。

    使用方式：
        limiter = RateLimiter(calls_per_hour=1600, burst_size=5, min_interval_ms=2250)
        await limiter.acquire()   # 在每次 API call 前呼叫
    """

    def __init__(
        self,
        calls_per_hour: int,
        burst_size: int,
        min_interval_ms: int,
        cooldown_on_429_sec: int = 120,
    ):
        """
        Args:
            calls_per_hour:      每小時允許的最大呼叫次數（帳號級別）
            burst_size:          啟動時初始 token 數（允許連續快速發送的上限）
            min_interval_ms:     兩次呼叫之間的最小間隔（毫秒），防止 burst 時頻率過高
            cooldown_on_429_sec: 收到 HTTP 429 時的冷卻秒數（預設 120 秒）
        """
        self.capacity            = burst_size
        self.tokens              = float(burst_size)   # 啟動時有 burst 額度
        self.refill_rate         = calls_per_hour / 3600.0  # tokens per second
        self.min_interval        = min_interval_ms / 1000.0  # 轉為秒
        self.cooldown_on_429_sec = cooldown_on_429_sec  # 供 api_client 讀取

        self._last_call_time   = 0.0
        self._last_refill_time = time.monotonic()

        logger.info(
            f"RateLimiter 初始化：calls_per_hour={calls_per_hour}, "
            f"burst_size={burst_size}, min_interval={min_interval_ms}ms, "
            f"cooldown_on_429={cooldown_on_429_sec}s"
        )

    async def acquire(self) -> float:
        """
        等待直到有 token 可用，回傳實際等待的秒數。
        每次 API call 前應先呼叫此方法。

        Returns:
            實際等待的秒數（0.0 表示無需等待）
        """
        start = time.monotonic()

        # 補充 token
        self._refill()

        # 等待直到有 token 可用
        while self.tokens < 1:
            # 計算需要等多久才能補充到 1 個 token
            wait = (1 - self.tokens) / self.refill_rate
            logger.debug(f"Token 不足，等待 {wait:.2f}s")
            await asyncio.sleep(wait)
            self._refill()

        # 確保最小呼叫間隔
        elapsed = time.monotonic() - self._last_call_time
        if elapsed < self.min_interval:
            gap = self.min_interval - elapsed
            await asyncio.sleep(gap)

        # 消耗一個 token
        self.tokens -= 1
        self._last_call_time = time.monotonic()

        waited = time.monotonic() - start
        return waited

    def cooldown(self, seconds: int) -> None:
        """
        收到 HTTP 429 時手動冷卻：清空 token 並凍結 N 秒。
        未來的 acquire() 呼叫會在凍結期間持續等待。

        Args:
            seconds: 冷卻秒數
        """
        logger.warning(f"Rate limit 觸發，冷卻 {seconds}s")
        self.tokens = 0
        # 將 last_call_time 推到未來，確保 min_interval 檢查也被延後
        self._last_call_time = time.monotonic() + seconds

    def _refill(self) -> None:
        """依照經過時間補充 token，上限為 capacity"""
        now     = time.monotonic()
        elapsed = now - self._last_refill_time
        # 補充 token，但不超過最大容量
        self.tokens = min(self.capacity, self.tokens + elapsed * self.refill_rate)
        self._last_refill_time = now
