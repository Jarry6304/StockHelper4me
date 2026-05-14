# dashboards/ — Streamlit demo

Aggregation Layer Phase B-3 視覺化 demo,對齊 m3Spec/aggregation_layer.md r1。

## 安裝

```bash
# 主套件 + Streamlit
pip install -e .
pip install streamlit pandas
```

## 用法

```bash
# 從 repo root 啟動
streamlit run dashboards/aggregation.py
```

開啟瀏覽器(預設 http://localhost:8501),sidebar 選股 + 日期 + lookback 後按
「撈資料」。

## 4 區塊呈現

1. **個股 facts 時間軸** — fact_date / source_core / statement / kind / value
   DataFrame + source_core 觸發次數 bar chart
2. **Indicator 最新值表** — 各 indicator core 最新一筆 indicator_values
3. **Structural snapshots** — neely scenario forest 摘要(forest size + 第 1 個 scenario)
4. **市場環境並排** — 5 個保留字 stock_id facts(`_index_taiex_` / `_index_us_market_` /
   `_index_business_` / `_market_` / `_global_`)

## 環境

需 `DATABASE_URL` 設定(`.env` 或環境變數),對齊 `src/agg/_db.py:get_connection()`。
快取 5 分鐘(走 `@st.cache_data(ttl=300)`),改參數重撈。

## 限制

MVP 範圍,**不**支援:
- 多用戶 / auth / SSL deploy(對外網站化屬 Phase B-4 未來工作)
- 圖表 interactive zoom / brush
- 跨股 pair 對比(對齊 spec §7.3 個股間不交叉)
- 即時推播

## Spec 對應

| Spec 段 | Dashboard 對應 |
|---|---|
| §四 `as_of()` API | `@st.cache_data` wrap `agg.as_of()` |
| §六 Look-ahead bias | `agg._lookahead.filter_visible()` 已過濾 |
| §七 跨 stock 並排 | 「市場環境」區塊 |
| §八 Output 結構 | `snap.to_dict()` 各區塊讀對應 key |
| §9.1 個股深度查詢 | 整頁的主流程 |
