"""Audit FinMind tier:對 collector.toml 內所有 unique dataset 各打 1 次 API
(2330 + 短日期段),回報哪些 OK / 哪些被拒(403/400/422)。

用法:
    python scripts/probe_collector_tier.py

對齊 v1.36 short-circuit fix 後的場景:user FinMind tier 不夠時,collector
會 short-circuit 跳過該 entry,本 script 預先告訴 user 哪些 entry 會被跳過。
"""

from __future__ import annotations

import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


def _load_env():
    """讀 .env 把 FINMIND_TOKEN 進 process env。"""
    env_path = Path(".env")
    if not env_path.exists():
        return
    for line in env_path.read_text(encoding="utf-8").splitlines():
        if "=" in line and not line.lstrip().startswith("#"):
            k, v = line.split("=", 1)
            os.environ.setdefault(k.strip(), v.strip().strip('"').strip("'"))


def _extract_datasets(toml_path: Path) -> list[tuple[str, str]]:
    """從 collector.toml 抽出 [(api_name, dataset), ...] only enabled = true。"""
    text = toml_path.read_text(encoding="utf-8")
    # 拆 [[api]] block
    blocks = re.split(r"^\[\[api\]\]\s*$", text, flags=re.MULTILINE)
    out: list[tuple[str, str]] = []
    for block in blocks[1:]:  # blocks[0] 是 file header
        # name / dataset / enabled 在前面 [section] 之前
        # 截到下一個 [...] 或 EOF
        body = re.split(r"^\[", block, maxsplit=1, flags=re.MULTILINE)[0]
        name_m = re.search(r'^\s*name\s*=\s*"([^"]+)"', body, flags=re.MULTILINE)
        ds_m = re.search(r'^\s*dataset\s*=\s*"([^"]+)"', body, flags=re.MULTILINE)
        en_m = re.search(r'^\s*enabled\s*=\s*(true|false)', body, flags=re.MULTILINE)
        if not name_m or not ds_m:
            continue
        if en_m and en_m.group(1) == "false":
            continue
        out.append((name_m.group(1), ds_m.group(1)))
    return out


def _probe(token: str, dataset: str, stock_id: str | None) -> tuple[int, str]:
    """對單個 dataset 打 1 次 API,回 (http_status, summary)。"""
    params = {
        "dataset": dataset,
        "start_date": "2026-05-10",
        "end_date":   "2026-05-13",
        "token": token,
    }
    if stock_id:
        params["data_id"] = stock_id
    url = "https://api.finmindtrade.com/api/v4/data?" + urllib.parse.urlencode(params)
    try:
        with urllib.request.urlopen(url, timeout=15) as r:
            body = r.read().decode("utf-8", errors="replace")
            try:
                j = json.loads(body)
                rows = j.get("data") or []
                msg = j.get("msg", "")
                return r.status, f"OK rows={len(rows)} msg={msg[:60]}"
            except json.JSONDecodeError:
                return r.status, f"non-JSON body[:80]: {body[:80]}"
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        return e.code, f"{body[:160]}"
    except Exception as e:
        return -1, f"ERROR {type(e).__name__}: {e}"


def main():
    _load_env()
    token = os.environ.get("FINMIND_TOKEN", "")
    if not token:
        print("ERROR: FINMIND_TOKEN 未設(.env 或 environment)", file=sys.stderr)
        sys.exit(1)

    toml_path = Path("config/collector.toml")
    if not toml_path.exists():
        print(f"ERROR: 找不到 {toml_path}", file=sys.stderr)
        sys.exit(1)

    entries = _extract_datasets(toml_path)
    # 同 dataset 多個 entry 都列(讓 user 知道哪 entry 對應)
    print(f"== Probing {len(entries)} enabled entries against FinMind API ==")
    print(f"   (token 前 8 碼: {token[:8]}...)")
    print()

    # 按 dataset 唯一去重以省 API quota(不同 entry 同 dataset 行為一樣)
    seen_dataset: dict[str, tuple[int, str]] = {}

    ok_entries: list[str] = []
    tier_blocked: list[str] = []
    other_errors: list[tuple[str, int]] = []

    for i, (name, dataset) in enumerate(entries):
        if dataset in seen_dataset:
            status, summary = seen_dataset[dataset]
            cached = "[cached]"
        else:
            status, summary = _probe(token, dataset, "2330")
            seen_dataset[dataset] = (status, summary)
            cached = ""
            time.sleep(0.5)  # 對齊 rate limit

        # 分類
        if status == 200 and "rows=" in summary and "msg=" in summary:
            ok_entries.append(name)
            tag = "✅"
        elif status == 400 and "level is" in summary:
            tier_blocked.append(name)
            tag = "🔒 TIER"
        elif status in (403, 422):
            tier_blocked.append(name)
            tag = "🔒 TIER"
        elif status == 404:
            other_errors.append((name, status))
            tag = "❓ 404"
        else:
            other_errors.append((name, status))
            tag = "❌"
        print(f"  {tag:8s} {name:35s} dataset={dataset:48s} [{status}] {summary[:90]} {cached}")

    print()
    print("=" * 80)
    print(f"摘要:{len(ok_entries)} OK / {len(tier_blocked)} tier-blocked / {len(other_errors)} 其他")
    print("=" * 80)

    if tier_blocked:
        print()
        print(f"🔒 Tier 不夠的 entries({len(tier_blocked)} 個):")
        print("   建議 collector.toml 暫時 disable,等升 FinMind tier 後切回 true:")
        for n in tier_blocked:
            print(f"      - {n}")

    if other_errors:
        print()
        print(f"❌ 其他錯誤 entries({len(other_errors)} 個):")
        for n, s in other_errors:
            print(f"      - {n} [{s}]")

    if not tier_blocked and not other_errors:
        print()
        print("🎉 所有 enabled entries 都能存取 FinMind API。")


if __name__ == "__main__":
    main()
