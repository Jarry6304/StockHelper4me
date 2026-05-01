# Cores 全系列規格對齊審查 — 統合報告 r3(C 系列整合版)

> **整合來源**:三份分區審查報告(審查範圍互不重複)
> **整合日期**:2026-05-01(r3 修訂日:2026-05-01)
> **版本**:**r3**(C 系列整合版,於 r2.1 基礎上把 §十一 r3 預備清單的 10 條 gap 正式整合進 P0/P1/P2 表)
> **版本沿革**:r1(初稿)→ r2(13 處邏輯/引用/數量修正)→ r2.1(12 處事實/流程修正)→ **r3(本次,C 系列 10 條 gap 整合 + 編組重排)**
> **共同對齊基準**:`cores_overview.md` v2.0 r1 + `collector_rust_restructure_blueprint_v3_2.md` r1
> **整合方法**:交叉比對 → 依賴關係分析 → 衝突合併 → 阻塞項排序 → overview/blueprint 修正清單
>
> **r3 整合重點**(詳見 §10.4):C1/C2/C3 → P0(P0-8/P0-9/P0-10);C4/C5/C6 → P1(P1-14/P1-15/P1-16);C7/C8/C9/C10 → P2(P2-12/P2-13/P2-14/P2-15)。動工順序、依賴鏈、§九 編組同步更新。原 §十一 改為「C 系列整合對應表」保留追溯。

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

### 1.1 🔴 P0 動工前阻塞項(r3:共 10 項,含 C 系列 promote 3 項)

> **r1 修正**:r1 列 8 項時將 P0-2(A-V3)與 P0-7(Volume 還原)分列,但兩者實為同一阻塞鏈的兩面,合併為 P0-2。
> **r3 整合**:P0-8 / P0-9 / P0-10 從 r2.1 §十一 C 系列 promote。

| # | 項目 | 來源報告 | 影響範圍 | 依賴 |
|---|---|---|---|---|
| **P0-1** | **K-1**:`chip_cores.MarginPoint.margin_maintenance` 欄位未移除(§4.4 line 184)| R1 §2.1(實質確認);R2 §2.3 僅標「無法驗證」,**非實質指出**,僅作旁證 | `chip_cores.md` §4.4 | 獨立 |
| **P0-2** | **A-V3 + 連動 OHLCVSeries / Volume 還原**:`price_daily_fwd.volume` 是否已調整未驗證,連帶 OHLCVSeries 缺欄位、TW-Market Core volume 合併雙重失真風險 | R2 §2.1 + R3 §A4 | volume Cores 全部 + `OHLCVSeries` schema + TW-Market Core §五 5.1 | **上游阻塞**,影響 P0-3 / P0-8 連動 |
| **P0-3** | **`_fwd` 表職責邊界**:Silver 已做後復權 vs TW-Market Core §五宣稱要做 | R3 §A1 | D2(`tw_market_core.md`) §五 / §九 | 部分依賴 P0-2 結論 |
| **P0-4** | **Trait 簽名矛盾**:`compute(ohlcv: &OHLCVSeries)` 與 12 個非 OHLCV Core 衝突 | R1 §2.2 | overview §3 + 三子類 §2.1 | 獨立 |
| **P0-5** | **`price_len` 度量空間未定義**(百分比/絕對/log)+ base price 不明 + 還原版本不明 | R3 §A3 | **僅 D4(traditional_core)**;neely_core 整篇沒有 price_len(monowave 度量靠 ATR) | 獨立(原 r2 標「依賴 P0-2」過度;且 traditional_core 是 P3,本項應 **降為 P3-限定**)|
| **P0-6** | **Forest overflow 用 power_rating 排序**違反「不選 primary」哲學 | R3 §A5 | `neely_core §十二 12.2-12.3` + §八 Output schema(r2.1 修節編號)| 獨立 |
| **P0-7** | **dirty queue 觸發契約缺**:Core 不知道何時該重算(**r3.1 補:av3 Test 3 直接 production 證據** — 3363 2026-01-20 / 1312 2023-11-28 stock_dividend 事件 fwd 沒處理,fwd_close=raw_close)| R2 §2.2 + av3 Test 3 | overview §七 + `src/post_process.py`(暫時補丁) | 獨立 |
| **P0-8**(C1)| **tw_market_core §五 5.1 volume 合併物理層 ambiguity**:沒講合併發生在「後復權前」還是「後復權後」(r3 promote)| spec 原文 spot-check | tw_market_core.md §五 5.1 | **P0-2 裡層**,A-V3 結論定案後一併補 |
| **P0-9**(C2)| **跨 Core warmup_periods 合成規則完全缺**:11 篇 spec 全無 max() / sum() / 各自取的規則(r3 promote)| spec 原文 spot-check | overview §7.3 | 獨立 |
| **P0-10**(C3)| **NeelyDiagnostics 缺 `dropped_scenario_ids`**:§八 Output 只有 `overflow_triggered: bool`,違反可重現原則(r3 promote;**剛性,無論 P0-6 方案 1/2 都要**)| spec 原文 spot-check | neely_core.md §八 | 與 P0-6 同條 schema 改動 |
| **P0-11**(av3) | **Rust split / par_value_change / capital_increase volume 算錯方向 production bug**:`rust_compute/src/main.rs:447` 對所有事件用 `volume / multiplier`(其中 multiplier 來自 `adjustment_factor`),但 `field_mapper.py:194-203` 寫的 `volume_factor` 對非 dividend 事件 = ap/bp ≈ 1/AF。Rust 完全不讀 vf 欄位,對 split (1→N) backward adj volume 應 ×N (post-split equivalent shares) 卻變 /N(反方向)| av3 Test 5(全市場 dividend 25/par_value 16/split 31 三類事件 af≠vf 100%)+ Rust main.rs:447 | `rust_compute/src/main.rs` AdjEvent struct + load_adj_events SELECT + compute_forward_adjusted | **production bug,r3.1 新增**;A-V3 Test 1-2 cash dividend 沒揭(因 vf=1.0)|

> **真實依賴關係**(r3.1 更新,加入 av3 揭露的 P0-11):
> ```
> P0-2 (A-V3) ──┬──→ P0-3 (_fwd 職責中 volume 處理連動)
>               └──→ P0-8 (volume 合併與後復權順序)
>
> P0-6 ──→ P0-10 (Output schema 同檔案改動)
>
> P0-11 (Rust volume bug) — 獨立,需 cargo build + 全 stock 重跑 Phase 4
>
> P0-1 / P0-4 / P0-7 / P0-9   彼此獨立,可並行修正
> P0-5  降 P3-限定(traditional_core 開工時處理,非動工前阻塞)
> ```

### 1.2 🟠 P1 一致性與完整度問題(r3:**15 條 P1 + 3 條 P0 集中包帶解 = 18 引用列**)

> **r1 修正**:
> - P1-9(R2 §2.4)應為「**部分關閉**」而非完全關閉:R1 雖補上規格審查,但未做欄位級對齊(`valuation_core` 是否消費新增 `market_value_weight` 等)
> - r1 P1-4(`produce_facts` 無示範)併入 P0 修正集中包(共識點 ③),仍須 D 系列規格內補(見 r2.1 §10.3 B3)
>
> **r2.1 修正**:r2 §1.2 「14 項」、§四「13 項」、+「P0 帶解 3 項」三處數量對不上。r2.1 統一表頭計法為「12 條獨立 P1 + 3 條 P0 帶解列 = 15 引用列」。
>
> **r3 整合**:從 §十一 C 系列 promote 3 條進 P1 → P1-14(C4 Combined params 拆)/ P1-15(C5 vwap params_hash 演算法)/ P1-16(C6 Fibonacci tiebreak)。新計法:**15 條 P1(P1-1 ~ P1-13 + P1-14 ~ P1-16)+ 3 條 P0 帶解 = 18 引用列**。

| # | 項目 | 來源 | 修正位置 | 狀態 |
|---|---|---|---|---|
| P1-1 | `revenue_core.warmup_periods` 月份/K 棒單位混亂 | R1 §3.1 | overview §7.3(若採 P0-4 associated type 則自動解) | 條件性 |
| P1-2 | `_market_` / `_global_` stock_id 保留值對照表缺失 | R1 §3.2 | overview 或 environment_cores | 獨立 |
| P1-3 | `taiex_core` 資料源不明(`market_index_tw` vs `market_ohlcv_tw`) | R1 §3.3 | environment_cores §3.1 | 獨立 |
| P1-4 | `shareholder_core` 週頻 = 「事件型」第三類,overview §7.2 沒列 | R1 §3.5 | overview §7.2 | 獨立 |
| P1-5 | `institutional_market_daily` 處理盲點(全市場法人) | R1 §3.6 | overview §十 | **與 P0-6 共構 Core 邊界三原則** |
| P1-6 | 子類規格「對應資料表」全寫 Bronze 表名,應為 Silver `*_derived` | R1 §4.4 | 三子類 §一 表格 | 獨立 |
| P1-7 | **MergeAtLimitPrice 策略無書面定義**(三選一砍或補) | R3 §A2 | D2 §五 | 獨立 |
| P1-8 | **Fact 極值類事實**歸屬模糊(5y low / 1y high)| R2 §3.5 | overview 新增 §6.5 第 4 條 + indicator_volatility 規格 | **P0 部份帶解,仍須 D 系列規格內補**(spec 已明寫由 bollinger/atr 產出,真實問題是「extreme_low/high」是描述詞;§6.5 方案 C 移出 Core 會大幅改 indicator 規格)|
| P1-9 | params_hash 寫入 `indicator_values` schema 待確認 | R2 §3.2 | blueprint 附錄 B DDL | 獨立 |
| P1-10 | Pipeline 級暖機區計算規則未明定 | R2 §3.4 | overview §7.3 | 獨立 |
| P1-11 | **R15 與 R15-Ramki RuleId 撞號**(P3 前修) | R3 §A6 | D4 附錄 A + 新增 §十 | 獨立 |
| P1-12 | **Combined 模式與 params_hash 去重衝突** | R3 §A7 | D4 §三 + overview §6.5 第 7 條 | **P0 部份帶解,仍須 D 系列規格內補**(D4 Combined Params 結構需拆 frost_params/ramki_params,光改 §6.5 不足)|
| (P1-rem) | **Fundamental / Environment 規格欄位級對齊未驗** | R2 §2.4 殘餘 | 三子類各 Core 對 blueprint Silver 欄位 | **R1 未涵蓋,r2 補列** |
| (合併) | `produce_facts` 在 13 個子類 Core 全部沒示範(實際應為 25 個 Core 全無實作示範,r2.1 補)| R1 §3.4 | overview §6.5 第 1 條 + 各 Core spec 各補一個 reference 範例 | **P0 部份帶解,仍須 D 系列規格內補**(§6.5 寫通則,各 Core 仍需個別寫範例)|
| **P1-14**(C4)| **traditional_core Combined 模式 params_hash collision** — Frost ∪ Ramki forest 沒處理 hash(r3 promote)| spec 原文 spot-check | D4 §三 Combined Params 結構需拆 `engine: Combined { frost_params, ramki_params }` | 獨立 |
| **P1-15**(C5)| **vwap_core anchor 多錨 + params_hash 演算法定義缺** — overview §7.4 只到「canonical JSON + blake3 前 16」沒列 anchor/timeframe 是否進 hash(r3 promote)| spec 原文 spot-check | overview §7.4 + 各 Core spec 個別寫 | 獨立 |
| **P1-16**(C6)| **traditional_core Fibonacci ratio tiebreak 缺** — W2 同時退 38.2% 與 50% 哪個先寫 Fact 沒講,**直接違反 r2 §2.3 抽出的「無選擇原則」**(r3 promote)| spec 原文 spot-check | D4 §附錄 C 加 deterministic tiebreak | 與 Core 邊界三原則衝突,**優先處理** |

### 1.3 🟡 P2 措辭/順序/命名不一致(r3:共 15 項,含 C 系列 promote 4 項)

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

**兩份報告的交集**(r1 誤寫「三份報告交集」,實為 R2+R3 共識,R1 因不審 indicator/wave 故未交集):`price_daily_fwd` 的 volume 與後復權語意未明確 → 影響 **3 個 volume indicator Core**(`obv` / `vwap` / `mfi`)+ 1 個 TW-Market Core(R3 範圍)= 共 **4 個 Core** 直接受影響。

> **r2.1 修正**:r2 寫「4 個 Core(obv/vwap/mfi/bollinger)」過度。**bollinger_core 完全不吃 volume**(`indicator_cores_volatility.md §3.2-3.4` 的 `BollingerParams` 只用 `period / std_multiplier / source: PriceSource / timeframe`)。正確受影響為 3 個 volume indicator Core + 1 TW-Market Core = 4。

> **r2.1 修正**:**P0-2 在 spec 端不是「未陳述」**。`indicator_cores_volume.md §2.4` 已明寫「本子類 Core 吃 TW-Market Core 處理過的 volume(已隨除權息調整)」。真實 gap 是 **collector blueprint 端沒驗證該假設**(`price_daily_fwd.volume` 實際是 raw 還是已調整)。問題在 collector / Rust,不在 Core spec。

**統合結論**:**A-V3 是 P0 階段最大的單點阻塞**,卡住 4 個 Core 的 schema 與計算正確性。建議**先排 A-V3 驗證 → 再決定 OHLCVSeries 欄位 → 再決定 Volume Core 與 TW-Market Core 對齊路徑**。

---

#### 共識點 ②:overview §3 trait 簽名問題

> **r1 邏輯錯誤修正**:r1 寫「R2 §2.4 隱含同類問題」屬無中生有,R2 §2.4 實際只談 chip/fundamental/environment 規格未送審,完全不涉及 trait 簽名。已從共識點 ② 移除 R2 引用。
>
> **r2.1 修正**:r2 把 P0-4 描述為「`compute(ohlcv: &OHLCVSeries)` 與 12 個非 OHLCV Core 衝突 + Wave/Market 也有問題」**過度誇大**。真相:
> - `cores_overview.md §3.3` 已明文「**Wave Cores 與 Market Cores 不強制走 IndicatorCore trait,有自己的 trait(例:WaveCore / MarketCore)**」 — 對 Wave/Market 已 carve-out
> - 真實 gap 是 **Wave/Market trait signature 沒給出**:`neely_core.md §1.2` / `traditional_core.md §1.2` / `tw_market_core.md §1.2` 三 spec 都標「走 WaveCore/MarketCore trait(草案)」**但無 signature**
> - chip / fundamental / environment §2.1 也都明確記載「輸入不是 OHLCVSeries,而是各自對應資料表」(spec 自承知道,只是沒解 trait 設計)
>
> r2 §2.1 共識點 ② 的 associated type 解法**本身正確**,只是把「已 carve-out 的 Wave/Market 部份」說成「衝突」過度。

| 指出問題的報告 | 角度 |
|---|---|
| R1 §2.2 | 12 個非 OHLCV Core(chip 5 + fundamental 3 + environment 4)用不到 OHLCV 參數,但各 spec §2.1 自承「輸入非 OHLCV」(已知設計問題,非隱藏 bug)|
| R3 §B2 | WaveCore trait 應與 IndicatorCore trait 並列(overview §3.3 已 carve-out 但 trait signature 三 spec 都沒寫);Neely vs Traditional 對 trait 固化時點認知不一 |

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

## 三、🔴 P0 動工前阻塞項 — 統合修正清單(r3:10 項,含 C 系列 promote 3 項)

> **r1 修正**:依**真實依賴鏈**重新組織,而非簡單線性順序。多數 P0 項目可並行修正。
>
> **r3 整合**:新增 P0-8(C1)/ P0-9(C2)/ P0-10(C3)三條詳述。

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

**動作**:`chip_cores.md` §4.4 line 184 移除 `MarginPoint.margin_maintenance` 欄位(1 行刪除)。

> **r2.1 修正**:r2 寫「同步檢查 enum 與 §4.5 Fact 範例引用,目前 enum 沒列 `MaintenanceLow` 但 §4.5 範例引用了」**子論點誤判**。spec spot-check 結果:
> - §4.4 line 184 確實有 `pub margin_maintenance: f64,` → **主件正確,該砍**
> - §4.5 line 210 的 `maintenance_low` 是**Fact metadata tag** 不是 enum variant
> - §4.2-4.3 `MarginEventKind` enum 確實沒列 `MaintenanceLow`,但 metadata tag 跟 enum variant **本來就不同層級**,不算內部不一致
>
> 因此本項只需砍欄位,enum 對齊不在範圍。

### 修正 P0-2:A-V3(R2 §2.1 + R3 §A4)— **r3.1 結論定案**

> **r3.1 av3 結果(2026-05-01 user 本機 PG 17 跑)**:
> - **現金 dividend**: Rust 派確認 ✓,`dollar_vol_preserved = 1.0000` 完美(Test 1 4/4 + Test 2 17 筆 dividend 中 15 筆 `vol_ratio = 1/AF`)
> - **stock_dividend / split / par_value_change**: 🔴 Rust **算錯方向**(P0-11 新增),Rust line 447 用 AF 不是 vf
> - **spec assumption「volume 已隨除權息調整」**:**僅對現金 dividend 成立**,對其他事件不成立(Rust bug)
> - **staleness production 證據**(Test 3): 3363 2026-01-20 / 1312 2023-11-28 stock_dividend 事件 fwd 沒處理 → P0-7 升從理論變實機
>
> **A-V3 verdict 等於「**現金 dividend Rust 派 ✓ + 其他事件 Rust bug 待修(P0-11)**」**。

**已執行**(r3.1):
- ✅ blueprint §四 4.4: 砍 `volume_adjusted` ALTER, 改加 `cumulative_adjustment_factor`(本 commit 同步動)
- ✅ D2 §五 5.1 (P0-8/C1) 走「先復權再合併 sum」分支(對齊 Rust 派,合併用 fwd volume)
- ✅ 新增 P0-11(Rust split volume bug)
- ✅ P0-7 加實機證據(本 commit §1.1 P0-7 列已補)

**已 close**:
- ~~D2 §六 6.1 OHLCVSeries 加 `volume_adjusted` 欄位~~(Rust 派下不需)
- ~~volume Indicator Core §2.4 加註依賴 A-V3~~(spec 已陳述,本來就多餘)

### 修正 P0-3:`_fwd` 表職責邊界(R3 §A1)

> **r2.1 修正**:r2 寫「§九 lacks §9.2」過度誇大。真相:`tw_market_core.md §五 5.3` 已寫「還原計算的精度由 Rust 端負責,**禁止前端聚合或還原**」+ §九 9.1 已說明 `price_*_fwd` 表用途。**問題不是邊界缺,是兩處資訊散落**,需補一節集中說明。

**動作**:在 D2 §九 **新增 §9.2「與 Silver 層的職責切分集中說明」**(將既散在 §五 5.3 + §九 9.1 的內容統合到一處),明訂後復權屬 Silver 階段 / 漲跌停合併屬 Core runtime / TAIEX neutral 閾值屬 Core runtime。§五 5.3 已寫「禁止重新計算」,**不需重寫**,只需在新 §9.2 引用。

### 修正 P0-4:Trait 簽名重構(R1 §2.2 + R3 §B2)

**動作**:overview §3 改 associated type(代碼見 §2.1 共識點 ②)。

**連動修正**:
- chip_cores §2.1、fundamental_cores §2.1、environment_cores §2.1 改寫 trait 描述
- WaveCore trait 固化時點統一為「P0 開發 Neely Core 時草擬,P0 Gate 通過後固化」(D3 §1.2 / D4 §1.2 / D5 §六 W-1 三處同步)
- `revenue_core.warmup_periods` 月份單位歸屬於 RevenueSeries Input(R1 §3.1 自動解,P1-1 條件關閉)

### 修正 P0-5:price_len 度量空間定義(R3 §A3)— **r2.1 降為 P3-限定**

> **r2.1 修正**:r2 寫「在 D3(neely_core)與 D4(traditional_core)新增 §2.x」**範圍錯**。真相:`neely_core.md` 整篇沒有 `price_len` 這個詞 — monowave 度量靠 ATR(§三 3.1 / §六 6.3-6.5)。**只有 `traditional_core.md §附錄 A`** 在 R02 / R05 用 price_len 但沒定義。
>
> 且 `traditional_core` 是 P3 優先級,把 P3 議題硬拉到 P0 動工前阻塞鏈不合理。**P0-5 應降為 P3-限定**(traditional_core 開工時才處理,不阻塞 P0 collector / Neely Core 動工)。

**動作**(P3 開工時):在 D4(traditional_core)新增「§2.x 度量空間定義」,明示:
1. 公式 → 依專案歷史背景:**百分比 + Engine_N 在 linear close space**
2. 還原版本 → **後復權**(P3 開工時 A-V3 已定案)
3. base price 選定(W1.start / scenario.start)

**將既有對話歷史中的決策正式寫入 spec**,避免後續實作者重新討論。

### 修正 P0-6:Forest overflow 機制(R3 §A5)

**動作**:**user 需先決議方案 1 vs 方案 2**(見 §2.3 r1 邏輯張力提示)。
- **方案 2(r2 建議)**:用 monowave 起點時間 + 結構長度 + scenario_id 字典序做 deterministic 排序,power_rating 完全不參與
- **方案 1(備案)**:保留 power_rating 排序但 Output 加 `overflow_dropped_scenario_ids: Vec<String>` + `overflow_dropped_count: usize`

> **r2.1 修正**:r2 寫「前端必須顯示橫幅契約寫入 §18.4 與 §6.3 串連」**橫幅實際早已存在**。`neely_core.md §十八 18.4` 明文:「`NeelyDiagnostics.overflow_triggered = true` 必須傳到前端,顯示『此股結構過於複雜,系統呈現 Top K 解讀』橫幅」。**真實該補的是 Output 透明化欄位 `overflow_dropped_scenario_ids: Vec<String>`**,§八 Output schema 目前只有 `overflow_triggered: bool`,沒列哪幾條 scenario 被丟,違反**可重現原則**(C3 補件,屬剛性,無論方案 1/2 都要)。

**剛性需求(無論方案 1/2 都要)**:`neely_core.md §八` Output schema 加:
- `overflow_dropped_scenario_ids: Vec<String>`
- `overflow_dropped_count: usize`

§十八 18.4 banner 契約**保留不動**(已存在)。

### 修正 P0-7:dirty queue 觸發契約(R2 §2.2)

**動作**:overview 新增 §7.5「Batch 觸發來源與 dirty 契約」(R2 §2.2 已給草稿,直接採用)

### 修正 P0-集中包(R1/R2/R3 共識點 ③)— overview 新增 §6.5

**動作**:overview 新增 §6.5「Fact / Indicator 寫入契約總則」(7 條子規範,見 §2.1 共識點 ③)。**此修正自動帶解 P1-8(Fact 極值類)、P1-12(Combined params_hash)、合併項(produce_facts 範例)**。

### 修正 P0-Core 邊界三原則(R1 §3.6 + R3 §A5)

**動作**:overview §十「不獨立成 Core 的清單」前置補「邊界判定三原則」(可重現 / 無選擇 / 無經驗)。

### 修正 P0-8(C1):tw_market_core §五 5.1 volume 合併物理層 ambiguity(r3 promote)

> **問題**:spec §五 5.1 寫 `volume = sum of merged days` 但**沒講合併發生在「後復權前」還是「後復權後」**。先合併再復權對合併前的 sum 套 AF;先復權再合併對已復權的多日 volume 加總,**兩種結果數值不同**。

**動作**(連動 P0-2 A-V3 結論):
- 若 A-V3 確認 `price_daily_fwd.volume` **已調整**:tw_market_core §五 5.1 補「合併讀已復權 volume 後再 sum,**避免雙重復權**」+ 加分支處理
- 若 A-V3 確認 **raw**:tw_market_core §五 5.1 補「合併在 raw 端做,後復權只算 OHLC 不動 volume」(交給 Volume Cores 自處)

**先後順序**:A-V3 結論定案後 5 分鐘修文。

### 修正 P0-9(C2):跨 Core warmup_periods 合成規則(r3 promote)

> **問題**:同一 workflow 同時跑 `ma(period=200)`(暖機 800 天)+ `macd(12,26,9)`(暖機 ~50 天)+ `rsi(14)`(暖機 ~30 天),**pipeline 取多少歷史**?11 篇 spec 全無規則。

**動作**:overview **§7.3 補「跨 Core warmup 合成規則」**。建議規則:
```
pipeline_warmup = max(core.warmup_periods(params) for core in workflow.cores)
```

**理由**:max() 確保每個 Core 都暖到;sum() 過大會炸資料窗口;各自取會導致長暖機 Core 早期數值不可靠。

**例外標註**:Wave / Pattern Core 用「全量重算」策略,不適用增量 warmup,以 `core_kind` 標記。

### 修正 P0-10(C3):NeelyDiagnostics 加 `dropped_scenario_ids`(r3 promote;與 P0-6 同檔)

> **問題**:`neely_core.md §八` Output schema 只有 `overflow_triggered: bool`,**沒列哪幾條 scenario 被丟**,違反 §2.3 抽出的「可重現原則」(同樣輸入丟掉的 scenario_ids 應可重現)。
>
> **r3 強調**:r2 P0-6 動作把 `dropped_scenario_ids` 列為「方案 1 條件」,**錯**。無論方案 1(power_rating 排序)或方案 2(deterministic 中性鍵),**Output 都該透明化**,否則無法審計。

**動作**(剛性,與 P0-6 同檔同次改動):
```rust
pub struct NeelyDiagnostics {
    pub overflow_triggered: bool,
    pub overflow_dropped_count: usize,           // r3 加
    pub overflow_dropped_scenario_ids: Vec<String>,  // r3 加
    // ... existing fields
}
```

§十八 18.4 frontend banner 契約保留不動。

### 修正 P0-11(av3 揭露):Rust split / par_value / capital_increase volume bug — **r3.1 新增**

> **問題**:`rust_compute/src/main.rs:447` 對所有事件用 `volume / multiplier`(multiplier 從 `adjustment_factor` 累積),但 `field_mapper.py:194-203` 已寫對的 `volume_factor` 進 `price_adjustment_events.volume_factor` 欄位 — Rust 完全不讀。對 split/par_value 兩派計算結果反方向。
>
> **av3 證據**:
> - Test 5: dividend (af≈1.015 vs vf=1.0) / par_value_change (af=6.53 vs vf=0.27) / split (af=5.87 vs vf=1.09) — 三類事件 af≠vf 100%
> - Test 4: 0 rows(user 本機沒 backfill 到含 split/par_value 的股票,直接驗證受限)— 但 Test 5 統計足夠

**動作 4 處**(都在 `rust_compute/src/main.rs`):
1. `AdjEvent` struct 加 `volume_factor: f64` 欄位
2. `load_adj_events` SELECT 加 `volume_factor::float8`
3. `compute_forward_adjusted` 拆兩個 multiplier:`price_multiplier`(從 AF) + `volume_multiplier`(從 vf)
4. `volume = raw_volume / volume_multiplier`(原為 `/multiplier`)

**語意改變**:對現金 dividend,fwd_volume 將 = raw_volume(不再 / AF) → dollar_vol 對現金 dividend 不再守恆,但**反映實際 share 流動性**(OBV / VWAP 用)。對 split,fwd_volume = raw_volume / vf = raw_volume × N(post-split equivalent),物理正確。

**已知遺留 bug(本次不修,進 P1)**:
- `field_mapper.py:194-198` 對 stock_dividend 事件也寫 vf=1.0(把 cash 跟 stock dividend 混為一談)
- av3 Test 3 row 3363 2023-10-17 vf 應該是 ~0.79 不是 1.0
- 本次只修 Rust 讀 vf,**field_mapper 修正獨立 PR 處理**(避免一次改太多)
- 進 §四 P1-17(新增):「field_mapper.py:194 對 stock_dividend 事件 vf 計算錯誤」

**動工後驗證**(scripts/av3_spot_check.sql 重跑):
- Test 1 vol_ratio 從 0.924 變 1.0(因 vf=1.0 for dividend)
- Test 1 dollar_vol_preserved 從 1.0 變 ~AF(>1)
- Test 4 若 user 補 backfill split 股票,vol_ratio = AF(>1)出現

---

## 四、🟠 P1 級修正清單(動工同時或 P0 Gate 後立刻處理)

> **r1 修正**:r1 §四 表只列 12 項,但 §1.2 列 14 項,**數量對不上**。r2 統一以 §1.2 為準,並標註「P0 自動帶解」項目。
>
> **r2.1 修正**:統一表頭計法為「**13 條獨立 P1 列(下表 1-13)+ 3 條 P0 集中包帶解(表後三點)= 16 引用列**」,跟 §1.2 表的「12 條 P1 + 3 條 P0 帶解 + 2 條 (P1-rem)/(合併) 標籤列 = 17 行」差 1 列(本表 P1-7 在 §1.2 對應 MergeAtLimitPrice = 同一條,編號規則上 §1.2 為主、§四為從)。本版本不重新編號避免破壞外部引用。

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
| **14**(C4)| **traditional_core Combined 模式 params 結構拆**(`engine: Combined { frost_params, ramki_params }`)— r3 promote | D4 §三 | spec spot-check | 否 |
| **15**(C5)| **vwap params_hash 演算法明示** include 全部 Params 欄位(含 anchor 日期 / mode / timeframe / source)— r3 promote | overview §7.4 + 各 Core spec | spec spot-check | 否 |
| **16**(C6)| **Fibonacci ratio tiebreak 規則**(deterministic 排序鍵)— r3 promote | D4 §附錄 C | spec spot-check | 否,**違反 Core 邊界三原則「無選擇」優先處理** |
| **17**(av3)| **`field_mapper.py:194-198` 對 stock_dividend 事件 vf 計算錯誤** — 把 cash + stock dividend 統一寫 vf=1.0(只對 cash 對) | ~~修法~~ **已實作**(commit 6):`post_process._recompute_stock_dividend_vf` 用 vf = 1/(1 + stock_div/10) UPDATE 修正;`scripts/fix_p1_17_stock_dividend_vf.sql` 一次性 backfill 既存資料 | av3 Test 3 預期 vol_ratio:7.61→0.568 / 0.42→0.960 / 2.64→0.791 | ✅ **r3.1 已修**(限制:面額非 10 個股不精確,後續 P2 改成查 par_value 動態算) |

**P0 自動帶解項**(見 §1.2):
- `produce_facts` 無示範 → 由 §6.5 第 1 條解
- Fact 極值類事實歸屬 → 由 §6.5 第 4 條解
- Combined 模式 params_hash → 由 §6.5 第 7 條解

---

## 五、🟡 P2 級 Polish(r3:共 15 項,含 C 系列 promote 4 項)

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
12. **(C7)Bollinger PriceSource 進 params_hash 規則明示** — `BollingerParams.source: PriceSource` 7 個 enum 是否進 hash 沒講 → bollinger(20,2,Close) vs bollinger(20,2,Hlc3) 可能 collide(r3 promote)
13. **(C8)RSI/MFI overbought/oversold thresholds 進 params_hash 明示** — 兩 Core 的 thresholds 是 params with default,不進 hash 會導致「overbought_streak」Fact 不可重現(r3 promote)
14. **(C9)Pattern Stage 4a/4b timeout / retry / partial-failure semantics** — `indicator_cores_pattern.md §2.3` 只列 stage 順序(r3 promote)
15. **(C10)OBV anchor reversibility on data re-ingestion** — volume §3.6 只給 default anchor=None,沒講 immutability(r3 promote)

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

**Track A(可立即啟動,獨立)**(r3 補):
- **P0-9(C2)跨 Core warmup 合成規則**:overview §7.3 補 max() 規則,獨立可立修
- **P0-10(C3)NeelyDiagnostics dropped_scenario_ids**:與 P0-6 同檔同次改

**Track B(需 user 決議)**:
6. **P0-6 Forest overflow**:user 拍板方案 1 vs 方案 2(P0-10 schema 改動同檔同次跟著做)

**Track C(依賴 A-V3 結論)**:
7. **P0-2 A-V3**:blueprint 端先驗證 → 結論定案後連動 P0-3 + **P0-8(C1)volume 合併與後復權順序**

### Phase 1:動工同時或 P0 Gate 後立刻處理(r3:**16 項 P1 + P0 帶解 3 項 = 19 引用列**)

依 §四 表執行。新增 P1-14(C4)/ P1-15(C5)/ P1-16(C6)。

### Phase 2:Polish(r3:**15 項**)

依 §五 表執行。新增 P2-12 ~ P2-15(C7-C10)。

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
>
> **r2.1 修正**:r2 §九 r2-3 把「K-1 + Silver 表名 + 欄位級對齊」三件規模差 10× 的事併同輪。r2.1 拆 r2-3a(改 1 行)/ r2-3b(三子類表格)/ r2-3c(8 個 Core 補審),原 r2-4 拆 r2-4(TW-Market 端)/ r2-5(Wave 端,P3 限定),節編號 `D3 §6.3 forest overflow` 修正為 `neely_core §十二 12.2-12.3`(實際位置)。
>
> **r3 整合**:把 C 系列 10 條 promote 進 r2-X 編組;C1 併進 r2-4(TW-Market)、C3 併進 r2-5(Wave)、C2 併進 r2-2(overview §7.3)、C4-C6 各歸位、C7-C10 進 r2-Polish。

| 輪次 | 範圍 | 目標 |
|---|---|---|
| **r2-1**(blueprint 端) | A-V3 驗證結論 + blueprint 附錄 B DDL 補完 | 解 P0-2 + P1-9 |
| **r2-2**(overview 端) | overview §3(trait)/ §6.5(M3 契約)/ §7.5(dirty)/ §十(Core 邊界)/ **§7.3 跨 Core warmup max() 規則(C2)**/ **§7.4 params_hash 演算法(C5)**| 解 P0-4 / P0-7 / P0-集中包 / Core 邊界三原則 / **P0-9** / **P1-15** + 部份帶解 P1-8 / P1-12 / 合併項 |
| **r2-3a** | chip_cores K-1(§4.4 砍 margin_maintenance,1 行)| 解 P0-1 |
| **r2-3b** | 三子類 §一 表格全部加 `_derived` 後綴 + 補「Core 讀 Silver」一行 | 解 P1-6 |
| **r2-3c** | Fundamental / Environment 規格欄位級對齊補審(8 個 Core 對 blueprint Silver 欄位逐欄驗證)| 解 P1-13 (R2 §2.4 殘餘)|
| **r2-4**(TW-Market 端) | D2 §9.2(_fwd 職責集中說明)+ **D2 §五 5.1(volume 合併與後復權順序明示,P0-8/C1 連動 A-V3)**| 解 P0-3 + **P0-8** |
| **r2-5**(Wave 端) | **D3 §八 加 `dropped_scenario_ids`(剛性,P0-10/C3)** + D3 §1.2 寫 WaveCore trait signature + `neely_core §十二 12.2-12.3`(forest overflow)+ **D4 §三 Combined Params 拆(P1-14/C4)**| 解 P0-3 補件 / P0-4 真實 gap / P0-6 / **P0-10** / **P1-14** |
| **r2-6**(P3 限定) | D4 §附錄 A §2.x(price_len 度量空間,P3 限定)+ **D4 §附錄 C Fibonacci tiebreak(P1-16/C6)**| 解 P0-5(P3 開工時)+ **P1-16** |
| **r3-Polish**(下次 PR 帶) | C7-C10 + 既有 P2 11 項 = 15 項 polish | 解 P2 級全部 |

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

### 10.3 r2.1 對 r2 的事實/流程修正(共 12 處)

> **驗證方法**:從 origin 取齊 11 篇 Core spec 到 /tmp/cores/(共 5686 行),3 個平行 Explore agent 對 r2 的 43 條具體 claim 逐條對 spec 原文 spot-check,以下修正每條都附 spec 引用。

**A 系列 — 事實錯誤(7 處)**:

| # | r2 錯誤 | spec 原文真相 | r2.1 修正位置 |
|---|---|---|---|
| A1 | 共識點 ① 受影響 Core 數寫「4 個(obv/vwap/mfi/bollinger)」 | `indicator_cores_volatility.md §3.2-3.4` `BollingerParams` 只用 period/std_multiplier/source/timeframe,**完全不吃 volume** | §2.1 共識點 ① 改 3 個(obv/vwap/mfi)+ §1.1 P0-2 影響範圍同步 |
| A2 | P0-2 (A-V3) 動作含「volume Indicator Core §2.4 加註依賴 A-V3」 | `indicator_cores_volume.md §2.4` 已明寫「吃處理過的 volume(已隨除權息調整)」 | §三 P0-2 修正動作刪該 bullet,改補 r3 預備 C1 |
| A3 | P0-5 (price_len) 範圍含 D3(neely_core) | neely_core 整篇沒有 price_len(monowave 用 ATR);只有 traditional_core §附錄 A 用 price_len | §1.1 P0-5 + §三 P0-5 改 D4 限定 + 降為 P3-限定;依賴鏈圖移出 P0-2 下游 |
| A4 | P0-3 寫「§九 lacks §9.2」 | tw_market_core §五 5.3 + §九 9.1 已有,只是散落 | §三 P0-3 改「兩處資訊散落,需補 §9.2 集中說明」 |
| A5 | P0-6 動作寫「前端必須顯示橫幅契約寫入 §18.4」 | neely_core §十八 18.4 已存在 banner | §三 P0-6 改「真實補件是 §八 Output 加 `dropped_scenario_ids`(剛性)」 |
| A6 | P0-1 描述「(含 enum 與 Fact 範例不一致)」 | `maintenance_low` 是 metadata tag,`MarginEventKind` enum 是不同層級 | §1.1 P0-1 描述精簡為「§4.4 line 184 欄位移除」 |
| A7 | 全文多處引用「D3 §6.3 forest overflow」 | 真實位置 `neely_core.md §十二 12.2-12.3` | §九 r2-5 修正節編號(原 r2-4 拆) |

**B 系列 — 流程/邏輯問題(5 處)**:

| # | r2 流程問題 | 真實情況 | r2.1 修正位置 |
|---|---|---|---|
| B1 | 編號統計 §1.2「14 項」vs §四「13 項」+「P0 帶解 3 項」三處對不上 | r2 §10.2 #3 說已修但實際換了一種不一致 | §1.2 + §四 表頭統一計法,標明「12 條 P1 + 3 條 P0 帶解」 |
| B2 | 依賴鏈圖 P0-2 → P0-5(連動還原版本) | P0-5 屬 traditional_core(P3),neely 沒用 price_len,不該被 P0 阻塞鏈拖 | §1.1 依賴鏈圖移除 P0-2 → P0-5 連結,標「P0-5 降 P3-限定」 |
| B3 | 共識點 ③ §6.5「自動帶解」P1-8 / P1-12 / 合併項 | P1-12 需 D4 §三 同步改;P1-8 spec 已明寫由 bollinger/atr 產出;produce_facts 各 Core 仍要寫範例 | §1.2 三條表格列改「P0 部份帶解,仍須 D 系列規格內補」 |
| B4 | 共識點 ② 把「12 個非 OHLCV Core 衝突 + Wave/Market 也有問題」並列 | overview §3.3 已 carve-out Wave/Market trait;真實 gap 是 Wave/Market trait signature 沒給(三 spec §1.2 都標「草案」)| §2.1 共識點 ② 開頭加 carve-out 說明 + 修真實 gap |
| B5 | r2-3 編組散度太大(K-1 改 1 行 vs 欄位級對齊 8 個 Core 同輪)| 規模差 10× | §九 r2-3 拆 r2-3a(K-1)/ r2-3b(表名)/ r2-3c(欄位級對齊),原 r2-4 拆 r2-4(TW-Market)/ r2-5(Wave,P3 限定)|

### 10.4 r3 對 r2.1 的整合修正(C 系列 promote,共 10 處 + r3.1 av3 新增 2 條)

> **整合動作**:r2.1 §十一「r3 預備清單」的 10 條 r2 漏抓 gap,本版本(r3)正式 promote 進 P0/P1/P2 表 + 動工編組。
>
> **r3.1 av3 動工新增**:user 本機 PG 17 跑 av3_spot_check.sql 揭露 Rust split volume bug + field_mapper stock_dividend bug,新增 P0-11 + P1-17。

| 編號 | r3 編號 | 嚴重度 | 修正位置 | 動作摘要 |
|---|---|---|---|---|
| **C1** | **P0-8** | 🔴 P0 | tw_market_core §五 5.1 + 連動 P0-2 | volume 合併與後復權順序明示;A-V3 結論定案後 5 分鐘修文 |
| **C2** | **P0-9** | 🔴 P0 | overview §7.3 補規則 | 跨 Core warmup 合成規則 = max() across cores;Wave/Pattern 例外標 `core_kind=full_recompute` |
| **C3** | **P0-10** | 🔴 P0 | neely_core §八 Output schema(與 P0-6 同檔)| 加 `overflow_dropped_scenario_ids: Vec<String>` + `overflow_dropped_count: usize`(剛性,無論 P0-6 方案 1/2)|
| **C4** | **P1-14** | 🟠 P1 | traditional_core §三 | Combined Params 結構拆 `engine: Combined { frost_params, ramki_params }` |
| **C5** | **P1-15** | 🟠 P1 | overview §7.4 + 各 Core spec | params_hash 演算法明示 include 全部 Params 欄位(anchor / mode / timeframe / source / thresholds 都進 hash)|
| **C6** | **P1-16** | 🟠 P1(優先)| D4 §附錄 C | Fibonacci ratio 多重命中 deterministic tiebreak;**違反「無選擇原則」優先處理** |
| **C7** | **P2-12** | 🟡 P2 | indicator_cores_volatility | Bollinger PriceSource 進 params_hash |
| **C8** | **P2-13** | 🟡 P2 | indicator_cores_momentum + volume | RSI/MFI thresholds 進 params_hash |
| **C9** | **P2-14** | 🟡 P2 | indicator_cores_pattern §2.3 | Stage 4a/4b timeout/retry/partial-failure semantics |
| **C10** | **P2-15** | 🟡 P2 | indicator_cores_volume §3.6 | OBV anchor reversibility on data re-ingestion |
| **(av3-1)** | **P0-11** | 🔴 P0(production bug)| `rust_compute/src/main.rs` AdjEvent + load_adj_events + compute_forward_adjusted | Rust 改用 `volume_factor` 不用 `adjustment_factor`(4 處改);user 須 cargo build + 重跑 Phase 4 |
| **(av3-2)** | **P1-17** | 🟠 P1(P0-11 後浮現)| `src/field_mapper.py:194-203` | stock_dividend 事件 vf 計算錯誤(混淆 cash 跟 stock dividend);需根據 stock_dividend 值算正確 vf |

**動工順序更新**(§七 已同步):
- **Track A**(可並行):P0-1 / P0-4 / P0-7 / P0-9(C2)/ P0-集中包 / Core 邊界三原則
- **Track B**(user 決議):P0-6 + P0-10(C3 同檔同次)
- **Track C**(依 A-V3):P0-2 → P0-3 + P0-8(C1)
- **Track D**(av3 production bug):**P0-11**(獨立,優先)+ P1-17(P0-11 後處理)

---

## 十一、C 系列整合對應追溯表(r3 從 r2.1 §十一 promote)

> **歷史保留**:r2.1 把 C1-C10 列為「r3 預備清單」。r3 完成 promote,本節改為**追溯對應表**,讓未來引用「C2」/「P0-9」/「overview §7.3」三種命名都能對得上。

| 原 r2.1 編號 | r3 編號 | 嚴重度 | spec 修正位置 | r3 動作位置(章節)|
|---|---|---|---|---|
| C1 | P0-8 | 🔴 P0 | tw_market_core §五 5.1 | §1.1 / §三 P0-8 / §九 r2-4 |
| C2 | P0-9 | 🔴 P0 | cores_overview §7.3 | §1.1 / §三 P0-9 / §九 r2-2 |
| C3 | P0-10 | 🔴 P0 | neely_core §八 | §1.1 / §三 P0-10 / §九 r2-5 |
| C4 | P1-14 | 🟠 P1 | traditional_core §三 | §1.2 / §四 #14 / §九 r2-5 |
| C5 | P1-15 | 🟠 P1 | cores_overview §7.4 | §1.2 / §四 #15 / §九 r2-2 |
| C6 | P1-16 | 🟠 P1(優先)| traditional_core §附錄 C | §1.2 / §四 #16 / §九 r2-6 |
| C7 | P2-12 | 🟡 P2 | indicator_cores_volatility | §五 #12 / §九 r3-Polish |
| C8 | P2-13 | 🟡 P2 | indicator_cores_momentum + volume | §五 #13 / §九 r3-Polish |
| C9 | P2-14 | 🟡 P2 | indicator_cores_pattern §2.3 | §五 #14 / §九 r3-Polish |
| C10 | P2-15 | 🟡 P2 | indicator_cores_volume §3.6 | §五 #15 / §九 r3-Polish |

**spec 引用一覽**(用於下次 grep 對齊驗證):

| spec 文件 | 被觸及的章節 | r3 編號 |
|---|---|---|
| `cores_overview.md` | §3 / §6.5(新)/ §7.3 / §7.4 / §7.5(新)/ §十 | P0-4 / P0-集中包 / P0-9 / P1-15 / P0-7 / P0-Core邊界 |
| `chip_cores.md` | §4.4 + §一 表格 | P0-1 / P1-6 |
| `fundamental_cores.md` | §一 表格 + 各 Core 欄位級 | P1-6 / P1-13 |
| `environment_cores.md` | §一 表格 + 各 Core 欄位級 + §3.1 taiex 資料源 | P1-3 / P1-6 / P1-13 |
| `tw_market_core.md` | §四 MergeAtLimitPrice / §五 5.1(C1) / §六 6.1 / §九 9.2(新)| P1-7 / **P0-8** / P0-2 / P0-3 |
| `neely_core.md` | §一 1.2 trait signature / §八 Output(C3)/ §十二 12.2-12.3 forest overflow / §十八 18.4 banner | P0-4 / **P0-10** / P0-6 / (已存) |
| `traditional_core.md` | §一 1.2 trait / §三 Combined(C4)/ §附錄 A R15 / §附錄 A price_len(P3)/ §附錄 C Fibonacci(C6)| P0-4 / **P1-14** / P1-11 / P0-5 / **P1-16** |
| `indicator_cores_momentum.md` | RSI thresholds 進 hash | **P2-13** |
| `indicator_cores_volatility.md` | Bollinger source 進 hash + Fact 極值描述語言 | **P2-12** / P1-8 |
| `indicator_cores_volume.md` | OBV anchor reversibility / VWAP anchor 進 hash | **P2-15** / P1-15 |
| `indicator_cores_pattern.md` | §2.3 Stage 4a/4b semantics | **P2-14** / P1-10 |

---

## 十二、r3 漏抓自我警示

r3 完成 C 系列 promote。**仍可能存在的三階遺漏**(r3 → r4 偵測項):

1. 11 篇 Core spec 中,3 個 agent 用 43 條 claim 抽樣驗證(覆蓋約 60-70% 顯眼問題),**未驗到的內部演算法細節**(如 Neely R1-R7 / Z1-Z4 / T1-T10 閾值)仍未審 → **下輪需專門 audit Neely 內部驗證器**
2. **`params_hash` 16 是字元數 vs byte 數**仍未對齊:overview §7.4 寫「前 16 字元」,m2/schema_m2_pg.sql line 49 用 `VARCHAR(16)`,若實作端取 16 byte = 32 hex 字元就 mismatch → **P1-15(C5)動工時必驗**
3. cores_overview §3 trait `compute(ohlcv: &OHLCVSeries)` 跟 §3.3「Wave/Market 不強制」之間的關係,**spec 本身還沒改**(只在統合報告 §2.1 給 associated type 草案範例)→ r2-2 真正動工時要把草案落地進 spec
4. 全文在 §1.2 / §四 / §五 加了「(C-N)→ P-N」雙編號,但**表內的編號順序沒重排**,外部引用既存「P1-7」/「P1-12」維持不變;**新引用建議用 r3 編號**(P0-8/P1-14 等)
5. r3 動工順序 §七 Track A/B/C 重排,但**沒寫各 PR 切法**(哪幾條同 PR / 哪幾條跨 PR);user 動工前需自行決議
6. **C 系列 agent 抽樣未涵蓋的 spec 區段**:`indicator_cores_momentum.md` 9 個 Core(macd/rsi/kd/adx/ma/ichimoku/williams_r/cci/coppock)只查 RSI threshold 一條,其他 8 個 Core 內部演算法仍未 spot-check

下輪(r4 / 動工複審)應**先做一次跨 spec 全文 grep**確認:
- (a) `price_daily_fwd` 出現處的層級(raw/Bronze/Silver)
- (b) `params_hash` 字長/字元/byte 用語
- (c) `dropped_scenario_ids` schema 已加未加
- (d) trait signature 已寫未寫
- (e) Wave/Pattern Core `core_kind=full_recompute` 標記是否各 spec §1 都加(對應 P0-9 例外)
- (f) Indicator 9 個 Momentum Core 的演算法細節(P3 後)

---

**(統合報告 r3 完)**
