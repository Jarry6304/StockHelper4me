"""CORS middleware 整合測 — 跨來源請求應命中 Access-Control-Allow-Origin。

對齊 Phase 0:Svelte dev(:5173)跨域打 API 不被瀏覽器擋。
"""

from __future__ import annotations

import os
from contextlib import contextmanager

import pytest
from fastapi.testclient import TestClient


@contextmanager
def _env(**kwargs: str | None):
    saved = {k: os.environ.get(k) for k in kwargs}
    try:
        for k, v in kwargs.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        yield
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


def _fresh_app():
    # CORS origins are read at add_cors() time, which runs inside create_app().
    # No reload needed — just call create_app() under the desired env.
    from web_api.app import create_app

    return TestClient(create_app())


def test_health_with_dev_origin_returns_cors_header():
    with _env(WEB_API_CORS_ORIGINS=None):
        client = _fresh_app()
        r = client.get("/health", headers={"Origin": "http://localhost:5173"})
        assert r.status_code == 200
        assert r.headers.get("access-control-allow-origin") == "http://localhost:5173"


def test_preflight_options_for_dev_origin():
    with _env(WEB_API_CORS_ORIGINS=None):
        client = _fresh_app()
        r = client.options(
            "/health",
            headers={
                "Origin": "http://localhost:5173",
                "Access-Control-Request-Method": "GET",
            },
        )
        assert r.status_code == 200
        assert r.headers.get("access-control-allow-origin") == "http://localhost:5173"
        assert "GET" in r.headers.get("access-control-allow-methods", "")


def test_unknown_origin_does_not_get_allow_header():
    with _env(WEB_API_CORS_ORIGINS=None):
        client = _fresh_app()
        r = client.get("/health", headers={"Origin": "http://evil.example.com"})
        assert r.status_code == 200
        assert r.headers.get("access-control-allow-origin") != "http://evil.example.com"


def test_env_override_adds_prod_origin():
    with _env(WEB_API_CORS_ORIGINS="https://app.example.com"):
        client = _fresh_app()
        r = client.get("/health", headers={"Origin": "https://app.example.com"})
        assert r.status_code == 200
        assert r.headers.get("access-control-allow-origin") == "https://app.example.com"


def test_env_override_replaces_default_dev_origin():
    with _env(WEB_API_CORS_ORIGINS="https://app.example.com"):
        client = _fresh_app()
        r = client.get("/health", headers={"Origin": "http://localhost:5173"})
        assert r.status_code == 200
        assert r.headers.get("access-control-allow-origin") != "http://localhost:5173"


def test_env_multiple_comma_separated_origins():
    with _env(WEB_API_CORS_ORIGINS="http://localhost:5173,https://app.example.com"):
        client = _fresh_app()

        r1 = client.get("/health", headers={"Origin": "http://localhost:5173"})
        assert r1.headers.get("access-control-allow-origin") == "http://localhost:5173"

        r2 = client.get("/health", headers={"Origin": "https://app.example.com"})
        assert r2.headers.get("access-control-allow-origin") == "https://app.example.com"
