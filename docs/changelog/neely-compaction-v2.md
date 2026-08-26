# neely_core Compaction v2 — Tiling-Round 引擎(G2.x 系列)

> 進度:G2.0 ✅ / G2.1 ✅(六檔 gate 實測收案)/ **G2.2 實作落地**(Q3 翻轉率
> 待本機六檔量測定案)/ G2.3、G2.4 未開工。

> 規格:`m3Spec/neely_compaction_v2.md`(r3 draft,2026-08-26)。
> 現行 Compaction 三層缺陷(D-1 ~ D-6)正式記錄後,分 G2.0 ~ G2.4 五個里程碑
> 以 tiling-round 引擎取代 `compaction/exhaustive.rs` 遞迴迴圈 +
> `compaction/three_rounds.rs` 弱比對聚合;本檔逐里程碑記錄落地內容。

## G2.0 — 止血三補丁(2026-08-25)

規格 §8:三補丁互相獨立,不依賴 tiling 重構;落地後現行引擎由「可產錯誤結構」
降級為「**枚舉不完整但產出皆合法**」。

| 補丁 | 檔案 | 內容 |
|---|---|---|
| 規格入庫 | `m3Spec/neely_compaction_v2.md`(新) | r3 draft 全文:缺陷表 D-1~D-6 / 不變量 I1–I6 / try_all_neely 七階梯 W1–W7 / round 引擎 / Scenario 23 欄 Level-N 語意表 / G2.0~G2.4 里程碑 / P0 Gate v3 門檻 / ADR A-1~A-10 / 開放問題 Q1/Q3/Q6 |
| P1 相鄰性硬檢查(§8.1) | `compaction/three_rounds.rs` | 新增 `window_is_contiguous`,`try_aggregate_3/5/7/11` 進入點先於一切既有檢查:① 視窗每相鄰對 `wave_tree.end == next.wave_tree.start`(日期精確相等,不設容差);② 視窗內任兩 scenario 時間範圍不得相同(排除同段替代解自我聚合)。失敗回 None 不記 rejection(止血期屬過濾)。D-1/D-2 止血:重疊替代解讀不可再拼成 Level-N 形態 |
| P2 文件修正(§8.2) | `lib.rs` / `compaction/mod.rs` / `exhaustive.rs` | Stage 8 header 由「pass-through」改述 v3.7 遞迴迴圈現況 + G2.0 止血狀態,引用 v2 規格標注 D-1~D-5 修/未修;`compaction_paths` 欄位 doc 同步;D-6 文件債清償 |
| P3 排序墊底修正(§8.3) | `compaction/three_rounds.rs` | `build_aggregated.rules_passed_count` 暫填 Σ(children.rules_passed_count),`passed_rules` 維持空(provisional until G2.2);不影響任何規則判定 |
| 雙重排序鍵 2 補實 | `compaction/beam_search.rs` | architecture §10.3 規定、lib.rs 自 Phase 8 起宣稱的組內第二鍵 `rules_passed_count` 實作長期缺席(單鍵 \|power_rating\|);T-5 需要鍵 2 才可測,一併補上 — 鍵 1 \|rating\| 級別 → 鍵 2 組內 count → tie-break Bullish 側(既有語意保留) |

### 測試(§8.4 T-1 ~ T-6)

| 編號 | 測試 | 驗證 |
|---|---|---|
| T-1 | `t1_overlapping_scenarios_do_not_aggregate` | 5 個時間重疊 scenario(label/方向/S&B 全過)→ 回空;無 P1 時會拼出 Trending Impulse |
| T-2 | `t2_gap_in_window_does_not_aggregate` | 4 連續 + 1 間隙 → 回空(全 :_3 構造使 5-窗為唯一候選) |
| T-3 | `t3_contiguous_impulse_children_share_endpoints` | 5 連續合法 → 恰 1 個 Level-1 Impulse;children 端點鏈逐對相等 |
| T-4 | `t4_duplicate_date_range_in_window_does_not_aggregate` + `window_is_contiguous_rejects_duplicate_range_even_when_chain_holds` | 同段替代解在窗內 → 回空;P1 規則 2(零長度退化段令端點鏈成立但範圍重複)由 helper 單元測獨立驗證 |
| T-5 | `t5_level_n_with_summed_rules_not_ranked_bottom` | BeamSearch 同組內 Level-1(Σrules=12)不墊底,淘汰的是組內 count 最低的 Level-0 |
| T-6 | exhaustive tests 日期債清償 | `make_simple` 同日退化值 → 真實日期鏈;`max_compaction_levels_respected` 之 `% 28` 月內 wrap(產生 end < start)→ chrono 連續 50 段 × 5 天;`p3_aggregated_rules_passed_count_sums_children` 鎖 P3 語意 |

### 與 spec §8.4 字面的偏差(記錄)

1. **T-3**:spec「產 1 個 Level-1」讀在 tiling 語意(G2.1+ 單一視窗);現行滑窗
   引擎對同一條鏈另合法聚合出 2 個 :5:3:5 sub-window Zigzag,故測試斷言收斂為
   「恰 1 個 **Impulse**」(端點鏈斷言照 spec)。
2. **T-4**:spec 寫「同 T-3」構造,但該構造下未受污染的 [a,b,c] 3-窗仍會合法
   聚合(結果非空、斷言不可滿足)→ 改全 :_3 構造使 5-窗為唯一候選。重複範圍
   同時打斷端點鏈,integration 層實際由 P1 規則 1 攔下;規則 2 僅零長度退化段
   可達,由 helper 單元測覆蓋(數學上:正向日期鏈 + 重複範圍 ⇒ 中間段全零長)。
3. **T-5**:以 `keep_top_k_by_power_rating` 單元測直測排序鍵 2(spec 斷言的
   行為本體);P3 Σ 語意另由 `p3_aggregated_rules_passed_count_sums_children`
   鎖定,未另做 compact() overflow 端到端。
4. **規格入庫最小編修**:§9.3 之孤懸「§13.3」補限定為「architecture §13.3」
   (§6.3 已明示同一目標,文內無 §13.3 節)。其餘含 r3 日期全數照錄。

### P3 影響面補充(spec §8.3 未列,production verify 注意)

§8.3 稱影響面僅「BeamSearchFallback 第二鍵、beam 前處理」且不影響規則判定 —
規則判定確實不受影響,但 `rules_passed_count` 隨 Scenario 序列化外流,以下
消費端在 G2.2 真值重驗前會看到 Level-N 的**未驗證暫定 Σ 值**(原為 0 恆墊底):

- `facts.rs` fact 敘述(`rules passed = N`)與 metadata JSON — Level-N fact
  會宣稱非零 count 而 `passed_rules` 為空
- 下游排序/選圖鍵:`fusion/dual_track/track1.py`、`fusion/wave_summary.py`、
  `cross_cores/wave_impulse_screen.py`、`forecast/neely_emitter.py`、
  `mcp_server/_forecast.py`、`frontend/lib/wave/power.ts`(top-scenario picker)
  — Level-N 的 Σ 可能高於所有 Level-0,top scenario 選擇可能翻轉

此為 D-3 修復方向的預期效果(Level-N 不再恆墊底),但翻轉幅度留下次
production run 觀察;若翻轉不可接受,G2.2 前可於下游 picker 對
`passed_rules.is_empty() && rules_passed_count > 0`(暫定值特徵)降權。

### 驗證(2026-08-25 sandbox)

- `cargo test -p neely_core`:**429 passed / 0 failed**(compaction 模組 37,含新測 7)
- `cargo test --workspace`:**654 passed / 0 failed**(647 baseline + 7 新測)
- production 影響:P1 使既有 forest 中「違反 I1/I2 的 Level-N 聚合」消失(v2 §9.3
  明訂此類為修復非回歸);Level-N 於 overflow 剪枝時不再必然墊底。全市場分布變化
  留 G2.4 P0 Gate v3 量測,G2.0 不設 gate

### 未修(留 tiling-round 引擎)

D-4(邊界波以視窗內第 1/2 段近似,真鄰居需 tiling)、D-5(3-3-3-3-3 一律判
Triangle,Terminal Impulse 產不出)→ G2.1(tiling/round 迴圈)/ G2.2(W5 端點
泛化 + W6 分岔判別)。開放問題 Q3 → Q1 → Q6 決議順序與期限見規格 §12。

---

## G2.1 — tiling-round 引擎骨架 + shadow 雙軌啟動(2026-08-25)

規格 §3 / §5(附錄 B G2.1 交付):CompactionNode / tiling / round 迴圈 / dedup /
beam / `level_cap_hit`(A-8)全落地;**shadow 雙軌**啟動 — 新引擎每次 compute
隨行,僅寫 `NeelyDiagnostics.shadow_compaction`,serving forest 完全不動。

| 項 | 檔案 | 內容 |
|---|---|---|
| 引擎本體 | `compaction/round_engine.rs`(新) | `CompactionNode`(Rc 共享,§5.1 合約:kind / base_label(葉持 Stage 0 候選集)/ degree_level / bar+date+price 端點 / children / net_direction / canonical 預算);base tiling(§5.2:非 Neutral + **合成葉橋接**,Neutral 段併前一 directional 節點、開頭 Neutral 記 diagnostics 後排除);round 迴圈(視窗 {3,5,7,11} × 全 tiling,原 tiling 保留為「停在此度數」alternate);per-round canonical_key dedup(§5.4);beam(§5.5 三鍵:max \|PowerRating\| → Σ rules → Σ degree);memoization(§5.6 視窗 children key);`compaction_timeout_secs` 硬保險 |
| 接受階梯 | 同上 | W1(I2 防衛:debug panic / release 記 `w1_violations`)/ W2(I5 閉合表,葉任一候選匹配即過)/ W3(net direction 交替)/ W4(S&B 端點版)/ W7(Fib² **全**相鄰對,§4.2 語意修正);**W5/W6 stub 留 G2.2** — validation 恆 None、3-3-3-3-3 暫僅出 Triangle、Flat 以 Common 佔位 |
| 不變量檢查器 | 同上 | `check_invariants`:I1(時間正序)/ I2(共享端點,日期+價格精確相等)/ I3(children 遞迴分割)/ I4(層級單調)/ I5(label 閉合)逐 tiling 計數;I6 由凍結收集 canonical 去重保證。G2.1 gate 準則 = 六檔 I1–I6 零違反 |
| 診斷合約 | `output.rs` | 新增 `ShadowCompactionDiagnostics`(engine tag / base_tiling_len / neutral_bridged / leading_neutral_dropped / rounds_run / tiling_count / `level_cap_hit` / timed_out / w1_violations / `round_branch_cap_hits` / node_count_by_level / invariant_violations / §9.3 召回計數 / elapsed_us)+ `NeelyDiagnostics.shadow_compaction: Option<…>`(JSONB 加欄相容;ts-rs 契約已重生) |
| 工程參數 | `config.rs` | `round_beam_size = 32` 新增、`max_compaction_levels = 4`(A-8 定案,自 exhaustive.rs 常數升級 config 欄,舊迴圈同步改讀);`beam_width` 標 deprecated(附錄 A,G2.4 切換後移除) |
| 接線 | `lib.rs` | Stage 8s:`run_shadow(&classified, &old_forest, cfg)` 於 Stage 8 後隨行;`stage_elapsed_us["stage_8s_compaction_v2_shadow"]` 計時 |

### G2.1 拍板(G2.2 可推翻)

1. **W4 time 維度用日曆日**(與現行 `similarity_and_balance` 同基準,shadow 比對
   不引入額外差異);bars 基準留 G2.2 隨 Q3 實驗一併裁決。
2. **W7 語意修正落地**(全相鄰對,舊引擎僅首尾邊界對):spec §4.2 明訂;由此
   產生的召回缺口屬 §9.3 允許清單外的第三類,Gate 報告需標注(spec §9.3 允許
   清單建議於 r4 增補此類)。
3. §9.3 召回**僅計數不設門檻**:G2.1 階梯缺 W5/W6,98% 門檻於 Gate v3(全階梯)
   才適用。投影嚴格依 spec 取新引擎 **degree_level = 1** 節點(非 ≥ 1),召回鍵
   = (start_bar, end_bar, pattern_tag),舊 scenario 日期經 monowave 端點反查 bar;
   舊 forest 內 Level ≥ 2 聚合結構上不可能被 degree-1 投影命中,屬 Gate 缺口
   diff 報告的預期類別。
4. **round 內分支護欄**:新分支 materialize 上限 = `round_beam_size × 8`
   (G2.1 階梯無 W5/W6 → 接受稠密,無上限實測 600-monowave 檔 18.5s + 3.1GB;
   beam 只保 round_beam_size,超額 materialize 無意義)。截斷偏向先枚舉視窗,
   輪數記 `round_branch_cap_hits` 供 Gate 觀測;逾時檢查下沉至視窗粒度。
   工程護欄非 Neely 語意,G2.2 加 W5/W6 後接受率下降可重估。

### 驗證(2026-08-25 sandbox)

- `cargo test -p neely_core`:**443 passed / 0 failed**(+14 round_engine:合成葉
  橋接 ×2 / 聚合+不變量零違反 / round-2 嵌套 / level_cap_hit 正反 ×2 / beam 截斷 /
  少節點早退 / 多義葉多解 / 不變量檢查器壞輸入 + I1 覆蓋條款 / §9.3 召回 / dedup /
  稠密鏈 branch cap 有界);`cargo test --workspace`:**668 passed / 0 failed**;
  ts 契約經 `codegen/generate.sh` Track A 重生(`cargo test --features ts` 全綠)
- 六檔 + 全市場 shadow 觀測:讀 `snapshot->'diagnostics'->'shadow_compaction'`
  (structural_snapshots 的 payload 欄是 `snapshot`,無獨立 diagnostics 欄)

### G2.1 gate 六檔實測(2026-08-26 本機,bars ~991-996)

| stock | base | rounds | cap_hit | branch_caps | levels | 召回 | shadow ms(舊 s8 ms) |
|---|---|---|---|---|---|---|---|
| 0050 | 151 | 4 | T | 3 | 1:23 / 2:1 | 0/15 | 7.6(0.0) |
| 1312 | 140 | 4 | T | 3 | 1:22 / 2:1 | 0/20 | 8.8(0.1) |
| 2330 | 179 | 4 | T | 3 | 1:35 | 3/20 | 6.0(0.1) |
| 3363 | 149 | 4 | T | 3 | 1:19 / 2:2 | 1/14 | 8.5(0.0) |
| 6547 | 127 | 4 | T | 0 | 1:16 | 3/13 | 8.2(0.0) |

- **gate 判定:五檔 I1–I6 = 0 / w1_violations = 0 / 無逾時 → G2.1 gate 收案** ✅。
  TAIEX 載到 0 bars(`price_daily_fwd` 無 `_index_taiex_` 列 — 指數不在 Silver
  fwd 供料路徑,既有現況非本 PR 造成;另列 backlog,gate 以五檔個股認定)
- **A-8 量測**:`level_cap_hit` 5/5(G2.1 無 W5/W6 階梯接受稠密,100% 觸 4 輪頂);
  `round_branch_cap_hits` 4 檔 3 輪命中(6547 為 0)。shadow 6.0–8.8 ms/檔 vs 舊
  stage 8 0.0–0.1 ms → 全市場 2172 檔推估 +~20s,遠低於 Gate v3 的 2× 門檻
- **召回 7/82(8.5%)— 預期低,兩個結構性因素記為 G2.2 設計輸入**:
  (a) branch cap 偏向先枚舉視窗 → 時間軸晚段行情的視窗未被探索,召回數字受工程
  護欄混淆,G2.2 應改為「先枚舉後依 beam 鍵選 materialize」或視窗輪替;
  (b) W4 S&B 於 round-1 對 monowave 視窗生效,而舊 Stage 3 的 Level-0 candidates
  從未受 S&B 過濾 → 語意差屬 §9.3 允許清單之外,r4 需增補該清單或重議 W4 的
  round-1 適用性(W5/W6 補上後重量再定)
- **(C) 揭露 G2.0 的 production 效果**:五檔 forest(13–20)全為 Level-0,
  `children>0 且 rules>0` 為 0 — **P1 後 old-engine 的 Level-N 聚合實測歸零**
  (重疊滑窗清單相鄰項無法成鏈,D-1 假聚合徹底消失,對齊「枚舉不完整但產出皆
  合法」設計);連帶 P3 Σ 外流與下游 top-scenario 翻轉疑慮在 G2.4 切換前實務上
  不發生(serving forest 無 Level-N 可翻)

---

## G2.2 — W5 端點泛化 + W6 分岔判別 + Q3 雙軌儀表(2026-08-26)

規格 §4(七階梯補完)+ §12 Q3;附帶修一個既有 production bug 與 G2.1 gate
實測揭露的 branch cap 偏置。D-3 / D-5 的引擎側修復到位;Q3 結論待六檔量測。

| 項 | 檔案 | 內容 |
|---|---|---|
| **既有 bug 修復** | `classifier/mod.rs` | `rules_passed_count` 原填 `report.passed.len()` — validator Pass 分支從不記 passed(清單靠 `default_passed_rules` 反推),**production 所有 Level-0 的 count 恆 0** 且與 `passed_rules` 欄不一致(BeamSearch 鍵 2 / fusion / 前端 picker 的該排序鍵長期 inert;昨日 (C) 查全 0 的真因)。改與 passed_rules 同源(`derived_passed.len()`)+ 回歸鎖測試 |
| W5 端點泛化(§4.3) | `round_engine.rs` | `synth_window`:視窗 → 合成 ClassifiedMonowave 序列(端點價 / duration_bars / label 候選;ATR 等 bar 級概念不虛構,規則本體亦不消費)+ WaveCandidate → 餵既有 `validator::validate_candidate` — 同一套規則碼雙形態輸入;passed 與 Level-0 同源推導(`default_passed_rules` 開 pub(crate) 共用),ValidationReport 附掛節點,**beam 鍵 2 起真值** |
| **W5 閘門按 I5 族別**(實作詮釋) | 同上 | Ch5 Essential R1–R7 是衝動建構規則::5 族 kind(Impulse / Terminal)以 `overall_pass` 硬閘;:3 族(Zigzag/Flat/Triangle/Combination)essentials 天生不成立(Triangle W3 < W2 即 R4 fail),一律硬閘會使 W6 分岔成死碼、D-5 不可修 — 對齊 validator 自身「變體規則 Fail 資訊性,交 Classifier」語意。**此詮釋需 r4 spec 增補 §4.3 明文** |
| W6 分岔判別(§4.4,D-5 修復) | 同上 | 3-3-3-3-3 端點幾何:Contracting(a-c/b-d 兩線收斂 + e 不破 a-c 線 ±5%)/ Expanding(發散 + 逐波擴大 ±10% 鬆綁)/ **Terminal Impulse**(W2/W4 不完全回測 + W3 非最短 + W1/W4 範圍重疊;以 `Diagonal{Ending}` 表徵,對齊 classifier 慣例,I5 → :5)— 可同時接受(各產分支,不選 primary)。趨勢線端點內建幾何,trendline_core 耦合留 P1 |
| Q3 雙軌儀表(§12) | 同上 + `lib.rs` | 每唯一 5-窗:端點版 vs bars 反查版(`bar_indices` 範圍掃真實 high/low,結果快取)的 Overlap(W1/W4)與 W2/W4 回測判定並跑;`q3_windows` / `q3_flips` 進 shadow diagnostics — **翻轉率 = q3_flips/q3_windows,> 5% 依 spec 落 bars 反查**。run_shadow 簽名 + `&input.bars` |
| branch cap 偏置修正(G2.1 gate 輸入 a) | 同上 | round 內生成改**兩階段**:先枚舉全視窗收輕量 splice spec(不 materialize),再依 beam 鍵近似分數(parent \|power\| → W5 passed 數 → degree)降序 materialize 至 cap — 時間軸晚段行情不再因先枚舉被截斷 |
| 診斷合約 | `output.rs` | `ShadowCompactionDiagnostics` 加 `w5_rejected_windows` / `q3_windows` / `q3_flips`(唯一視窗語意,memo 命中不重計);engine tag → `tiling-round-g2.2`;ts 契約重生 |

### 驗證(2026-08-26 sandbox)

- `cargo test -p neely_core`:**449 passed / 0 failed**(+6:W5 essential 拒絕 /
  W5 通過附真值計數 / W6 terminal 接受 / W6 contracting 過族別閘(同窗 :5 被閘,
  閘門語意直測)/ Q3 影線翻轉正反組 / classifier count 回歸鎖)
- `cargo test --workspace`:**674 passed / 0 failed**;ts codegen 全綠

### 待本機(G2.2 收案條件)

1. **Q3 量測**:重跑六檔 `tw_cores run --stock-id ... --write` 後讀
   `shadow_compaction` 的 `q3_flips / q3_windows` — 翻轉率 ≤ 5% → 端點版定案;
   > 5% → 落 bars 反查(W4 time 基準一併重議)。
2. **W5 拒絕率 / cap 命中觀測**:`w5_rejected_windows`、`level_cap_hit` 與
   `round_branch_cap_hits` 相對 G2.1 基線的變化(預期:接受變稀,cap 命中下降)。
3. **Q1(Running 類)**:production 抽 Running 樣本人工覆核 net_direction 語意
   (spec §12,G2.2 結束前定案)。

### G2.2 已知邊界(留後續)

- Flat 七變體 / CombinationKind 細分:classifier 泛化留 G2.3(A-9 同批)。
- W5 拒絕的完整 RuleRejection 記錄(§4.1):shadow 期以計數觀測,G2.4 切換時進
  `NeelyDiagnostics.rejections`。
- Alternation complexity 軸(degree_level 差)與邊界重評 / Complexity 真算:G2.3(§6)。
