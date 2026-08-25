# neely_core Compaction v2 — Tiling-Round 引擎(G2.x 系列)

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
| T-5 | `t5_level_n_with_summed_rules_not_ranked_bottom` | BeamSearch 同組內 Level-1(Σrules=12)不墊底,淘汰的是組內 count 最低的 Level-0 |
| T-6 | exhaustive tests 日期債清償 | `make_simple` 同日退化值 → 真實日期鏈;`max_compaction_levels_respected` 之 `% 28` 月內 wrap(產生 end < start)→ chrono 連續 50 段 × 5 天;`p3_aggregated_rules_passed_count_sums_children` 鎖 P3 語意 |

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
- 六檔 + 全市場 shadow 觀測(I1–I6 計數、level_cap_hit / round_branch_cap_hits
  命中率、召回率、stage_8s 耗時)留 user 本機 production run:讀
  `snapshot->'diagnostics'->'shadow_compaction'`(structural_snapshots 的 payload
  欄是 `snapshot`,無獨立 diagnostics 欄)
