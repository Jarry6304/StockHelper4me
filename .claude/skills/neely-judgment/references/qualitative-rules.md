# 不可程式化規則判讀清單(protocol 步 3)

> 這些規則引擎**沒有**實作或只做部分:判讀時人工套。逐條寫進
> `rationale.rule_refs` / `rationale.emulation_considered`。
> 原文依據:`m3Spec/neely_rules.md`(行號如標)。

## 1. Emulation 對照(7 型;rules 2585-2597 + Ch12)

對每個 live-edge 候選問:「它是不是另一型的模仿?」命中 → 降權或改選,寫進
`emulation_considered`(格式:`"<模仿型>-as-<候選型>: <判別證據>, <採信/排除>"`)。

| # | 假象 | 判別鑰匙 |
|---|---|---|
| 1 | Double Failure 模仿 Triangle | 假 d-wave 突破 a/b 端點即破除三角假設;Triangle 內 a/c 多 61.8% Internal,Double Failure 的 c 多落 a 的 External Fib |
| 2 | Double Flat 模仿 1-2-3 with 3rd Ext | 假 wave-2 回測 > 61.8%×wave-1(wave-1 應為修正性)、2/4 缺 Alternation、「3rd Ext」≤ 161.8%×wave-1;把最長段切半當隱形 x-wave |
| 3 | Double/Triple Zigzag 模仿 Impulse | 通道過於完美、各段太類似(缺 Extension)、wave-5 後 2-4 線未及時破(看 ch6_status!)、2/4 缺 Alternation |
| 4 | 1st Ext 缺 wave-4 模仿 Zigzag(c ≤ a) | Missing Wave 表對照(見 §2);資料點數不足 → 缺漏可能 |
| 5 | 5th Ext 缺 wave-2 模仿 Zigzag(c > a) | 同上 |
| 6 | Diagonal(Terminal)模仿 Trending Impulse | wave-2/4 重疊(引擎 1.1.1 起 Overlap 閘已擋大部分;殘餘灰色地帶人工看) |
| 7 | Triangle 模仿 5-wave Failure / 1st Ext 模仿 Terminal | 引擎 `emulation_suspects` 有部分偵測;dossier 外的 suspects 走 /neely/forest 查 |

## 2. Missing Wave 最少資料點表(rules 2559-2582;引擎未實作)

候選 polywave 的覆蓋 bar 數(`age_bars` + span 換算)對照:

| Polywave 形態 | 最少資料點 |
|---|---:|
| Zigzag / Flat | 5 |
| Impulse / Triangle | 8 |
| Double Flats / Zigzags | 10 |
| Doubles 以 Triangle 結尾 | 13 |
| Triple Flats / Zigzags | 15 |
| Triples 以 Triangle 結尾 | 18 |

- 資料點 < 50% × 最低 → 缺漏波幾乎肯定(候選計數不可靠,傾向 no_fit 或降權)
- 50% × 最低 ≤ 點 < 2× 最低 → 缺漏可能(寫進評註)
- 點 ≥ 2× 最低 → 缺漏不應存在
- 缺漏永遠是 monowave(多為 x-wave;Impulsive polywave 可能缺最小的 2/4)

## 3. Rule of Proportion(rules 243-249;繪圖刻度層,引擎無法看圖)

- 判讀前自問:候選的「方向性/非方向性」判定是否因刻度誤讀?
- Directional:第一個 monowave 通常被回測 ≤ 61.8%;某沿趨勢 monowave 被回測 > 100% → Directional 通常結束
- Non-Directional:第一個 monowave 一定被回測 > 61.8%;價格超出整區間 161.8% → 通常結束
- 引擎以 ATR/數值處理,無「45° 視覺」概念 — 候選與這條衝突時寫進 notes

## 4. Rule of Neutrality Aspect-2(rules 251-258;引擎只做 Aspect-1 式方向分類)

- 水平段分隔**同向**波:可忽略(併大 monowave)或切三段;超過一個時間單位且分隔同向波 → **必須**切
- 判讀時:若 Aspect-2 切法能改善 Alternation / Complexity / 消除 Missing Wave → 候選端點可能該重切,
  但引擎不會重切 — 此情境寫 `no_fit_reason`(引擎缺口:Neutrality Aspect-2 未實作)

## 5. Reverse Logic 人類語意(rules 2599-2608;引擎 E4 只給計數,不下結論)

- `live_edge.ambiguity.count` 多(多套完美計數)→ 市場在某形態**中段**(b 的 b / 3 的 3 / x)
- 操作:剔除「形態即將完成」的候選解讀後,通常剩一套 → 那套才是 preferred 方向
- count 收斂為 1 → 可進 `single`(若 robust 支持)
- 注意:這是**判讀規則**不是過濾器 — 剔除理由逐條寫 rule_refs

## 6. Localized Progress Label Changes(rules 2610-2612;J2 absorbed 的人類面)

- 走勢突發違反原判讀 → 多半「形態還在擴大」:原判讀的結尾其實是更大形態的 wave-a / wave-1
- 先做**最小修改**:degree 降一級、當更大同類形態第一段 — 對應 dossier 裡
  「原判讀成為某候選 wave_tree 子樹」(J2 `absorbed` + `parent_anchor_key`)
- 重判時 preferred 應優先考慮該 parent 候選,`minimal_change` 記下對應
