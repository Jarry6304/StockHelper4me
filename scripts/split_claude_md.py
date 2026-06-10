#!/usr/bin/env python3
"""一次性拆分腳本(架構整備 P0-1):CLAUDE.md → docs/changelog/ + docs/claude_history.md 追加。

以 `^## ` header 為單位機械分類(內文不重寫):
- 保留名單        → 暫存 /tmp/claude_kept_sections.md(新入口檔手寫時取材)
- `^## v[34].x`   → docs/changelog/ 對應版本帶檔
- Traditional Core → docs/changelog/traditional-core.md
- verify / 流水線 / backlog → docs/changelog/process-logs.md
- v1.x / 過期 schema 段 → 追加 docs/claude_history.md(段首標 ⚠️ 過期)

特例(執行時印出,commit message 須記錄):
1. v4.37 / v4.35 兩段在歷史 merge 中遺失 `##` 標頭(orphan 內文),拆分時補回
   標頭並各加一行按語;補回後落正確帶檔 + INDEX 各一列。
2. 「Fusion Layer — API 規劃落地(2026-05-20)」無版本號;按日期歸
   v4.10-v4.19 帶檔(機械 catch-all 會誤落 claude_history)。

腳本只產出分類後檔案;新 CLAUDE.md 入口檔另行手寫,本腳本不覆寫 CLAUDE.md。
用法:repo root 下 `python scripts/split_claude_md.py`
"""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = (ROOT / "CLAUDE.md").read_text(encoding="utf-8").splitlines(keepends=True)
OUT_DIR = ROOT / "docs" / "changelog"

KEEP = (
    "專案概要", "常用指令", "架構", "關鍵慣例", "規格與歷史檔", "關鍵架構決策",
    "已知問題清單", "helper 腳本清單", "完整重跑流程", "環境細節",
    "下次 session 建議優先序",
)

SPLIT_DATE = "2026-06-10"


def band(major: int, minor: int) -> str:
    """帶檔名以實際存在的版本範圍命名(對齊規格目標結構,非 lo+9 機械式)。"""
    if major == 4:
        if minor >= 30:
            return "v4.30-v4.38.md"
        if minor >= 20:
            return "v4.20-v4.29.md"
        if minor >= 10:
            return "v4.10-v4.19.md"
        return "v4.00-v4.09.md"
    # major == 3:v3.5–v3.19 合一帶(對齊規格目標結構)
    if minor >= 30:
        return "v3.30-v3.38.md"
    if minor >= 20:
        return "v3.20-v3.29.md"
    return "v3.05-v3.19.md"


# ── 1. 切 section(^## 為界) ────────────────────────────────────────────
sections: list[tuple[str, list[str]]] = []  # (header_line, body_lines)
preamble: list[str] = []
cur: tuple[str, list[str]] | None = None
for ln in SRC:
    if ln.startswith("## "):
        cur = (ln, [])
        sections.append(cur)
    elif cur is None:
        preamble.append(ln)
    else:
        cur[1].append(ln)


# ── 2. 特例:v4.38 / v4.36 內的 orphan 段補回標頭 ──────────────────────
def split_orphan(body: list[str], marker: str, restored_header: str,
                 note: str) -> tuple[list[str], tuple[str, list[str]] | None]:
    """在 body 中找 `---` 後接 marker 開頭的 orphan 段,切出並補標頭。"""
    for i, ln in enumerate(body):
        if ln.startswith(marker):
            # 往回找最近的 --- 分隔線(orphan 段含其前置空行)
            j = i
            while j > 0 and body[j - 1].strip() in ("", "---"):
                j -= 1
                if body[j].strip() == "---":
                    break
            head = body[:j + 1]          # 原段(含 --- 分隔線)
            orphan_body = [f"> (按:本段原在 CLAUDE.md 中遺失 `##` 標頭,{SPLIT_DATE} 拆分時補回。)\n",
                           "\n"] + body[j + 1:]
            print(f"  [特例] 補回標頭:{restored_header.strip()}  ({note})")
            return head, (restored_header, orphan_body)
    return body, None


restored: list[tuple[str, list[str]]] = []
for idx, (header, body) in enumerate(sections):
    if header.startswith("## v4.38"):
        new_body, orphan = split_orphan(
            body, "本 session 在 `claude/neely-forest-cloud-zigzag-Xv13d`",
            "## v4.37 — Traditional Core production 收尾:compaction Rc 共享 + 全市場 P0-Gate 驗證(2026-06-06)\n",
            "v4.37")
        if orphan:
            sections[idx] = (header, new_body)
            sections.insert(idx + 1, orphan)
            restored.append(orphan)
        break
for idx, (header, body) in enumerate(sections):
    if header.startswith("## v4.36"):
        new_body, orphan = split_orphan(
            body, "`magic_formula_ranked_derived`(2026-05-15 最早建)",
            "## v4.35 — magic_formula `is_top_30` → `is_top_n` schema 統一(2026-06-01)\n",
            "v4.35")
        if orphan:
            sections[idx] = (header, new_body)
            sections.insert(idx + 1, orphan)
            restored.append(orphan)
        break


# ── 3. 分類 ────────────────────────────────────────────────────────────
def classify(header: str) -> str:
    h = header.rstrip()
    # KEEP 用「整名 / 整名+(」前綴比對 — 純子字串會誤收
    # `## v3.5 — 5 層架構大型重構…`(含「架構」)之類的版本段
    h2 = h.removeprefix("## ").strip()
    # "（" = 全形(;原檔 KEEP header 的括注(不要改 / 從零開始…)用全形
    if any(h2 == k or h2.startswith(k + "(") or h2.startswith(k + "（")
           or h2.startswith(k + "(") for k in KEEP):
        return "KEEP"
    m = re.match(r"## v([34])\.(\d+)", h)
    if m:
        return band(int(m.group(1)), int(m.group(2)))
    if "Traditional Core" in h:
        return "traditional-core.md"
    if any(k in h for k in ("verify", "流水線", "Backlog triage", "待辦 backlog")):
        return "process-logs.md"
    if "Fusion Layer" in h:  # 特例 2:無版本號的 2026-05-20 feature 段
        print(f"  [特例] Fusion Layer 段按日期歸 v4.10-v4.19.md:{h}")
        return "v4.10-v4.19.md"
    return "HISTORY"


buckets: dict[str, list[tuple[str, list[str]]]] = {}
for header, body in sections:
    buckets.setdefault(classify(header), []).append((header, body))


# ── 4. 寫帶檔 + traditional + process-logs ─────────────────────────────
OUT_DIR.mkdir(parents=True, exist_ok=True)
BAND_INTRO = ("> 自 CLAUDE.md 機械搬移({d} P0-1 拆分),內文未重寫;"
              "版本索引見 [INDEX.md](INDEX.md)。\n")
TITLES = {
    "traditional-core.md": "# Traditional Core v2 / v3 歷程(Frost & Prechter 波浪 vertical)",
    "process-logs.md": "# Process logs — verify chain / 流水線 / backlog triage",
}
for fname, secs in sorted(buckets.items()):
    if fname in ("KEEP", "HISTORY"):
        continue
    title = TITLES.get(fname, f"# Changelog {fname.removesuffix('.md')}")
    out = [title + "\n", "\n", BAND_INTRO.format(d=SPLIT_DATE), "\n"]
    for header, body in secs:
        out.append(header)
        out.extend(body)
    (OUT_DIR / fname).write_text("".join(out), encoding="utf-8")
    print(f"  wrote docs/changelog/{fname}  ({len(secs)} sections)")


# ── 5. INDEX.md ────────────────────────────────────────────────────────
def index_row(header: str, fname: str) -> str | None:
    h = header.rstrip().removeprefix("## ").strip()
    m = re.match(r"(v[\d.]+x?(?:\s*/\s*v[\d.]+)*(?:\s*→\s*v[\d.]+)?)", h)
    if not m:
        return None
    ver = m.group(1).strip().rstrip(".")
    date_m = re.search(r"(\d{4}-\d{2}-\d{2})", h)
    date = date_m.group(1) if date_m else "—"
    rest = re.sub(r"^v[\d./ →x]+\s*[—-]?\s*", "", h)
    rest = re.sub(r"[((][^()()]*\d{4}-\d{2}-\d{2}[^()()]*[))]\s*$", "", rest).strip()
    summary = (rest[:30] + "…") if len(rest) > 30 else (rest or "—")
    return f"| {ver} | {date} | {summary} | {fname} |\n"


rows: list[str] = []
for fname, secs in buckets.items():
    if fname == "KEEP":
        continue
    target = "../claude_history.md" if fname == "HISTORY" else fname
    for header, _ in secs:
        row = index_row(header, target)
        if row:
            rows.append(row)
index = ["# Changelog INDEX\n", "\n",
         f"> 一版一列;自 CLAUDE.md {SPLIT_DATE} P0-1 拆分時生成。"
         "新版本段寫進對應帶檔後在此加一列(規則見 CLAUDE.md「禁止事項」)。\n", "\n",
         "| 版本 | 日期 | 一句話 | 檔案 |\n", "|---|---|---|---|\n"] + rows
(OUT_DIR / "INDEX.md").write_text("".join(index), encoding="utf-8")
print(f"  wrote docs/changelog/INDEX.md  ({len(rows)} rows)")


# ── 6. claude_history.md 追加(⚠️ 過期標註) ────────────────────────────
hist_path = ROOT / "docs" / "claude_history.md"
hist = [f"\n\n---\n\n# 以下段落自 CLAUDE.md 搬入({SPLIT_DATE} P0-1 拆分)\n\n"]
for header, body in buckets.get("HISTORY", []):
    hist.append(header)
    hist.append(f"> ⚠️ 已過期({SPLIT_DATE.rsplit('-', 1)[0]} 拆分時標註),"
                "現行 schema 以 docs/schema_master.md 為準、現行狀態以 CLAUDE.md 為準。\n\n")
    hist.extend(body)
hist_path.write_text(hist_path.read_text(encoding="utf-8") + "".join(hist),
                     encoding="utf-8")
print(f"  appended {len(buckets.get('HISTORY', []))} sections to docs/claude_history.md")


# ── 7. KEEP 段暫存(手寫新 CLAUDE.md 取材用) ───────────────────────────
kept = ["".join(preamble)]
for header, body in buckets.get("KEEP", []):
    kept.append(header)
    kept.extend(body)
Path("/tmp/claude_kept_sections.md").write_text("".join(kept), encoding="utf-8")
print(f"  wrote /tmp/claude_kept_sections.md  ({len(buckets.get('KEEP', []))} kept sections)")

total_old = len(SRC)
total_moved = sum(1 + len(b) for f, ss in buckets.items() if f != "KEEP" for _, b in ss)
print(f"\n  原 CLAUDE.md {total_old} 行;搬出 {total_moved} 行;"
      f"保留段 {total_old - total_moved - len(preamble)} 行 + 導言 {len(preamble)} 行")
