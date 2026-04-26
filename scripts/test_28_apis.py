"""
test_28_apis.py
---------------
最小依賴的 FinMind API 連線健檢腳本。
逐一呼叫 config/collector.toml 中所有 enabled = true 的 API entry，
回報每支 API 的：HTTP code / FinMind 業務 status / 回傳筆數 / 樣本欄位。

設計目標：
- 不依賴 aiohttp、不依賴專案模組，只用標準函式庫
- per_stock 模式預設用 2330（market_index_us 自動切到 fixed_stock_ids[0]）
- 每次呼叫間隔 2.5s，貼齊 collector.toml 設定的 1600 calls/hour 限制
- 詳細結果寫入 scripts/test_28_apis_result.json 供後續分析

執行方式：
    FINMIND_TOKEN="<your_token>" python3 scripts/test_28_apis.py [stock_id] [days]

預設：stock_id=2330, days=30
"""

from __future__ import annotations

import json
import os
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from datetime import date, timedelta
from pathlib import Path

REPO_ROOT        = Path(__file__).resolve().parent.parent
CONFIG_PATH      = REPO_ROOT / "config" / "collector.toml"
RESULT_PATH      = REPO_ROOT / "scripts" / "test_28_apis_result.json"
FINMIND_URL      = "https://api.finmindtrade.com/api/v4/data"
MIN_INTERVAL_SEC = 2.5  # 略大於 collector.toml 的 2250ms，留安全邊界


def build_params(api: dict, stock_id: str, start: str, end: str, token: str) -> dict[str, str]:
    """依 param_mode 組裝 query params，對齊 src/api_client.py 的 _build_params"""
    params: dict[str, str] = {
        "dataset":    api["dataset"],
        "start_date": start,
        "token":      token,
    }
    if api["param_mode"] in ("per_stock", "per_stock_no_end"):
        params["data_id"] = stock_id
    if api["param_mode"] in ("per_stock", "all_market", "all_market_no_id"):
        params["end_date"] = end
    return params


def call_api(api: dict, stock_id: str, start: str, end: str, token: str) -> dict:
    params = build_params(api, stock_id, start, end, token)
    safe_params = {**params, "token": "***" if token else "(empty)"}
    url = f"{FINMIND_URL}?{urllib.parse.urlencode(params)}"

    base = {
        "name":        api["name"],
        "phase":       api["phase"],
        "dataset":     api["dataset"],
        "param_mode":  api["param_mode"],
        "stock_used":  stock_id if api["param_mode"] in ("per_stock", "per_stock_no_end") else "-",
        "params":      safe_params,
    }

    started = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "tw-stock-collector-test/1.0"})
        with urllib.request.urlopen(req, timeout=30) as resp:
            body = json.loads(resp.read())
        elapsed = round(time.time() - started, 2)
        api_status = body.get("status")
        data = body.get("data") or []
        return {
            **base,
            "http":        resp.status,
            "api_status":  api_status,
            "msg":         body.get("msg", ""),
            "rows":        len(data),
            "sample_keys": list(data[0].keys()) if data else [],
            "elapsed":     elapsed,
            "ok":          resp.status == 200 and api_status == 200,
            "error":       None,
        }
    except urllib.error.HTTPError as e:
        try:
            body = json.loads(e.read())
        except Exception:
            body = {}
        return {
            **base,
            "http":        e.code,
            "api_status":  body.get("status"),
            "msg":         body.get("msg", str(e.reason)),
            "rows":        0,
            "sample_keys": [],
            "elapsed":     round(time.time() - started, 2),
            "ok":          False,
            "error":       f"HTTPError {e.code}",
        }
    except Exception as e:
        return {
            **base,
            "http":        None,
            "api_status":  None,
            "msg":         "",
            "rows":        0,
            "sample_keys": [],
            "elapsed":     round(time.time() - started, 2),
            "ok":          False,
            "error":       f"{type(e).__name__}: {e}",
        }


def main() -> int:
    token = os.environ.get("FINMIND_TOKEN", "").strip()
    if not token:
        print("ERROR: FINMIND_TOKEN 環境變數未設定", file=sys.stderr)
        print('Usage: FINMIND_TOKEN="<your_token>" python3 scripts/test_28_apis.py [stock_id] [days]',
              file=sys.stderr)
        return 2

    stock_id = sys.argv[1] if len(sys.argv) > 1 else "2330"
    days     = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    end      = date.today().isoformat()
    start    = (date.today() - timedelta(days=days)).isoformat()

    with open(CONFIG_PATH, "rb") as f:
        cfg = tomllib.load(f)
    apis = [a for a in cfg.get("api", []) if a.get("enabled", True)]
    apis.sort(key=lambda a: (a["phase"], a["name"]))

    print("==== tw-stock-collector API 連線測試 ====")
    print(f"config       : {CONFIG_PATH}")
    print(f"stock_id     : {stock_id}  (per_stock 模式使用)")
    print(f"date range   : {start} ~ {end}  ({days} 天)")
    print(f"total APIs   : {len(apis)}")
    print(f"min_interval : {MIN_INTERVAL_SEC}s\n")

    results: list[dict] = []
    for i, api in enumerate(apis, 1):
        used_stock = api["fixed_stock_ids"][0] if api.get("fixed_stock_ids") else stock_id

        print(f"[{i:2d}/{len(apis)}] phase{api['phase']} {api['name']:24s} "
              f"({api['dataset']:42s}) ", end="", flush=True)
        r = call_api(api, used_stock, start, end, token)
        results.append(r)

        flag = "OK    " if r["ok"] and r["rows"] > 0 else (
               "EMPTY " if r["ok"] and r["rows"] == 0 else "FAIL  ")
        print(f"{flag} http={r['http']} api={r['api_status']} rows={r['rows']} ({r['elapsed']}s)")
        if r["error"]:
            print(f"        error: {r['error']}")
        if r["msg"] and not r["ok"]:
            print(f"        msg  : {r['msg']}")

        if i < len(apis):
            time.sleep(MIN_INTERVAL_SEC)

    ok_full  = [r for r in results if r["ok"] and r["rows"] > 0]
    ok_empty = [r for r in results if r["ok"] and r["rows"] == 0]
    failed   = [r for r in results if not r["ok"]]

    print("\n" + "=" * 78)
    print(f"OK (有資料)  : {len(ok_full):2d} / {len(results)}")
    print(f"OK (零筆)    : {len(ok_empty):2d} / {len(results)}")
    print(f"失敗         : {len(failed):2d} / {len(results)}")

    if failed:
        print("\n--- 失敗清單 ---")
        for r in failed:
            print(f"  FAIL phase{r['phase']} {r['name']:24s} http={r['http']} "
                  f"api={r['api_status']} err={r['error']} msg={r['msg'][:80]}")

    if ok_empty:
        print("\n--- 零筆回傳（API OK 但 data=[]，可能日期範圍 / stock 無資料） ---")
        for r in ok_empty:
            print(f"  EMPTY phase{r['phase']} {r['name']:24s} stock={r['stock_used']}")

    RESULT_PATH.write_text(json.dumps(results, ensure_ascii=False, indent=2))
    print(f"\n詳細結果寫入：{RESULT_PATH}")

    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
