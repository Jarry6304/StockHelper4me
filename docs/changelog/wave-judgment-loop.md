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
| 2 | E1 assumptions/E4 ambiguity/E2 robust + reverse_logic 退場(neely 1.3.0) | ✅ |
| 3 | J1 wave_judgments 表 + anchor_key + dossier 讀路徑切換 | ✅ |
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

## Phase 2 紀錄(E1/E2/E4,neely 1.3.0)

- **E1**:新 `assumptions.rs` — 8 常數(REVERSAL_ATR 0.5 / NEUTRAL_ATR 1.0 / ±10% / ±4% /
  Exception 10% / SB 0.382 / touch 2% / POLYWAVE 3)升 `pub(crate)` 原地引用不重打數值
  (「不外部化常數」invariant 不變,僅回報);`assumption_hash` = sha256(排序 `name=value`)
  前 16 hex(`sha2` 新 dep;blake3 保留給 params_hash);`NeelyCoreOutput.assumptions` +
  `.assumption_hash` 進 snapshot 頂層
- **E4**:新 `live_edge.rs` — 候選 = `wave_tree.end` bar ≥ last−3 且 degree 最大,
  count = distinct `(pattern_tag, end)`(同 end 同型異子結構不重計);
  `NeelyCoreOutput.live_edge_ambiguity`(非 Option,無候選 = 全零);
  `reverse_logic/` 模組刪除,`reverse_logic_observation` 欄恆 `None` 標 deprecated 留一版
- **E2**:新 `monowave/robustness.rs` — `detect_monowaves_with_multiplier` 變體
  (原簽名 = 0.5 wrapper),`{0.3, 0.7}` 兩組只重跑偵測(分類不移動端點,較 spec
  字面「+ neutrality」再省一步,語意等價);`Scenario.robust` = wave_tree 頂層 children
  端點日期兩組皆存在(Neutral monowave 端點入集合 → 合成葉端點視同存在);
  **multiplier 刻意不進 NeelyEngineConfig**(params_hash 不可變 — 變了 = snapshot 另立
  row + `fetch_structural_latest` 不分 params_hash 讀取不確定);Stage 13/14 新 timing key
- **traditional**:首個版本常數 `VERSION = "3.0.0"`(prose 世代命名;舊 inventory
  "0.1.0" 為 skeleton 遺留)+ `TraditionalDiagnostics.engine_version` additive
  (traditional_snapshots 無 source_version 欄,由此欄承載;舊 row 讀取端容缺)

沙箱驗證:`cargo test --workspace` 666 passed / 0 failed;codegen diff 僅 additive
(`+Assumption/AssumptionSource/LiveEdgeAmbiguity` + Scenario.robust + NeelyCoreOutput 三欄);
`svelte-check` 0 / `vitest` 145 passed。

## Phase 3 紀錄(M1:J1 表 + anchor_key + dossier)

- **J1**:alembic `k7l8m9n0o1p2` — `wave_judgments` 全 DDL(§5)+ **repo 首個
  RAISE EXCEPTION trigger**(`trg_wave_judgments_append_only`,BEFORE UPDATE OR DELETE,
  訊息 + SQLSTATE P0001 為契約);schema_pg.sql 同步 + 剝掉過期「無 trigger」註記。
  「active」語意 = `status='active'` 且無子列(supersedes 鏈最新);
  沙箱不可測 trigger 本體 → runbook probe
- **anchor_key**(`src/fusion/judgment/anchor_key.py`):§6 日期樹鍵,golden test 凍結。
  兩處格式細節(實作拍板,屬 PIT 身分一部分):
  (1) **頭部標籤剝顯示尾碼** ` L{degree}{arrow}` — 使 standalone 判讀鍵與「同子樹
  作為更大候選 children」同鍵,§J2 判定 3(absorbed)的比對前提;
  (2) **children 串 > 2048 chars → `#<sha256 前 16>` 決定性收斂** — 大型聚合全遞迴鍵
  可達數十 KB(炸 payload 與 judgments 儲存);同一函式供 dossier/驗證/J2 →
  收斂後等值與子樹比對全數一致,淺樹(判讀常態)保持人可讀
- **dossier**(`src/fusion/judgment/dossier.py`;`mcp_server/_dossier.py` 薄轉接 —
  builder 落 fusion 因 web_api 不能 import mcp_server):§4 全鍵;
  live-edge 過濾以 monowave_series 端點 date↔bar 對映計算;候選排序
  `(degree desc, end desc, start asc)` 無分數鍵;`active_judgment` 為 **per-timeframe
  dict**(spec 例為 scalar;§5 表以 (stock, timeframe) 為鍵,faithful 讀法);
  payload 護欄:per-tf 候選 cap 12 + `wave_tree` 序列化深度 cap 2(更深
  `children_omitted` 計數;anchor_key 仍由完整樹算);payload 政策改釘
  verify_mcp_toolkit 的 soft 50KB / hard 1MB(舊 5K-token 釘屬已退役 compact 回應)
- **讀路徑切換**:`mcp_server/_forecast.py` **刪除**(picker 三處 + prob/key_levels/
  missing-wave 機械全退場;quality_caveat 邏輯遷入 dossier 改吃候選集);
  `neely_forecast` 回應 = dossier,`primary_scenario`/`scenario_count`/`scenario_staleness`
  三鍵刪除;`/stocks/{id}/waves` **additive** 加 `dossier` 段(raw 兩鍵不動,
  graceful degrade);`fetch_traditional_latest` 新增於 fusion/raw
- scripts:`verify_mcp_toolkit_v4_29._summary_note` + `verify_mcp_kalman_neely._check_neely`
  改讀 dossier 鍵(snapshot_ref 新鮮度 + 候選計數)
- tests:`test_toolkit_v2.py` 摘除 neely 段(~750 行:picker/degradation/prob 全屬
  舊契約);新 `test_judgment_anchor_key`(golden)/`test_judgment_dossier`(15)/
  `test_dossier`(tool-level + payload 雙釘);web `/waves` additive 斷言;
  **pytest 全套 1036 passed / 2 xfailed**(基線 1037;淨 -1 = 舊契約測試汰換)
