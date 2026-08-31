# [討論稿] 漸進收攏(Progressive Settlement)— 波浪引擎尾端錨定提案

> ⚠️ **本檔為討論稿,未拍版,不得作為實作依據。** 拍版前不動 Rust(鐵則
> 「best-guess 不上 Rust」);拍版後改寫進正式 m3Spec 並另案排程。
> 緣起與表現層止血紀錄見 `docs/changelog/wave-view-tuning.md`(2026-06-11)。
>
> **v4.39 註記(2026-08-31)**:`m3Spec/wave_judgment_loop.md` 已落地 —
> 「引擎替讀者選 primary」的各處啟發式(picker / 表現層預設)由**判讀迴路**
> (dossier → wave_judgments → J2 錨定 diff)取代;本提案 S1 判準 2 需要的
> `ch6_status`(Deferred = live edge)訊號亦已由 neely 1.2.0 Ch6 閘產出。
> 本提案剩餘範圍 = 引擎側歷史段**定型**(settlement)本體,仍屬另案拍版。

## 1. 問題陳述(2026-06-11 實測事實)

| 事實 | 出處 |
|---|---|
| 兩引擎資料窗 = 固定 lookback(日線 1500 根 ≈ 6 年) | `tw_cores/src/helpers.rs:153-181`、`traditional_core/src/loader.rs:136-138` |
| Neely candidate 滑窗遍歷整段 monowave 序列,任意位置匹配 | `neely_core/src/candidates/generator.rs:56-112` |
| 寫 snapshot 前無 recency 過濾,全 forest 進 JSONB | `tw_cores/src/writers.rs` |
| compaction = 窮舉聚合、所有 level 全保留(擴 forest) | `neely_core/src/compaction/exhaustive.rs:30-53` |
| 無任何「live(進行中)scenario」分類欄位 | `awaiting_l_label` 是 forest 級 Round-3 暫停旗標,非尾端分類 |

後果:forest 內新舊形態混雜,`wave_tree.end` 可落在窗內任意年份;下游(UI 雲層、
facts、key_levels)只能靠 recency 啟發式事後補救。user 預期行為:
**舊有資料收攏成固定波型(定型),引擎只從資料尾端反推未來。**

## 2. 對齊原典:NEoWave 漸進分析法

Neely 的實戰流程本身就是漸進的:已完成且被後續走勢確認的 pattern **壓成高一階
單波(compact)**,之後的分析以收攏後的序列為底,只在 live edge 展開新計數;
「市場即訊息,形態完成後下一浪澄清前浪」。現行引擎借用了 compact 機制
(bottom-up 聚合)但缺了兩件事:**淘汰**(settled 區的低階展開不再保留)與
**尾端錨定**(新 candidate 必須觸及 live edge)。

## 3. 提案(兩階段)

### S1 — 歷史段 settlement(定型)

pattern 視為 settled 的規則候選(具體判準 = 主要拍版點):

1. 其 `invalidation_triggers` 未被後續任何價格觸發;
2. 其 `post_pattern_behavior` 已被後續走勢實現(量化方式開放:如後續 N 根內
   走出預期方向 ≥ 某 fib 比例);
3. 結尾距資料尾端 ≥ N 個 monowave(N 候選 21,避免 live 區被誤定型);
4. (可選)已被更高階 aggregation 收編為子結構。

settled pattern 壓成高一階 monowave 後**淘汰其低階展開**(不進 forest)。
settlement 需 deterministic 且增量穩定(隔天 +1 bar 不得翻轉既 settled 區 —
這正是 user 要的「固定」;需 hysteresis 規則防抖)。

### S2 — candidate 生成尾端錨定

滑窗只保留「觸尾窗」:window 必須包含最後 K 個 directional monowave 之一
(K 候選 3-5)。輸出形狀:

- `scenario_forest` = live forest(全部觸尾);
- 新增 `settled_backbone`(定型骨架:settled 高階 monowave 序列 + 其壓階前
  的 pattern 摘要)→ UI 畫歷史結構、V1 卡 default focus 永遠 live。

## 4. 影響面(取捨清單)

| 下游 | 影響 | 處置方向 |
|---|---|---|
| Reverse Logic §2.4 | 「多套合法計數 = 中段」現靠全域 forest;改 live-only 後統計域變小 | 重定義為 live 域內診斷;`reverse_logic_observation` 語意 bump |
| `flat_fib_zones` / fusion key_levels | key_levels 要全歷史支撐壓力,live-only 聯集會丟歷史 level | 拆兩層:`settled_levels`(自 backbone)+ `live_fib_zones`(自 live forest) |
| facts / EventKind | per-scenario facts 數量大降 → 觸發率全面重校(≤12/yr/stock 標準) | settlement 本身可成新 EventKind(PatternSettled) |
| forecast spine | 若消費 neely facts → 重校 conformal | 對照回測 |
| PIT 重建 | settlement 必須可由任一 as_of 重放(point-in-time deterministic) | spec 內給 replay 規則 |
| traditional_core | 同概念可平行套用(已有 multi-degree compact,缺淘汰/錨定) | 分開拍版,不綁同案 |
| 效能 | live-only 生成域大幅縮小 → run-all 應變快 | 正面,P0-Gate 重測 |

## 5. 開放問題(拍版點)

1. settled 判準的量化(§3-S1 規則 2 的「實現」定義)與參數 N / K;
2. `settled_backbone` 的 JSONB 形狀與 `source_version` bump 策略
   (structural_snapshots schema 0 改,additive);
3. 既有消費端(MCP 14 tools / Web API / V1 V2 前端 / dual_track)遷移順序;
4. 驗證方法:PIT replay + 2330/3030 等 production case 對照
   (settled 區是否與人工 Neely 計數一致)。

## 6. 範圍聲明

本稿不排程、不動 Rust。表現層(雲層 live-only、stale 窗錨定、收盤背景線、
`scenario_age_days`)已於 2026-06-11 先行落地止血 — 即使本案永不拍版,
那些修法獨立成立;本案拍版後它們自然降級為 settled_backbone 的消費端。
