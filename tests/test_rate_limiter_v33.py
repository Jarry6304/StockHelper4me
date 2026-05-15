"""Tests for v3.3 RateLimiter — async lock + no min_interval。

對齊 plan §規格 1。
"""

from __future__ import annotations

import asyncio
import sys
import time
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


class TestRateLimiterV33:
    """v3.3 RateLimiter:無 min_interval / async lock 並發安全。"""

    def test_init_no_min_interval_kwarg(self):
        """Constructor 不再接受 min_interval_ms(對齊 plan §規格 1)。"""
        from rate_limiter import RateLimiter

        # 接受 calls_per_hour / burst_size / cooldown_on_429_sec
        limiter = RateLimiter(calls_per_hour=5700, burst_size=10)
        assert limiter.capacity == 10
        assert limiter.tokens == 10.0
        assert abs(limiter.refill_rate - (5700 / 3600)) < 1e-9

        # min_interval_ms 已砍 → TypeError(unexpected kwarg)
        with pytest.raises(TypeError):
            RateLimiter(calls_per_hour=5700, burst_size=10, min_interval_ms=2250)

    @pytest.mark.asyncio
    async def test_burst_immediate_acquire(self):
        """初始 burst_size token 應該瞬間放完(無等待)。"""
        from rate_limiter import RateLimiter

        limiter = RateLimiter(calls_per_hour=3600, burst_size=5)
        start = time.monotonic()
        for _ in range(5):
            wait = await limiter.acquire()
            assert wait < 0.05  # 各次 < 50ms(基本上 0)
        elapsed = time.monotonic() - start
        assert elapsed < 0.1   # 5 個 token 全 burst,< 100ms

    @pytest.mark.asyncio
    async def test_token_refill_after_burst(self):
        """耗盡 token 後 acquire 應該等待 refill。"""
        from rate_limiter import RateLimiter

        # calls_per_hour=3600 → 1 token/sec
        limiter = RateLimiter(calls_per_hour=3600, burst_size=2)
        await limiter.acquire()  # token=1
        await limiter.acquire()  # token=0
        # 第 3 次應該需要等 ~1 sec
        start = time.monotonic()
        await limiter.acquire()
        elapsed = time.monotonic() - start
        assert 0.8 < elapsed < 1.5  # 容忍 wait sleep 的不精確

    @pytest.mark.asyncio
    async def test_concurrent_acquire_with_lock(self):
        """並發 acquire 不會踩 race condition 超發 token。"""
        from rate_limiter import RateLimiter

        # calls_per_hour=36000 → 10 token/sec;burst_size=5
        limiter = RateLimiter(calls_per_hour=36000, burst_size=5)
        # 並發 10 個 acquire,前 5 個 burst,後 5 個各等 ~0.1s
        start = time.monotonic()
        await asyncio.gather(*(limiter.acquire() for _ in range(10)))
        elapsed = time.monotonic() - start
        # 後 5 個應該 serial wait ~0.5s(若 race 會 burst 完 10 個 ~0s,失敗)
        assert 0.3 < elapsed < 1.5

    def test_cooldown_resets_token(self):
        """cooldown 應該清空 tokens 且延後下次 refill。"""
        from rate_limiter import RateLimiter

        limiter = RateLimiter(calls_per_hour=3600, burst_size=10)
        assert limiter.tokens == 10.0
        limiter.cooldown(seconds=60)
        assert limiter.tokens == 0
        # _last_refill_time 推到未來 → _refill 算 elapsed 是負,token 不會補
        limiter._refill()
        assert limiter.tokens == 0
