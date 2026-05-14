"""Aggregation Layer Streamlit dashboard(Phase C)。

對齊 m3Spec/aggregation_layer.md r1 + plan /root/.claude/plans/squishy-foraging-stroustrup.md。

6 tabs:
1. 📈 K-line  ── candlestick + bollinger + MA + neely zigzag + 6 indicator subplots + facts markers
2. 💰 Chip    ── institutional / margin / foreign_holding / day_trading / shareholder
3. 📊 Fundamental ── revenue 月頻 + valuation percentile + financial_statement 季表
4. 🌐 Environment ── TAIEX/TPEx + SPY/VIX + Fear-Greed gauge + market margin + business indicator
5. 🌳 Neely Wave ── scenario picker + zigzag deep-dive + Fib zones
6. ⭐ Facts 散點雲 ── x=fact_date / y=source_core / color=kind

用法:
    pip install -e ".[dashboard]"
    streamlit run dashboards/aggregation.py
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path

import streamlit as st

# 確保從 repo root 跑 streamlit 時 src/ 可 import(對齊 .pth 但 streamlit 不一定吃)
REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from agg import as_of_with_ohlc  # noqa: E402

from dashboards.charts import (  # noqa: E402
    candlestick,
    chip,
    environment,
    facts_cloud,
    fundamental,
    indicators,
    overlays,
)


# ────────────────────────────────────────────────────────────
# Page config
# ────────────────────────────────────────────────────────────

st.set_page_config(
    page_title="台股 Aggregation Dashboard",
    page_icon="📊",
    layout="wide",
    initial_sidebar_state="expanded",
)

st.title("📊 台股 Aggregation Dashboard")
st.caption("M3 即時請求路徑層 · 並排呈現,不整合 · plotly 視覺化主幹(Phase C)")


# ────────────────────────────────────────────────────────────
# Sidebar:查詢參數 + layer toggles
# ────────────────────────────────────────────────────────────

with st.sidebar:
    st.header("🔎 查詢")
    stock_id = st.text_input("Stock ID", value="2330", help="例 2330 / 2317 / 0050")
    as_of_date = st.date_input("As of", value=date.today() - timedelta(days=1))
    lookback_days = st.slider("Lookback days", 30, 365, 90)
    include_market = st.checkbox("並排 market-level", value=True)
    fetch_clicked = st.button("撈資料", type="primary", use_container_width=True)

    st.markdown("---")
    st.markdown("### 📈 K-line layers")
    layer_volume     = st.checkbox("Volume",      value=True)
    layer_ma         = st.checkbox("MA(SMA20/60/200)", value=True)
    layer_bollinger  = st.checkbox("Bollinger",   value=True)
    layer_neely      = st.checkbox("Neely zigzag", value=True)
    layer_facts      = st.checkbox("Facts markers", value=True)

    st.markdown("### Indicator subplots")
    layer_macd  = st.checkbox("MACD", value=True)
    layer_rsi   = st.checkbox("RSI",  value=True)
    layer_kd    = st.checkbox("KD",   value=True)
    layer_adx   = st.checkbox("ADX",  value=False)
    layer_atr   = st.checkbox("ATR",  value=False)
    layer_obv   = st.checkbox("OBV",  value=False)


# ────────────────────────────────────────────────────────────
# Data fetch(快取 5 分鐘)
# ────────────────────────────────────────────────────────────

if not fetch_clicked:
    st.info("👈 設定 Stock ID + As of 後按「撈資料」")
    st.stop()


@st.cache_data(ttl=300, show_spinner="撈 PG 三表 + price_daily_fwd...")
def _fetch_all(stock_id: str, as_of_date: date, lookback_days: int, include_market: bool):
    """as_of_with_ohlc → (snapshot_dict, ohlc_rows)。"""
    snapshot, ohlc = as_of_with_ohlc(
        stock_id,
        as_of_date,
        lookback_days=lookback_days,
        include_market=include_market,
    )
    return snapshot.to_dict(), ohlc


try:
    snapshot, ohlc = _fetch_all(stock_id, as_of_date, lookback_days, include_market)
except Exception as e:
    st.error(f"❌ 撈資料失敗: {e}")
    st.exception(e)
    st.stop()


# 摘要 metrics
facts_list = snapshot["facts"]
indicators_dict = snapshot["indicator_latest"]
structural_dict = snapshot["structural"]
market_dict = snapshot.get("market", {})

c1, c2, c3, c4, c5 = st.columns(5)
c1.metric("OHLC days", len(ohlc))
c2.metric("Facts", len(facts_list))
c3.metric("Indicators", len(indicators_dict))
c4.metric("Structural", len(structural_dict))
c5.metric("Market facts", sum(len(v) for v in market_dict.values()))


# ────────────────────────────────────────────────────────────
# Tabs
# ────────────────────────────────────────────────────────────

tab_kline, tab_chip, tab_fund, tab_env, tab_neely, tab_facts = st.tabs([
    "📈 K-line",
    "💰 Chip",
    "📊 Fundamental",
    "🌐 Environment",
    "🌳 Neely Wave",
    "⭐ Facts 散點雲",
])


# ──────── Tab 1: K-line ────────

with tab_kline:
    if not ohlc:
        st.warning(f"price_daily_fwd 無 {stock_id} 在 {as_of_date} 往前 {lookback_days} 天的資料")
    else:
        # 算 indicator subplots 個數
        active_indicators = []
        if layer_macd:  active_indicators.append(("MACD",  "macd_core"))
        if layer_rsi:   active_indicators.append(("RSI",   "rsi_core"))
        if layer_kd:    active_indicators.append(("KD",    "kd_core"))
        if layer_adx:   active_indicators.append(("ADX",   "adx_core"))
        if layer_atr:   active_indicators.append(("ATR",   "atr_core"))
        if layer_obv:   active_indicators.append(("OBV",   "obv_core"))
        n_ind = len(active_indicators)

        fig = candlestick.build_kline_figure(
            ohlc,
            n_indicator_subplots=n_ind,
            indicator_titles=[t for t, _ in active_indicators],
            with_volume=layer_volume,
        )

        # Overlays(主圖 row 1)
        if layer_ma:
            overlays.add_ma_lines(fig, indicators_dict.get("ma_core@daily"))
        if layer_bollinger:
            overlays.add_bollinger_band(fig, indicators_dict.get("bollinger_core@daily"))
        if layer_neely:
            overlays.add_neely_zigzag(
                fig,
                structural_dict.get("neely_core@daily"),
                show_fib_zones=False,
            )

        # Indicator subplots(動態 row 安排)
        # Row layout:1=K-line, 2=Volume(if layer_volume), 之後依序是 indicators
        row_offset = 2 + (1 if layer_volume else 0)  # next row index after K-line + Volume
        actual_row = row_offset
        row_for_core: dict[str, int] = {}
        for label, core_key in active_indicators:
            ind = indicators_dict.get(f"{core_key}@daily")
            if label == "MACD":
                indicators.add_macd_subplot(fig, ind, row=actual_row)
            elif label == "RSI":
                indicators.add_rsi_subplot(fig, ind, row=actual_row)
            elif label == "KD":
                indicators.add_kd_subplot(fig, ind, row=actual_row)
            elif label == "ADX":
                indicators.add_adx_subplot(fig, ind, row=actual_row)
            elif label == "ATR":
                indicators.add_atr_subplot(fig, ind, row=actual_row)
            elif label == "OBV":
                indicators.add_obv_subplot(fig, ind, row=actual_row)
            row_for_core[core_key] = actual_row
            actual_row += 1

        # Facts markers(animation row 對到 active subplot;沒被啟用的 indicator core facts 標主圖)
        if layer_facts and facts_list:
            facts_cloud.add_facts_to_kline(
                fig,
                facts_list,
                row_map=row_for_core,
                default_row=1,
            )

        # Height 隨 row 數動態調(避免 subplots 太擠)
        total_rows = 1 + (1 if layer_volume else 0) + n_ind
        fig_height = 400 + total_rows * 110
        fig.update_layout(height=fig_height)
        st.plotly_chart(fig, use_container_width=True)


# ──────── Tab 2-6 stub(Phase C-4 ~ C-8 接) ────────

with tab_chip:
    if not ohlc:
        st.warning(f"price_daily_fwd 無 {stock_id} 資料")
    else:
        chip_fig = chip.build_chip_figure(
            ohlc,
            institutional=indicators_dict.get("institutional_core@daily"),
            margin=indicators_dict.get("margin_core@daily"),
            foreign_holding=indicators_dict.get("foreign_holding_core@daily"),
            day_trading=indicators_dict.get("day_trading_core@daily"),
            shareholder=indicators_dict.get("shareholder_core@weekly"),
        )
        # Chip facts markers(institutional/margin/foreign/day_trading/shareholder → 對應 row)
        chip_facts = [
            f for f in facts_list
            if f.get("source_core") in {
                "institutional_core", "margin_core",
                "foreign_holding_core", "day_trading_core", "shareholder_core",
            }
        ]
        if chip_facts:
            chip_row_map = {
                "institutional_core":  2,
                "margin_core":         3,
                "foreign_holding_core": 4,
                "day_trading_core":    5,
                "shareholder_core":    5,
            }
            facts_cloud.add_facts_to_kline(
                chip_fig, chip_facts, row_map=chip_row_map, default_row=1,
            )
        chip_fig.update_layout(height=900)
        st.plotly_chart(chip_fig, use_container_width=True)

with tab_fund:
    sub_revenue, sub_valuation, sub_financial = st.tabs(
        ["月營收", "估值 percentile", "財報季頻"]
    )
    with sub_revenue:
        rev_fig = fundamental.build_revenue_chart(
            indicators_dict.get("revenue_core@monthly")
            or indicators_dict.get("revenue_core@daily")
        )
        st.plotly_chart(rev_fig, use_container_width=True)
    with sub_valuation:
        val_fig = fundamental.build_valuation_chart(indicators_dict.get("valuation_core@daily"))
        st.plotly_chart(val_fig, use_container_width=True)
    with sub_financial:
        fin_fig, fin_rows = fundamental.build_financial_statement_view(
            indicators_dict.get("financial_statement_core@quarterly")
            or indicators_dict.get("financial_statement_core@daily")
        )
        st.plotly_chart(fin_fig, use_container_width=True)
        if fin_rows:
            st.markdown("**財報季頻 raw rows**")
            st.dataframe(fin_rows, use_container_width=True, height=300)

with tab_env:
    sub_taiex, sub_us, sub_global = st.tabs(["TAIEX/TPEx", "美股 + VIX", "全球指標"])
    with sub_taiex:
        st.plotly_chart(
            environment.build_taiex_chart(indicators_dict.get("taiex_core@daily")),
            use_container_width=True,
        )
    with sub_us:
        st.plotly_chart(
            environment.build_us_market_chart(indicators_dict.get("us_market_core@daily")),
            use_container_width=True,
        )
    with sub_global:
        col_l, col_r = st.columns(2)
        with col_l:
            st.plotly_chart(
                environment.build_fear_greed_gauge(indicators_dict.get("fear_greed_core@daily")),
                use_container_width=True,
            )
            st.plotly_chart(
                environment.build_exchange_rate_chart(indicators_dict.get("exchange_rate_core@daily")),
                use_container_width=True,
            )
        with col_r:
            st.plotly_chart(
                environment.build_market_margin_dial(indicators_dict.get("market_margin_core@daily")),
                use_container_width=True,
            )
            st.plotly_chart(
                environment.build_business_indicator_matrix(
                    indicators_dict.get("business_indicator_core@monthly")
                    or indicators_dict.get("business_indicator_core@daily")
                ),
                use_container_width=True,
            )

with tab_neely:
    st.info("🌳 Neely Wave Tab 留 Phase C-7(scenario picker + deep-dive)")

with tab_facts:
    if not facts_list:
        st.warning(f"無 facts(stock={stock_id}, lookback={lookback_days})")
    else:
        # Filter sidebar(within tab)
        all_cores = sorted({f.get("source_core") for f in facts_list if f.get("source_core")})
        filt = st.multiselect("過濾 source_core(空 = 全部)", options=all_cores, default=[])
        fig_scatter = facts_cloud.build_facts_scatter(
            facts_list,
            source_cores=filt or None,
            title=f"{stock_id} Facts 散點雲(as_of {as_of_date}, lookback {lookback_days}d)",
        )
        st.plotly_chart(fig_scatter, use_container_width=True)


# ────────────────────────────────────────────────────────────
# Raw debug(摺疊)
# ────────────────────────────────────────────────────────────

with st.expander("🔧 Raw snapshot dict(debug)"):
    st.json(snapshot, expanded=False)
