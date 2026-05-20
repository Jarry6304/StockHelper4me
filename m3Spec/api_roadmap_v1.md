# StockHelper4me API 規劃雛形 v1.0

> **版本**:v1.0
> **日期**:2026-05-20
> **狀態**:✅ 已由 Fusion Layer 落地(2026-05-20,P0+P1+P2)— 落地紀錄見
> `CLAUDE.md` §「Fusion Layer — API 規劃落地」+ `m3Spec/fusion_layer.md`。
> **基準**:對齊 `m3Spec/cores_overview.md` v2.0 r5、`m3Spec/fusion_layer.md`、`mcp_server/server.py`
> **目的**:從「對使用者的價值面向」反推 API 設計,補完目前 MCP 暴露的盤口

## 目錄

1. [規劃背景](#一規劃背景)
2. [五大視角總覽](#二五大視角總覽)
3. [視角 A:個股健診](#三視角-a個股健診)
4. [視角 B:波浪/趨勢預測](#四視角-b波浪趨勢預測)
5. [視角 C:跨股篩選](#五視角-c跨股篩選)
6. [視角 D:市場環境](#六視角-d市場環境)
7. [視角 E:技術指標組合](#七視角-e技術指標組合)
8. [視角間的協作流程](#八視角間的協作流程)
9. [設計原則與一致性約束](#九設計原則與一致性約束)
10. [實作優先級](#十實作優先級)
11. [未來擴充](#十一未來擴充)

---

## 一、規劃背景

### 1.1 現況盤點

**Cores 層(計算層)**:已完整(36+ 個 Core,對齊 `cores_overview.md` §8)。
計算層飽和、API 層稀疏 —— 大量已建好的維度沒對外曝露。

### 1.2 規劃方法

不從「哪些 Core 該暴露」反推,改從**使用者實際決策流程**正推,抽象出 5 個視角
(A~E),每個視角對應一類使用者意圖。

---

## 二、五大視角總覽

| 視角 | 名稱 | 對應使用者意圖 |
|---|---|---|
| **A** | 個股健診 | 「這檔股票現在狀況如何?」 |
| **B** | 波浪/趨勢預測 | 「接下來會怎麼走?」 |
| **C** | 跨股篩選 | 「我該買哪幾檔?」 |
| **D** | 市場環境 | 「現在能不能進場?」 |
| **E** | 技術指標組合 | 「給我指標數值我自己判斷」 |

工具數:現有 8 個 MCP tool → 目標 18(A 擴充內容、B +3、C 維持、D +2、E +5)。

---

## 三、視角 A:個股健診

回答「**這檔股票現在狀況如何?**」。採方案 A:擴充現有 `stock_snapshot`,從
6-in-1 升級為 **10-in-1**。

涵蓋 4 大面向:基本面(fundamentals)、技術面(technical_summary)、籌碼面
(institutional + loan + block + shareholder + day_trading + risk)、環境面
(market_context + commodity)。

新增 4 個 section:`fundamentals` / `institutional` / `shareholder` /
`technical_summary`。單一 tool 不拆;graceful degradation;只給結論性數值,
要 series 走 E 視角。

---

## 四、視角 B:波浪/趨勢預測

回答「**接下來會怎麼走?關鍵價位在哪?該設什麼止損?**」。

| 工具 | 功能 |
|---|---|
| `neely_forecast` | NEoWave 波浪預測(既有) |
| `kalman_trend` | 1-D Kalman trend + 5-class regime(既有) |
| `key_levels` | 支撐壓力 + Fib + 趨勢線整合(新) |
| `pattern_scan` | K 線型態識別 + context(新) |
| `stop_loss_calc` | ATR/支撐位/Fib 止損止盈計算(新) |

`key_levels` 是 B 的中心;`pattern_scan` 吃 `key_levels` context;`stop_loss_calc`
是純計算 wrapper,只整合既有來源不引入新規則。

---

## 五、視角 C:跨股篩選

回答「**我該買哪幾檔?**」。維持現狀 5 個工具:`magic_formula_screen` /
`monthly_screen` / `quarterly_screen` / `annual_low_risk_screen` /
`monthly_trigger_scan`。D 視角的市場狀態應驅動使用哪一個 C toolkit。

---

## 六、視角 D:市場環境

回答「**現在能不能進場?整體市場狀態?**」。

設計哲學:純資料、零主觀規則、無 LLM、程式判斷、output 對齊 `cores_overview.md`
§7.5 同構規範。

雙 API:
- `market_dashboard(date)` — 每個 metric 配「值 + 歷史百分位 + 短期變化」。
- `market_events(start_date, end_date, severity_min)` — event 時間軸,統一 schema
  `{date, source, kind, severity, value, metadata}`,severity 分 4 級
  `info / notable / warning / critical`。

D 驅動 C(risk_off → 防禦;risk_on → 動能);獨立於 B。

---

## 七、視角 E:技術指標組合

回答「**給我指標數值,我自己判斷**」。對齊 cores 4 個子類規格各拆一個 tool +
一個 preset 整合 tool:

| 工具 | 對應子類 |
|---|---|
| `indicator_momentum` | 動量/趨勢/強度(9 cores) |
| `indicator_volatility` | 波動/通道(4 cores) |
| `indicator_volume` | 量能(3 cores) |
| `indicator_pattern` | 型態/價位(3 cores) |
| `indicator_stack` | preset 整合 + 自訂 |

統一介面:`indicator_<category>(stock_id, date, indicators, lookback)` → 對齊
cores §7.5 的 `{series, events}`。`indicator_stack` preset:`default` /
`day_trade` / `swing` / `position`。

---

## 八、視角間的協作流程

典型決策流程:D(市場環境)→ 影響 C 的選擇 → C 篩出個股 → A(健診)確認 →
B(前瞻)進出場 → E(細節)自訂技術分析。每個視角都可獨立呼叫,以上是最佳實踐。

---

## 九、設計原則與一致性約束

| # | 原則 |
|---|---|
| 1 | 強制 as_of date(回測 / 即時同介面) |
| 2 | Look-ahead bias 防衛(沿用 aggregation_layer / fusion.raw 機制) |
| 3 | Graceful degradation(sub-section 互不影響) |
| 4 | 無 LLM 在資料層 |
| 5 | Output schema 對齊 cores §7.5(`series + events` 同構) |

### 9.2 跨視角共用結構

- **Event 統一 schema**:`{date, source, kind, severity, value, metadata}`。
- **Percentile 規範**:預設視窗 1 年(252 交易日);命名 `{metric}_percentile_{window}`。

### 9.4 與既有架構的相容性

新工具走 MCP;不新增 Core,允許對既有 cores 加欄位 / 補 EventKind(Fusion Layer
落地時鬆綁此條)。

---

## 十、實作優先級

| 優先級 | 範圍 |
|---|---|
| P0 | 補完 A 視角(`stock_snapshot` 擴充 4 sections) |
| P1 | E 視角全面 + B 視角補 3 工具 |
| P2 | D 視角上線 |
| P3 | B 視角追蹤類(`forecast_track` 等) |
| P4 | 視角 F(族群分析) |

> **落地實況(2026-05-20)**:P0~P2 已由 Fusion Layer 一次落地(見 `fusion_layer.md`
> §11)。視角 F(族群分析)留未來。

---

## 十一、未來擴充

### 11.1 視角 F:族群分析(未 LOCK)

待解決:族群歸類資料來源(人工 / NLP / 統計聚類)、族群層級設計(L1 產業別 /
L2 概念股)。可能工具:`sector_snapshot` / `sector_ranking` / `sector_rotation`。

### 11.3 不會做的方向

API 層加 LLM、主觀標籤化 D 視角、預測「該買該賣」、即時 streaming。

---

## 十二、附錄

### 12.1 工具總覽(最終 18 個)

8 既有(`neely_forecast` / `kalman_trend` / `magic_formula_screen` /
`stock_snapshot` / `monthly_screen` / `quarterly_screen` /
`annual_low_risk_screen` / `monthly_trigger_scan`)+ 10 新(`market_events` /
`market_dashboard` / `key_levels` / `stop_loss_calc` / `pattern_scan` /
`indicator_momentum` / `indicator_volatility` / `indicator_volume` /
`indicator_pattern` / `indicator_stack`)。

> 註:本文 §2 / §七曾述「+9 新」,實際枚舉為 **10 新**(B:3、D:2、E:5)—
> headline 的 off-by-one 已於 Fusion Layer 落地時校正為 10。

### 12.3 修訂歷史

| 版本 | 日期 | 內容 |
|---|---|---|
| v1.0 | 2026-05-20 | 初稿,5 視角規劃;同日由 Fusion Layer P0+P1+P2 落地 |
