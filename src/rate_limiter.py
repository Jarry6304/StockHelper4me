"""
rate_limiter.py
----------------
Token Bucket 流量控制模組(v3.3 重寫)。

設計要點:
- 全域共用,所有 API call 共享同一個 bucket(FinMind 限額是帳號級別)
- 不分 per-API 限額
- 支援 cooldown():收到 429 時手動清空 token 並暫停
- v3.3 砍 `min_interval_ms`:refill_rate 已是真實 throttle,額外的最小間隔
  與 token bucket 重疊限速,在並發場景(asyncio.gather)下會把吞吐量壓回
  序列水準。砍掉後 1.58/s(Sponsor 5700/hr)能完整釋出。
- v3.3 加 `asyncio.Lock` 保護 acquire():並發 caller 不會同時通過 token 檢查
  造成超發(舊版單執行緒 asyncio 看起來沒問題,但 Semaphore + gather 場景下
  race condition 確實會踩到)。
"""

import asyncio
import logging
import time

logger = logging.getLogger("collector.rate_limiter")


class RateLimiter:
    """
    Token Bucket Rate Limiter(非同步版本)。

    容量(burst_size)= 允許短時間連續發送的最大次數。
    補充速率(refill_rate)= calls_per_hour / 3600 tokens/秒。

    使用方式:
        limiter = RateLimiter(calls_per_hour=5700, burst_size=10)
        await limiter.acquire()   # 在每次 API call 前呼叫
    """

    def __init__(
        self,
        calls_per_hour: int,
        burst_size: int,
        cooldown_on_429_sec: int = 120,
    ):
        """
        Args:
            calls_per_hour:      每小時允許的最大呼叫次數(帳號級別)
            burst_size:          啟動時初始 token 數(允許連續快速發送的上限)
            cooldown_on_429_sec: 收到 HTTP 429 時的冷卻秒數(預設 120 秒)
        """
        self.capacity            = burst_size
        self.tokens              = float(burst_size)
        self.refill_rate         = calls_per_hour / 3600.0  # tokens per second
        self.cooldown_on_429_sec = cooldown_on_429_sec      # 供 api_client 讀取

        self._last_refill_time = time.monotonic()
        self._lock             = asyncio.Lock()

        logger.info(
            f"RateLimiter 初始化:calls_per_hour={calls_per_hour}, "
            f"burst_size={burst_size}, "
            f"refill_rate={self.refill_rate:.4f}/s, "
            f"cooldown_on_429={cooldown_on_429_sec}s"
        )

    async def acquire(self) -> float:
        """
        等待直到有 token 可用,回傳實際等待的秒數。
        每次 API call 前應先呼叫此方法。

        並發安全:async with self._lock 確保 token 計數不被 race。
        wait 期間仍 hold lock,因此實際 wait 是序列化的 — 對 token-bucket
        語意是正確的(若兩個 caller 同時等待,他們應該各等一個 refill 週期,
        而不是同時搶同一個 token)。

        Returns:
            實際等待的秒數(0.0 表示無需等待)
        """
        start = time.monotonic()
        async with self._lock:
            self._refill()
            while self.tokens < 1:
                wait = (1 - self.tokens) / self.refill_rate
                logger.debug(f"Token 不足,等待 {wait:.2f}s")
                await asyncio.sleep(wait)
                self._refill()
            self.tokens -= 1
        return time.monotonic() - start

    def cooldown(self, seconds: int) -> None:
        """
        收到 HTTP 429 時手動冷卻:清空 token 並把下次 refill 推後 N 秒。

        Args:
            seconds: 冷卻秒數
        """
        logger.warning(f"Rate limit 觸發,冷卻 {seconds}s")
        self.tokens = 0
        # 把 last_refill_time 推到未來,讓 _refill() 在 cooldown 期間補不出 token
        self._last_refill_time = time.monotonic() + seconds

    def _refill(self) -> None:
        """依照經過時間補充 token,上限為 capacity"""
        now     = time.monotonic()
        elapsed = now - self._last_refill_time
        if elapsed <= 0:
            return
        self.tokens = min(self.capacity, self.tokens + elapsed * self.refill_rate)
        self._last_refill_time = now
