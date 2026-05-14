# dashboards/ — Streamlit 視覺化 Dashboard(Phase B-3 + C 視覺化主幹)

對齊 m3Spec/aggregation_layer.md r1 + plan
`/root/.claude/plans/squishy-foraging-stroustrup.md`(已 archive)。

## 安裝

```bash
pip install -e ".[dashboard]"   # 拉 streamlit + pandas + plotly
```

## 用法

```bash
# 從 repo root 啟動
streamlit run dashboards/aggregation.py
```

開啟瀏覽器(預設 http://localhost:8501),sidebar 設定:
- **Stock ID** — 例 2330 / 2317 / 0050
- **As of** — 查詢日期(回測 + 即時共用此參數)
- **Lookback days** — 期間天數(預設 90)
- **K-line layers / Indicator subplots** — 各 layer 顯隱 toggle

按 **「撈資料」** 後,6 tabs 分別呈現:

## 6 tabs 內容

### 📈 Tab 1:K-line(主圖 + 6 indicator subplots)

Plotly 多 row subplot,shared x-axis,TradingView 風格:

- **Row 1 主圖**:Candlestick + 疊圖
  - Bollinger band(upper/lower fill)
  - MA 多線(`ma_core.series_by_spec` SMA20/60/200/EMA/WMA/...)
  - Neely zigzag(monowave_series 連線 + wave label arrow)
  - Facts markers(diamond,color by `metadata.kind`)
- **Row 2 Volume**:成交量 bar(漲綠跌紅)
- **Row 3-8 Indicator subplots**:
  - MACD(line + signal + histogram bar)
  - RSI(1 line + 70/30 reference)
  - KD(K + D + 80/20 reference)
  - ADX(+DI / -DI + 25 reference)
  - ATR / OBV(可選)
- **Facts overlay**:macd 事件標 MACD subplot,rsi 事件標 RSI subplot,
  其他 cores(neely / chip / fund / env)標主圖 row 1

### 💰 Tab 2:Chip 籌碼

5-row subplot:
- Row 1:K-line reference
- Row 2:外資 / 投信 / 自營 net 三色 bar(`institutional_core`)
- Row 3:融資 / 融券 餘額 line(`margin_core`)
- Row 4:外資持股 % + 持股上限 reference(`foreign_holding_core`)
- Row 5:當沖比 area + 大戶/中戶/散戶 stack area(`day_trading_core` + `shareholder_core`)

對應 chip cores 的 facts markers 標到對應 row。

### 📊 Tab 3:Fundamental 基本面

3 個 sub-tabs:
- **月營收** — `revenue_core` revenue bar + YoY/MoM line(twin axis)
- **估值 percentile** — `valuation_core` per/pbr/yield 三 row + 5y 高/低位 zone hrect
- **財報季頻** — `financial_statement_core` EPS bar + 收入/毛利/淨利 line + 完整欄位 raw table

### 🌐 Tab 4:Environment 環境

3 個 sub-tabs:
- **TAIEX/TPEx** — `taiex_core.series_by_index` 雙序列(close + RSI)
- **美股 + VIX** — `us_market_core` SPY + VIX twin axis + VIX zone vrect 背景色
- **全球指標** — 4 個 gauge / chart 並排:
  - Fear-Greed gauge(0-100,5 zone color)+ 30 天時序
  - 融資維持率 dial(100-200,Safe/Warning/Danger)
  - USD/TWD 匯率 line + MA
  - 景氣指標(月頻)4 line + 對策信號 5-color bar

### 🌳 Tab 5:Neely Wave deep-dive

- **Scenario picker**(selectbox)— 列 forest 內所有 scenarios(顯示 id / 浪數 / power_rating)
- **K-line + zigzag**:選定 scenario 的 monowave 連線 + wave label 箭頭標註
- **Fibonacci zones hrect**(可關)
- **Diagnostics expander**:forest_size / stage_elapsed_ms / rejections

### ⭐ Tab 6:Facts 散點雲(對齊 user 「星雲圖」需求)

獨立 figure scatter:
- x = `fact_date`
- y = `source_core`(23 cores rows)
- color = `metadata.kind`(同 kind 永遠同色,雜湊 palette)
- size = abs(`metadata.value`)— size 8 預設,有 value 時放大
- hover 顯示 statement / kind / value
- multiselect filter source_core(空 = 全部)

## 環境

需 `DATABASE_URL` 設定(`.env` 或環境變數),對齊 `src/agg/_db.py:get_connection()`。

快取 5 分鐘(走 `@st.cache_data(ttl=300)`),改參數會 refetch。

## Spec 對應

| Spec 段 | Dashboard 對應 |
|---|---|
| `aggregation_layer.md` §四 `as_of()` API | `agg.as_of_with_ohlc()` 一次撈 snapshot + ohlc |
| `aggregation_layer.md` §六 Look-ahead bias | `agg._lookahead.filter_visible()` 已過濾,圖直接消費 |
| `aggregation_layer.md` §七 跨 stock 並排 | facts 散點雲 + market metrics |
| `aggregation_layer.md` §九 use cases | 6 tabs 各對映 use case |
| `cores_overview.md` §九 並排呈現 | charts 各 helper 不做跨 core 整合 |
| `cores_overview.md` §十一 跨指標訊號 | TTM Squeeze 等不在本 dashboard;UI 提示 |

## 模組布局

```
dashboards/
├── aggregation.py             # 主入口(6 tabs + sidebar)
├── charts/                    # Plotly figure builders
│   ├── _base.py               # palette + make_kline_subplots + helpers
│   ├── candlestick.py         # build_kline_figure
│   ├── overlays.py            # add_ma_lines / add_bollinger_band / add_neely_zigzag
│   ├── indicators.py          # 6 indicator subplots(macd/rsi/kd/adx/atr/obv)
│   ├── chip.py                # 5 chip subplots + build_chip_figure
│   ├── fundamental.py         # revenue / valuation / financial_statement
│   ├── environment.py         # 6 environment cores(taiex/us/er/fg/mm/bi)
│   ├── neely_wave.py          # list_scenarios / build_neely_deep_dive / render_diagnostics
│   └── facts_cloud.py         # build_facts_scatter / add_facts_to_kline
└── README.md                  # 本檔
```

## 限制(MVP scope)

對齊 plan 「不做」清單:

- ❌ **跨指標訊號合成**(TTM Squeeze 等)— 違反零耦合;教學文件 / UI 提示層處理
- ❌ **個股之間 pair compare**(2330 vs 2317 對比)— spec §7.3 不交叉
- ❌ **多用戶 / auth / SSL deploy** — 屬 Phase B-4 FastAPI 對外網站化
- ❌ **即時推播 / 警報** — 屬 notification service
- ❌ **使用者下單 / 模擬交易**

## 性能

- 單股 90 天:facts ~500-2000 rows,撈資料 ~100ms
- Plotly figure render:~500ms-2s(視 trace 數)
- `@st.cache_data(ttl=300)` 5 分鐘快取避免重撈

## 已知 graceful degrade

- 各 indicator core 若無資料 → 該 subplot 不畫(skip,不 raise)
- shareholder_core(週頻)走 `@weekly` key
- revenue_core / business_indicator_core(月頻)走 `@monthly` 或 daily fallback
- financial_statement_core(季頻)走 `@quarterly` 或 daily fallback
- neely structural 為空 → 顯示 placeholder
- 未啟用的 indicator subplot,該 core 的 facts 標到主圖 row 1
