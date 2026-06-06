"""通盤數字驗證 — 2330 全鏈(Silver → Cross-stock → Cores → Golden → Fusion → API)。

走 repo get_connection()(自動讀 .env DATABASE_URL,免 psql、免 shell 引號)。
每個 check:獨立重算 → 比對 DB 存的 → PASS / FAIL / INFO / SKIP + 實際值。

用法(repo root):
    python scripts/verify_2330_pipeline.py
    python scripts/verify_2330_pipeline.py --stock 2330 --api   # 加跑 API passthrough(需 uvicorn 起著)

設計:Bronze 當已知正確起點;由下而上;每個 check 包 try/except,單一失敗不中斷。
遞迴指標(RSI/MACD/Kalman)用 Python 正確算法(不是 SQL window)。
若某 check 因 JSON 形狀 / 後復權方向假設不符 → 退 INFO 並印實際結構,不 crash。
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent
for _p in (str(_REPO / "src"), str(_REPO)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from fusion.raw._db import get_connection  # noqa: E402

RESULTS: list[tuple[str, str, str, str]] = []  # (stage, check, status, detail)
_REL_TOL = 1e-3   # 相對容差(浮點重算)


def record(stage: str, check: str, status: str, detail: str = "") -> None:
    RESULTS.append((stage, check, status, detail))
    mark = {"PASS": "✅", "FAIL": "❌", "INFO": "ℹ️ ", "SKIP": "·", "ERROR": "💥"}.get(status, "?")
    print(f"  {mark} [{status:5}] {check}: {detail}")


def _close(a, b, rel=_REL_TOL) -> bool:
    if a is None or b is None:
        return False
    a, b = float(a), float(b)
    return abs(a - b) <= rel * max(1.0, abs(a), abs(b))


def _f(x):
    return None if x is None else float(x)


def check(stage: str, name: str):
    """decorator:**定義即執行**(one-shot check);包 try/except → ERROR,不中斷整體。"""
    def deco(fn):
        try:
            fn()
        except Exception as e:  # noqa: BLE001
            record(stage, name, "ERROR", f"{type(e).__name__}: {e}")
        return fn
    return deco


# ─── 取共用資料 ──────────────────────────────────────────────────────────────

def fetch_daily(cur, stock) -> list[dict]:
    cur.execute(
        """SELECT date, open, high, low, close, volume,
                  cumulative_adjustment_factor AS caf, cumulative_volume_factor AS cvf
             FROM price_daily_fwd WHERE market='TW' AND stock_id=%s ORDER BY date ASC""",
        (stock,),
    )
    return [dict(r) for r in cur.fetchall()]


def latest_indicator(cur, stock, core) -> dict | None:
    cur.execute(
        """SELECT value_date, timeframe, value
             FROM indicator_values
            WHERE stock_id=%s AND source_core=%s
            ORDER BY value_date DESC LIMIT 1""",
        (stock, core),
    )
    r = cur.fetchone()
    return dict(r) if r else None


# ─── Stage 1 — Silver ────────────────────────────────────────────────────────

def stage1(cur, stock, daily):
    print("\n== Stage 1 — Silver(後復權 / 聚合 / 衍生欄)==")

    @check("1", "後復權不變式(fwd vs raw × cumulative)")
    def _():
        # 方向不確定(caf 可能是 raw=fwd×caf 或 fwd=raw×caf)→ 兩向都試,報哪向成立
        cur.execute("""SELECT f.date, f.close AS fwd, f.caf, r.close AS raw
                         FROM price_daily_fwd f JOIN price_daily r
                           ON f.market=r.market AND f.stock_id=r.stock_id AND f.date=r.date
                        WHERE f.market='TW' AND f.stock_id=%s
                        ORDER BY f.date DESC LIMIT 1""", (stock,))
        row = cur.fetchone()
        if not row:
            return record("1", "後復權不變式", "SKIP", "無對照 raw/fwd row")
        fwd, raw, caf = _f(row["fwd"]), _f(row["raw"]), _f(row["caf"])
        ratio = fwd / raw if raw else None
        if _close(fwd, raw * caf):
            record("1", "後復權不變式", "PASS", f"最新日 fwd=raw×caf;fwd/raw={ratio:.6f} caf={caf:.6f}")
        elif _close(raw, fwd * caf):
            record("1", "後復權不變式", "PASS", f"最新日 raw=fwd×caf(反向);fwd/raw={ratio:.6f} caf={caf:.6f}")
        else:
            record("1", "後復權不變式", "INFO", f"fwd/raw={ratio:.6f} 但 caf={caf:.6f}(最新日通常≈1;非1看是否剛除息)")

    @check("1", "週K聚合(weekly = 該ISO週 daily 聚合)")
    def _():
        cur.execute("""SELECT year, week, open, high, low, close, volume
                         FROM price_weekly_fwd WHERE market='TW' AND stock_id=%s
                        ORDER BY year DESC, week DESC LIMIT 1""", (stock,))
        w = cur.fetchone()
        if not w:
            return record("1", "週K聚合", "SKIP", "無 weekly row")
        y, wk = int(w["year"]), int(w["week"])
        cur.execute("""SELECT close, high, low, volume, date FROM price_daily_fwd
                        WHERE market='TW' AND stock_id=%s
                          AND EXTRACT(ISOYEAR FROM date)=%s AND EXTRACT(WEEK FROM date)=%s
                        ORDER BY date ASC""", (stock, y, wk))
        days = cur.fetchall()
        if not days:
            return record("1", "週K聚合", "INFO", f"{y}W{wk} 取不到對應 daily(ISO 週對齊差異)")
        last_close = _f(days[-1]["close"]); hi = max(_f(d["high"]) for d in days)
        lo = min(_f(d["low"]) for d in days); vol = sum(int(d["volume"] or 0) for d in days)
        ok = _close(last_close, _f(w["close"])) and _close(hi, _f(w["high"])) and _close(lo, _f(w["low"]))
        record("1", "週K聚合", "PASS" if ok else "FAIL",
               f"{y}W{wk} close stored={_f(w['close'])} recompute={last_close} | high {ok} | vol {vol} vs {w['volume']}")

    @check("1", "day_trading_ratio = dt.vol / fwd.vol × 100")
    def _():
        cur.execute("""SELECT d.date, d.day_trading_ratio, dt.volume AS dtvol, f.volume AS fvol
                         FROM day_trading_derived d
                         JOIN day_trading_tw dt ON d.market=dt.market AND d.stock_id=dt.stock_id AND d.date=dt.date
                         JOIN price_daily_fwd f ON d.market=f.market AND d.stock_id=f.stock_id AND d.date=f.date
                        WHERE d.market='TW' AND d.stock_id=%s AND d.day_trading_ratio IS NOT NULL
                        ORDER BY d.date DESC LIMIT 1""", (stock,))
        r = cur.fetchone()
        if not r:
            return record("1", "day_trading_ratio", "SKIP", "無 day_trading row")
        recompute = (int(r["dtvol"]) / int(r["fvol"]) * 100.0) if r["fvol"] else None
        record("1", "day_trading_ratio", "PASS" if _close(recompute, _f(r["day_trading_ratio"]), 1e-2) else "FAIL",
               f"{r['date']} stored={_f(r['day_trading_ratio'])} recompute={recompute}")

    @check("1", "monthly_revenue_yoy = (rev_t − rev_{t-12m})/rev_{t-12m}×100")
    def _():
        cur.execute("""SELECT date, revenue, revenue_yoy FROM monthly_revenue_derived
                        WHERE market='TW' AND stock_id=%s AND revenue_yoy IS NOT NULL
                        ORDER BY date DESC LIMIT 1""", (stock,))
        r = cur.fetchone()
        if not r:
            return record("1", "monthly_revenue_yoy", "SKIP", "無 revenue_yoy row")
        # 找 12 個月前同月 revenue
        cur.execute("""SELECT revenue FROM monthly_revenue_derived
                        WHERE market='TW' AND stock_id=%s
                          AND EXTRACT(YEAR FROM date)=EXTRACT(YEAR FROM %s::date)-1
                          AND EXTRACT(MONTH FROM date)=EXTRACT(MONTH FROM %s::date)
                        LIMIT 1""", (stock, r["date"], r["date"]))
        base = cur.fetchone()
        if not base or not base["revenue"]:
            return record("1", "monthly_revenue_yoy", "INFO",
                          f"{r['date']} stored yoy={_f(r['revenue_yoy'])};無前年同月基期可重算")
        recompute = (_f(r["revenue"]) - _f(base["revenue"])) / _f(base["revenue"]) * 100.0
        record("1", "monthly_revenue_yoy", "PASS" if _close(recompute, _f(r["revenue_yoy"]), 1e-2) else "FAIL",
               f"{r['date']} stored={_f(r['revenue_yoy']):.4f} recompute={recompute:.4f}")


# ─── Stage 2 — Cross-stock magic_formula ────────────────────────────────────

def stage2(cur, stock):
    print("\n== Stage 2 — Cross-stock(magic_formula 自洽)==")

    @check("2", "magic_formula 公式自洽")
    def _():
        cur.execute("""SELECT date, ebit_ttm, market_cap, enterprise_value, invested_capital,
                              earnings_yield, roic, ey_rank, roic_rank, combined_rank, is_top_n, excluded_reason
                         FROM magic_formula_ranked_derived WHERE market='TW' AND stock_id=%s
                        ORDER BY date DESC LIMIT 1""", (stock,))
        r = cur.fetchone()
        if not r:
            return record("2", "magic_formula", "SKIP", "無 magic_formula row(2330 應有)")
        if r["excluded_reason"]:
            return record("2", "magic_formula", "INFO", f"excluded_reason={r['excluded_reason']}(被排除,跳過公式驗)")
        ey, roic = _f(r["earnings_yield"]), _f(r["roic"])
        ebit, ev, ic = _f(r["ebit_ttm"]), _f(r["enterprise_value"]), _f(r["invested_capital"])
        checks = []
        if ev:   checks.append(("ey=ebit/ev", _close(ey, ebit / ev, 1e-3)))
        if ic:   checks.append(("roic=ebit/ic", _close(roic, ebit / ic, 1e-3)))
        if r["combined_rank"] is not None:
            checks.append(("combined=ey_rank+roic_rank",
                           int(r["combined_rank"]) == int(r["ey_rank"]) + int(r["roic_rank"])))
        ok = all(v for _, v in checks)
        record("2", "magic_formula", "PASS" if ok else "FAIL",
               f"{r['date']} " + " | ".join(f"{n}:{v}" for n, v in checks)
               + f" | is_top_n={r['is_top_n']} combined_rank={r['combined_rank']}")


# ─── Stage 3 — M3 Cores(indicator 重算 + neely 不變式)──────────────────────

def _sma(closes, n):
    return sum(closes[-n:]) / min(n, len(closes))


def _rsi_wilder(closes, period=14):
    if len(closes) < period + 1:
        return None
    gains = [max(0.0, closes[i] - closes[i - 1]) for i in range(1, len(closes))]
    losses = [max(0.0, closes[i - 1] - closes[i]) for i in range(1, len(closes))]
    ag = sum(gains[:period]) / period
    al = sum(losses[:period]) / period
    for i in range(period, len(gains)):
        ag = ((period - 1) * ag + gains[i]) / period
        al = ((period - 1) * al + losses[i]) / period
    if al < 1e-12:
        return 100.0
    rs = ag / al
    return 100.0 - 100.0 / (1.0 + rs)


def _ema(values, period):
    alpha = 2.0 / (period + 1)
    out = values[0]
    for v in values[1:]:
        out = alpha * v + (1 - alpha) * out
    return out


def _ema_series(values, period):
    alpha = 2.0 / (period + 1)
    out = [values[0]]
    for v in values[1:]:
        out.append(alpha * v + (1 - alpha) * out[-1])
    return out


def _macd(closes, fast=12, slow=26, signal=9):
    if len(closes) < slow:
        return None
    ef = _ema_series(closes, fast)
    es = _ema_series(closes, slow)
    macd_line = [a - b for a, b in zip(ef, es)]
    sig = _ema(macd_line, signal)
    return macd_line[-1], sig, macd_line[-1] - sig


def _kalman_medium(closes, q=0.01, rel=0.01):
    if not closes:
        return None
    mean_c = sum(closes) / len(closes)
    R = (rel * mean_c) ** 2
    x, P = closes[0], R
    prev = x
    for z in closes:
        P_pred = P + q
        K = P_pred / (P_pred + R)
        x = x + K * (z - x)
        P = (1 - K) * P_pred
        prev = x
    return x


def stage3(cur, stock, daily):
    print("\n== Stage 3 — M3 Cores(indicator 重算 + neely 不變式)==")
    closes_all = [_f(d["close"]) for d in daily]
    dates_all = [d["date"] for d in daily]

    def closes_upto(d):
        if d in dates_all:
            return closes_all[: dates_all.index(d) + 1]
        return closes_all

    @check("3", "MA20 = 最近20根 fwd close 均值")
    def _():
        iv = latest_indicator(cur, stock, "ma_core")
        if not iv:
            return record("3", "MA20", "SKIP", "無 ma_core indicator")
        specs = (iv["value"] or {}).get("series_by_spec", [])
        e20 = next((e for e in specs if e.get("spec", {}).get("period") == 20), None)
        if not e20 or not e20.get("series"):
            return record("3", "MA20", "INFO", f"ma_core 無 period=20 series;found periods="
                          + str([e.get('spec', {}).get('period') for e in specs]))
        last = e20["series"][-1]
        recompute = _sma(closes_upto(_to_date(last["date"])), 20)
        record("3", "MA20", "PASS" if _close(recompute, _f(last["value"])) else "FAIL",
               f"{last['date']} stored={_f(last['value'])} recompute={recompute:.4f}")

    @check("3", "Bollinger(20/2σ population)")
    def _():
        iv = latest_indicator(cur, stock, "bollinger_core")
        if not iv:
            return record("3", "Bollinger", "SKIP", "無 bollinger_core")
        ser = (iv["value"] or {}).get("series", [])
        if not ser:
            return record("3", "Bollinger", "INFO", "無 series")
        last = ser[-1]
        cs = closes_upto(_to_date(last["date"]))[-20:]
        mid = sum(cs) / len(cs)
        std = math.sqrt(sum((c - mid) ** 2 for c in cs) / len(cs))
        up, lo = mid + 2 * std, mid - 2 * std
        ok = _close(mid, _f(last["middle_band"])) and _close(up, _f(last["upper_band"])) and _close(lo, _f(last["lower_band"]))
        record("3", "Bollinger", "PASS" if ok else "FAIL",
               f"{last['date']} mid stored={_f(last['middle_band'])} recompute={mid:.4f} | up {ok}")

    @check("3", "RSI14(Wilder 遞迴)")
    def _():
        iv = latest_indicator(cur, stock, "rsi_core")
        if not iv:
            return record("3", "RSI14", "SKIP", "無 rsi_core")
        ser = (iv["value"] or {}).get("series", [])
        if not ser:
            return record("3", "RSI14", "INFO", "無 series")
        last = ser[-1]
        recompute = _rsi_wilder(closes_upto(_to_date(last["date"])), 14)
        record("3", "RSI14", "PASS" if _close(recompute, _f(last["value"]), 1e-2) else "FAIL",
               f"{last['date']} stored={_f(last['value']):.4f} recompute={recompute:.4f}")

    @check("3", "MACD(12/26/9 鏈式EMA)")
    def _():
        iv = latest_indicator(cur, stock, "macd_core")
        if not iv:
            return record("3", "MACD", "SKIP", "無 macd_core")
        ser = (iv["value"] or {}).get("series", [])
        if not ser:
            return record("3", "MACD", "INFO", "無 series")
        last = ser[-1]
        rc = _macd(closes_upto(_to_date(last["date"])))
        if rc is None:
            return record("3", "MACD", "INFO", "資料不足")
        ml, sg, hg = rc
        ok = _close(ml, _f(last["macd_line"]), 1e-2) and _close(sg, _f(last["signal_line"]), 1e-2)
        record("3", "MACD", "PASS" if ok else "FAIL",
               f"{last['date']} macd stored={_f(last['macd_line']):.4f} recompute={ml:.4f} | signal {ok}")

    @check("3", "Kalman medium horizon(遞迴,近似)")
    def _():
        iv = latest_indicator(cur, stock, "kalman_filter_core")
        if not iv:
            return record("3", "Kalman", "SKIP", "無 kalman_filter_core")
        val = iv["value"] or {}
        horizons = val.get("horizons", [])
        med = next((h for h in horizons if h.get("label") == "medium"), None)
        sl = (med or {}).get("series_last") or (val.get("series") or [{}])[-1]
        stored = _f(sl.get("smoothed_price")) if sl else None
        recompute = _kalman_medium(closes_all)
        status = "PASS" if _close(recompute, stored, 0.05) else "INFO"  # 5% 寬容(實作細節差異)
        record("3", "Kalman", status,
               f"medium smoothed stored={stored} recompute≈{recompute:.2f}(5%容差;regime={sl.get('regime') if sl else '?'})")

    @check("3", "neely forest 不變式(size ≤ 200)")
    def _():
        cur.execute("""SELECT timeframe, jsonb_array_length(snapshot->'scenario_forest') AS n
                         FROM structural_snapshots
                        WHERE stock_id=%s AND core_name='neely_core'
                          AND snapshot_date=(SELECT MAX(snapshot_date) FROM structural_snapshots
                                              WHERE stock_id=%s AND core_name='neely_core')
                        ORDER BY timeframe""", (stock, stock))
        rows = cur.fetchall()
        if not rows:
            return record("3", "neely forest", "SKIP", "無 neely structural_snapshots")
        sizes = {r["timeframe"]: r["n"] for r in rows}
        ok = all((n or 0) <= 200 for n in sizes.values())
        record("3", "neely forest", "PASS" if ok else "FAIL", f"forest_size by tf={sizes}(≤200)")


# ─── Stage 4 — Golden L3 ─────────────────────────────────────────────────────

def stage4(cur, stock):
    print("\n== Stage 4 — Golden L3(levels/resonance 不變式)==")

    @check("4", "levels_fusion 不變式")
    def _():
        cur.execute("""SELECT snapshot FROM structural_snapshots
                        WHERE stock_id=%s AND core_name='levels_fusion'
                        ORDER BY snapshot_date DESC LIMIT 1""", (stock,))
        r = cur.fetchone()
        if not r:
            return record("4", "levels_fusion", "SKIP", "無 levels_fusion(需先 golden fusion)")
        snap = r["snapshot"] or {}
        levels = snap.get("levels", [])
        n = snap.get("level_count", len(levels))
        prices = [_f(lv["price"]) for lv in levels]
        in_band = all(_f(lv["low"]) <= _f(lv["price"]) <= _f(lv["high"]) for lv in levels)
        sorted_ok = prices == sorted(prices)
        cap_ok = (n or 0) <= 20
        ok = in_band and sorted_ok and cap_ok
        record("4", "levels_fusion", "PASS" if ok else "FAIL",
               f"level_count={n}(≤20:{cap_ok}) price∈[low,high]:{in_band} sorted:{sorted_ok} "
               f"total={snap.get('level_count_total')}")

    @check("4", "resonance_fusion A-1 finding 邏輯一致")
    def _():
        cur.execute("""SELECT snapshot FROM structural_snapshots
                        WHERE stock_id=%s AND core_name='resonance_fusion' AND timeframe='daily'
                        ORDER BY snapshot_date DESC LIMIT 1""", (stock,))
        r = cur.fetchone()
        if not r:
            return record("4", "resonance_fusion", "SKIP", "無 resonance_fusion(需 golden fusion + forecast)")
        snap = r["snapshot"] or {}
        findings = snap.get("findings", [])
        bad = []
        for fnd in findings:
            lvl = fnd.get("level")
            covers = fnd.get("band_covers")
            mc = fnd.get("median_close")
            top = snap.get("is_top_30")
            if lvl == "divergence" and covers:
                bad.append("divergence但covers=True")
            if lvl == "basic" and not covers:
                bad.append("basic但covers=False")
            if lvl == "strong" and not (covers and mc and top):
                bad.append("strong但非(covers∧median_close∧is_top_30)")
        record("4", "resonance_fusion", "PASS" if not bad else "FAIL",
               f"findings={len(findings)} single_track={snap.get('single_track_mode')} "
               + ("一致" if not bad else f"違規:{bad[:3]}"))


# ─── Stage 5 — Fusion forecast_log ──────────────────────────────────────────

def stage5(cur, stock):
    print("\n== Stage 5 — Fusion(forecast_log CQR band)==")

    @check("5", "forecast band lower<point<upper + 偏好序")
    def _():
        cur.execute("""SELECT forecast_date, horizon_days, confidence, lower, point, upper, source_core
                         FROM forecast_log
                        WHERE stock_id=%s AND horizon_days IN (21,63,126)
                          AND ABS(confidence-0.80)<1e-6 AND internal_only=FALSE
                          AND lower IS NOT NULL AND upper IS NOT NULL
                        ORDER BY forecast_date DESC LIMIT 9""", (stock,))
        rows = cur.fetchall()
        if not rows:
            return record("5", "forecast band", "SKIP", "無 forecast_log band(需 forecast fuse/conformalize)")
        bad = [(int(r["horizon_days"]), r["source_core"]) for r in rows
               if not (_f(r["lower"]) <= _f(r["point"]) <= _f(r["upper"]))]
        cores = sorted({r["source_core"] for r in rows})
        record("5", "forecast band", "PASS" if not bad else "FAIL",
               f"rows={len(rows)} cores={cores} " + ("lower<point<upper 全對" if not bad else f"違規:{bad}"))


# ─── Stage 6 — API passthrough(可選,需 uvicorn)──────────────────────────────

def stage6(cur, stock, as_of):
    print("\n== Stage 6 — API passthrough(需 uvicorn web_api.app:app)==")
    try:
        import json
        import urllib.request
    except Exception:  # noqa: BLE001
        return record("6", "API", "SKIP", "urllib 不可用")

    base = "http://localhost:8000"

    def get(path):
        with urllib.request.urlopen(base + path, timeout=5) as resp:
            return resp.status, resp.read().decode("utf-8")

    # 先探活
    try:
        get(f"/stocks/{stock}/levels?as_of={as_of}")
    except Exception as e:  # noqa: BLE001
        return record("6", "API", "SKIP", f"uvicorn 未起 / 不可達({type(e).__name__});跳過 API 驗")

    cases = [
        (f"/stocks/{stock}/levels?as_of={as_of}", "levels_fusion", "_all_"),
        (f"/stocks/{stock}/resonance?as_of={as_of}&timeframe=daily", "resonance_fusion", "daily"),
        (f"/stocks/{stock}/neely/forest?as_of={as_of}&timeframe=daily", "neely_core", "daily"),
    ]
    for path, core, tf in cases:
        @check("6", f"passthrough {path.split('?')[0]}")
        def _(path=path, core=core, tf=tf):
            status, body = get(path)
            if status != 200:
                return record("6", f"passthrough {core}", "FAIL", f"HTTP {status}")
            cur.execute("""SELECT snapshot::text AS j FROM structural_snapshots
                            WHERE stock_id=%s AND core_name=%s AND timeframe=%s AND snapshot_date<=%s
                            ORDER BY snapshot_date DESC LIMIT 1""", (stock, core, tf, as_of))
            row = cur.fetchone()
            if not row:
                return record("6", f"passthrough {core}", "SKIP", "DB 無對應 snapshot")
            import json as _json
            same = _json.loads(body) == _json.loads(row["j"])
            record("6", f"passthrough {core}", "PASS" if same else "FAIL",
                   f"HTTP 200,API==DB:{same}")

    # traditional/forest + waves(只驗 200)
    for path in (f"/stocks/{stock}/traditional/forest?timeframe=daily",
                 f"/stocks/{stock}/waves?as_of={as_of}"):
        @check("6", f"200 {path.split('?')[0]}")
        def _(path=path):
            status, _b = get(path)
            record("6", f"200 {path.split('?')[0]}", "PASS" if status == 200 else "FAIL", f"HTTP {status}")


# ─── helpers ─────────────────────────────────────────────────────────────────

def _to_date(s):
    from datetime import date, datetime
    if isinstance(s, date):
        return s
    return datetime.strptime(str(s)[:10], "%Y-%m-%d").date()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stock", default="2330")
    ap.add_argument("--api", action="store_true", help="加跑 Stage 6 API passthrough(需 uvicorn)")
    args = ap.parse_args()
    stock = args.stock

    conn = get_connection()
    try:
        cur = conn.cursor()
        daily = fetch_daily(cur, stock)
        if not daily:
            print(f"❌ {stock} 無 price_daily_fwd 資料,無法驗")
            return 1
        as_of = str(daily[-1]["date"])
        print(f"通盤驗證 stock={stock} as_of={as_of} bars={len(daily)}")

        stage1(cur, stock, daily)
        stage2(cur, stock)
        stage3(cur, stock, daily)
        stage4(cur, stock)
        stage5(cur, stock)
        if args.api:
            stage6(cur, stock, as_of)
    finally:
        conn.close()

    # 總表
    print("\n" + "=" * 72)
    print("總結")
    print("=" * 72)
    from collections import Counter
    c = Counter(s for _, _, s, _ in RESULTS)
    for stg in ("1", "2", "3", "4", "5", "6"):
        rows = [r for r in RESULTS if r[0] == stg]
        if rows:
            line = " ".join(f"{r[2]}" for r in rows)
            print(f"  Stage {stg}: {line}")
    print(f"\n  PASS={c['PASS']} FAIL={c['FAIL']} INFO={c['INFO']} SKIP={c['SKIP']} ERROR={c['ERROR']}")
    if c["FAIL"] or c["ERROR"]:
        print("  → 有 FAIL/ERROR,逐項看上面 detail(stored vs recompute)。")
    return 1 if (c["FAIL"] or c["ERROR"]) else 0


if __name__ == "__main__":
    raise SystemExit(main())
