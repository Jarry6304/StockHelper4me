# Neely Combination 上游補完 — 實作計畫

> **狀態**:里程碑 A+B+C **code 完成 + 沙箱驗證**(2026-05-20,commit `5bdc7ba`);
> **P0 Gate 待 user 本機跑**。
> **範圍**:補 Stage 3 / 4 / 5 / 8 上游,讓 `NeelyPatternType::Combination` 真正在
> production 產生,點亮 v4.5 G2 已建好但原為 dead code 的
> `ch8_xwave` / `ch8_multiwave` / `triggers` / `emulation` / `post_validator` /
> `power_rating` Combination 鏈路。
> **baseline**:v4.10 / neely_core 410 lib tests → 實作後 **419**;alembic head
> `d9e0f1g2h3i4`(本計畫 0 schema migration)。

---

## 實作摘要(2026-05-20)

里程碑 A+B+C 一次性實作完成(1 commit `5bdc7ba`,branch `claude/start-v4.9-j7WZX`)。

| 沙箱驗證 | 結果 |
|---|---|
| `cargo test --release -p neely_core --lib` | **419 passed / 0 failed**(410 baseline → +9)|
| `cargo build --release -p tw_cores` | **0 warnings** |
| `cargo test --release --workspace --no-fail-fast` | **596 passed / 0 failed**(587 → +9)|

下一步:user 本機跑 P0 Gate#1~#3(見 §6),沙箱無 DATABASE_URL 無法跑。

---

## 實作待辦清單

> `[x]` = code 完成 + 沙箱驗證;`[ ]` = 待 user 本機。每個 P0 Gate 是 abort 點。

### 里程碑 0 — 前置
- [x] v4.10 baseline:`cargo test --release -p neely_core --lib` → 410 passed / 0 failed
- [x] CLAUDE.md 加 v4.11 段(取代「§2 backlog 條目」,功能等價 — 修正「Out-of-Scope 全清空」誤導)

### 里程碑 A — Double-*
- [x] **P1a** `candidates/generator.rs` — wave_count `{3,5}` → `{3,5,7}` + per-wave-count cap
- [x] **P1.5** `validator/core_rules.rs` — R3/R4 guard `< 3` → `!matches!(3 | 5)`
- [x] **P2a** `classifier/mod.rs` — `classify_7wave_combination`(Double-* 5 variant)
- [x] 沙箱:neely_core 419 passed / tw_cores build 0 warning
- [ ] **P0 Gate#1**(user 本機):Combination > 0 / forest p95 ≤ 50 / max ≤ 250 / ch8 advisory ≥ 10 檔 / 5 檔 Double-* spot-check

### 里程碑 B — Triple-*
- [x] **P1b** `candidates/generator.rs` — wave_count `{3,5,7}` → `{3,5,7,11}`
- [x] **P2b** `classifier/mod.rs` — `classify_11wave_combination`(Triple-* variant)
- [ ] **P0 Gate#2**(user 本機):Triple-* scenarios 出現 ≥ 3 檔

### 里程碑 C — P3 Compaction Aggregate
- [x] **P3** `compaction/three_rounds.rs` — `try_aggregate_7` / `try_aggregate_11` + wire 進 `aggregate_one_level`
- [ ] **P0 Gate#3**(user 本機):`emulation::CombinationAsImpulse` fire ≥ 1 / `post_validator` Combination Stage 2 fire ≥ 1

---

## 1. 背景與動機

### 1.1 問題:Combination 在 production 從不產生

核實 `classifier` + `compaction` 的 `pattern_type` 實際產出:

| 來源 | 產出的 pattern_type |
|---|---|
| `classifier/mod.rs:199-632`(非 test) | Impulse / Diagonal / Zigzag / Flat / RunningCorrection |
| `compaction/three_rounds.rs:104/117/156/175` | Impulse / Triangle / Zigzag / Flat |

`NeelyPatternType::Combination` 的構造點**全在 test module** —— production code path 0 個。

### 1.2 後果:v4.5 G2 下游全是 dead code

下列鏈路在 v4.5 Group 2 已完整建好,但因上游不產 Combination → **永不 fire**:

| 下游 | 位置 | 原現況 |
|---|---|---|
| `ch8_xwave::detect` | `ch8_xwave/mod.rs` | 對非 Combination early-return |
| `ch8_multiwave::detect` | `ch8_multiwave/mod.rs` | 同上 |
| triggers Combination arm | `triggers/mod.rs:182` | 永不命中 |
| `emulation::check_combination_as_impulse` | `emulation/mod.rs` | 永不命中 |
| post_validator Combination Stage 2 | `post_validator/mod.rs:69` | 永不命中 |
| power_rating Combination(11 variant 完整) | `power_rating/table.rs:97` | 永不命中 |

→ 補上游 = 點亮已存在但未通電的下游。codebase 本身已預留此工作:
`generator.rs:21`「5-wave-of-3 嵌套(Combination 類型需要)」、
`classifier.rs:27`「Combination DoubleThree / TripleThree」。

---

## 2. 核實修正紀錄 — 為什麼有 P1.5

> 初版 plan 只列 P1(generator)+ P2(classifier)。核實 code 後發現中間夾著
> **Stage 4 Validator**,是 hard blocker,必須補 P1.5。

`classifier::classify`(`classifier/mod.rs:50`)第一行:
`if !report.overall_pass { return None; }` —— candidate 必須先過 validator。

核實 `validator/core_rules.rs` 9 條 core rule 的 wave_count guard:

| 規則 | 原 guard | 對 wc=7/11 |
|---|---|---|
| R1 | `wave_count == 5 && len == 5` | ✅ NotApplicable |
| R2 | `wave_count != 5` | ✅ NotApplicable |
| **R3 / R4** | **`wave_count < 3`** | ❌ **照跑**(bug)|
| R5 / R6 / R7 | `wave_count != 5` | ✅ NotApplicable |
| Overlap_Trending / Overlap_Terminal | `wave_count != 5` | ✅ NotApplicable |

7/11-wave Combination 的 monowaves 不具 impulse 語意,被 R3/R4 套用 → 不可預測地
Fail。`validate_candidate`:任一 `Ch5_Essential` Fail(gap ≥ 10%)→ `essential_fail =
true` → `overall_pass = false` → `classify()` 回 `None`。

**結論**:不做 P1.5,P1 + P2 產出 **0 個 Combination scenario**。

**P1.5 實作**:只需改 **R3 / R4**(R5-R7 / Overlap 早已正確)。guard 改用
`!matches!(candidate.wave_count, 3 | 5)`(**非** `!= 5`)—— 保留 wc=3 corrective
既有適用、wc=5 不變,只對 wc∉{3,5} NotApplicable,**零附帶 wc=3 行為變動**。

---

## 3. 動工順序與里程碑

```
里程碑 A(Double-*)   P1a ─▶ P1.5 ─▶ P2a ──────────▶ P0 Gate#1
里程碑 B(Triple-*)   P1b ─▶ P2b ───────────────────▶ P0 Gate#2
里程碑 C(P3 aggregate) try_aggregate_7 / _11 ───────▶ P0 Gate#3
```

- **依賴**:P1 → P1.5 → P2 強依賴(順序不可換)。P2 完成的瞬間,6 條下游同時點亮。
- **abort 點**:每個 P0 Gate。最小可交付是里程碑 A(空集合風險已由 P1.5 解除)。
- **calibration 分工**:Claude 寫 code + 沙箱 `cargo test`;**user 本機跑 P0 Gate**
  (沙箱無 DATABASE_URL)。

---

## 4. 步驟明細(實作結果)

### P1a — Stage 3 generator 擴 wave_count = 7
- **檔**:`candidates/generator.rs`
- **實作**:`for &wc in &[3usize, 5]` → `&[3usize, 5, 7, 11]`(P1a 加 7、P1b 加 11 同 commit)
- **per-wave-count cap**:原本單一共用 `cap`(`beam_width × 10`)→ 各 wave_count 各有
  獨立 `cap` 額度(`let wc_cap_base = candidates.len();` + `candidates.len() -
  wc_cap_base >= cap → break`)。防 wc=7/11 被 wc=3/5 佔滿共用 cap 而完全產不出。
- **magnitude 預篩**:**未實作 — 延後至 P0 Gate#1**。理由:spec line 1858-1859 的
  Condition 1/2 細節未確認,best-guess 不上 code;per-wc cap 已防 starve;若 P0 Gate#1
  揭露 candidate 數爆量,屆時再加(plan §7 risk table 列為 tuning 手段)。
- **不動**:`directions_alternate` —— 已核實 Combination monowaves 嚴格交替
  (D-U-D-U-D-U-D),無需改。
- **tests**:`seven_alternating_monowaves_yield_wc7_candidate` /
  `eleven_alternating_monowaves_yield_wc11_candidate` / `beam_width_cap_limits_candidates`
  (改 per-wc cap 斷言)。

### P1.5 — Stage 4 validator guard(關鍵,見 §2)
- **檔**:`validator/core_rules.rs`
- **實作**:`rule_r3` / `rule_r4` 的 guard `if candidate.wave_count < 3` →
  ```rust
  if !matches!(candidate.wave_count, 3 | 5) {
      return RuleResult::NotApplicable(rid);
  }
  ```
  R5/R6/R7/Overlap_Trending/Overlap_Terminal **不動**(已正確 guard `!= 5`)。
- **tests**(`validator/mod.rs`):`seven_wave_candidate_not_rejected_by_essential_rules`
  —— wc=7 candidate → `overall_pass == true` + 9 條 core rule 全 NotApplicable。

### P2a — Stage 5 classifier 加 classify_7wave_combination
- **檔**:`classifier/mod.rs`
- **實作**:
  - `classify` match 加 `7 => classify_7wave_combination(candidate, classified)?`
  - `classify_3wave` 抽出核心為 `classify_3wave_segment(mi: &[usize], classified)`,
    供 sub-segment 複用(`classify_3wave` 變薄包裝)
  - 新 `classify_7wave_combination`:7 monowaves = sub_a(`mi[0..3]`)+ x-wave(`mi[3]`)
    + sub_b(`mi[4..7]`);兩 sub-segment 各跑 `classify_3wave_segment` → 對映 CombinationKind
  - 新 `x_wave_is_large(x_idx, sub_a, sub_b, classified)`:x magnitude ≥ 61.8% ×
    min(兩側 sub-segment 淨幅)→ 大 x-wave(Table B)
  - 新 `map_double_combination(kind_a, kind_b, large_x)`:Table A(小 x)允許 Zigzag /
    Table B(大 x)構成段不可有 Zigzag(spec Ch8 Table B 修正)→ None
- **Double-* variant**:DoubleZigzag / DoubleCombination / DoubleFlat / DoubleThree /
  DoubleThreeCombination
- **tests**:`seven_wave_double_zigzag_classifies_as_combination` /
  `seven_wave_combination_full_classify_produces_scenario` /
  `large_x_wave_with_zigzag_component_rejected`

### P1b — generator wave_count = 11
- **檔**:`candidates/generator.rs` — `&[3,5,7]` → `&[3,5,7,11]`(與 P1a 同 commit)

### P2b — classifier classify_11wave_combination
- **檔**:`classifier/mod.rs`
- **實作**:`classify` match 加 `11 => classify_11wave_combination(...)?`;
  11 monowaves = sub_a(`mi[0..3]`)+ x1(`mi[3]`)+ sub_b(`mi[4..7]`)+ x2(`mi[7]`)
  + sub_c(`mi[8..11]`);三 sub-segment 各跑 `classify_3wave_segment`;
  新 `map_triple_combination(kind_a, kind_b, kind_c, large_x)`(任一 x 為大 → Table B)
- **Triple-* variant**:TripleZigzag / TripleCombination / TripleThree / TripleThreeRunning
- **tests**:`eleven_wave_triple_zigzag_classifies_as_combination` /
  `map_triple_combination_table_b_rejects_zigzag`

### P3 — compaction aggregate
- **檔**:`compaction/three_rounds.rs`
- **實作**:`aggregate_one_level` 加 7-pattern / 11-pattern 滑窗;新
  `try_aggregate_7` / `try_aggregate_11` —— 從 Level-N scenarios(已分類)拼
  higher-degree Combination:全 `:_3` corrective(`StructureLabel::Three`)+ 方向交替
  + `all_pairs_pass_sb` + `boundary_retracement_extreme` 拒絕極端邊界 → `build_aggregated`
  Combination。CombinationKind 取通用 `DoubleThree` / `TripleThree`,細分留 P0 Gate#3。
- ⚠️ **已知限制**:P3 產的是 Level-1+ Combination,出現在 Stage 7.5 之後 →
  **拿不到 `ch8_xwave` / `ch8_multiwave` advisory**(advisory 只在 Stage 7.5 pre-compaction
  跑);P3 只點亮 `power_rating`(Stage 10a,post-compaction)。
- **tests**:`aggregate_7_double_combination`

---

## 5. 範圍邊界

| 動 | 不動 |
|---|---|
| `candidates/generator.rs` wave_count 擴展 + per-wc cap | architecture 19-stage 結構 |
| `validator/core_rules.rs` **R3 / R4** guard | 既有 R1/R2/R5/R6/R7/Overlap 邏輯 |
| `classifier/mod.rs` 新增 7/11-wave 路徑 | 既有 5 種 pattern 分類邏輯 |
| `compaction/three_rounds.rs` `try_aggregate_7` / `_11` | 既有 `try_aggregate_3` / `_5` |
| 新 unit tests(+9)| 既有 410 tests |

**不在範圍**:
- §9.2 Ch11 Triangle advisory ordering(Triangle 只在 compaction 後產生,
  advisory 在 compaction 前跑 —— 獨立工程)
- advisory → hard 全面轉換(見 Neely 盤點報告 §8 中間路線議題)
- spec 文件改寫(`neely_rules.md` / `neely_core_architecture.md` 不動)
- 0 alembic / 0 collector.toml / 0 Python

---

## 6. 驗證

### 沙箱(Claude 已跑,全綠)
- `cargo test --release -p neely_core --lib` → 419 passed / 0 failed
- `cargo build --release -p tw_cores` → 0 warnings
- `cargo test --release --workspace --no-fail-fast` → 596 passed / 0 failed

### P0 Gate(user 本機跑,待執行)
`tw_cores run-all --write` + forest_size 分布 SQL + spot-check。

**里程碑 A 綠燈條件**:
- [ ] 1266 stocks 跑出 Combination scenario 數 > 0(production 不再是 0)
- [ ] forest p95 ≤ 50(v4.10 baseline 28,容許膨脹至 50)
- [ ] forest max ≤ 250(v4.10 baseline 196)
- [ ] `ch8_xwave` / `ch8_multiwave` advisory_findings 出現於 ≥ 10 檔股票
- [ ] 5 檔已知 Double-* 走勢手動 spot-check 命中

**完整方案綠燈追加**:
- [ ] Triple-* scenarios 出現於 ≥ 3 檔股票
- [ ] `emulation::CombinationAsImpulse` 至少 fire 1 次
- [ ] `post_validator` Combination Stage 2 取 sub_kinds 細分至少 fire 1 次

**abort 條件**:forest max 從 196 跳 > 300。

> P0 Gate 詳細 SQL 見 `CLAUDE.md` v4.11 段。

---

## 7. 風險

| 風險 | 量化指標 | 緩解 |
|---|---|---|
| beam_cap 撞頂(wc=7/11 滑窗多) | candidate_count vs `beam_width × cap` | per-wc cap 已防 starve;若 candidate 爆量 → 加 magnitude 預篩 |
| forest_size 膨脹 | p95 從 28 → ?(目標 ≤ 50) | validator 對 Combination 套規則 / Combination 專屬 wave_rules |
| Combination 誤判 | 10 檔已知非 Combination 走勢 spot-check | 收緊 x-wave 偵測門檻 |
| 既有 scenarios 受影響 | 1266 stocks max / mean / overflow 對比 v4.10 | max > 300 → abort |
| calibration 跨 session | —— | 逐里程碑交付,P0 Gate 之間 user 本機跑 |

---

## 8. 已知限制

1. **wave_count {7, 11} 固定假設**:假設每個元件是最小 3-monowave 結構。
   Flat(3)+ Triangle(5)等非最小元件組合會漏 —— 第一刀接受此限制,後續可擴。
2. **P3 Level-1+ Combination 無 ch8 advisory**(見 §4 P3):
   advisory 評估只在 Stage 7.5(pre-compaction)跑。若要 P3 Combination 也走 ch8,
   需另把 advisory 評估移到 / 增跑於 compaction 後 —— 屬獨立架構工程,不在本計畫。
3. **magnitude 預篩未實作**:P1a 只做 per-wc cap;spec line 1858-1859 的 Condition 1/2
   預篩延後至 P0 Gate#1 —— 若 production candidate 數爆量再加。
4. **P3 CombinationKind 取通用值**:`try_aggregate_7/_11` 一律標 DoubleThree /
   TripleThree,未依 sub-segment 細分 —— 留 P0 Gate#3 校準。

---

## 附錄:實作後的 classify_7wave_combination

```rust
fn classify_7wave_combination(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<NeelyPatternType> {
    let mi = &candidate.monowave_indices;
    if mi.len() != 7 {
        return None;
    }
    // 7 monowaves = sub_a(0..3) + x-wave(3) + sub_b(4..7)
    let kind_a = classify_3wave_segment(&mi[0..3], classified);
    let kind_b = classify_3wave_segment(&mi[4..7], classified);
    let large_x = x_wave_is_large(mi[3], &mi[0..3], &mi[4..7], classified);
    let combination_kind = map_double_combination(&kind_a, &kind_b, large_x)?;
    Some(NeelyPatternType::Combination {
        sub_kinds: vec![combination_kind],
    })
}

// map_double_combination 對映(Table A 小 x-wave / Table B 大 x-wave):
//   large_x  + 任一 Zigzag        → None(spec Ch8 Table B:大 x-wave 不可有 Zigzag)
//   large_x  + 兩 Flat            → DoubleThree(含 RunningCorrection → DoubleThreeCombination)
//   small_x  + (Zigzag, Zigzag)   → DoubleZigzag
//   small_x  + (Zigzag, Flat)     → DoubleCombination
//   small_x  + (Flat, Flat)       → DoubleFlat
```

實際完整實作見 `rust_compute/cores/wave/neely_core/src/classifier/mod.rs`
(`classify_7wave_combination` / `classify_11wave_combination` / `x_wave_is_large` /
`map_double_combination` / `map_triple_combination`)。

---

> 本文件由 2026-05-20 Neely 原作落實盤點 session 衍生:
> 盤點 → advisory→hard 複雜度評估 → 揭露 Combination 上游缺口(classifier 不分類)
> → user 拍板「完整方案」→ 核實 plan、補 P1.5 validator → 實作里程碑 A+B+C
> (commit `5bdc7ba`)→ 本文件同步至實作後狀態。
