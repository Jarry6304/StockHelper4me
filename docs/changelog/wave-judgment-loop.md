# wave_judgment_loop — 證據 → 判讀 → 錨定迴路(2026-08-31)

> spec:`m3Spec/wave_judgment_loop.md` + 前置 `m3Spec/neely_ch6_gate_running_fix.md`(M0)。
> 本檔逐 phase 追記;production 全市場重跑與首批判讀見文末 runbook。

## 拍版紀錄(2026-08-30)

1. M0(neely 1.2.0)併入本 branch;M1–M4 全部 code 一次做;M5(J3 報告)另案
2. `/stocks/{id}/waves` **additive**:response = `{neely, traditional, dossier}`,raw 兩鍵不動(V1 圖表零斷源);MCP `neely_forecast` 照 spec 全換 dossier(`primary_scenario`/`scenario_count`/`scenario_staleness` 三鍵刪除)
3. 判讀寫入 = `POST /judgments`(web API 首個寫端點)+ CLI `judgment submit`,同一驗證器
4. spec 未列 pick 站(V2 cell `wave_summary.py`、`wave_impulse_screen` fallback)**一併 judgment-aware**:有 active judgment 用 accepted[preferred],無則回現行表現層預設

## Phase 進度

| Phase | 內容 | 狀態 |
|---|---|---|
| 0 | spec 落地(m3Spec 兩檔 + 本檔 + INDEX) | ✅ |
| 1 | M0:Ch6 閘接回 ladder + Running b>a+c(neely 1.2.0) | ✅ |
| 2 | E1 assumptions/E4 ambiguity/E2 robust + reverse_logic 退場(neely 1.3.0) | ⬜ |
| 3 | J1 wave_judgments 表 + anchor_key + dossier 讀路徑切換 | ⬜ |
| 4 | 判讀驗證器 + CLI/POST 寫路徑 + neely-judgment skill | ⬜ |
| 5 | S3 下游(track1/emitter/V2/wave_impulse)+ J2 diff + refresh hook + 前端 | ⬜ |
| 6 | 收尾(CLAUDE.md 輪替 / runbook 定稿) | ⬜ |

## Phase 1 紀錄(M0,neely 1.2.0)

落地面(全對齊 `m3Spec/neely_ch6_gate_running_fix.md`,含六條單元驗收 test):
`post_validator/` 重寫為 `WaveView` + `Ch6Report` + `post_validate_window`(舊 `post_validate(&Scenario,..)` 刪除);
`try_ladder` 加 `classified` 參數,W5 族別閘後 per-kind 跑 Ch6,Stage 1 fail → kind 拒絕 +
`RuleRejection{Ch6_*_Stage1}`(同 `w5_rejection_records`,cap 64)+ 新 counter `ch6_rejected_kinds`
(**不**混入 `w5_rejected_windows`,Ch5/Ch6 歸因分開);Stage 1 pass 於 `make_parent` 的節點私有
report clone push `passed`(memo 共享 `Rc<ValidationReport>` 不可變,beam 鍵 2 自動反映);
`CompactionNode.ch6` → 凍結 `Scenario.ch6_status`(additive,ts-rs 重生 diff 僅
`+Ch6Status.ts` / `Scenario.ts` / `CompactionV2Diagnostics.ts`);`is_running_correction` 改
`a > 0 && b > a + c`(三組向量翻轉對齊);版本字面量 5 處收攏 `neely_core::VERSION = "1.2.0"`
(順修 `facts.rs` 兩處寫死 `"0.21.0"` 的過期 source_version)。

**與 M0 spec 字面的偏差(3 處,實作時證據拍板)**:

1. **post_pattern 切片 `≥` 取代 `>`**:monowave 偵測共享 pivot bar(前波 end_bar = 次波
   start_bar),spec 字面 `start_bar > window.last().end_bar` 會把「緊接形態終點的第一段
   反轉波」— 正是 Stage 1 的確認走勢 — 排除在外,已被市場確認的 Impulse 因此被誤拒
   (`shadow_round2` test 直接證明:確認回測波被跳過 → 超時 → round 2 聚合結構性不可能)。
   舊 Scenario 版同病但屬死碼。改 `≥` 才涵蓋完整 post-pattern bar 級路徑。
2. **`Ch6Report.rule_id` 為 `Option<RuleId>`**:Combination / RunningCorrection / Diagonal
   無 Stage 1 規則可引(spec 契約註解本身只列三族 Stage1 變體);引擎內部型別,不上 wire。
3. **Correction「wave-c 完全回測」時限以 bars 累計**:舊碼 `take(wave_c_dur)` 以 monowave
   個數當時限,與同函式內 line-breach 檢查的 bars 基準不一致;「不長於形成時間」語意取 bars。

行為變化(test 修正對齊):有 post-pattern 但未在時限內確認的視窗 kind 不再入 forest —
`impulse_chain` fixture 的 Zigzag[0..3](0-B 線未破)自 forest 消失,相關計數 test
(node_count/boundary/a10/dedup)同步改期望值。全市場 forest 收縮幅度屬 gate 觀測項
(spec:p50/p95/p99 預期下降,上升即紅燈)。

沙箱驗證:`cargo test --workspace` **666 passed / 0 failed**(基線 660;+4 ladder 驗收
+1 post_validator +1 running 向量);codegen Track A diff 僅 additive 三檔;前端
`svelte-check` 0 errors(順修 `power.test.ts` fixture 既有缺欄)+ `vitest` 145 passed。
