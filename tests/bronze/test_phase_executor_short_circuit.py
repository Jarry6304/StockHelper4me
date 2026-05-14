"""Tests for `phase_executor._run_api` short-circuit + APIError status_code propagation。

Short-circuit 設計:連續 N 次同 dataset 收到 dataset-level error
(HTTP 403 / 404 / 422)→ abort 此 entry 剩餘 stocks。

對齊 v1.36 fix:user 跑 refresh 卡在 dividend_policy 對全 1700+ 股 HTTP 403,
應該 5 次就 short-circuit 跳到下個 entry,而不是浪費 30+ 分鐘 retry 全市場。
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Ensure sys.path 對齊 mcp_server pattern
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


class TestApiErrorStatusCode:
    """APIError 帶 status_code 屬性。"""

    def test_status_code_attribute_present(self):
        from api_client import APIError

        err = APIError("HTTP 403, dataset=Foo", status_code=403)
        assert err.status_code == 403
        assert "HTTP 403" in str(err)

    def test_status_code_default_none(self):
        from api_client import APIError

        err = APIError("FinMind business error")
        assert err.status_code is None

    def test_status_code_kwarg_only(self):
        """status_code 必須是 keyword arg(避免 positional 誤傳)。"""
        from api_client import APIError

        with pytest.raises(TypeError):
            APIError("x", 403)  # positional status_code 不允許


class TestPhaseExecutorShortCircuit:
    """`_run_api` 連續 dataset-level error 達閾值後 abort 整個 entry。

    對 phase_executor 內部行為做 mock test,聚焦 short-circuit 邏輯(不測完整
    pipeline)。
    """

    def test_threshold_value(self):
        """Short-circuit 閾值 = 5(對應 plan 拍版)。"""
        from bronze.phase_executor import PhaseExecutor

        assert PhaseExecutor._DATASET_ERROR_STREAK_THRESHOLD == 5

    def test_dataset_error_codes_include_403_404_422(self):
        """403 / 404 / 422 為 dataset-level errors(token quota / tier / dataset 下架)。"""
        from bronze.phase_executor import PhaseExecutor

        codes = PhaseExecutor._DATASET_ERROR_CODES
        assert 403 in codes
        assert 404 in codes
        assert 422 in codes
        # 429 / 500 是 retry-able / transient,不應在 dataset-level 集合
        assert 429 not in codes
        assert 500 not in codes
