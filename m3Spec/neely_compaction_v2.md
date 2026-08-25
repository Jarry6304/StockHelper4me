# neely_core Compaction v2 — Tiling-Round 引擎規格

**狀態**:r3 draft(2026-08-26)
**取代對象**:`compaction/exhaustive.rs` v3.7 遞迴迴圈 + `compaction/three_rounds.rs` 弱比對聚合
**上位文件**:`m3Spec/neely_core_architecture.md` r6(下稱 architecture)、`m3Spec/neely_rules.md`(下稱 rules)
**參照實作**:`rust_compute/cores/wave/traditional_core/`(v3 fractal round 引擎,生產驗證)
**本文件不含實作程式碼**;型別以合約表呈現,實作於 G2.x 各 PR 落地。

---

## r3 修訂摘要

- 收掉 r2 殘餘風險:Q3 決議由 Gate v3 抽驗**提前至 G2.2 六檔雙軌實驗**(端點版 vs bars 反查版並跑,量 Overlap / 回測判定翻轉率;> 5% 即於 G2.2 落 bars 反查)。
- §12 決議順序改 **Q3 → Q1 → Q6**,與附錄 C 影響評級一致;觸發式提前條款移除(已無必要)。
- 附錄 B G2.2 交付與 gate 同步納入 Q3 實驗;附錄 C Q3 可逆成本與殘餘風險紀錄更新。
- Q3 仍列開放問題(定案機制已確定、結論待實驗);Q1 / Q6 不動。

## r2 修訂摘要

- Q2 / Q4 / Q5 裁決收案,移入 §10 ADR(A-8 ~ A-10);§12 縮編為 Q1 / Q3 / Q6 三題,**編號維持不變**以保交叉引用穩定;「重新定序」作用於決議順序,非編號。
- A-8 配套:新增 `level_cap_hit` 診斷旗標(§5.2 / §9.2 / 附錄 A / G2.1),把輪數上限的靜默缺漏轉為可觀察。
- A-9 配套:CombinationKind 細分由「留 Gate 校準」改排入 G2.3(§4.2.1 / 附錄 B)。
- A-10 配套:pattern_isolation_anchors 取覆蓋葉 union 定案(§7.2 / G2.3)。
- 新增附錄 C:開放問題影響層級與決議順序(裁決依據存檔,含殘餘風險紀錄)。
- Q1 / Q3 / Q6 維持開放,原裁定與期限不動。

## r1 修訂摘要

- 正式記錄現行 Compaction 三層缺陷(§1.2 D-1 ~ D-6),含檔案級證據。
- 定義 Tiling 六條不變量 I1–I6(§2.2),為全引擎正確性基準。
- 決策 D1:聚合定義域由「forest(重疊替代解)」改為「tiling(時間軸連續分割)」;否決 compatibility-graph 橋接案(§3.2)。
- 決策 D2:Stage 3–7 收編為 round 引擎 round-1,shadow 雙軌過渡(§3.3)。
- 視窗接受函式 `try_all_neely` 七階梯 W1–W7 規格(§4)。
- Level-N 逐欄語意表:Scenario 全 23 欄在聚合節點上的填值規則(§7.2)。
- G2.0 止血補丁規格與測試案例清單(§8);P0 Gate v3 驗收門檻(§9)。

---

## 目錄

- 一、定位與問題陳述
- 二、術語與不變量
- 三、架構總覽與核心決策
- 四、視窗接受函式 try_all_neely
- 五、Round 引擎
- 六、Round 2 Reassessment / Complexity / Degree
- 七、輸出合約變更
- 八、G2.0 止血規格(可先行)
- 九、驗收:P0 Gate v3
- 十、風險與否決方案
- 十一、非目標(YAGNI)
- 十二、開放問題
- 附錄 A:工程參數變更
- 附錄 B:里程碑對映
- 附錄 C:開放問題影響層級與決議順序

---

## 一、定位與問題陳述

### 1.1 範圍

本規格重定義 neely_core Stage 8(Compaction)的**聚合定義域、視窗接受條件、每級規則重驗、輸出語意**。不動 Stage 1–2(monowave 偵測/分類)與 Stage 9–12(missing wave / power rating / fib / triggers / degree ceiling / hints)的內部邏輯,但重定其輸入來源(§7)。

### 1.2 現行缺陷(正式記錄)

| 編號 | 缺陷 | 證據 | 後果 |
|---|---|---|---|
| D-1 | **聚合定義域錯誤**:`aggregate_one_level` 對 forest 清單滑窗,forest 是重疊替代解讀,非時間分割 | `three_rounds.rs::aggregate_one_level` 對 `scenarios[start..start+N]` 取窗;Stage 3 候選為 wave_count ∈ {3,5,7,11} 重疊滑窗,重疊是構造必然 | 時間上互相重疊的替代計數可被拼成 Level-1 形態,結構無意義 |
| D-2 | **無相鄰性檢查**:視窗內只驗 label 序列、方向交替、S&B、邊界 Fib²,不驗 `child[i].end == child[i+1].start` | `try_aggregate_3/5/7/11` 全部;`build_aggregated` 直接 clone `window[*].wave_tree` 為 children | children 可重疊或有間隙,違反波浪嵌套定義 |
| D-3 | **每級不重驗規則**:聚合 scenario `passed_rules = []`、`rules_passed_count = 0`,不過 Stage 4 | `build_aggregated` 置空全部規則欄位 | 違反 rules Ch7「壓縮後須回 Ch3 重新評估」;連帶 BeamSearchFallback 雙重排序(architecture §10.3)第二鍵失效,Level-N 組內永遠墊底 |
| D-4 | **Round 2 邊界波語意錯位**:m(−1)/m(+1) 應為視窗**外**前後鄰居,現以視窗**內**第 1/2 段近似 | `boundary_retracement_extreme` 與 `build_round_advisories` 取 `window[0]/window[1]` | rules line 1249-1251 動作 B 未被真實作;無 tiling 即無「鄰居」概念 |
| D-5 | **Terminal Impulse 產不出**:3-3-3-3-3 序列一律判 Triangle | `try_aggregate_5` 全 `:3` 分支唯一出口為 `Triangle{Contracting}`;rules Ch7 表(line 1803-1811)同序列另有 Terminal Impulse → `:5` | Level-N 永久缺一類 `:5`,上一級 Impulse/Zigzag 聚合連鎖受阻 |
| D-6 | **文件與測試債**:`lib.rs` header 仍稱 pass-through;`exhaustive.rs` 測試 scenario 日期全為同日退化值 | `lib.rs` Stage 8 註釋;`exhaustive.rs` tests `"2026-01-01"→"2026-01-01"` | spec-first 儲存庫中文件說謊;相鄰性從未被測試觸及 |

### 1.3 與既有 spec 的關係

| 文件 | 關係 |
|---|---|
| architecture §7.1 Pipeline / §7.4 Three Rounds 對應 | 本規格重編 Stage 3–8 為 round 引擎(§3.3),§7.1 需隨 G2.1 落地改版 r7 |
| architecture §10(Forest 上限保護)/ §10.3 雙重排序 | 護欄機制保留;beam 排序鍵沿用,D-3 修復後第二鍵恢復有效 |
| architecture §6.6 / §4.5 不可外部化 | Neely 規則常數維持寫死;本規格新增之 `round_beam_size` 等屬工程參數,進 NeelyEngineConfig(附錄 A) |
| rules Ch4 Three Rounds(line 1198-1256) | Round 1/2 由 round 引擎逐輪實現;Round 3 暫停 = 收斂條件(§5.3) |
| rules Ch7 Compaction 表(line 1803-1811) | `:3` / `:5` 閉合表,為 I5 不變量依據(§2.2) |
| rules §Rule of Similarity & Balance(line 1189-1197) | 保留為相鄰配對謂詞 W4(§4.2) |
| traditional_core `compaction.rs` / `node.rs` / `mode.rs` | 基礎設施移植來源;`Mode` ↔ `StructureLabel :3/:5` 對應(§3.4) |

---

## 二、術語與不變量

### 2.1 術語

| 詞 | 定義 |
|---|---|
| **Tiling** | 對時間區間 [t₀, t_N] 的一個**連續分割**:有序節點串,無重疊、無間隙。一個 tiling = 一種對整段行情的完整解讀路徑 |
| **CompactionNode** | 引擎內部節點(internal-only,不進 wire contract)。degree_level 0 為 monowave 葉;≥1 為聚合形態 |
| **Round** | 對「當前所有 tilings」各枚舉連續視窗並嘗試聚合的一輪;成功聚合產生新 tiling 分支 |
| **視窗** | tiling 內連續 3 / 5 / 7 / 11 個節點 |
| **degree_level** | 結構嵌套層級(整數,葉=0)。與 Neely 命名度數(Degree,11 級)分離;對映規則見 §6.3 |
| **base_label** | 節點壓縮後的 `:3` / `:5`(rules Ch7 表) |
| **凍結(freeze)** | 引擎結束時將 CompactionNode 樹轉為輸出用 `WaveNode` + `Scenario` 的單向轉換 |

### 2.2 不變量 I1–I6

所有 round、所有 tiling、所有節點,任一時刻必須滿足:

| 編號 | 不變量 | 檢查點 |
|---|---|---|
| I1 | **Partition**:tiling 節點依時間排序,聯集覆蓋 [t₀, t_N],兩兩不重疊 | 每 round 後 debug 檢查;Gate v3 production 統計 |
| I2 | **共享端點**:`node[i].end_date == node[i+1].start_date` 且 `node[i].end_price == node[i+1].start_price`。依據 monowave 慣例:新 monowave 自前一段 extreme 起算(`pure_close.rs`),端點日期精確相等,**不使用容差** | 視窗接受 W1;凍結前全樹掃描 |
| I3 | **遞迴分割**:任一 degree_level ≥ 1 節點的 children 構成其自身時間範圍的 tiling(I1/I2 遞迴成立) | 凍結時遞迴驗證 |
| I4 | **層級單調**:`parent.degree_level == max(children.degree_level) + 1` | 聚合建構時 |
| I5 | **Label 閉合**:parent 的 base_label 由 rules Ch7 表唯一決定(§4.2 W2 表);children base_label 序列必須是表中合法序列 | 視窗接受 W2 |
| I6 | **Forest 收集**:最終 forest = 全部 tilings 中 degree_level ≥ 1 節點,依 canonical_key 去重;順序不反映優先級(architecture §8.2 不選 primary 不變) | 凍結階段 |

違反任一不變量 = 引擎 bug,不是資料問題;production 出現即 Gate 紅燈(§9.2)。

---

## 三、架構總覽與核心決策

### 3.1 目標架構

```mermaid
flowchart TD
    A[classified monowaves<br/>= degree-0 base tiling<br/>天然滿足 I1/I2] --> R{Round N 迴圈}
    R --> B[對每個 tiling 枚舉<br/>連續視窗 3/5/7/11]
    B --> C{try_all_neely}
    C -->|接受 k 種形態| D[每種形態產一個新 tiling:<br/>視窗換成 parent,其餘節點保留]
    C -->|拒絕| B
    D --> E[per-round dedup canonical_key<br/>→ beam-cap round_beam_size]
    E --> F{本輪有任何聚合?}
    F -->|有| R
    F -->|無 = Round 3 暫停| G[凍結:collect degree≥1 nodes<br/>→ Scenario forest]
    G --> H[forest_max_size / BeamSearchFallback<br/>護欄(不變,architecture §10)]
    subgraph try_all_neely 七階梯 W1-W7
      W1[W1 相鄰性 I2] --> W2[W2 label 序列<br/>Ch7 表 + Figure 4-3]
      W2 --> W3[W3 方向交替]
      W3 --> W4[W4 S&B 0.382–2.618]
      W4 --> W5[W5 Ch5 Validator<br/>端點價格版]
      W5 --> W6[W6 Classifier<br/>含 Terminal vs Triangle 判別]
      W6 --> W7[W7 內部比例極端檢查<br/>Fib² 0.236–4.236]
    end
```

### 3.2 決策 D1:定義域 = tiling(否決 compatibility-graph)

| 方案 | 內容 | 判定 |
|---|---|---|
| **A(採用)** | base tiling = classified monowaves;聚合永遠作用在 tiling 上,分支即替代解讀 | 不變量可局部維護(替換連續視窗天然保 I1/I2);traditional v3 生產驗證同構 |
| B(否決) | 保留現行 Stage 3–7 重疊 forest,事後建相容圖(節點=scenario,邊=時間相鄰),枚舉 maximal chains 拼 tiling,未覆蓋區段以 monowave 補洞 | 等價於 set-packing 枚舉,複雜度不低於方案 A;產生兩套視窗語意(Stage 3 滑窗 vs chain 枚舉);補洞規則額外複雜。否決 |

### 3.3 決策 D2:Stage 3–7 收編為 round-1(shadow 過渡)

現行 Stage 3(monowave 滑窗)→ Stage 4(Ch5 Validator)→ Stage 5(Classifier)→ Stage 6(Post-Constructive)→ Stage 7(Complexity 篩選),本質上就是「round-1 作用在 degree-0 tiling」。收編後:

| 現行 Stage | 收編位置 | 備註 |
|---|---|---|
| 3 候選滑窗 + 交替過濾 | round 引擎視窗枚舉 + W3 | beam_width × 10 cap 改由 round_beam_size 承接 |
| 3.5 Pattern Isolation / DETOUR | 引擎外圍資訊性 stage,輸入改 base tiling | 產出 anchors 語意不變 |
| 4 Validator | W5(泛化為端點介面,§4.3) | 同一套規則碼,雙形態輸入 |
| 5 Classifier | W6 | 同一套分類碼 |
| 6 Post-Constructive | W6 之後、接受之前(pattern_complete = false → 拒絕視窗) | 由「事後過濾 forest」改為「聚合門檻」 |
| 7 Complexity 篩選 | 凍結階段對 forest 套用 | 位置不變 |
| 8 現行 exhaustive/three_rounds | **刪除**,由 round 引擎取代 | `three_rounds.rs` 弱比對(label 序列 + S&B)是 Validator/Classifier 的劣化複製,整檔淘汰;S&B 謂詞遷入 W4 |

**過渡策略(verification-first)**:G2.1–G2.2 期間新舊雙軌並行(shadow mode)——舊 pipeline 照常產 forest 供比對,新引擎產出僅寫 diagnostics;Gate v3 通過後一個 PR 內切換並刪舊路徑。shadow 比對規則見 §9.3。

### 3.4 決策 D3:traditional v3 構件移植對照

| traditional 構件 | 處置 | Neely 側調整 |
|---|---|---|
| tiling 替換式聚合(`aggregate_one`) | 直接移植 | 視窗長度集合改 {3,5,7,11} |
| `Rc` 節點共享 | 直接移植 | 深拷貝曾致 traditional ~100s/股,**一開始就用 Rc**,不重走 |
| per-round `canonical_key` dedup | 移植 | key 定義見 §5.4 |
| `beam_cap` | 移植 | 排序鍵改 Neely 版(§5.5) |
| `mode.rs` Motive/Corrective + slot 表 | **不移植** | Neely 已有等價物:`StructureLabel :3/:5` + rules Ch7 表(I5) |
| `patterns::try_all` | 不移植 | 換 `try_all_neely`(§4),重用既有 Validator/Classifier |
| terminal tiling 保留(un-aggregated 當「停在此度數」alternate) | 移植 | 對齊「forest 含所有 level」現行行為 |

---

## 四、視窗接受函式 try_all_neely

### 4.1 合約

| 項 | 內容 |
|---|---|
| 輸入 | 視窗 = 同一 tiling 內連續 3 / 5 / 7 / 11 個 `CompactionNode`;全域 `bars`(volume/gap 事實用);`NeelyEngineConfig` |
| 輸出 | 0..k 個 `AcceptedPattern`(同一視窗可有多種合法解讀,各自產生 tiling 分支) |
| 純函式性 | 不得改動輸入節點;可 memoize(§5.6) |
| 失敗記錄 | 每個被 W5 拒絕的視窗產 `RuleRejection`(含 gap),進 NeelyDiagnostics——恢復 architecture §15.1「完整保留拒絕原因」在 Level-N 的覆蓋 |

`AcceptedPattern` 合約欄位:

| 欄位 | 型別 | 語意 |
|---|---|---|
| pattern_type | NeelyPatternType | W6 產出 |
| base_label | StructureLabel | 由 I5 表決定 |
| validation | ValidationReport | W5 產出(passed / deferred / failed 全記) |
| sb_metrics | (price_ratio, time_ratio) 序列 | W4 產出,供 advisory |

### 4.2 七階梯(cheap → expensive,任一階失敗即短路)

| 階 | 檢查 | 規格 |
|---|---|---|
| W1 | 相鄰性 | I2 全視窗成立。tiling 建構已保證,此處為 `debug_assert` 級防衛;違反 = 引擎 bug,panic(debug)/ 記 Engineering rejection(release) |
| W2 | label 序列 | 視窗 children 的 base_label 序列必須匹配下表(rules Ch7 line 1803-1811 + rules Ch4 Figure 4-3);monowave 葉(degree 0)的 base_label 取其 Stage 0 Pre-Constructive 候選集合,**任一候選匹配即過**(候選多義性延後到 W5/W6 消解) |
| W3 | 方向交替 | 相鄰節點 net direction 交替(Up/Down;Neutral 節點不進聚合視窗)。net direction = end_price − start_price 符號;例外見 Q1(Running 類) |
| W4 | S&B | 每相鄰對 price magnitude 或 time duration 其一之比 ∈ [0.382, 2.618](rules line 1189-1197)。price 取節點端點差絕對值;沿用現行 `Option` 語意:price 不可得時退 time 單維 |
| W5 | Ch5 Validator | 端點價格版規則重驗,規則適用表見 §4.3 |
| W6 | Classifier | 定 pattern_type 與變體;**含 D-5 修復**:3-3-3-3-3 分岔判別見 §4.4 |
| W7 | 內部比例極端 | 視窗內相鄰對 magnitude 比落 [0.236, 4.236](Fib²)外 → 拒絕(沿用 v4.8 G1.3 閾值)。**語意修正**:此為視窗內部先驗,不再冒充「邊界波」;真邊界波檢查移 Round 2(§6.1) |

### 4.2.1 W2 合法序列表(I5 閉合表)

| children base_label 序列 | 候選形態(交 W6 分辨) | parent base_label |
|---|---|---|
| :5 :3 :5 :3 :5 | Trending Impulse | :5 |
| :5 :3 :5 | Zigzag(Single) | :3 |
| :3 :3 :5 | Flat(七變體交 W6) | :3 |
| :3 :3 :3 :3 :3 | Contracting/Expanding Triangle **或** Terminal Impulse(§4.4) | Triangle → :3;Terminal → :5 |
| :3 ×7(3+x+3) | Double Combination 族 | :3 |
| :3 ×11(3+x+3+x+3) | Triple Combination 族 | :3 |

含 x-wave 形態一律 `:3`(rules Ch7 表末列);7/11 視窗中 x-wave 位置(第 4 / 第 4、8 節點)之判別為 W6 責任,現階段沿用「位置慣例 + DoubleThree/TripleThree 通用 kind」近似,細分已裁決排入 G2.3(A-9):依 rules Ch8 Table A/B 對映至既有 11-variant `CombinationKind`,判定規則與 monowave 級 `classify_7wave_combination` 同源,取代原「留 Gate 校準」處置。

### 4.3 W5 規則適用表(Level-N)

原則:**bar 級概念不上樓,價格結構規則全上樓**。「波」的介面泛化為端點結構(start/end date、start/end price、duration_bars),monowave 與聚合節點同構餵入。

| 規則群 | Level-N 適用 | 調整 |
|---|---|---|
| Ch5 Essential R1–R7 | ✅ 全數 | 價格距離、回測比全以節點端點計;R6 之 38.2% × W4、R7 之 W3 非最短照算 |
| Overlap_Trending / Overlap_Terminal | ✅ | W1/W4 價格範圍以節點端點區間計(注:端點區間非真實 high/low,見 Q3 極值精化) |
| Flat / Zigzag / Triangle 變體規則 | ✅ | 同上 |
| Equality / Alternation | ✅ | Alternation 軸(price/time/complexity)中 complexity 改用子節點 degree_level 差 |
| R3 臨界區容差(architecture §4.3,ATR 基準) | ⚠️ 替換 | ATR 為 bar 級概念;Level-N 改用 §4.2 三檔相對容差(±4%/±5%/±10%)之對應檔,**不得**為 Level-N 節點虛構 ATR |
| Rule of Neutrality / Proportion(Stage 2) | ❌ 不重跑 | monowave 級專屬;聚合節點方向由 net direction 定(W3) |
| Channeling / Ch9 | ❌ 不進接受階梯 | 維持 advisory 定位,凍結後照跑(§7.3) |

### 4.4 W6:3-3-3-3-3 分岔判別(D-5 修復)

同序列雙候選,判別依端點幾何,**兩者皆可同時接受**(各產一 tiling 分支,交 forest 並存;不選 primary 原則):

| 形態 | 必要幾何條件(端點版) | base_label |
|---|---|---|
| Contracting Triangle | b-d 線與 a-c 線收斂(斜率符號相反或同號但收斂);e 不破 a-c 線(容差 §4.2) | :3 |
| Expanding Triangle | 兩線發散;逐波擴大 | :3 |
| Terminal Impulse | 具 1-2-3-4-5 推動結構:W2/W4 不完全回測前波、W3 非最短(R7)、**W1/W4 價格範圍重疊**(terminal 特徵,Overlap_Terminal 規則反向作為必要條件) | :5 |

兩組條件皆不滿足 → 視窗拒絕(記 rejection)。趨勢線判定先用端點內建幾何;引入 trendline_core(architecture §18.3 唯一允許耦合)留 P1,見 Q3。

---

## 五、Round 引擎

### 5.1 CompactionNode 合約(internal-only)

| 欄位 | 型別 | 語意 |
|---|---|---|
| kind | Leaf \| Pattern(NeelyPatternType) | Leaf = monowave |
| base_label | StructureLabel(:3/:5)\| LeafCandidates | 葉節點持 Stage 0 候選集合;聚合節點持唯一值(I5) |
| degree_level | usize | 葉 = 0;I4 單調 |
| start_bar / end_bar | usize | 原始 bars index 區間(dedup / gap_count / volume 用) |
| start_date / end_date | NaiveDate | I2 依據 |
| start_price / end_price | f64 | monowave 為 (H+L)/2 端點;聚合節點繼承首末子節點端點 |
| children | Vec<Rc<CompactionNode>> | 葉為空;I3 遞迴分割 |
| validation | Option<ValidationReport> | W5 產出;葉為 None |
| net_direction | Up \| Down | end − start 符號;W3 用 |

**Rc 強制**:tiling 分支僅複製節點指標串,不深拷貝子樹(traditional v3 教訓:深拷貝 ~100s/股)。

### 5.2 主迴圈

1. base tiling = classified monowaves 中 **非 Neutral** 者;Neutral monowave 依現行 Stage 3 慣例排除於聚合,但保留於 bars 供事實計算。若排除後相鄰兩節點端點不再相接,以「合成葉」規則橋接:Neutral 段併入前一 directional 節點之時間範圍、價格端點取前節點 start 至 Neutral 段 end(記 diagnostics;此為現況「過濾 Neutral 後滑窗」行為的顯式化,語意不變)。
2. `tilings = { base }`;`level = 0`。
3. **迴圈**(至多 `max_compaction_levels` 輪,預設 4,A-8):
   a. 對每個 tiling、每個視窗長度 ∈ {3,5,7,11}、每個起點:呼叫 `try_all_neely`。
   b. 每個 AcceptedPattern → 生成新 tiling(視窗換 parent,其餘保留);原 tiling 一律保留(「停在此度數」alternate,對齊 traditional)。
   c. dedup(§5.4)→ beam(§5.5)。
   d. 本輪零聚合 → **Round 3 暫停**,跳出(rules line 1258-1265;`round3_pause` 語意沿用)。
   e. 因觸輪數上限跳出(非零聚合收斂)→ 標 `level_cap_hit = true` 進 NeelyDiagnostics(A-8:上限造成的枚舉缺漏由靜默轉為可觀察,供動態化決策量測)。
4. 凍結(§7)。

### 5.3 收斂與 Round 對映(rules Ch4)

| rules 概念 | 引擎對映 |
|---|---|
| Round 1(識別 Standard Series) | W2 + W3 + W4 |
| Round 2(壓縮成 base label + 重評) | AcceptedPattern 建構(I5)+ §6.1 邊界重評 |
| Round 3(暫停等新 :L 標) | 主迴圈零聚合收斂;`awaiting_l_label` 標記凍結時對「最後一輪仍可延伸但無新標」情境填寫,判定規則沿用現行 `three_rounds::apply`(Stage 8.5)並改讀 tiling 末端狀態 |

### 5.4 canonical_key(dedup)

遞迴定義:

- 葉:`L(start_bar,end_bar)`
- 聚合:`P(pattern_type_tag, base_label, start_bar, end_bar, [children keys…])`

tiling key = 節點 key 以 `;` 串接。同 key 視為同一解讀,round 內去重。pattern_type_tag 含變體(FlatKind 等),避免不同變體被誤併。

### 5.5 beam 排序鍵(round 內 top-N 保留)

依序比較:

1. tiling 內節點 PowerRating 最強級別(±3 > ±2 > ±1 > 0)——對齊 architecture §10.3 第一鍵精神;
2. Σ rules_passed_count(D-3 修復後 Level-N 為真值);
3. Σ degree_level(偏好深樹,traditional `scenario_score` 同義)。

`round_beam_size` 為新工程參數(附錄 A)。此 beam 與最終 forest 護欄(forest_max_size / BeamSearchFallback)**分層並存**:前者控 round 內分支爆炸,後者控輸出上限,互不取代。

### 5.6 memoization 與複雜度上界

- memo key = 視窗 children canonical_key 串;重疊 tilings 大量共享視窗,`try_all_neely` 結果快取命中率為主要省時來源。
- 記號:m = base 節點數,B = round_beam_size,R = max_compaction_levels,L = 當前 tiling 平均長度(逐輪遞減)。每輪視窗數 ≤ B × Σ_{w∈{3,5,7,11}} (L−w+1) ≤ 4·B·L;總接受函式呼叫(去 memo 前)O(R·B·L)。W5 為最貴階,擺在 W1–W4 便宜過濾之後(D3 階梯排序理由)。
- 硬保險:`compaction_timeout_secs` 沿用;逾時 → 以當前已凍結內容返回並標 `compaction_timeout`(頂層旗標,architecture §8.1 對齊,行為不變)。

---

## 六、Round 2 Reassessment / Complexity / Degree

### 6.1 邊界波重評(D-4 修復;rules line 1249-1251 動作 B)

聚合成功後,parent 在其 tiling 中取**真實前後鄰居** m(−1)、m(+1):

| 比對 | 閾值 | 動作 |
|---|---|---|
| \|m(−1)\| 與 parent 首子波、parent 末子波與 \|m(+1)\| 之 magnitude 比 | ∈ [0.382, 2.618] | 通過,無事 |
| 同上 | ∈ [0.236, 0.382) ∪ (2.618, 4.236] | AdvisoryFinding(Info,Ch4_Round2)——現行 mild 檔語意搬移至真邊界 |
| 同上 | < 0.236 或 > 4.236 | **不拒絕聚合**(形態內部合法性已由 W5/W7 保證),寫 AdvisoryFinding(Warning)標示該解讀在更大序列中的角色可疑,供 beam 第 1–2 鍵之外的下游參考 |

tiling 首/末 parent 無對應鄰居 → 該側跳過(記 diagnostics)。

### 6.2 ComplexityLevel 真算(取代硬寫 Complex)

依 rules Ch7 Complexity Rule(Neely Extension)遞迴定義:

| Level | 條件(以節點樹判) |
|---|---|
| Level-0(Simple) | 葉 |
| Level-1(Polywave) | degree_level 1 之合法形態 |
| Level-2(Multiwave) | 推動形態中至少一個 `:5` 子節點自身為 impulsive polywave |
| Level-3(Macrowave) | 至少一個 `:5` 子節點為 Multiwave,且另一個 `:5` 至少為 Polywave |
| Triplexity | 同一結構內出現三個不同 Complexity Level 的 Impulse 段 → `triplexity_detected = true`(取代現行「由 pattern_type 直接推導」) |

### 6.3 degree_level → Degree(命名度數)對映

- `degree_ceiling`(Stage 11,依資料跨度,architecture §13.3)不變,仍為**上限**。
- 節點命名度數 = ceiling 錨定法:tiling 中最高 degree_level 對映至 ceiling 允許之最高 Degree,逐層向下遞減;超出 11 級下界時夾至 SubMicro 並記 diagnostics。
- 本對映僅供輸出展示與 cross_timeframe_hints;**不**回饋任何接受條件(度數命名不影響結構合法性)。

---

## 七、輸出合約變更

### 7.1 凍結流程

1. 收集:I6(全 tilings、degree ≥ 1、canonical 去重)。
2. 每個收集節點 → 一個 `Scenario`;`WaveNode` 樹由 CompactionNode 樹一比一凍結。
3. 既有 Stage 6(pattern_complete)已於 W6 後把關;Stage 7(Complexity 篩選)、Stage 8.5(round_state / anchors)、Stage 9–12 照跑,輸入改為凍結後 forest。
4. 護欄:forest_max_size(200)/ BeamSearchFallback(k=100)/ 雙重排序,原樣。

### 7.2 Scenario 逐欄 Level-N 語意表(23 欄)

| 欄位 | Level-N 填值規則 | 變更類型 |
|---|---|---|
| id | `cmp{level}-b{start_bar}-b{end_bar}-{pattern_tag}`(可重現、不含隨機) | 格式變更 |
| wave_tree | 凍結全嵌套(children 遞迴);I3 保證 | **語意升級**(現行單層) |
| pattern_type | W6 產出 | 不變 |
| initial_direction | 節點 net_direction(**定義變更**:現行取首子波方向;Running 類淨向與首子波可異向 → Q1 裁決前先用 net) | 定義變更 |
| compacted_base_label | I5 表 | 不變 |
| structure_label | 人讀字串,格式:`{Pattern} L{degree_level} [{child labels}]`;**下游禁止再 parse 此欄**(§7.4) | 格式變更 |
| complexity_level | §6.2 真算 | 修復 |
| power_rating | 凍結後 Stage 10a 查表照跑(rate_scenario 輸入不變) | 不變 |
| max_retracement | 查表照跑;in_triangle_context 例外照舊 | 不變 |
| post_pattern_behavior | 查表照跑 | 不變 |
| passed_rules / deferred_rules / 兩計數 | 取自 W5 ValidationReport(D-3 修復;Level-N 為真值) | 修復 |
| invalidation_triggers | Stage 10c 邏輯不變,W1/W2 價位取**子節點端點**(泛化端點介面天然支援) | 輸入泛化 |
| expected_fib_zones | Stage 10b 同上,以子節點端點價投影 internal/external | 輸入泛化 |
| structural_facts.fibonacci_alignment | 子節點 magnitude 比對 NEELY_FIB_RATIOS ±4% | 輸入泛化 |
| structural_facts.alternation | 取 W5 報告之 Ch5_Alternation 結果 | 修復(現行 Level-N 缺) |
| structural_facts.time_relationship | 子節點 duration_bars | 輸入泛化 |
| structural_facts.overlap_pattern | W1/W4 端點區間比對 | 輸入泛化 |
| structural_facts.channeling | 凍結後 advisory 階段照填 | 不變 |
| structural_facts.gap_count / volume_alignment | 以 start_bar..end_bar 對 bars 照算 | 不變 |
| advisory_findings | W4 sb_metrics 異常 + §6.1 邊界重評 + 既有 Channeling/Ch9 | 來源擴充 |
| in_triangle_context | 凍結後掃描:節點範圍被任一 Triangle 節點**真包含**(同 tiling 血緣)才為 true;現行「日期範圍重疊即算」的近似廢除 | 語意收緊 |
| awaiting_l_label / round_state | §5.3;Round1/2/3Pause 依實際輪次,不再以「跑過 compaction 即 Round2」概括 | 修復 |
| monowave_structure_labels | 葉層 Pre-Constructive 候選(Pass 1/Pass 2 diff 機制沿用);聚合節點記其覆蓋葉之全域 index 集 | 不變 |
| pattern_isolation_anchors | anchors 以 base tiling 葉 index 計,取節點覆蓋葉之 anchors **union**(A-10);現行 wave_tree 日期重疊近似廢除 | 語意收緊 |
| triplexity_detected | §6.2 | 修復 |

### 7.3 WaveNode wire 合約變更

| 欄位 | 動作 | 相容策略 |
|---|---|---|
| label / start / end / children | 不變 | — |
| **degree_level: usize**(新增) | 巢狀展示與 MCP 語意必需 | JSONB 加欄不刪欄;ts-rs 重生成 `frontend/src/contracts/neely/`;舊消費端忽略未知欄 |
| **base_label: StructureLabel**(新增) | `:3/:5` 前端標示 | 同上 |

### 7.4 下游契約協調(阻斷性,G2.4 前完成)

| 消費端 | 現況依賴 | 動作 |
|---|---|---|
| `src/fusion/dual_track/track1.py` | `wave_count_from_label` **parse structure_label 字串** | Scenario 新增結構化欄 `wave_count: usize`(= 頂層 children 數);fusion 改讀新欄;字串 parse 標 deprecated,一個 release 後移除(Q6 定時程) |
| `frontend/coords.ts flattenWaveTree` | 已遞迴,僅吃過單層資料 | 無介面變更;Plotly 波標密度視覺調校列 Gate v3 檢視項 |
| `mcp_server/tools/wave.py` | forest 語意 | 工具說明補 degree_level / 巢狀語意,避免 LLM 把 Level-0 與 Level-2 scenario 當同級並列誤讀 |
| `structural_snapshots` JSONB | forest 陣列 | 加欄相容;`schema_dump.txt` 與 architecture §14.1 範例同步更新 |

---

## 八、G2.0 止血規格(獨立可先行,不依賴 tiling 重構)

三補丁互相獨立,合計一日量級;落地後現行引擎由「可產錯誤結構」降級為「枚舉不完整但產出皆合法」。

### 8.1 補丁 P1:相鄰性硬檢查

- 位置:`try_aggregate_3/5/7/11` 進入點,先於一切既有檢查。
- 規則:對視窗每相鄰對,要求 `window[i].wave_tree.end == window[i+1].wave_tree.start`(日期精確相等,不設容差;依 monowave 共享端點慣例)。另要求視窗內任兩 scenario 時間範圍不相同(排除同段替代解自我聚合)。
- 失敗:回 None,不記 rejection(止血期屬過濾,非規則拒絕)。

### 8.2 補丁 P2:文件修正

- `lib.rs` Stage 8 header 段改述 v3.7 現況(遞迴迴圈存在)+ 引用本規格編號標注 D-1~D-5 未修狀態。
- `compaction/mod.rs`「M3 PR-5 簡化版」段同步。

### 8.3 補丁 P3:Level-N 排序墊底修正

- `build_aggregated` 之 `rules_passed_count` 暫填 Σ(children.rules_passed_count),`passed_rules` 維持空並註記「provisional until G2.2」。
- 影響面:BeamSearchFallback 第二鍵、beam 前處理;不影響任何規則判定。

### 8.4 G2.0 測試案例(最低集)

| 編號 | 構造 | 斷言 |
|---|---|---|
| T-1 | 5 個時間重疊 scenario(label/方向/S&B 全過) | `aggregate_one_level` 回空 |
| T-2 | 4 連續 + 1 間隙(end ≠ next.start) | 回空 |
| T-3 | 5 連續合法(:5:3:5:3:5 交替) | 產 1 個 Level-1;children 端點鏈逐對相等 |
| T-4 | 同 T-3 但兩 scenario 覆蓋同一日期範圍 | 回空 |
| T-5 | overflow 情境:Level-1(children Σrules 高)vs Level-0(rules 低) | BeamSearch 後 Level-1 不因 count=0 墊底 |
| T-6 | 既有 exhaustive tests 之同日退化日期改為真實日期鏈 | 全綠(測試債清償) |

---

## 九、驗收:P0 Gate v3

### 9.1 範圍

沿用 P0 Gate 慣例:六檔實測(沿用既有清單)+ 全市場 production run(≈1264 檔);shadow 雙軌期間新舊同跑。

### 9.2 門檻表

| 項 | 門檻 | 級別 |
|---|---|---|
| 不變量 | I1–I6 violation = 0(post_validator 新增檢查器,production 統計輸出) | 硬性,任一違反紅燈 |
| Terminal Impulse | 全市場至少可觀察到 Terminal `:5` 聚合樣本(D-5 修復存在性證明);六檔人工覆核誤報率 | 硬性(存在)+ 檢視(質) |
| forest_size | p99 ≤ 40(現行 16;巢狀允許成長,cap 200 內留 5× 餘量) | 硬性 |
| runtime | 全市場 wall time ≤ 2× 現行 neely baseline(Gate 前先量 baseline 入 docs/benchmarks;traditional 7.7s 為量級參照非門檻) | 硬性 |
| Level-N rules | 抽樣 Level-1 Impulse 100 例,R7(W3 非最短)、Overlap 判定與端點手算一致 | 抽驗 |
| 記憶體 | 峰值 RSS 相對現行 ≤ 1.5× | 硬性 |
| level_cap_hit | 全市場命中率寫入 Gate 報告(A-8 觀測項,無硬門檻;為 max_compaction_levels 動態化提供量測) | 觀測 |
| 前端 | 六檔巢狀 wave_tree 於 Plotly 正確展開;波標密度可讀(視覺檢視紀錄入 changelog) | 檢視 |
| fusion | track1 讀新 `wave_count` 欄,舊字串 parse 路徑回歸不破 | 硬性 |

### 9.3 shadow 比對規則

- 投影定義:新引擎 forest 之 degree_level = 1 節點,依 (start_bar, end_bar, pattern_type) 對舊 forest scenario 匹配。
- 門檻:舊 forest 召回率 ≥ 98%;缺口逐檔 diff 報告,允許之差異來源僅限:(a) beam/dedup 時序,(b) 舊引擎產出但違反 I1/I2 之聚合(此類為修復,非回歸)。
- 新增率不設上限,但 Level-N(N≥2)分布寫入 Gate 報告供 architecture §13.3 metadata 對齊延伸。

### 9.4 產出物

`docs/benchmarks/neely_compaction_v2_gate_results_<date>.md` + followup SQL,格式沿用 P0 Gate v2。

---

## 十、風險與否決方案(ADR 摘要)

| 編號 | 風險/選項 | 處置 |
|---|---|---|
| A-1 | 組合爆炸(tiling 分支 × 視窗多解) | 三層控:per-round dedup、round_beam_size、memoization;硬保險 timeout。Gate 量測 p99 分支數 |
| A-2 | 方案 B(compatibility-graph)| **否決**,理由見 §3.2 |
| A-3 | Stage 收編一次到位 vs 漸進 | 採 shadow 雙軌(§3.3);一次切換但長期並跑否決——雙路徑維護成本高於價值 |
| A-4 | W5 直接沿用 ATR 容差 | **否決**(bar 級概念上樓即虛構資料);改 §4.2 三檔相對容差 |
| A-5 | 3-3-3-3-3 只判 Triangle(現狀) | 否決;雙候選並存(§4.4),多解交 forest,符合不選 primary 哲學 |
| A-6 | mode.rs 移植 | 否決;`:3/:5` 已是等價物,雙表並存必然漂移 |
| A-7 | 邊界重評不通過即拒絕聚合 | 否決;形態內部合法性與其在更大序列的角色是兩件事,後者屬 advisory(§6.1) |
| A-8 | Q2:max_compaction_levels 常數 vs 依 degree_ceiling 動態 | **裁決**:常數 4 定案 + `level_cap_hit` 可觀察性旗標(§5.2);動態化待 Gate 量測命中率後另議 |
| A-9 | Q4:7/11 視窗 x-wave 位置與 CombinationKind 細分 | **裁決**:位置慣例維持;kind 細分排入 G2.3,依 rules Ch8 Table A/B 對映既有 11-variant enum,與 monowave 級判定同源(§4.2.1) |
| A-10 | Q5:pattern_isolation_anchors union vs 重算 | **裁決**:取覆蓋葉 union(資訊性欄位寧多勿漏,§7.2) |

## 十一、非目標(YAGNI)

- missing-wave / x-wave 缺席位補位的聚合變體(missing_wave 維持 Stage 9a advisory;啟用觸發條件:production 出現「因缺 missing-wave 聚合而整檔零 Level-2」案例)
- 真「窮舉所有 compression paths」不設 beam(architecture §10.1 已裁定不可行)
- CompactionNode 進 wire contract(internal-only,凍結輸出)
- Level-N 專屬新 Neely 規則發明(規則集以 rules 文件現有章節為封閉範圍)
- trendline_core 耦合(留 P1,Q3)

## 十二、開放問題

Q2 / Q4 / Q5 已於 r2 裁決收案,移入 §10(A-8 ~ A-10)。存續三題**沿用原編號不重排**(§4.2 / §4.3 / §4.4 / §7.2 / §7.4 交叉引用穩定優先)。

| 編號 | 問題 | 現行裁定(可推翻) | 決議期限 | 影響層級(附錄 C) |
|---|---|---|---|---|
| Q1 | Running 類形態 net_direction 與首子波異向,W3 交替與 initial_direction 語意 | 先用 net_direction;Running 樣本於 Gate 抽出人工覆核後定案 | G2.2 結束前 | 高 |
| Q3 | Level-N 節點僅有端點價,真實 high/low 極值(Overlap、Triangle 觸線)是否需反查 bars 精化 | **G2.2 內六檔雙軌實驗**:端點版 vs bars 反查版並跑,量 Overlap / 回測判定翻轉率;翻轉率 > 5%(初始門檻)→ 即於 G2.2 落 bars 反查(成本:每視窗 O(bars) 掃描),否則端點版定案、Gate v3 僅確認性抽驗 | G2.2 結束前 | 高 |
| Q6 | fusion structure_label 字串 parse 移除時程 | deprecated 一個 release;跨 repo PR 同週合併 | G2.4 前 | 中 |

**決議順序:Q3 → Q1 → Q6**,與附錄 C 影響評級一致。Q3、Q1 同落 G2.2:Q3 居首,其結論是 W5 端點介面與 §4.4 分岔判別(D-5 修復)的實作前提;Q1 於 W5/W6 定稿前定案(Power Rating 符號鏈,附錄 C);Q6 無設計不確定性,期限貼 G2.4 部署順序。r2 之殘餘風險(Q3 於 Gate 期才定案的重工暴險)已由本序收掉。

---

## 附錄 A:工程參數變更(NeelyEngineConfig)

| 參數 | 動作 | 預設 | 依據 |
|---|---|---|---|
| round_beam_size | 新增 | 32(Gate 校準) | §5.5;traditional round_beam_size 慣例 |
| max_compaction_levels | 由 exhaustive.rs 常數升級為 config 欄 | 4(A-8 定案) | 工程參數非 Neely 常數,可外部化不違 §6.6;動態化依 `level_cap_hit` 量測另議 |
| forest_max_size / compaction_timeout_secs / overflow_strategy | 不變 | 200 / 60 / BeamSearchFallback{k:100} | architecture §10.2 |
| beam_width(Stage 3 舊 cap) | G2.1 落地後標 deprecated,round_beam_size 承接 | — | §3.3 |

Neely 規則常數(Fib 比率、S&B 0.382–2.618、Fib² 0.236–4.236、三檔容差、Ch7 表、Power Rating 表)維持寫死,不外部化(architecture §4.5/§6.6)。

## 附錄 B:里程碑對映

| 里程碑 | 交付 | 對應章節 | Gate |
|---|---|---|---|
| G2.0 | 止血三補丁 + 測試 T-1~T-6 | §8 | 現有 444 tests 全綠 + 新測試 |
| G2.1 | CompactionNode / tiling / round 迴圈 / dedup / beam;`level_cap_hit` 旗標(A-8);shadow 雙軌啟動 | §3、§5 | 六檔 I1–I6 零違反 |
| G2.2 | W5 端點泛化 + W6 分岔判別(D-3/D-5 修復);Level-N 規則欄真值;Q3 六檔雙軌實驗(端點 vs bars 反查) | §4、§12 | 抽樣規則一致性(§9.2);Q3 翻轉率量測定案(> 5% → 落 bars 反查) |
| G2.3 | 邊界重評 / Complexity 真算 / Degree 對映 / 欄位語意收緊(anchors union,A-10)/ CombinationKind 細分(A-9) | §6、§7.2、§4.2.1 | rules line 1249-1251 逐條對照;Combination kind 與 monowave 級判定同源 |
| G2.4 | 契約協調(wave_count 欄、ts-rs、MCP 說明)+ P0 Gate v3 全市場 + 切換刪舊 | §7.3–7.4、§9 | §9.2 全門檻 |

---

## 附錄 C:開放問題影響層級與決議順序(r2 裁決依據)

評級兩軸:裁決錯誤的**失效模式**,與經 I5 的**向上傳染性**。

| 題 | 狀態 | 失效模式(若裁決錯誤) | 傳染性 | 可逆成本 | 層級 |
|---|---|---|---|---|---|
| Q3 | 開放 | 端點雙重去極值(monowave mid-price 一次、Level-N 端點再一次)→ W5 回測/Overlap 誤判 → §4.4 Terminal/Triangle 誤分 → base_label 錯 | **最強**:經 I5 逐級向上;shadow 比對基準同步污染;D-5 修復品質完全繫於此 | 低:G2.2 雙軌實驗內定案,無 Gate 期重工暴險(r3) | 高 |
| Q1 | 開放 | Running 視窗接受/拒絕翻轉(結構可達性);Power Rating Bullish/Bearish **符號反向**(initial_direction 欄之原始用途)→ max_retracement / post_behavior 方向語意 → fusion direction → 多空判讀顛倒 | 中:結構面限 Running 族;符號錯沿 direction 鏈直通 MCP / 前端 | 低:定義切換重跑 | 高 |
| Q6 | 開放 | 運行時炸鏈:label 新格式先於 fusion 改讀上線 → `wave_count_from_label` 全市場解析失敗,Track1 → Golden L3 → forecast_log 下游中斷(非靜默,當日可見) | 廣但零結構傳染 | 極低:expand–migrate–contract 部署順序 | 中 |
| Q2 | **已裁決 A-8** | 靜默缺漏:ceiling 允許高階但引擎止於 4 層,不觸紅燈 | 無(枚舉不全,非結構錯) | 極低 | 中低 → 以 `level_cap_hit` 可觀察性收案 |
| Q4 | **已裁決 A-9** | Combination 族 Power / post_behavior 查表取通用值而非精確值;Double/Triple 之分(reverse_logic 中段/末段依據)不受影響 | 局部,限 Combination 族 | 低 | 低中 → 細分排入 G2.3 |
| Q5 | **已裁決 A-10** | 資訊欄冗餘或缺漏 | 零(不進接受條件、beam、Power) | 零 | 低 → union 定案 |

**殘餘風險紀錄**:r2 曾接受「Q3 最晚定案、端點版先行承擔 Gate 期重工」之暴險;r3 將 Q3 決議提前至 G2.2 雙軌實驗,該暴險收掉,決議順序與影響評級恢復一致。
