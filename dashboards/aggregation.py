"""Streamlit dashboard — Aggregation Layer 即時請求視覺化 demo。

對齊 m3Spec/aggregation_layer.md §11 Phase B-3。

用法:
    pip install streamlit pandas
    streamlit run dashboards/aggregation.py

4 區塊並排:
1. 個股 facts 時間軸
2. Indicator 最新值表
3. 結構性 snapshot 摘要(neely scenario forest)
4. 市場環境並排(5 個保留字 stock_id facts)
"""

from __future__ import annotations

from datetime import date, timedelta

import streamlit as st

# 確保從 repo root 跑 streamlit 時 src/ 可 import
# (對齊 pip install -e . 後的 .pth 行為,但 streamlit 不一定吃 site-packages)
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from agg import as_of  # noqa: E402


# ────────────────────────────────────────────────────────────
# Page config
# ────────────────────────────────────────────────────────────

st.set_page_config(
    page_title="台股 Aggregation Layer Demo",
    page_icon="📊",
    layout="wide",
)

st.title("📊 台股 Aggregation Layer demo")
st.caption("M3 即時請求路徑層 · 並排呈現,不整合 · 對齊 m3Spec/aggregation_layer.md r1")

# ────────────────────────────────────────────────────────────
# Sidebar:查詢參數
# ────────────────────────────────────────────────────────────

with st.sidebar:
    st.header("查詢參數")
    stock_id = st.text_input("Stock ID", value="2330", help="例 2330 / 2317 / 0050")
    as_of_date = st.date_input("As of 日期", value=date.today() - timedelta(days=1))
    lookback_days = st.slider("Lookback 天數", 30, 365, 90)
    include_market = st.checkbox("並排 market-level facts", value=True)

    st.markdown("---")
    st.markdown("**保留字 stock_id**")
    st.code(
        "\n".join(
            [
                "_index_taiex_      TAIEX",
                "_index_us_market_  SPY / VIX",
                "_index_business_   景氣指標",
                "_market_           市場層級籌碼",
                "_global_           匯率 / fear_greed",
            ]
        ),
        language="text",
    )

    fetch_clicked = st.button("🔎 撈資料", type="primary", use_container_width=True)


# ────────────────────────────────────────────────────────────
# 撈資料 + 渲染
# ────────────────────────────────────────────────────────────

if not fetch_clicked:
    st.info("👈 設定參數後按「撈資料」")
    st.stop()


@st.cache_data(ttl=300)
def fetch_snapshot(stock_id: str, as_of_date: date, lookback_days: int, include_market: bool):
    """走 agg.as_of() 撈資料(快取 5 分鐘)。"""
    snap = as_of(
        stock_id,
        as_of_date,
        lookback_days=lookback_days,
        include_market=include_market,
    )
    return snap.to_dict()


try:
    snapshot_dict = fetch_snapshot(stock_id, as_of_date, lookback_days, include_market)
except Exception as e:
    st.error(f"❌ 撈資料失敗:{e}")
    st.exception(e)
    st.stop()


facts = snapshot_dict["facts"]
indicator_latest = snapshot_dict["indicator_latest"]
structural = snapshot_dict["structural"]
market = snapshot_dict.get("market", {})


# ────────────────────────────────────────────────────────────
# Header 摘要
# ────────────────────────────────────────────────────────────

col1, col2, col3, col4 = st.columns(4)
col1.metric("個股 facts", len(facts))
col2.metric("Indicator cores", len(indicator_latest))
col3.metric("Structural snapshots", len(structural))
total_market = sum(len(v) for v in market.values())
col4.metric("Market facts", total_market)


# ────────────────────────────────────────────────────────────
# 個股 facts 時間軸(主區塊)
# ────────────────────────────────────────────────────────────

st.markdown("---")
st.subheader(f"📌 {stock_id} 個股 facts 時間軸")

if not facts:
    st.warning(f"{stock_id} 在 {as_of_date} 往前 {lookback_days} 天內無 fact")
else:
    try:
        import pandas as pd

        df = pd.DataFrame(facts)
        # 抽 metadata.kind / value
        if "metadata" in df.columns:
            df["kind"] = df["metadata"].apply(lambda m: (m or {}).get("kind"))
            df["value"] = df["metadata"].apply(lambda m: (m or {}).get("value"))
        st.dataframe(
            df[["fact_date", "source_core", "statement", "kind", "value", "timeframe"]]
                .sort_values("fact_date", ascending=False),
            use_container_width=True,
            height=400,
        )

        # source_core 觸發次數 chart
        st.bar_chart(df["source_core"].value_counts())
    except ImportError:
        st.dataframe(facts, use_container_width=True)


# ────────────────────────────────────────────────────────────
# Indicator 最新值表
# ────────────────────────────────────────────────────────────

st.markdown("---")
st.subheader("📈 各 Indicator Core 最新值")

if not indicator_latest:
    st.warning(f"{stock_id} 無 indicator_values 資料(可能 cores 還沒寫入或 stock 不在 backfill 範圍)")
else:
    indicator_rows = []
    for key, ind in indicator_latest.items():
        value = ind.get("value", {}) or {}
        # series JSONB 的 indicator(macd / rsi 等)從 series 取最後一筆
        latest_point = None
        if isinstance(value.get("series"), list) and value["series"]:
            latest_point = value["series"][-1]

        indicator_rows.append({
            "core@timeframe": key,
            "value_date": ind.get("value_date"),
            "series_len": len(value.get("series", [])) if isinstance(value.get("series"), list) else None,
            "latest": str(latest_point)[:200] if latest_point else "(無 series)",
        })

    st.dataframe(indicator_rows, use_container_width=True)


# ────────────────────────────────────────────────────────────
# Structural snapshot(neely scenario forest 等)
# ────────────────────────────────────────────────────────────

st.markdown("---")
st.subheader("🌳 Structural Snapshots(neely scenario forest)")

if not structural:
    st.warning(f"{stock_id} 無 structural snapshot")
else:
    for key, snap_row in structural.items():
        with st.expander(f"{key} @ {snap_row.get('snapshot_date')}", expanded=False):
            snapshot_data = snap_row.get("snapshot", {})
            # neely_core 有 scenario_forest + diagnostics
            forest = snapshot_data.get("scenario_forest", [])
            diag = snapshot_data.get("diagnostics", {})

            if forest:
                st.metric("Forest size", len(forest))
                # 印第 1 個 scenario
                top = forest[0]
                st.json(top, expanded=False)
            if diag:
                st.markdown("**Diagnostics**")
                st.json(diag, expanded=False)


# ────────────────────────────────────────────────────────────
# 市場環境並排
# ────────────────────────────────────────────────────────────

if include_market and market:
    st.markdown("---")
    st.subheader("🌐 市場環境 facts 並排")

    cols = st.columns(min(len(market), 3))
    for i, (sid, market_facts) in enumerate(market.items()):
        col = cols[i % len(cols)]
        with col:
            st.markdown(f"**{sid}** ({len(market_facts)} facts)")
            if market_facts:
                # 印最新 5 筆
                top5 = market_facts[:5]
                for f in top5:
                    md = f.get("metadata", {}) or {}
                    kind = md.get("kind", "")
                    st.caption(f"`{f['fact_date']}` `{f['source_core']}` **{f['statement']}** {kind}")
            else:
                st.caption("(無 fact)")


# ────────────────────────────────────────────────────────────
# Raw JSON dump(debug)
# ────────────────────────────────────────────────────────────

with st.expander("🔧 Raw JSON snapshot(debug)"):
    st.json(snapshot_dict, expanded=False)
