# Neely Combination 上游補完 — 實作計畫

> **狀態**:待動工(2026-05-20 拍板「完整方案」)
> **範圍**:補 Stage 3 / 4 / 5 上游,讓 `NeelyPatternType::Combination` 真正在
> production 產生,點亮 v4.5 G2 已建好但目前為 dead code 的
> `ch8_xwave` / `ch8_multiwave` / `triggers` / `emulation` / `post_validator` /
> `power_rating` Combination 鏈路。
> **工期**:~20-26 工程天 / 3-5 輪 P0 Gate(calibration 須 user 本機跑,沙箱無 DATABASE_URL)。
> **baseline**:v4.10 / neely_core 410 lib tests / alembic head `d9e0f1g2h3i4`(本計畫 0 schema migration)。

---

## 實作待辦清單

> 動工時逐項勾選。每個 P0 Gate 是 abort 點 —— 不綠不進下一里程碑。

### 里程碑 0 — 前置
- [ ] 確認 v4.10 baseline:`cargo test --release -p neely_core --lib` → 410 passed / 0 failed
- [ ] `CLAUDE.md` §2 backlog 加「Combination 上游補完」條目(避免「Out-of-Scope 全清空」誤導後續 session)

### 里程碑 A — Double-*（最小可交付）
- [ ] **P1a** `candidates/generator.rs:76` — wave_count `{3,5}` → `{3,5,7}` + magnitude 預篩
- [ ] **P1.5** `validator/core_rules.rs` — R3-R7 加 `wave_count != 5 → NotApplicable` guard ⚠️ **關鍵,漏做則產 0 個 Combination**
- [ ] **P2a** `classifier/mod.rs:58-62` — 加 `classify_7wave_combination`(Double-* 5 variant)
- [ ] 沙箱:`cargo test --release -p neely_core --lib` 全綠 + `cargo build --release -p tw_cores` 0 warning
- [ ] **P0 Gate#1**(user 本機):Combination scenario > 0 / forest p95 ≤ 50 / max ≤ 250 / ch8 advisory 出現 ≥ 10 檔 / 5 檔 Double-* 走勢 spot-check 命中

### 里程碑 B — Triple-*
- [ ] **P1b** `candidates/generator.rs` — wave_count `{3,5,7}` → `{3,5,7,11}`
- [ ] **P2b** `classifier/mod.rs` — 加 `classify_11wave_combination`(Triple-* 5 variant)
- [ ] 沙箱:`cargo test --release -p neely_core --lib` 全綠
- [ ] **P0 Gate#2**(user 本機):Triple-* scenarios 出現 ≥ 3 檔

### 里程碑 C — P3 Compaction Aggregate
- [ ] **P3** `compaction/three_rounds.rs` — 加 `try_aggregate_7` / `try_aggregate_11`
- [ ] 沙箱:`cargo test --release --workspace --no-fail-fast` 全綠
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

| 下游 | 位置 | 現況 |
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

## 2. ⚠️ 核實修正紀錄 — 為什麼有 P1.5

> 初版 plan 只列 P1(generator)+ P2(classifier)。核實 code 後發現中間夾著
> **Stage 4 Validator**,是 hard blocker,必須補 P1.5。

`classifier::classify`(`classifier/mod.rs:50`)第一行:
`if !report.overall_pass { return None; }` —— candidate 必須先過 validator。

核實 `validator/core_rules.rs`:
- **R1 / R2**:`wave_count != 5 → NotApplicable` ✅ 已正確 guard
- **R3**:guard 是 `wave_count < 3`(`core_rules.rs:158`)→ **對 7/11-wave candidate 照跑**
- **R4-R7**:guard 不一致(R3 已足證問題)

7/11-wave Combination 的 monowaves 不具 impulse 語意,被 Ch5 Essential R3-R7 套用
→ 不可預測地 Fail。`validate_candidate`:任一 `Ch5_Essential` Fail(gap ≥ 10%)
→ `essential_fail = true` → `overall_pass = false` → `classify()` 回 `None`。

**結論**:不做 P1.5,P1 + P2 產出 **0 個 Combination scenario**。
P1.5 修法 —— R3-R7 各加 `wave_count != 5 → NotApplicable`(對齊 R2 既有 pattern,
`core_rules.rs:109-111`),~1-2 天含 tests。

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

## 4. 步驟明細

### P1a — Stage 3 generator 擴 wave_count = 7
- **檔**:`candidates/generator.rs`
- **改**:line 76 `for &wc in &[3usize, 5]` → `&[3usize, 5, 7]`;`WaveCandidate.wave_count` doc 更新
- **加**:magnitude 預篩 —— 對齊 `neely_rules.md` line 1858-1859 Condition 1/2
  (中段 < 61.8% 回測 + 連續壓縮 ≥ 161.8%),先剔除明顯不對稱視窗,避免 beam_cap 撞頂
- **檢視**:`BEAM_CAP_MULTIPLIER`(現 `generator.rs:47` = 10);wc=7 滑窗增多,P0 Gate#1 後可能需調
- **不動**:`directions_alternate` —— 已核實 Combination monowaves 嚴格交替(D-U-D-U-D-U-D),無需改
- **tests**:7-wave candidate 生成 / 交替檢查 / cap 行為

### P1.5 — Stage 4 validator guard（關鍵,見 §2）
- **檔**:`validator/core_rules.rs`
- **改**:`rule_r3` ~ `rule_r7` 各加開頭 guard:
  ```rust
  if candidate.wave_count != 5 {
      return RuleResult::NotApplicable(rid);
  }
  ```
- **確認**:`rule_overlap_trending` / `rule_overlap_terminal` 對 wc≠5 → NotApplicable
  或不會雙雙 Fail(`both_overlaps_failed` 也會 block `overall_pass`)
- **tests**:7-wave candidate → `validate_candidate().overall_pass == true`(0 Essential fail)

### P2a — Stage 5 classifier 加 classify_7wave_combination
- **檔**:`classifier/mod.rs`
- **改**:line 58-62 match 加 `7 => classify_7wave_combination(candidate, classified)?`
- **新 fn `classify_7wave_combination`**:
  1. `split_at_x_wave` —— 找中段 x-wave(< 61.8% 回測 + duration 最短),切 `(seg_a, x, seg_b)`
  2. `classify_sub_segment` 遞迴(呼 `classify_3wave`)取 `kind_a` / `kind_b`
  3. `(kind_a, kind_b)` → `CombinationKind` 查表(對齊 `neely_rules.md` Table A / B)
  4. 回 `NeelyPatternType::Combination { sub_kinds }`
- **新 helper**:`is_large_x_wave` / `is_small_x_wave`(對齊 spec line 1874 / 1892)
- **Double-* 5 variant**:DoubleZigzag / DoubleCombination / DoubleFlat / DoubleThree / DoubleThreeCombination
- **tests**:5 個 Double variant classification

→ **P0 Gate#1**(user 本機):`tw_cores run-all --write`;驗收見 §6。

### P1b — generator wave_count = 11
- **檔**:`candidates/generator.rs`:`&[3,5,7]` → `&[3,5,7,11]`

### P2b — classifier classify_11wave_combination
- **檔**:`classifier/mod.rs`
- 5 段切分 `seg_a + x1 + seg_b + x2 + seg_c`
- **Triple-* 5 variant**:TripleZigzag / TripleCombination / TripleThree / TripleThreeCombination / TripleThreeRunning

→ **P0 Gate#2**。

### P3 — compaction aggregate
- **檔**:`compaction/three_rounds.rs`
- 加 `try_aggregate_7` / `try_aggregate_11`,從 Level-0 scenarios 拼 higher-degree Combination
- ⚠️ **已知限制**:P3 產的是 Level-1+ Combination,出現在 Stage 7.5 之後 →
  **拿不到 `ch8_xwave` / `ch8_multiwave` advisory**(advisory 只在 Stage 7.5 pre-compaction 跑);
  P3 只點亮 `power_rating`(Stage 10a,post-compaction)

→ **P0 Gate#3**。

---

## 5. 範圍邊界

| 動 | 不動 |
|---|---|
| `candidates/generator.rs` wave_count 擴展 | architecture 19-stage 結構 |
| `validator/core_rules.rs` R3-R7 guard | 既有 R1/R2 / Overlap 邏輯 |
| `classifier/mod.rs` 新增 7/11-wave 路徑 | 既有 5 種 pattern 分類邏輯 |
| `compaction/three_rounds.rs` `try_aggregate_7` / `_11` | 既有 `try_aggregate_3` / `_5` |
| 新 unit tests | 既有 410 tests |

**不在範圍**:
- §9.2 Ch11 Triangle advisory ordering(Triangle 只在 compaction 後產生,
  advisory 在 compaction 前跑 —— 獨立工程)
- advisory → hard 全面轉換(見本 session 盤點報告 §8 中間路線議題)
- spec 文件改寫(`neely_rules.md` / `neely_core_architecture.md` 不動)
- 0 alembic / 0 collector.toml / 0 Python

---

## 6. 驗證

### 沙箱(每步,Claude 跑)
- `cargo test --release -p neely_core --lib` 全綠
- `cargo build --release -p tw_cores` 0 warning

### P0 Gate(每里程碑後,user 本機跑)
- `tw_cores run-all --write` + forest_size 分布 SQL + spot-check

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

---

## 7. 風險

| 風險 | 量化指標 | 緩解 |
|---|---|---|
| beam_cap 撞頂(wc=7/11 滑窗多) | candidate_count vs `beam_width × cap` | 調 magnitude 預篩 / 提高 `BEAM_CAP_MULTIPLIER` |
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

---

## 附錄:classify_7wave_combination 核心邏輯草圖

```rust
fn classify_7wave_combination(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> Option<NeelyPatternType> {
    // 1. 找中段 x-wave(< 61.8% 回測 + duration 最短),切三段
    let (seg_a, _x, seg_b) = split_at_x_wave(candidate, classified)?;

    // 2. 遞迴分類兩個 corrective sub-segment
    let kind_a = classify_sub_segment(&seg_a, classified)?;
    let kind_b = classify_sub_segment(&seg_b, classified)?;

    // 3. (kind_a, kind_b) → CombinationKind(對齊 neely_rules.md Table A/B)
    let combination_kind = match (kind_a, kind_b) {
        (SubKind::Zigzag, SubKind::Zigzag)   => CombinationKind::DoubleZigzag,
        (SubKind::Zigzag, SubKind::Flat)     => CombinationKind::DoubleCombination,
        (SubKind::Flat,   SubKind::Flat)     => CombinationKind::DoubleFlat,
        (SubKind::Flat,   SubKind::Triangle) => CombinationKind::DoubleThreeCombination,
        // ... 其餘對齊 spec Table A/B
        _ => return None,
    };

    Some(NeelyPatternType::Combination { sub_kinds: vec![combination_kind] })
}
```

---

> 本文件由 2026-05-20 Neely 原作落實盤點 session 衍生:
> 盤點 → advisory→hard 複雜度評估 → 揭露 Combination 上游缺口(classifier 不分類)
> → user 拍板「完整方案」→ 核實 plan、補 P1.5 validator → 本實作計畫。
