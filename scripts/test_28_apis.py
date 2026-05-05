"""
test_28_apis.py
---------------
最小依賴的 FinMind API 連線健檢腳本。
逐一呼叫指定 toml 中所有 enabled = true 的 API entry，回報每支 API 的：
HTTP code / FinMind 業務 status / 回傳筆數 / 樣本欄位。

支援 5 種 param_mode（與 v1.4 設定對齊）：
  all_market        → dataset (+ start_date/end_date)
  all_market_no_id  → dataset + start_date + end_date（語意同上）
  per_stock         → dataset + data_id + start_date + end_date
  per_stock_no_end  → dataset + data_id + start_date
  per_stock_fixed   → 同 per_stock 但 data_id 來自 fixed_ids；多個 id 各跑一次

設計目標：
- 不依賴 aiohttp、不依賴專案模組，只用標準函式庫（urllib + tomllib）
- 每次呼叫間隔 2.5s，貼齊 1600 calls/hour 限制
- 詳細結果寫入 scripts/test_28_apis_result.json

執行方式：
    FINMIND_TOKEN="<your_token>" python3 scripts/test_28_apis.py [options]

Options（位置參數，皆可省略）：
    --config PATH    指定 toml 路徑（預設 config/collector.toml）
    --stock ID       per_stock 用的股票（預設 2330）
    --days N         測試日期範圍（預設 30）

範例：
    # 測試官方建議版設定，2330 / 近 90 天
    FINMIND_TOKEN="..." python scripts/test_28_apis.py --config config/collector.toml.suggested.toml --days 90
"""

from __future__ import annotations

import argparse
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
DEFAULT_CONFIG   = REPO_ROOT / "config" / "collector.toml"
RESULT_PATH      = REPO_ROOT / "scripts" / "test_28_apis_result.json"
FINMIND_URL      = "https://api.finmindtrade.com/api/v4/data"
MIN_INTERVAL_SEC = 2.5


def build_params(api: dict, data_id: str, start: str, end: str, token: str) -> dict[str, str]:
    """依 param_mode 組裝 query params。v4 HTTP API 統一使用 data_id 作為股票識別。"""
    params: dict[str, str] = {
        "dataset":    api["dataset"],
        "start_date": start,
        "token":      token,
    }
    mode = api["param_mode"]

    if mode in ("per_stock", "per_stock_no_end", "per_stock_fixed"):
        params["data_id"] = data_id

    if mode in ("per_stock", "per_stock_fixed", "all_market", "all_market_no_id"):
        if end:
            params["end_date"] = end

    return params


def call_api(api: dict, data_id: str, start: str, end: str, token: str) -> dict:
    params = build_params(api, data_id, start, end, token)
    safe_params = {**params, "token": "***" if token else "(empty)"}
    url = f"{FINMIND_URL}?{urllib.parse.urlencode(params)}"

    base = {
        "name":        api["name"],
        "phase":       api["phase"],
        "dataset":     api["dataset"],
        "param_mode":  api["param_mode"],
        "stock_used":  data_id if "data_id" in params else "-",
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


def expand_calls(api: dict, default_stock: str) -> list[str]:
    """
    依 param_mode 決定 data_id 清單：
      - per_stock_fixed             → 用 fixed_ids 全部跑一次
      - per_stock / per_stock_no_end → 若有 fixed_stock_ids（v1.2 用法）也展開，
                                       否則用 default_stock
      - 其他                         → 不需 data_id，回傳 [""]
    支援舊欄位名 fixed_stock_ids 作 alias。
    """
    mode = api["param_mode"]
    fixed = api.get("fixed_ids") or api.get("fixed_stock_ids")
    if mode == "per_stock_fixed":
        return list(fixed) if fixed else [default_stock]
    if mode in ("per_stock", "per_stock_no_end"):
        return list(fixed) if fixed else [default_stock]
    return [""]


def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--config", default=str(DEFAULT_CONFIG),
                   help=f"toml 路徑（預設 {DEFAULT_CONFIG.relative_to(REPO_ROOT)}）")
    p.add_argument("--stock", default="2330",
                   help="per_stock 模式使用的股票代碼（預設 2330）")
    p.add_argument("--days", type=int, default=30,
                   help="測試日期範圍（預設 30 天）")
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])

    token = os.environ.get("FINMIND_TOKEN", "").strip()
    if not token:
        print("ERROR: FINMIND_TOKEN 環境變數未設定", file=sys.stderr)
        print('Usage: FINMIND_TOKEN="<your_token>" python3 scripts/test_28_apis.py [--config ...] [--stock 2330] [--days 30]',
              file=sys.stderr)
        return 2

    config_path = Path(args.config).resolve()
    end   = date.today().isoformat()
    start = (date.today() - timedelta(days=args.days)).isoformat()

    with open(config_path, "rb") as f:
        cfg = tomllib.load(f)
    apis = [a for a in cfg.get("api", []) if a.get("enabled", True)]
    apis.sort(key=lambda a: (a["phase"], a["name"]))

    # 展開：per_stock_fixed 多 id 各算一支呼叫
    plan: list[tuple[dict, str]] = []
    for api in apis:
        for did in expand_calls(api, args.stock):
            plan.append((api, did))

    print("==== tw-stock-collector API 連線測試 ====")
    print(f"config       : {config_path}")
    print(f"per_stock id : {args.stock}")
    print(f"date range   : {start} ~ {end}  ({args.days} 天)")
    print(f"API entries  : {len(apis)}  (展開後 {len(plan)} 次呼叫)")
    print(f"min_interval : {MIN_INTERVAL_SEC}s\n")

    results: list[dict] = []
    for i, (api, did) in enumerate(plan, 1):
        suffix = f" data_id={did}" if did else ""
        print(f"[{i:2d}/{len(plan)}] phase{api['phase']} {api['name']:24s} "
              f"({api['dataset']:42s}){suffix}", flush=True)
        r = call_api(api, did, start, end, token)
        results.append(r)

        flag = "  OK    " if r["ok"] and r["rows"] > 0 else (
               "  EMPTY " if r["ok"] and r["rows"] == 0 else "  FAIL  ")
        print(f"       {flag} http={r['http']} api={r['api_status']} rows={r['rows']} ({r['elapsed']}s)")
        if r["error"]:
            print(f"          error: {r['error']}")
        if r["msg"] and not r["ok"]:
            print(f"          msg  : {r['msg']}")

        if i < len(plan):
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
            print(f"  FAIL phase{r['phase']} {r['name']:24s} stock={r['stock_used']:8s} "
                  f"http={r['http']} api={r['api_status']} err={r['error']} msg={r['msg'][:80]}")

    if ok_empty:
        print("\n--- 零筆回傳（API OK 但 data=[]） ---")
        for r in ok_empty:
            print(f"  EMPTY phase{r['phase']} {r['name']:24s} stock={r['stock_used']}")

    RESULT_PATH.write_text(json.dumps(results, ensure_ascii=False, indent=2))
    print(f"\n詳細結果寫入：{RESULT_PATH}")

    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
