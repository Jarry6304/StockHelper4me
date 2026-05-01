# Cores 全系列規格對齊審查 — 統合報告 r2(邏輯修正版)

> **整合來源**:三份分區審查報告(審查範圍互不重複)
> **整合日期**:2026-05-01
> **版本**:r2(邏輯檢查修正版,修正 r1 中 13 處邏輯/引用/數量錯誤)
> **共同對齊基準**:`cores_overview.md` v2.0 r1 + `collector_rust_restructure_blueprint_v3_2.md` r1
> **整合方法**:交叉比對 → 依賴關係分析 → 衝突合併 → 阻塞項排序 → overview/blueprint 修正清單

---

## 〇、三份報告的審查邊界

| 報告 | 審查 Core 範圍 | Core 數 |
|---|---|---|
| **R1** `cores_alignment_review_1.md` | `chip_cores` / `fundamental_cores` / `environment_cores` | 13 |
| **R2** `spec_alignment_audit_v1.md` | `indicator_cores_momentum / volatility / volume / pattern` | 19 |
| **R3** `spec_alignment_review_v2_0_r1.md` | `tw_market_core` / `neely_core` / `traditional_core` | 3 |
| **共同對齊基準** | `cores_overview.md` + `collector_blueprint_v3.2` | — |

✅ **三份報告審查焦點 Core 完全不重複**,合計 35 個 Core(此數為三報告的審查焦點總和,並非全系統 Core 數;overview §8 列出全系統 30 個 Core,本次未涵蓋:Indicator Cores 17 個內部演算法細節、Wave Cores 內部規則、System Cores 等)。但**共同對齊到同一份 overview 與 blueprint**,因此可在共同基準層做交叉驗證。

---

## 一、TL;DR — 跨報告嚴重度總表

### 1.1 🔴 P0 動工前阻塞項(去重後共 7 項)

> **r1 修正**:r1 列 8 項時將 P0-2(A-V3)與 P0-7(Volume 還原)分列,但兩者實為同一阻塞鏈的兩面(A-V3 結論未出 → Volume 還原語意才無法定),合併為 P0-2。

| # | 項目 | 來源報告 | 影響範圍 | 依賴 |
|---|---|---|---|---|
| **P0-1** | **K-1**:`chip_cores.MarginPoint.margin_maintenance` 未移除(含 enum 與 Fact 範例不一致) | R1 §2.1(實質確認);R2 §2.3 僅標「無法驗證」,**非實質指出**,僅作旁證 | `chip_cores.md` §4.4-§4.5 | 獨立 |
| **P0-2** | **A-V3 + 連動 OHLCVSeries / Volume 還原**:`price_daily_fwd.volume` 是否已調整未驗證,連帶 OHLCVSeries 缺欄位、TW-Market Core volume 合併雙重失真風險 | R2 §2.1 + R3 §A4 | volume Cores 全部 + `OHLCVSeries` schema + TW-Market Core §五 5.1 | **上游阻塞**,影響 P0-3 部分連動 |
| **P0-3** | **`_fwd` 表職責邊界**:Silver 已做後復權 vs TW-Market Core §五宣稱要做 | R3 §A1 | D2(`tw_market_core.md`) §五 / §九 | 部分依賴 P0-2 結論 |
| **P0-4** | **Trait 簽名矛盾**:`compute(ohlcv: &OHLCVSeries)` 與 12 個非 OHLCV Core 衝突 | R1 §2.2 | overview §3 + 三子類 §2.1 | 獨立 |
| **P0-5** | **`price_len` 度量空間未定義**(百分比/絕對/log)+ base price 不明 + 還原版本不明 | R3 §A3 | D3 / D4;與 P0-2 在「還原版本」維度有交集 | 部分依賴 P0-2 |
| **P0-6** | **Forest overflow 用 power_rating 排序**違反「不選 primary」哲學 | R3 §A5 | D3 §6.3 + Output schema | 獨立 |
| **P0-7** | **dirty queue 觸發契約缺**:Core 不知道何時該重算 | R2 §2.2 | overview §七 | 獨立 |

> **真實依賴關係**(r1 寫的「依修正依賴順序排列」過度簡化):
> ```
> P0-2 (A-V3) ──┬─→ P0-3 (_fwd 職責中 volume 處理連動)
>               └─→ P0-5 (還原版本連動)
>
> P0-1 / P0-4 / P0-6 / P0-7  彼此獨立,可並行修正
> ```

### 1.2 🟠 P1 一致性與完整度問題(r1 列 14 項,扣除可關閉與重複後實質 12 項)

> **r1 修正**:
> - P1-9(R2 §2.4)應為「**部分關閉**」而非完全關閉:R1 雖補上規格審查,但未做欄位級對齊(`valuation_core` 是否消費新增 `market_value_weight` 等)
> - r1 P1-4(`produce_facts` 無示範)已併入 P0 修正集中包(共識點 ③),不再獨立列為 P1

| # | 項目 | 來源 | 修正位置 | 狀態 |
|---|---|---|---|---|
| P1-1 | `revenue_core.warmup_periods` 月份/K 棒單位混亂 | R1 §3.1 | overview §7.3(若採 P0-4 associated type 則自動解) | 條件性 |
| P1-2 | `_market_` / `_global_` stock_id 保留值對照表缺失 | R1 §3.2 | overview 或 environment_cores | 獨立 |
| P1-3 | `taiex_core` 資料源不明(`market_index_tw` vs `market_ohlcv_tw`) | R1 §3.3 | environment_cores §3.1 | 獨立 |
| P1-4 | `shareholder_core` 週頻 = 「事件型」第三類,overview §7.2 沒列 | R1 §3.5 | overview §7.2 | 獨立 |
| P1-5 | `institutional_market_daily` 處理盲點(全市場法人) | R1 §3.6 | overview §十 | **與 P0-6 共構 Core 邊界三原則** |
| P1-6 | 子類規格「對應資料表」全寫 Bronze 表名,應為 Silver `*_derived` | R1 §4.4 | 三子類 §一 表格 | 獨立 |
| P1-7 | **MergeAtLimitPrice 策略無書面定義**(三選一砍或補) | R3 §A2 | D2 §五 | 獨立 |
| P1-8 | **Fact 極值類事實**歸屬模糊(5y low / 1y high) | R2 §3.5 | overview 新增 §6.5 第 4 條 | **併入 P0 共識點 ③ 集中修正** |
| P1-9 | params_hash 寫入 `indicator_values` schema 待確認 | R2 §3.2 | blueprint 附錄 B DDL | 獨立 |
| P1-10 | Pipeline 級暖機區計算規則未明定 | R2 §3.4 | overview §7.3 | 獨立 |
| P1-11 | **R15 與 R15-Ramki RuleId 撞號**(P3 前修) | R3 §A6 | D4 附錄 A + 新增 §十 | 獨立 |
| P1-12 | **Combined 模式與 params_hash 去重衝突** | R3 §A7 | D4 §三 + overview §6.5 第 7 條 | **併入 P0 共識點 ③** |
| (P1-rem) | **Fundamental / Environment 規格欄位級對齊未驗** | R2 §2.4 殘餘 | 三子類各 Core 對 blueprint Silver 欄位 | **R1 未涵蓋,r2 補列** |
| (合併) | `produce_facts` 在 13 個子類 Core 全部沒示範 | R1 §3.4 | overview §6.5 第 1 條 | **併入 P0 共識點 ③** |

### 1.3 🟡 P2 措辭/順序/命名不一致(共 11 項)

完整列於 §五。

---

## 二、跨報告交叉驗證重點(統合的核心價值)

### 2.1 **共同指向 overview/blueprint 同一節的問題**

#### 共識點 ①:`OHLCVSeries` / Volume 還原 是跨子類共同隱憂

| 指出問題的報告 | 角度 |
|---|---|
| R2 §2.1 | Volume Indicator Core 假設 volume 已調整,但 A-V3 未驗 |
| R3 §A4 | TW-Market Core 的 `OHLCVSeries` 缺 `cumulative_adjustment_factor` / `volume_adjusted` 欄位 + volume 合併雙重失真風險 |
| R3 §A1 | `_fwd` 表的後復權處理職責歸屬 Silver 還是 TW-Market Core |

**兩份報告的交集**(r1 誤寫「三份報告交集」,實為 R2+R3 共識,R1 因不審 indicator/wave 故未交集):`price_daily_fwd` 的 volume 與後復權語意未明確 → 影響 9 個 indicator Core(R2 範圍 — 實為 obv/vwap/mfi/bollinger 共 4 個,r1 寫 9 過度)+ 1 個 TW-Market Core(R3 範圍)= 共 5 個 Core 直接受影響。

**統合結論**:**A-V3 是 P0 階段最大的單點阻塞**,卡住 5 個 Core 的 schema 與計算正確性。建議**先排 A-V3 驗證 → 再決定 OHLCVSeries 欄位 → 再決定 Volume Core 與 TW-Market Core 對齊路徑**。

---

#### 共識點 ②:overview §3 trait 簽名問題

> **r1 邏輯錯誤修正**:r1 寫「R2 §2.4 隱含同類問題」屬無中生有,R2 §2.4 實際只談 chip/fundamental/environment 規格未送審,完全不涉及 trait 簽名。已從共識點 ② 移除 R2 引用。

| 指出問題的報告 | 角度 |
|---|---|
| R1 §2.2 | 12 個非 OHLCV Core(chip 5 + fundamental 3 + environment 4)用不到 OHLCV 參數 |
| R3 §B2 | WaveCore trait 應與 IndicatorCore trait 並列(D1 §3.3 已支援);Neely vs Traditional 對 trait 固化時點認知不一 |

**統合判斷**:overview §3 的單一 trait「一統天下」設計與實際 Core 多樣性不相容。R1 提出 3 個方案中,**方案 A(associated type)** 最乾淨,且能與 R3 §B2 主張的「Wave/Indicator 各自 trait」並存。

**最終建議**:
```rust
pub trait CoreCompute: Send + Sync {
    type Input;       // OHLCVSeries / RevenueSeries / ChipSeries / WaveContext / ...
    type Params: Default + Clone + Serialize;
    type Output: Serialize;
    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;
    fn warmup_periods(&self, params: &Self::Params) -> usize; // 單位由 Input 型別決定
}
```

`IndicatorCore` 改為 `type Input = OHLCVSeries` 的特化別名,`WaveCore` / `ChipCore` / `FundamentalCore` / `EnvironmentCore` 各自實作。

**此修正同時連動解**:R1 §2.2(trait 矛盾)+ R1 §3.1(warmup 單位)+ R3 §B2(WaveCore 草案狀態)三個問題。

---

#### 共識點 ③:Fact / params_hash / dirty queue 是 M3 端的契約黑洞

| 指出問題的報告 | 角度 |
|---|---|
| R1 §3.4 | `produce_facts` 在 13 個子類 Core 全部無示範 |
| R1 §4.3 | Fact statement 含描述性詞彙(`short squeeze`、`large transaction`) |
| R2 §2.2 | dirty queue 觸發契約完全缺(Core 不知何時重算) |
| R2 §3.2 | `params_hash` 是否進 `indicator_values` schema 待確認 |
| R2 §3.5 | Fact 極值類事實歸屬模糊 |
| R3 §A7 | Combined 模式 params_hash 去重衝突 |

**統合判斷**:M3 寫入端的契約(facts / indicator_values 兩表)需要**集中補強一節**。建議在 overview **新增 §6.5「Fact / Indicator 寫入契約總則」**(注意:統合報告全文統一用 §6.5,r1 中曾誤標 §6.4),涵蓋:

1. `produce_facts` 通則(events 一對一轉 Fact,series 不轉但可作 metadata)
2. Fact statement 語言統一(建議統一英文 + 數字)
3. Fact statement 邊界判準(機械式重現,禁用 `short squeeze`/`large transaction` 等描述詞)
4. Fact 極值類歸屬(採 R2 §3.5 方案 C:極值偵測移出 Core,由 Aggregation Layer 處理)
5. params_hash 寫入規範(指向 blueprint 附錄 B DDL,須含 `params_hash TEXT NOT NULL` + index)
6. dirty queue 讀取契約(Orchestrator 讀 Silver dirty,Core 不感知;m3_compute 不回寫 dirty)
7. Combined 模式 params_hash 處理(採 R3 §A7 方案 1:拆 Params,Combined 視為兩次獨立 compute)

> **注意**:此 §6.5 屬 P0 修正(M3 寫入契約集中補強),會自動帶解 §1.2 中標記為「併入 P0 共識點 ③」的 P1 項目(P1-8 / P1-12 / 合併項 produce_facts)。**避免 P0/P1 雙重歸類混淆**。

這一節集中修正 7 個跨報告問題,**整合效益最高**。

---

### 2.2 **三份報告獨立指出但措辭不同的同一問題**

#### 同一問題 ①:Silver vs Bronze 表名命名不一

- R1 §4.4:子類規格寫的全是 Bronze 表名(`monthly_revenue`),Core 應該讀 Silver `*_derived`
- R2 §3.1:overview 與 indicator_cores_volume.md 用「raw 表」描述 `price_daily_fwd`,但它是 Silver
- R3 §A1:`_fwd` 表的層級歸屬被 D2 與 D5 認知不一致

**根因**:整個 Cores spec 系列在描述 Silver 表時**用語混亂**,出現 `raw 表` / 表名不加 `_derived` 後綴 / 層級認知不清三種症狀。

**統合修正**:
1. **全文檢查**所有 spec 中的「raw 表」用語,改為「Silver 表」或「Bronze 表」明確區分
2. **三子類 §一表格**全部加 `_derived` 後綴,並補一行「Core 讀取 Silver Layer 的 `*_derived` 表」
3. **TW-Market Core spec(D2)** 補 §9.2 明確切分 Silver vs runtime 處理(R3 §A1 建議)

---

#### 同一問題 ②:跨 Core 整合禁止/並排不整合 哲學的論述位置

- R1 §5.3:三子類最後一節都重論述「跨 Core 不整合」,內容相近措辭略異 → 建議統一抽到 overview §十一
- R3 §C1:三份波浪/市場 core 也都各自一致拒絕 confidence/score/probability/composite

**統合建議**:overview §十一(目前只談 TTM Squeeze)擴充為「跨 Core 整合禁止通則」總章,涵蓋 TTM Squeeze、籌碼集中度、Engine_T+Engine_N 等案例,各子類規格簡述「適用 overview §十一通則」即可。

---

### 2.3 **三份報告之間的「未交集但相鄰」問題**

#### 相鄰問題 ①:R3 的 forest overflow 與 R1 的 institutional_market_daily 都涉及「Core 何時不該獨立」

- R3 §A5 質疑:overflow 用 power_rating 排序就是 Core 在做選擇 → 違反不選 primary 哲學
- R1 §3.6 質疑:全市場三大法人買賣超沒有 Core 處理 → 是不是該立 Core?

兩者方向相反:
- R3:Core 內部做了不該做的選擇(內縮 Core 邊界)
- R1:應該由 Core 處理的訊息沒有 Core 處理(外擴 Core 邊界)

**統合啟發**:overview §十「不獨立成 Core 的清單」需要更明確的判定原則:

> **Core 邊界判定三原則**:
> 1. **可重現原則**:給定相同輸入與 params,任何時候執行都產出相同 Output → 可立 Core
> 2. **無選擇原則**:Core 內部不做「擇一/排序/篩選」決策,所有候選並列輸出
> 3. **無經驗原則**:不需經驗判讀的事實由 Core 機械產出;經驗類由 Aggregation Layer 判讀

#### **r1 邏輯張力提示** — 對 R3 §A5 的方案選擇

> r1 直接判定「採方案 1(資訊保全)」,但**方案 1 仍保留 power_rating 排序**,只是丟棄清單透明化,在哲學上 Core 仍在做選擇,與「無選擇原則」有微妙張力。
>
> **r2 修訂建議**:**改採方案 2(中性排序鍵)** 更符合無選擇原則 — 用 monowave 起點時間 + 結構長度 + scenario_id 字典序做 deterministic 排序,power_rating 完全不參與 overflow 截斷。但若考量「power_rating 是 Neely 書內客觀屬性,不算經驗判讀」,方案 1 + 透明丟棄清單仍可接受。
>
> **此項決策需 user 拍板**,不單方面採方案 1。

對應修正:
- R3 §A5:**方案 1 或方案 2 擇一**(r2 建議方案 2,但保留方案 1 為備案),Output 至少要含 `overflow_dropped_scenario_ids`
- R1 §3.6:採選項 (b),全市場法人由 Aggregation Layer 直查 view,寫入 overview §十「不獨立成 Core 清單」

---

## 三、🔴 P0 動工前阻塞項 — 統合修正清單(7 項)

> **r1 修正**:依**真實依賴鏈**重新組織,而非簡單線性順序。多數 P0 項目可並行修正。

### 依賴鏈圖

```
P0-2 (A-V3 上游驗證)  ──┬──→ P0-3 (_fwd 職責邊界,部分連動 volume 處理)
                        └──→ P0-5 (還原版本維度)

P0-1 (K-1) ─── 獨立,可立刻修
P0-4 (Trait 簽名) ─── 獨立,可並行
P0-6 (Forest overflow) ─── 獨立,可並行
P0-7 (dirty queue 契約) ─── 獨立,可並行
共識點 ③ 集中修(overview §6.5) ─── 獨立,可並行
Core 邊界三原則(overview §十) ─── 獨立,可並行
```

### 修正 P0-1:K-1(R1 §2.1)

**動作**:`chip_cores.md` §4.4 移除 `MarginPoint.margin_maintenance` 欄位 + 同步檢查 enum 與 §4.5 Fact 範例引用。

**驗證重點**:目前 enum 沒列 `MaintenanceLow` 但 §4.5 範例引用了 → 屬內部不一致,移除欄位的同時要補齊 enum/Fact 對齊。

### 修正 P0-2:A-V3(R2 §2.1 + R3 §A4)

**動作**(blueprint 工作):驗證 `price_daily_fwd.volume` 是否已隨除權息調整。
- 若已調整:OHLCVSeries 加 `cumulative_adjustment_factor` 欄位、TW-Market Core §五 5.1 volume 合併規則加分支處理(避免雙重失真)
- 若 raw:blueprint §4.4 ALTER `price_daily_fwd` 加 `volume_adjusted` 欄位、Volume Core 改讀此欄位

**等到 A-V3 結論定案後,連動修正**:
- D2 §六 6.1 OHLCVSeries 補欄位
- D2 §五 5.1 補 volume 合併條件分支
- volume Indicator Core(`obv` / `vwap` / `mfi`)§2.4 加註依賴 A-V3

### 修正 P0-3:`_fwd` 表職責邊界(R3 §A1)

**動作**:在 D2 §九 新增 §9.2「與 Silver 層的職責切分」,明訂後復權屬 Silver 階段 / 漲跌停合併屬 Core runtime / TAIEX neutral 閾值屬 Core runtime。並將 D2 §五 5.3 改寫為「**讀取** Silver 後復權結果,**不重新計算**」。

### 修正 P0-4:Trait 簽名重構(R1 §2.2 + R3 §B2)

**動作**:overview §3 改 associated type(代碼見 §2.1 共識點 ②)。

**連動修正**:
- chip_cores §2.1、fundamental_cores §2.1、environment_cores §2.1 改寫 trait 描述
- WaveCore trait 固化時點統一為「P0 開發 Neely Core 時草擬,P0 Gate 通過後固化」(D3 §1.2 / D4 §1.2 / D5 §六 W-1 三處同步)
- `revenue_core.warmup_periods` 月份單位歸屬於 RevenueSeries Input(R1 §3.1 自動解,P1-1 條件關閉)

### 修正 P0-5:price_len 度量空間定義(R3 §A3)

**動作**:在 D3(neely_core)與 D4(traditional_core)新增「§2.x 度量空間定義」,明示:
1. 公式 → 依專案歷史背景:**百分比 + Engine_N 在 linear close space**
2. 還原版本 → **後復權**(連動 P0-2 結論)
3. base price 選定(W1.start / scenario.start)

**將既有對話歷史中的決策正式寫入 spec**,避免後續實作者重新討論。

### 修正 P0-6:Forest overflow 機制(R3 §A5)

**動作**:**user 需先決議方案 1 vs 方案 2**(見 §2.3 r1 邏輯張力提示)。
- **方案 2(r2 建議)**:用 monowave 起點時間 + 結構長度 + scenario_id 字典序做 deterministic 排序,power_rating 完全不參與
- **方案 1(備案)**:保留 power_rating 排序但 Output 加 `overflow_dropped_scenario_ids: Vec<String>` + `overflow_dropped_count: usize`

無論哪個方案,前端必須顯示「結構過於複雜,呈現 Top K 解讀」橫幅契約寫入 §18.4 與 §6.3 串連。

### 修正 P0-7:dirty queue 觸發契約(R2 §2.2)

**動作**:overview 新增 §7.5「Batch 觸發來源與 dirty 契約」(R2 §2.2 已給草稿,直接採用)

### 修正 P0-集中包(R1/R2/R3 共識點 ③)— overview 新增 §6.5

**動作**:overview 新增 §6.5「Fact / Indicator 寫入契約總則」(7 條子規範,見 §2.1 共識點 ③)。**此修正自動帶解 P1-8(Fact 極值類)、P1-12(Combined params_hash)、合併項(produce_facts 範例)**。

### 修正 P0-Core 邊界三原則(R1 §3.6 + R3 §A5)

**動作**:overview §十「不獨立成 Core 的清單」前置補「邊界判定三原則」(可重現 / 無選擇 / 無經驗)。

---

## 四、🟠 P1 級修正清單(動工同時或 P0 Gate 後立刻處理)

> **r1 修正**:r1 §四 表只列 12 項,但 §1.2 列 14 項,**數量對不上**。r2 統一以 §1.2 為準,並標註「P0 自動帶解」項目。

| # | 項目 | 修正位置 | 來源 | P0 自動帶解? |
|---|---|---|---|---|
| 1 | `_market_` / `_global_` 保留值對照表 | overview 或 environment_cores | R1 §3.2 | 否 |
| 2 | `taiex_core` 資料源註明 `taiex_index_derived` | environment_cores §3.1 | R1 §3.3 | 否 |
| 3 | `shareholder_core` 週頻納入「事件型」第三類 | overview §7.2 | R1 §3.5 | 否 |
| 4 | `institutional_market_daily` 寫入「不獨立成 Core 清單」 | overview §十 | R1 §3.6 | **與 P0 Core 邊界三原則同改** |
| 5 | 三子類「對應資料表」全部改 Silver 表名 | 三子類 §一 表格 | R1 §4.4 | 否 |
| 6 | MergeAtLimitPrice 策略 — 補定義或從 enum 移除 | D2 §五 | R3 §A2 | 否 |
| 7 | Pipeline 級暖機區 max(warmup_periods) 規則 | overview §7.3 | R2 §3.4 | 否 |
| 8 | params_hash 寫入 `indicator_values` schema 確認 | blueprint 附錄 B DDL | R2 §3.2 | 部分(§6.5 第 5 條會引用此 DDL) |
| 9 | R15 與 R15-Ramki 重編號為 R15 / R15a + 啟用矩陣 | D4 附錄 A + 新增 §十 | R3 §A6 | 否 |
| 10 | Stage 4a/4b vs Phase 7a/7b/7c/7d 加註區分 | indicator_cores_pattern.md §2.3 | R2 §3.3 | 否 |
| 11 | null 值與錯誤處理通則(各 Core 該宣告) | overview | R1 §6.1 | 否 |
| 12 | revenue_core.warmup_periods 單位 | RevenueSeries Input 自身 | R1 §3.1 | **是,P0-4 trait 重構自動解** |
| 13 | Fundamental / Environment 規格欄位級對齊未驗(R2 §2.4 殘餘) | 三子類各 Core 對 blueprint Silver 欄位 | R2 §2.4 | 否,需補審 |

**P0 自動帶解項**(見 §1.2):
- `produce_facts` 無示範 → 由 §6.5 第 1 條解
- Fact 極值類事實歸屬 → 由 §6.5 第 4 條解
- Combined 模式 params_hash → 由 §6.5 第 7 條解

---

## 五、🟡 P2 級 Polish(可隨下次 PR 帶,共 11 項)

1. Fact statement 統一語言 + 移除描述詞(`short squeeze` / `large transaction` / `extreme high`)— R1 §4.2 / §4.3
2. environment_cores §一 與 overview §8.6 順序對齊(taiex / us_market 互換)— R1 §4.1
3. `valuation_daily_derived` 表名等 Silver 命名一致性 — R1 #7
4. business_indicator 降級理由寫入 environment_cores 「§九 已評估但不獨立成 Core 的項目」— R1 §5.2
5. `params_hash` 三處措辭統一(16 字元 / 64 bits / 16 hex)— R1 #14
6. fear_greed_core 資料源不明的註記補充 — R1 #9
7. industry_chain_ref 文件盲點提醒 — R1 #12
8. 「raw 表」用語全文修正(改為 Silver/Bronze)— R2 §3.1
9. cores_overview「選項 A」補 trace(B/C 是什麼)— R3 §B1
10. shared/swing_detector 抽不抽出決議時點 — R3 §B3
11. shared/macd_compute / shared/rsi_compute 候選清單 + 已知技術債 backlog — R1 §5.1

---

## 六、🟢 對齊得宜部分

| 優點 | 來源 |
|---|---|
| 「並排不整合」哲學貫穿一致 | R3 §C1 |
| 棄用設計清單交叉驗證乾淨(16+14 條重疊) | R3 §C2 |
| 書頁追溯規範完整(`source_version`/`neely_page` 三層) | R3 §C3 |
| Hard Rules vs Guidelines 二分法清晰 | R3 §C4 |
| Collector blueprint 與 Core spec 職責邊界乾淨 | R3 §C5 / R2 §四 |
| Silver `*_derived` 命名規律對齊(子類規格表名問題例外) | R2 §四 |
| TW-Market Core 在 Neely Core 之前 | R2 §四 |
| TTM Squeeze / Volume / Fibonacci 不獨立 Core 跨文件一致 | R2 §四 |
| `trendline_core` 唯一耦合例外的明確標示 | R2 §四 |
| Monolithic Binary 部署模型一致 | R2 §四 |
| Overview 與三子類職責切分乾淨,無大量重複 | R1 §1.1 |

---

## 七、修正執行優先順序(依依賴鏈而非單純嚴重度)

### Phase 0:動工前必修(7 項 P0 + 2 項集中修)

**Track A(可立即啟動,獨立)**:
1. **P0-1 K-1**:chip_cores.md 移除 margin_maintenance(R1 §2.1)
2. **P0-4 Trait 簽名**:overview §3 改 associated type(R1 §2.2 + R3 §B2)
3. **P0-7 dirty queue 契約**(overview §7.5)
4. **P0-集中包 §6.5**:M3 寫入契約總則(R1/R2/R3 共識點 ③)
5. **P0-Core 邊界三原則**(overview §十 + 串接 P1-4 institutional_market_daily 與 P0-6 Forest overflow)

**Track B(需 user 決議)**:
6. **P0-6 Forest overflow**:user 拍板方案 1 vs 方案 2

**Track C(依賴 A-V3 結論)**:
7. **P0-2 A-V3**:blueprint 端先驗證 → 結論定案後連動 P0-3 / P0-5

### Phase 1:動工同時或 P0 Gate 後立刻處理(13 項,扣除 P0 自動帶解 3 項實質 10 項)

依 §四 表執行。

### Phase 2:Polish(11 項,隨下次 PR 帶)

依 §五 表執行。

---

## 八、本統合報告未涵蓋範圍(誠實標示)

1. **Wave Cores 完整內部規則**:R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2 等具體閾值,R3 已標明未審
2. **Aggregation Layer / Orchestrator 詳細規格**:屬即時路徑層
3. **Learner 離線模組**:屬 P3+ 範圍
4. **Wave Core 之間的 trendline_core 耦合例外**:三報告均提及但屬另立 spec 範圍
5. **System Cores**(`aggregation_layer` / `orchestrator`):屬即時路徑層,未審
6. **Indicator Cores 17 個的個別演算法**:R2 已涵蓋對齊但未細到內部演算法
7. **Fundamental / Environment Core 欄位級對齊**(R2 §2.4 殘餘部分):R1 補上規格審查但未對 blueprint Silver 欄位逐欄驗證

---

## 九、最終建議:Cores spec 系列下一輪審查的編組

> **r1 修正**:r1 §九 r2-2 範圍誤含 P0-6(Forest overflow),P0-6 屬 D3 修改不在 overview 範圍。已重新編組。

| 輪次 | 範圍 | 目標 |
|---|---|---|
| **r2-1**(blueprint 端) | A-V3 驗證結論 + blueprint 附錄 B DDL 補完 | 解 P0-2 + P1-9 |
| **r2-2**(overview 端) | overview §3(trait)/ §6.5(M3 契約)/ §7.5(dirty)/ §十(Core 邊界) | 解 P0-4 / P0-7 / P0-集中包 / Core 邊界三原則 + 自動帶解 P1-8 / P1-12 / 合併項 |
| **r2-3**(三類子規格端) | chip_cores K-1 + 三子類 Silver 表名 + 規格欄位級對齊 | 解 P0-1 / P1-6 / P1-13(R2 §2.4 殘餘)|
| **r2-4**(TW-Market + Wave 端) | D2 §9.2(_fwd 職責)+ D3/D4 §2.x(度量空間)+ D3 §6.3(forest overflow) | 解 P0-3 / P0-5 / P0-6 |
| **r3**(統合複審) | 全規格交叉驗證 + Polish 落實 | 解 P2 級全部 |

---

## 十、本統合報告的整合貢獻 + r2 邏輯修正記錄

### 10.1 整合貢獻

1. **發現跨報告共識點**(§2.1):3 組共識點,共識點 ③ 集中修正可一次解 7 個跨報告問題
2. **發現相鄰但反向的問題**(§2.3):R1 §3.6 與 R3 §A5 是 Core 邊界判定的反向案例 → 抽出三原則
3. **修正執行依賴排序**(§三 / §七):依「真實依賴鏈」分 Track A/B/C,而非單純線性
4. **去重與部分關閉**:R2 §2.4 「fundamental/environment 未送審」由 R1 部分補上,殘餘「欄位級對齊」獨立列為 P1-13
5. **修正集中策略**:overview §6.5 一節集中解 7 個跨報告問題

### 10.2 r2 對 r1 的邏輯修正(共 13 處)

| # | r1 錯誤類型 | r1 內容 | r2 修正 |
|---|---|---|---|
| 1 | 引用錯誤 | P0-1 來源寫「R1 §2.1 / R2 §2.3」並列 | 改為 R1 是實質確認,R2 §2.3 僅標旁證 |
| 2 | 數量錯誤 | P0 共 8 項 | 修為 7 項(P0-2 + P0-7 合併為一鏈) |
| 3 | 數量不一致 | §1.2 「14 項」vs §四「12 項」對不上 | 統一以 §1.2 為準,並標註 P0 自動帶解 |
| 4 | 無中生有 | 共識點 ② 寫「R2 §2.4 隱含 trait 問題」 | 移除,R2 §2.4 不涉及 trait |
| 5 | 重複歸類 | `produce_facts` 同列 P0(§三修正 7)與 P1-4(§1.2) | 統一歸 P0 共識點 ③,§1.2 標「併入 P0」 |
| 6 | 編號衝突 | P1 表標 §6.4(新增),§三標 §6.5 | 統一為 §6.5 |
| 7 | 數字膨脹 | 共識點 ① 寫「9 個 indicator Core 受影響」 | 修為實際 4 個(obv/vwap/mfi/bollinger)|
| 8 | 「三份報告交集」 | 共識點 ① 稱「三份報告交集」 | 修為「R2+R3 共識」(R1 不審 indicator/wave) |
| 9 | 依賴鏈過簡 | §三「依修正依賴順序排列(後者依賴前者完成)」 | 修為依賴鏈圖,Track A/B/C 並行 |
| 10 | 未論證的方案選擇 | R3 §A5 直接判定採方案 1 | 補論證:方案 1 仍有「無選擇原則」張力,建議方案 2,user 決議 |
| 11 | 範圍誤放 | §九 r2-2 含 P0-6(Forest overflow) | 移到 r2-4 |
| 12 | R2 §2.4 完全關閉 | r1 寫「R2 §2.4 可關閉」 | 修為「部分關閉」,殘餘欄位級對齊列為 P1-13 |
| 13 | Core 數歧義 | 〇章「13+19+3=35 個」未說明是審查焦點還是去重總數 | 補註:此為審查焦點總和,非全系統 Core 數 |

---

**(統合報告 r2 完)**
