# ADR 0001:TW-Market 處理職責歸 Silver 層

> **狀態**:Accepted
> **日期**:2026-05-06
> **取代**:`tw_market_core.md`(整份廢除)
> **關聯**:`cores_overview.md`、`layered_schema_post_refactor.md`、README 架構原則

---

## 一、背景

v1.x 規格曾規劃 `TW-Market Core` 作為 Cores 層的一員,負責台股市場特性處理(連續漲跌停合併、後復權、TAIEX neutral 閾值等),作為所有其他 Cores 的前置處理。

v2.0 重構期間 Schema(`layered_schema_post_refactor.md`)落地後揭示:

- 後復權計算已由 Silver S1_adjustment 的 Rust binary 產出 `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`
- 連續漲跌停合併由同一支 Rust binary 產出 `price_limit_merge_events`
- Wave Cores / Indicator Cores 的接點(Schema §8)直接讀 Silver 表,**不經過任何 Market Core**

換言之,原本 `TW-Market Core` 規劃的所有職責,在新架構中**全部由 Silver 層完成**。

---

## 二、決策

**完全廢除 `TW-Market Core`**,Cores 層不再存在 Market Core 類別。

具體執行:

1. `tw_market_core.md` 整份移除
2. 總綱 `cores_overview.md` 移除 `MarketCore` trait、Market Cores 章節、TW-Market 前置段落
3. Cores 層只剩 5 類:wave / indicator / chip / fundamental / environment
4. `cores_overview.md` 「不獨立成 Core 清單」新增一行:「TW-Market 處理 → 歸 Silver 層 S1_adjustment」

---

## 三、決策依據

### 3.1 違反「計算 / 規則分層原則」

依 README 架構原則:

- **Silver 層**:複雜計算(跨日狀態追溯、跨表 join、後復權倒推)
- **Cores 層**:規則 / 算式套用(SMA、Neely 規則、Fibonacci 投影)

原 `TW-Market Core` 三大職責全部屬「複雜計算」:

| 原職責 | 性質 | 該歸 |
|---|---|---|
| 連續漲跌停合併 | 跨日狀態追溯 | Silver |
| 後復權計算 | 倒推 multiplier、跨歷史 | Silver |
| TAIEX neutral 閾值 | 規則套用(屬 Neely Rule of Neutrality 子句) | Cores(Neely Core 內部) |

`TW-Market Core` 中沒有任何一段是「規則 / 算式套用」,留在 Cores 層違反分層原則。

### 3.2 已被 Schema 落地事實淘汰

Schema §8 接點清單已明確:

- wave_cores 接點直接讀 `price_daily_fwd`
- indicator_cores 接點直接讀 `price_daily_fwd` + `price_limit_merge_events`

無任何 Core 走「TW-Market Core 中介層」。保留 `TW-Market Core` 等於在事實已不存在的位置硬留空殼。

### 3.3 單一職責 / 可拔可插

總綱原則「Core 應單一職責、可拔可插」。`TW-Market Core` 在新架構下是空殼 —— 沒有規則套用、沒有算式計算,僅作為「資料中介」。中介職責由 Silver 層承擔即可,Core 層不必。

---

## 四、後果

### 4.1 正面

- Cores 層職責更純粹:只做規則 / 算式套用
- Silver 與 Cores 邊界清晰:複雜計算與規則計算分流
- 避免日後重蹈「漲跌停判斷加進 Neely Engine」這類違反單一職責的設計

### 4.2 負面

- Cores 層失去「換市場只需替換 Market Core」的擴充伏筆 —— 此考量改由 Silver 層承擔(見附錄 A)
- 既有對 `tw_market_core.md` 的引用需逐一修正(Neely §5 / §17 / §21.1、Traditional §2 / §7 / §8.1)

### 4.3 連帶處理

- TAIEX neutral 閾值 → 搬入 Neely Core §10.4(規則層)
- 多市場擴充考量 → 本 ADR 附錄 A 保留(架構伏筆)
- 漲跌停合併規則細節 / 後復權倒推算法 → 本 ADR 附錄 B 保留(設計溯源)

---

## 五、已棄用清單(承接自舊 `tw_market_core.md` §10)

以下設計於各版本曾被考慮,均已棄用,記錄於此防止重蹈:

| 棄用項 | 來源 | 棄用原因 |
|---|---|---|
| `[TW-MARKET]` Scorer 微調(`ext_type_prior_3rd`) | v1.1 Item 7.4 | 主觀加權,違反「忠於原作」 |
| `[TW-MARKET]` Scorer 微調(`alternation_tw_bonus`) | v1.1 Item 7.4 | 同上 |
| 漲跌停處理嵌在 Neely Engine | v1.1 Item 1.5 | 違反單一職責,Neely Core 不該知道台股 |
| 還原指數計算放在前端 | 早期討論 | 精度問題,前端不應做還原 |
| 在 Neely Core 內判斷加權指數 neutral 閾值 | v1.1 隱含 | 已修正:歸 Neely Core 但作為**規則參數**,而非台股市場特性處理 |
| `TW-Market Core` 作為 Cores 層的 Market Core | v1.x / v2.0 r1 | **本 ADR 棄用**:所有職責屬複雜計算,歸 Silver 層 S1 |

---

## 附錄 A:多市場擴充考量(架構伏筆)

### A.1 設計伏筆

雖然當前(v2.0)僅支援台股,Silver S1 的命名(`tw_market` 處理邏輯)暗示未來可能有 `us_market` / `hk_market` 等對應其他市場的前處理 binary。

### A.2 多市場架構草案(P3+)

```
Silver 層
   ├── S1_tw_adjustment(Rust binary)        ← 台股漲跌停 + 後復權
   ├── S1_us_adjustment(Rust binary,P3+)    ← 美股 split / dividend
   └── S1_hk_adjustment(Rust binary,P3+)    ← 港股
```

各市場 binary 各自實作,輸出 schema 同形(`price_daily_fwd` 等),Cores 層完全不知道市場差異。

### A.3 注意事項

- **不為「自動偵測市場」設計邏輯** — 由 collector 配置明確指定
- **不在同一 Pipeline 混用多市場** — 一次 Pipeline 處理一個市場的一檔股票
- **跨市場比較**屬於 Aggregation Layer 的並排呈現範疇,不在 Silver 或 Cores 層處理

---

## 附錄 B:Silver S1 處理規則細節(設計溯源)

> 本附錄記錄 Silver S1 binary 的處理邏輯設計意圖。具體實作細節以 Rust 程式碼為準,本附錄為「為什麼這樣做」的溯源紀錄。

### B.1 連續漲跌停合併規則

**意圖**:漲跌停期間市場流動性異常,K 線形狀不反映真實供需。Wave Cores 若不合併會將其視為獨立波段,造成誤判。

**規則**:

- 連續 N 個交易日 `close == limit_up_price` → 合併為單一 K
- 合併 K 的 OHLC:`open` = 第一日 open,`high` = max(all highs),`low` = min(all lows),`close` = 最後一日 close
- `volume` = sum(all volumes)
- 合併事件寫入 `price_limit_merge_events`,標記 `merge_type` 與 `detail`(含合併日數、起訖日)

**為何在 Silver 而非 Cores**:屬「跨日狀態追溯」,符合「複雜計算歸 Silver」原則。

### B.2 後復權倒推算法

**意圖**:除權息事件造成價格不連續,需還原為連續價格序列以利分析。

**選擇後復權(Backward)而非前復權(Forward)**:

- 後復權:保留現價,歷史價往下調 → 現價直觀(投資人看得懂)
- 前復權:保留歷史價,現價往上調 → 歷史價直觀(技術分析者看得懂)
- 本專案選後復權,因 Cores 層分析以「當前狀態 + 歷史趨勢」為主,現價直觀較符合使用情境

**算法**:

- 從最新交易日往回,逐除權息事件調整歷史價
- 還原比率來自 `price_adjustment_events.before_price / reference_price`
- 累計還原因子寫入 `price_daily_fwd.cumulative_adjustment_factor`,可反推 raw price
- 累計成交量因子(split 後)寫入 `cumulative_volume_factor`

**為何在 Silver 而非 Cores 或前端**:

- 跨歷史倒推,屬「跨日狀態追溯」
- Rust 端負責保證精度;前端做還原會有浮點精度問題

### B.3 `_fwd` 命名語意

`_fwd` 在本專案 = **forward processed**(後復權後的處理結果),**不是**金融慣例的 `forward adjusted`(前復權)。

此命名為歷史既定事實,不更動。新進開發者請以本 ADR 為準。

---

## 附錄 C:遷移檢查清單

廢除 `tw_market_core.md` 後,以下檔案已連帶修正:

- [x] `cores_overview.md` — 移除 MarketCore trait / Market Cores 章節 / §4.4 前置段
- [x] `neely_core.md` — §5 / §17 / §21.1 改讀 Silver `price_daily_fwd`;§10.4 補 TAIEX neutral 閾值
- [x] `traditional_core.md` — §2 / §7 / §8.1 改讀 Silver
- [x] `layered_schema_post_refactor.md` — §1.5 引用 README 架構原則
- [x] `tw_market_core.md` — 整份移除
