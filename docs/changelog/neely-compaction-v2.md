# neely_core Compaction v2 — Tiling-Round 引擎(G2.x 系列)

> 進度:G2.0 ✅ / G2.1 ✅ / G2.2 ✅(gate 五檔實測過 + Q3 拍板 + Q1 全市場
> 抽樣收案)/ G2.3 ✅(gate 五檔實測過)/ **G2.4 前半 ✅(契約協調 + Gate 工具;
> 後半切換刪舊等本機 P0 Gate v3 全門檻)**。

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
- **(C) 揭露 G2.0 的 production 效果**(依據於 G2.2 修正,結論不變):
  當日以「`children>0 且 rules>0` = 0」推論「forest 全 Level-0」— 該推論**無效**,
  因 (C)=0 的真因是 classifier `rules_passed_count` 恆 0 的既有 bug(G2.2 修),
  Level-0 的 wave_tree 本來就有 children。正確量測 = **深度 ≥ 2 scenario 數**
  (只有 old-engine Level-N 才有孫節點):G2.2 複測新 binary 五檔 depth2+ 全 0、
  舊 production binary 的 2330 歷史列 depth2+ = 6 — **「P1 後 old-engine Level-N
  歸零、且 pre-P1 確實產過假聚合」由此確證**。P3 Σ 下游翻轉疑慮在 G2.4 切換前
  實務不發生的結論維持

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

### G2.2 gate 五檔實測 + Q3 拍板(2026-08-26 本機)

| stock | inv | w1 | q3_win | q3_flip | w5_rej | levels |
|---|---|---|---|---|---|---|
| 0050 | 0 | 0 | 2 | 2 | 2 | 1:9 |
| 1312 | 0 | 0 | 3 | 0 | 3 | 1:10 |
| 2330 | 0 | 0 | 9 | 2 | 4 | 1:16 / 2:1 |
| 3363 | 0 | 0 | 2 | 0 | 1 | 1:12 |
| 6547 | 0 | 0 | 2 | 2 | 1 | 1:11 |

- **引擎 gate:五檔 I1–I6 = 0 / w1 = 0 → 過** ✅;W5 把關後 degree-1 節點
  9–16(G2.1 為 16–35,接受如預期變稀);shadow 10.5–13.6 ms/檔仍可忽略。
- **Q3 拍板:q3_windows Σ=18、q3_flips Σ=6 → 翻轉率 33.3% ≫ 5% → 依 spec
  「即於 G2.2 落 bars 反查」定案**。已落地(同日):`CompactionNode.true_range`
  (葉建構時掃 bars 一次,parent 取 children 聯集,internal-only 不進 wire)、
  W6 分岔的回測 / W1-W4 Overlap / e 觸線改以 `judge_range`(真實極值,無 bars
  退端點)判定;**W4 time 基準一併轉 duration_bars**(G2.1 拍板保留的重議點 —
  端點日曆日的舊引擎 parity 理由已隨舊 Level-N 消亡失效);`q3_windows/q3_flips`
  保留為**殘差觀測**(端點 vs bars 分歧率,供 Gate 報告與 r4 佐證)。
  spec §12 Q3 據此收案,r4 修訂時同步定稿。
- **查詢陷阱(runbook 必記)**:`structural_snapshots` PK 含 `params_hash`,
  例行排程與手動 run 在同一 snapshot_date 各留一列,`DISTINCT ON ... ORDER BY
  snapshot_date DESC` 對同日多列挑選非決定性(首輪誤讀舊 binary 列)—
  **verify 一律 `ORDER BY stock_id, created_at DESC`**。

### 待本機(G2.2 尾項)

1. **bars 判準複測**:下次六檔 run 讀 `q3_flips`(現在語意 = 殘差)與 W6 產出
   變化,確認 bars 判準落地後 Terminal / Triangle 分布合理。
2. **Q1(Running 類)**:~~production 抽 Running 樣本人工覆核~~ **已收案**
   (2026-08-26 全市場抽樣 + 讀碼確證,見下方「Q1 收案」節)。

### G2.2 已知邊界(留後續)

- Flat 七變體 / CombinationKind 細分:classifier 泛化留 G2.3(A-9 同批)。
- W5 拒絕的完整 RuleRejection 記錄(§4.1):shadow 期以計數觀測,G2.4 切換時進
  `NeelyDiagnostics.rejections`。
- Alternation complexity 軸(degree_level 差)與邊界重評 / Complexity 真算:G2.3(§6)。

---

## Q1(Running 類 net_direction)收案(2026-08-26)

spec §12 Q1:「Running 類形態 net_direction 與首子波異向,W3 交替與
initial_direction 語意」。全市場 production 抽樣 + 程式碼確證後**定案為雙軌定義**,
實作免改(G2.2 已是正確寫法)。

### 抽樣(2026-08-26 本機,`verify_q1_running.py`,latest per stock / created_at DESC)

- 2192 檔 / forest 40095 scenarios;方法自檢全過:`initial_direction ==
  首 monowave 方向` anomaly = 0、wave_tree 端點反查 unresolved = 0。
- **Running 類僅 16 筆**(全 `RunningCorrection`,無 Combination Running;佔 forest
  0.04%),**net 與首子波 16/16 同向、0 翻轉**;全型態對照組 net≠first 僅 1.0%。
- 樣態規律:first=Up 者一律 power=StrongBearish、first=Down 者一律 StrongBullish
  (符號鏈之「wave-a 逆勢」假設的直接展現,見下)。

### 確證(讀碼)

- 數學:a-b-c 視窗(b>a、c<a)net 與首子波**異向 ⟺ B > A+C ⟺ 教科書真 Running**
  (c 未退回 a 起點)。production 0 翻轉的真義:**現行 16 筆無一是真 Running 幾何**,
  全為「b 過衝 + c 偏短但已退越 a 起點」的偽 Running(detector 發現,見下)。
- `power_rating/table.rs lookup_power_rating` 的 ± 號建立在「initial_direction =
  wave-a = 逆勢」假設上:真 Running 的 net = 大勢方向,若改餵 net 會把 ±3 最強評級
  **精準反向**(漲勢中 running 被標 StrongBearish)。→ 符號鏈輸入不可改 net。

### 定案(雙軌)

| 鏈 | 方向定義 | 依據 |
|---|---|---|
| 符號鏈(`Scenario.initial_direction` → Power Rating → max_retracement / post_behavior → fusion direction) | **首子波方向**;Level-N = 首**子節點**之 net_direction | 符號表的逆勢假設;`round_engine.rs synth_window` 已如此實作(`window[0].net_direction`),免改 |
| 幾何鏈(W3 交替、tiling 建構) | 節點自身 net_direction | 照現行實作;真 Running 節點 net 與大勢同向會被 W3 拒收 — production 現為 0 筆真 Running,例外條款寫進 r4 當**條件式設計**(等 detector 修正後才有觸發對象),不先寫死碼 |

### r4 spec 待修清單(本次追加)

1. §7.2 `initial_direction` 列由「節點 net_direction」更正為「**首子節點方向**
   (幾何鏈才用父節點 net)」。
2. §4.2 W3 列的「例外見 Q1」改為條件式 Running 例外註記(掛 detector 修正 backlog)。
3. §12 Q1 收案移入 §10 ADR(建議 **A-11**)。

### 附帶發現:`is_running_correction` proxy 與語意不符(獨立拍板項,不入 G2.3)

`classifier/flat_classifier.rs is_running_correction` 用 `b>a AND c<a` 當
「c 不退至 a 起點」的 proxy — 正確條件是 **B > A+C**。現行 production 16 筆
偽 Running 全憑此拿到 ±3 最強評級(雜訊);修正後會落回 Flat
Irregular*/IrregularFailure(−1/−2)。**影響 serving forest / MCP / fusion 多空
判讀,需獨立拍板 + 重跑,不順路塞進 G2.3。**

---

## G2.3 — 邊界重評 / Complexity 真算 / Degree 對映 / anchors union / Combination 細分(2026-08-26)

規格 §6(Round 2 Reassessment)+ A-9 / A-10(附錄 B G2.3 交付)。全部落在
shadow 引擎與 diagnostics,serving forest 不受影響。

| 項 | 檔案 | 內容 |
|---|---|---|
| A-9 Combination / Flat 細分 | `round_engine.rs` + `classifier/mod.rs` | W2 rows 細分與 monowave 級**同源**:`classify_3wave_mags`(量值版核心)自 `classify_3wave_segment` 抽出、`map_double/triple_combination` 開 pub(crate) — 3-窗 [:3 :3 :5] 依幅度分 RunningCorrection / Flat 七變體(b/a < 61.8% 不符 Flat 最低要求 → **row 不成立**,G2.2 Common 佔位廢除);7/11-窗依 Ch8 Table A/B(x-wave ≥ 61.8% × min(兩側淨幅)= 大 x)對映 11-variant `CombinationKind`,大 x + Zigzag 構成段等不可辨識組合不產 garbage(與 monowave 級行為一致) |
| §6.1 邊界波重評(D-4 修復) | `round_engine.rs` | 聚合成功後 parent 於其 tiling 取**真實前後鄰居**,(\|m(−1)\|, parent 首子波)與(parent 末子波, \|m(+1)\|)magnitude 比三檔:[0.382, 2.618] Pass / [0.236, 4.236] 內 Advisory Info / 之外 Advisory Warning — **不拒絕聚合**;無鄰居或零幅度該側跳過。邊界重評屬 tiling 語境(同節點跨 tiling 鄰居不同)→ 逐 materialize 計數;凍結時 AdvisoryFinding 掛載留 G2.4 收集階段 |
| §6.2 Complexity 真算 + Triplexity | 同上 | 節點樹遞迴:葉 = 0(Simple)/ degree-1 = 1(Polywave)/ 任一 `:5` 子節點為 impulsive polywave = 2(Multiwave,修正波同式 — Zigzag/Flat 之 `:5` 子節點同計)/ 一 `:5` 為 Multiwave 且另一 `:5` ≥ Polywave = 3(Macrowave 上限);Triplexity = 子樹內 Impulse 段(impulsive pattern node + `:5` slot 上的葉 = Level-0)出現 ≥ 3 種不同 level。收集 forest 分布進 diagnostics;凍結時 `complexity_level`/`triplexity_detected` 欄對映留 G2.4(ComplexityLevel enum 3 值 vs Level 0-3 的壓縮拍板) |
| §6.3 Degree 對映 | 同上 | ceiling 錨定法:tiling 最高 degree_level 對映 Stage 11 同式 ceiling(`degree::compute_ceiling`)允許之最高 Degree,逐層向下遞減;超出 11 級下界夾 SubMicro 並計數。輸出展示用,**不**回饋任何接受條件 |
| A-10 anchors union | 同上 | 節點 anchors 語意收緊觀測:PatternBound **完整含於**節點覆蓋 monowave 範圍(union)vs 現行「日期範圍重疊即算」近似 — 兩者並行計數(`anchors_union_total` / `anchors_overlap_total`),收緊幅度供 Gate;凍結時實際掛載留 G2.4。`run_shadow` 簽名 + `&pattern_bounds` / `input.timeframe` 接線 |
| 診斷合約 | `output.rs` | `ShadowCompactionDiagnostics` +10 欄(boundary_* ×4 / complexity_count_by_level / triplexity_nodes / degree_map / degree_clamped_levels / anchors_union_total / anchors_overlap_total);engine tag → `tiling-round-g2.3`;ts 契約重生(加欄相容);`CombinationKind` 補 `PartialEq` |

### 驗證(2026-08-26 sandbox)

- `cargo test -p neely_core`:**463 passed / 0 failed**(+14:A-9 3-窗 Flat 細分 /
  Running / weak-b 落 row ×3、7-窗 DoubleZigzag / 大x+Zigzag 落 row / 大x DoubleThree
  ×3、§6.1 三檔 + impulse_chain 鄰居計數 ×2、§6.2 spec 表逐級 + triplexity ×2、
  §6.3 錨定 + 夾邊 ×2、A-10 union<overlap integration ×1)
- `cargo test --workspace`:**688 passed / 0 failed**(674 baseline + 14);
  ts codegen(`--features ts`)全綠
- production 影響:無(全 shadow);G2.2 已知邊界三項(Flat/Combination 細分、
  Alternation complexity 軸尚未消費 degree 差、邊界重評)本輪收掉前兩者的
  細分/重評主體;Alternation complexity 軸改用 degree_level 差(§4.3)屬 W5
  validator 內規則輸入,留 G2.4 凍結時與 structural_facts 一併接

### 待本機(G2.3 gate 觀測項)

六檔 run 讀 `snapshot->'diagnostics'->'shadow_compaction'`(`created_at DESC`):

1. `engine = "tiling-round-g2.3"`、inv/w1 仍全 0(I1–I6 gate 不回歸)。
2. **A-9 效應**:degree-1 節點 pattern 分布(Flat 變體 / Combination kind 細分
   後,節點數應較 G2.2 的 9–16 減或持平 — weak-b Flat row 廢除是唯一收緊源)。
3. **§6.1 分布**:boundary_advisory_info / warning 佔 pairs_checked 比例
   (預期多數 Pass;Warning 高 = 邊界比例極端的解讀多,供 beam 參考)。
4. **§6.2 / §6.3**:complexity 分布(五檔預期以 1 為主、2 少量)、degree_map
   合理(daily ~4 年資料 ceiling = Minor)。
5. **A-10 收緊幅度**:anchors_overlap_total − anchors_union_total。

### G2.3 gate 五檔實測(2026-08-26 本機,bars 991-996)

| stock | inv | w1 | levels | cplx | bnd checked/info/warn | anchors u/o | q3 殘差 | ms |
|---|---|---|---|---|---|---|---|---|
| 0050 | 0 | 0 | 1:10 | 1:10 | 1584/113/300 | 1/7 | 1/1 | 11.2 |
| 2330 | 0 | 0 | 1:19, 2:1 | 1:20 | 1612/353/81 | 2/6 | 1/8 | 13.6 |
| 3363 | 0 | 0 | 1:10 | 1:10 | 1542/165/272 | 1/5 | 0/2 | 11.1 |
| 6547 | 0 | 0 | 1:11 | 1:11 | 1274/85/46 | 3/5 | 2/2 | 10.0 |
| 1312 | 0 | 0 | 1:11 | 1:11 | 1572/241/253 | 1/4 | 0/3 | 10.8 |

- **gate 過**:engine 全 `tiling-round-g2.3`;inv / w1 五檔全 0;耗時與 G2.2 持平。
- **A-9 效應判讀修正**:degree-1 節點 10/19/10/11/11 vs G2.2 的 9/16/12/11/10 —
  增減互見,非本檔原預告的「減或持平」。機制:A-9 收緊的是單窗可產 kind,
  但被廢除的低分窗(Flat Common power 0)原佔 materialize 名額,兩階段按分
  選取下名額讓給高分窗;且細分後 RunningCorrection \|power\|=3 等抬高 splice
  分數 → beam 池組成重組,collected forest 聯集波動。屬預期池重組,非回歸。
- **§6.1 分布**:Pass 63–90% / Info 6.7–21.9% / Warning 3.6–18.9% — 多數 Pass,
  Warning 檔是否進 beam 參考鍵留 Gate v3;skipped=0(端點側 splice 未被按分
  選中,合理)。
- **§6.2 / §6.3**:complexity 全 1(2330 之 degree-2 節點 pattern 子節點屬 :3
  族,不構成 Multiwave — 語意正確);triplexity 0;degree_map 996 bars ≈ 4 年
  → ceiling Minor,2330 `{0:Minuette,1:Minute,2:Minor}`、餘 `{0:Minute,1:Minor}`,
  clamped 0。
- **A-10 收緊幅度**:union 1–3 vs overlap 4–7 — 現行日期重疊近似**高估
  60–86%**,收緊有實質效果(spec 廢除近似的實證依據)。
- **Q3 殘差**:Σ 4/16 = 25% 分歧率(bars 判準為主判後的殘差觀測),
  持續佐證 Q3 拍板。

**G2.3 收案(2026-08-26)**;餘 G2.4:下游契約協調(§7.4)+ P0 Gate v3
(全市場)+ 切換刪舊 — 依 spec 附錄 B,等拍板開工。

---

## G2.4 前半 — 下游契約協調(§7.3/§7.4)+ Gate v3 工具(2026-08-26)

切換刪舊(後半)依 spec §3.3 於 P0 Gate v3 全門檻過後一個 PR 內執行;
本段先落**阻斷性契約協調**與 Gate 聚合工具,全部加欄相容、serving 行為不變。

| 項 | 檔案 | 內容 |
|---|---|---|
| `Scenario.wave_count`(§7.4) | `output.rs` + `classifier/mod.rs` + `three_rounds.rs` | 結構化欄 = wave_tree 頂層 children 數(Level-0 = candidate.wave_count;three_rounds 聚合 = window.len());JSONB 加欄相容 |
| `WaveNode.degree_level` / `base_label`(§7.3) | 同上 | 巢狀展示與 MCP 語意欄:現行(切換前)引擎 best-effort 填值 — Level-0 root = degree 1 + Ch7 base;葉 = degree 0 + Stage 0 Primary 候選(無 Primary 依 pattern `:5` slot 推定);three_rounds parent = max(children)+1(I4)。切換後由 CompactionNode 凍結真值 |
| fusion 改讀新欄(Q6) | `src/fusion/dual_track/track1.py` | `primary.get("wave_count")` 優先,舊 snapshot 退 `wave_count_from_label` 字串 parse;`_picker.wave_count_from_label` docstring 標 **DEPRECATED**(一個 release 後移除,structure_label 格式屆時改 `{Pattern} L{degree} [...]`) |
| MCP 說明(§7.4) | `mcp_server/tools/wave.py` | `neely_forecast` docstring 補 forest 巢狀語意:degree_level / base_label 欄、「Level-0 與 Level-N 是不同層級解讀非同級並列」、wave_count 結構化欄 |
| Gate 資料源補欄 | `output.rs` + `round_engine.rs` | `ShadowCompactionDiagnostics.node_count_by_pattern`(canonical 去重後 pattern tag 分布)— §9.2 Terminal Impulse 存在性門檻與形態分布報告的資料源 |
| Gate v3 聚合腳本(§9.4) | `scripts/verify_compaction_v2_gate.py`(新) | 全市場 latest 列(`created_at DESC`)聚合:硬性五項自動判定(inv=0 / w1=0 / Terminal 存在 / forest proxy p99≤40 / 召回率≥98% + 低於門檻檔清單)+ 觀測項(cap 率 / 耗時分位 / level·complexity·pattern 分布 / §6.1 三檔 / A-10 / Q3 殘差);退碼 0/1;runtime/RSS 與抽驗/前端檢視列手動欄 |
| 文件同步(§7.4) | `m3Spec/neely_core_architecture.md` §14.1 | 範例 JSON 補 `wave_count` + 巢狀 `wave_tree`(degree_level/base_label)欄;`rust_compute/schema_dump.txt` 為表級 dump 無 JSONB 欄位樣例,無需更新 |

### 驗證(2026-08-26 sandbox)

- `cargo test -p neely_core` 463 passed / workspace **688 passed / 0 failed**
  (契約欄位為加欄,無行為變更;~28 個測試 fixture 補欄)
- ts 契約重生:`Scenario.ts` / `WaveNode.ts` / `ShadowCompactionDiagnostics.ts`
  (加欄相容);`--features ts` 全綠
- Python:fusion/wave 相關測試選集全綠;sandbox 全套 16 failed 為 bronze /
  rate_limiter / web_api-syspath 環境性失敗(乾淨 HEAD 重現,與本次無關,
  正式基準以本機 pytest 為準)

### G2.4 後半(等 Gate)

1. **本機 P0 Gate v3**:baseline 計時(現行 run-all wall time 入
   docs/benchmarks)→ 新 binary 全市場 `run-all --write` → 
   `scripts/verify_compaction_v2_gate.py` → 結果入
   `docs/benchmarks/neely_compaction_v2_gate_results_<date>.md`。
2. **切換刪舊**(Gate 全過後一 PR):§7.1/§7.2 凍結流程(CompactionNode →
   Scenario 23 欄)、serving forest 改吃 tiling-round、刪 `exhaustive.rs` 遞迴 +
   `three_rounds.rs` 弱比對路徑、`beam_width` 移除(附錄 A)、structure_label
   新格式 + fusion 字串 parse 移除(Q6 一個 release 條款自此起算)。

### P0 Gate v3 第一輪(2026-08-26 本機全市場)+ 收集語意修正

全市場 2192 檔(run-all wall 480 min — DB-bound 已知議題,neely 無關,見下):

| 門檻 | 結果 |
|---|---|
| I1–I6 / w1 | **0 / 0(2192 檔)✅** |
| Terminal Impulse 存在(D-5) | **191 顆 ✅** |
| forest proxy p99 ≤ 40 | p50=11 / p95=20 / p99=24 ✅ |
| §9.3 召回率 ≥ 98% | **10.68% ❌** → 根因分析見下,引擎修正後複測 |
| runtime | shadow Σ=49.5s 全市場(p50 14.4ms / p99 78.7ms)— stage_8 舊路徑經 G2.0 後近 no-op(Σ=0.2s),以其為分母無意義;公平判準改「shadow vs neely 全程占比」(腳本已改),實質過 |
| RSS | diagnostics `peak_memory_mb` 從未填值(全 0)→ 工作管理員觀測;欄位填值列 backlog |

觀測:level_cap_hit 94.8%(A-8 動態化議題的量測依據)、branch cap 1303 檔、
q3 殘差 31.3%、§6.1 Info 16.7%/Warn 11.0%、A-10 高估 80.6%、
pattern 分布健康(Flat 七變體/Running/Triangle/Terminal/Combination 細分全出值);
engines 混 1 檔 g2.1 = TAIEX 1900-01-01 殘列(既有 backlog,可刪)。

**召回 10.68% 根因(讀碼確證)**:實作僅從**最終 beam pool(32 tilings)**收集
forest — 違背 spec §7.1 I6「收集**全 tilings**」語意。每個階梯接受的視窗在概念上
都屬某條 tiling,branch cap / beam 是限制**深化探索**的工程護欄,不得限縮收集;
收集被截斷使新 degree-1 median 僅 11 顆/檔 vs 舊 forest ~17.7 顆/檔,分子結構性
塌陷。**修正(engine tag → `tiling-round-g2.4`)**:收集改於 materialize 時累積
(canonical 去重、同 canonical 跨 tiling 共享同一 Rc 節點),cap 之外的接受解讀
照收、僅不展開 tiling 分支;凍結側護欄(forest_max_size 200 / BeamSearchFallback)
依 §7.1 步驟 4 於切換時把關。鎖測試:beam=1(cap=8)稠密鏈收集數 > cap。

**§9.3 diff 工具**:diagnostics 加 `degree1_node_keys`(`"s-e:tag"` 排序序列,
shadow 期專用);gate 腳本加 `--stocks`(六檔複測)與 `--diff <stock>`
(召回缺口四分類:exact / tag 變體差 / bar 錯位 ≤3(Neutral 橋接嫌疑)/ 缺席)。
殘餘缺口需逐檔 diff 定 (a)/(b) 允許類別或 r4 新增類別(W7 全相鄰對已預告;
Neutral 橋接 bar 錯位為新嫌疑)。

驗證:`cargo test -p neely_core` 463 passed / workspace 688 passed / ts 重生。
複測程序:六檔 rerun → `--stocks` 聚合看召回方向 → `--diff` 分類殘餘 →
全市場 rerun(建議先跑 `maintain_facts_stats.sql` 解 DB 慢)。

### Gate 第二輪(六檔複測)+ 召回驗屍儀表(2026-08-26/27)

六檔複測(g2.4 收集修正後,`--stocks` 限定):inv/w1 仍 0;**收集修正生效**
(2330 degree-1 20→38)**但召回率未動(10.5%)** — `--diff` 分類把兩個嫌疑
同時排除:`bar_offset_le3 = 0`(非 Neutral 橋接錯位)、`tag_mismatch` 極少
(1213 僅 1 筆)。**80–98% 屬 `absent`:舊視窗在新階梯下整個未被接受**
(2330 exact 4/20;1213 exact 0/46、新 degree-1 僅 12)。根因移向接受階梯
本身的 spec 修正累積效應(W2 label 閘 / W4 bars / W7 全相鄰對 / W5 /A-9),
各項皆 spec 明訂,但 §9.3 98% 門檻未預期總量 — 需逐階段量化才能拍板
「r4 修門檻/允許清單」vs「引擎 bug」。

**召回驗屍儀表(本輪落地)**:`recall_miss_by_stage` — 每個未召回舊
scenario 對 base 葉序列找對齊視窗、重放階梯記第一個拒絕者
(no_aligned_start/end、len_mismatch、w1/w3/w4/w7/w2_label/w6/w5、
tag_diff、accepted_but_not_collected = 收集 bug 指標);gate 腳本聚合印出。
附修:forest proxy 轉觀測(收集修正後為 §7.1 護欄前全量,p99≤40 於切換後
以真 forest_size 判 — 另記 spec 張力:1213 舊 forest 46 > 40,98% 召回與
p99≤40 對密集檔在數學上互斥,r4 需明文兩者關係);`--stocks` 改 nargs
且**PowerShell 須加引號**(未引號逗號串被拆陣列、`0050` 數字化為 `50` —
第二輪實測掉檔的根因)。

複測程序:rebuild → 六檔 rerun → gate `--stocks "..."`(引號)看驗屍分布。
