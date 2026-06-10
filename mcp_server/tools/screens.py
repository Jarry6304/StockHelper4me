"""跨股篩選域 tools — magic_formula + v3.32 三 toolkit + Layer 5 trigger scan。

實作層:mcp_server/_magic_formula.py / _screens.py(讀 *_ranked_derived 表)。
註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _MAX_TOP_N, _clamp, _parse_date


def magic_formula_screen(
    date: str,
    top_n: int = 30,
) -> dict[str, Any]:
    """Greenblatt 2005 Magic Formula 跨股篩選(v3.4 plan §Phase C)。

    內部:讀 magic_formula_ranked_derived(Silver builder 跨股 cross-rank)→
    JOIN stock_info_ref 拿公司名 / industry → top N + median EY/ROIC + 1 句 narrative。
    輸出 ~5 KB / ~1250 tokens。

    Universe:排除金融保險 + 公用事業(Greenblatt 2005 §六 原版)。
    Rank:combined_rank = ey_rank + roic_rank,愈低愈好。

    Args:
        date:  查詢日 ISO 字串(例 "2026-05-15")
        top_n: 取 top N(預設 30 對齊 Greenblatt 原版 20-30)

    Returns:
        {
          "as_of": "2026-05-15",
          "ranking_date": "...",         # 實際 ranking 日(≤ as_of 的 latest)
          "universe_size": 1432,
          "top_n": 30,
          "top_stocks": [{"rank": 1, "stock_id": "2330", "name": "...",
                          "industry": "...", "earnings_yield": 0.082,
                          "roic": 0.31, "ey_rank": 145, "roic_rank": 12,
                          "combined_rank": 157}, ...],
          "stats": {"median_ey": 0.045, "median_roic": 0.08, ...},
          "narrative": "..."
        }

    References:
      - Greenblatt, J. (2005). *The Little Book That Beats the Market*. Wiley.
      - Larkin (2009). SSRN id=1330551(OOS 1988-2007 valid)
    """
    from mcp_server._magic_formula import compute_magic_formula_screen

    return compute_magic_formula_screen(_parse_date(date), top_n=_clamp(top_n, 1, _MAX_TOP_N))


# ────────────────────────────────────────────────────────────
# v3.32 Cross-Stock Factor Screens(4 個 toolkit MCP wrappers)
# ────────────────────────────────────────────────────────────


def monthly_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit A:Monthly screen — 3 factors + Barroso-Santa-Clara vol overlay。

    對齊 v1.1 提案 §四 Toolkit A:
      - A1 Persistent Momentum(Chen-Chou-Hsieh 2023 JFM)
      - A2 Revenue Momentum 3-consec(Hung-Lu-Yang 2025 RQFA)
      - A3 Institutional Concert(Sias 2004 / 周賓凰-池祥麟 2014)
      - Vol-managed overlay(Barroso-Santa-Clara 2015 JFE)

    Args:
        date:    ISO 字串(例 "2026-05-15")
        top_n:   每 factor 取 top N(預設 30)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative},
         vol_managed_overlay: {scale, rationale}, narrative}
    """
    from mcp_server._screens import compute_monthly_screen

    return compute_monthly_screen(_parse_date(date), top_n=_clamp(top_n, 1, _MAX_TOP_N),
                                   database_url=database_url)


def quarterly_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit B:Quarterly screen — F-Score + Low Vol + Industry-Adj GP。

    對齊 v1.1 提案 §四 Toolkit B:
      - B1 Piotroski F-Score ≥ 7(Piotroski 2000 JAR / Walkshäusl 2020 JAM)
      - B2 Low Volatility 252d(Ang et al 2009 JFE / Blitz-van Vliet 2007 JPM)
      - B3 Industry-Adjusted GP(Novy-Marx 2013 JFE / Ng-Shen 2020 A&F)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative}, narrative}
    """
    from mcp_server._screens import compute_quarterly_screen

    return compute_quarterly_screen(_parse_date(date), top_n=_clamp(top_n, 1, _MAX_TOP_N),
                                     database_url=database_url)


def annual_low_risk_screen(
    date: str,
    top_n: int = 30,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Toolkit C:Annual low-risk screen — Long-Term Low Vol + Dividend Yield + 12-1 Momentum。

    對齊 v1.1 提案 §四 Toolkit C:
      - C1 Long-Term Low Vol 36M(Blitz-van Vliet 2007)
      - C2 Cash Dividend Yield + yield trap filter(Boudoukh 2007;提案 v1.1 新增 12M return > -20%
        + 5y 至少 3y 配息 filter)
      - C3 12-1 Momentum(Jegadeesh-Titman 1993 JF)

    Returns:
        {as_of, top_n, toolkit, factors: {3 sub-factor 各 top_stocks + narrative}, narrative}
    """
    from mcp_server._screens import compute_annual_low_risk_screen

    return compute_annual_low_risk_screen(_parse_date(date), top_n=_clamp(top_n, 1, _MAX_TOP_N),
                                          database_url=database_url)


def monthly_trigger_scan(
    date: str,
    stock_id: str | None = None,
    top_n_per_type: int = 20,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """v3.32 Layer 5:Monthly trigger scan(實驗性 conviction adjustment)。

    對齊 v1.1 提案 §四 Layer 5:
      - Positive trigger:月營收 YoY > +30% + 過去 20D 法人累積買超 → 部位 +20% hint
      - Negative trigger:月營收 YoY < -20% + 法人賣超 > 流通股數 1% → 部位 -50% hint

    v3.32 hotfix(2026-05-18):原全攤 ~400+ triggers → ~94KB payload 爆量。修法:
      - stock_id(可選):指定某股,只回該股 trigger(0-2 筆,payload 小)
      - top_n_per_type(預設 20):全市場 scan 時 per trigger_type 取 yoy 最強 N 個
        (counts 仍回 total 不被截斷)

    底層因子 A 級(Hung-Lu-Yang 2025 月營收揭露 alpha + Sias 2004),
    Trigger 架構 C 級(自創 conviction adjustment),需實盤驗證。

    Returns:
        {as_of, signal_date, toolkit, stock_filter, counts: {positive_total, negative_total},
         positive_triggers: [...], negative_triggers: [...], narrative}
    """
    from mcp_server._screens import compute_monthly_trigger_scan

    return compute_monthly_trigger_scan(
        _parse_date(date),
        stock_id=stock_id, top_n_per_type=_clamp(top_n_per_type, 1, _MAX_TOP_N),
        database_url=database_url,
    )
