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
| 4 | 判讀驗證器 + CLI/POST 寫路徑 + neely-judgment skill | ✅ |
| 5 | S3 下游(track1/emitter/V2/wave_impulse)+ J2 diff + refresh hook + 前端 | ✅ |
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

## Phase 4 紀錄(M3:驗證器 + 寫路徑 + skill)

- **驗證器**(`src/fusion/judgment/validate.py`,§2 階段 4 / §10 全項):候選集約束
  (拒絕附 `legal_keys`)、`single` ⇒ 恰 1 preferred 且 robust ≠ false(null = 未知放行)、
  `contested` ⇒ 1 preferred + ≥1 alternate、`no_fit` ⇒ accepted=[] + 非空 no_fit_reason
  (落 `rationale.no_fit_reason`;缺口表 = `confidence_class='no_fit'` 查詢,不另開表)、
  `as_of ≤ snapshot_date`(§11)、invalidation 禁空;PIT 錨定
  (snapshot_date/params_hash/engine_version/assumption_hash)提交時從 dossier 拷貝;
  accepted 候選 triggers 併入 `invalidation.recorded_triggers`(J2 用記錄值不重算)
- **CLI**:`python src/main.py judgment submit --file j.json [--judged-by]` /
  `judgment list --stocks` / `judgment diff [--stocks]`(diff 實作 Phase 5 接);
  submit 拒絕時印合法 anchor_key 清單,退碼 1
- **POST /judgments**(web API 首個寫端點,2026-08-30 拍版):同一套驗證器;
  422 帶 `{error, legal_anchor_keys}`;CORS methods 加 POST(app 門面文案同步
  「除 /judgments 外全唯讀」)
- **skill**:`.claude/skills/neely-judgment/`(**repo 首個 project skill**,
  jarry-skill-ref 格式)— SKILL.md(7 步 protocol mermaid + 禁止清單)+
  `references/qualitative-rules.md`(Emulation 7 型 / Missing Wave 最少資料點表 /
  Proportion / Neutrality Aspect-2 / Reverse Logic 人類語意 / Localized Change,
  全部引 m3Spec/neely_rules.md 行號)+ `dossier-reading.md` + `output-schema.json`
  (與驗證器以測試互鎖)
- tests:validate 逐規則 14 + POST 3 + schema 互鎖 2;**pytest 全套 1053 passed / 2 xfailed**

## Phase 5 紀錄(M4′:S3 下游 + J2 + refresh hook + 前端)

- **J2 錨定 diff**(`src/fusion/judgment/diff.py`,§6):per-anchor 判定 —
  命中 = intact 不寫列;未命中依序 (1) **記錄的** triggers
  (`invalidation.recorded_triggers`,提交時拷貝)對 `(as_of, now]` daily bars
  檢查 PriceBreakBelow(low ≤ level)/ PriceBreakAbove(high ≥ level)/
  TimeExceeds + 判讀 `time_limit_bar` ⇒ `invalidated` + diff_detail{rule,bar,price};
  (2) 嚴格子樹(結構遞迴,非字串包含)⇒ `absorbed` + parent_anchor_key;
  (3) 其餘 `vanished` — `source_version`+`assumption_hash` 與判讀時同 ⇒
  `cause=engine_regression` + `logger.error` 告警(引擎 bug 非市場),異 ⇒
  `engine_changed`;多錨最差優先(invalidated > vanished > absorbed > intact);
  狀態列拷貝內容欄 + supersedes_id,`judged_by` 沿用原列(J2 非判讀者);
  bars lazy fetch(全 intact 不撈價)
- **refresh 接線**:`_run_refresh` 於 tw_cores 後、forecast 前插 Step 7「J2 錨定
  diff」(同 `--skip-cores` guard,total_steps 8→9;J2 先降級失效判讀,emitter
  才不會對過期判讀發列);forecast step 文案 emit-neely → emit-judgment
- **S3 emitter**(`forecast/neely_emitter.py` 重寫):`emit_judgment_forecast` —
  active judgment 的 accepted[preferred] 以 anchor_key 對回最新 forest,
  發 `source_core='judgment'` 單列外包絡(calibrated=False / internal_only=True;
  params_hash=`judgment|id=…|tf=…|degree=…|by=…`);無判讀/no_fit/對不回 → 不發
  (`anchor_not_in_forest` 屬 J2 責任區,emitter 不代判);stale gate(7d)沿用;
  舊 `neely_fib` 值凍結唯讀(alembic `l8m9n0o1p2q3` 白名單 +'judgment',
  schema_pg.sql 同步);`forecast/__init__` 出口同步
- **track1 重寫**(§8 judgment-or-aggregate):picker 刪除;judgment 路徑 =
  accepted[preferred] 候選(pattern/fib zones/失效價/A-3 閘門,source="judgment");
  aggregate 路徑 = `up_share`(live-edge 候選中「有方向性前瞻」的等權比例 —
  `post_pattern_behavior` ∈ {Unconstrained, HintsAtPattern} 或 power=Neutral
  不入分母,方向取 power_rating sign;engine 的 post_behavior 本就由
  (pattern, power, ctx) 查表,方向資訊在 power)、`invalidation_band{min,max}`、
  `ambiguity_count`(E4);`up_share > 0.6` bullish / `< 0.4` bearish / 其餘
  **"undecided"**(wire 變更:direction 新字面值,消費端 else 分支安全);
  aggregate fib_lines = flat_fib_zones 聯集(無選取),invalidated 恆 False;
  `Track1View` additive 欄(source/judgment_id/up_share/invalidation_band/
  ambiguity_count),`contracts.py` + Track B codegen(fusion.ts diff additive)
- **pick 站 judgment-aware(拍版 4)**:`wave_summary.digest_from_docs` 先查
  active judgment(batch),命中 ⇒ cell 取 preferred + `judged: true`,無/對不回
  ⇒ 現行 pickDefaultScenario 鏡射排序;`WaveSummaryRow.judged` additive;
  `wave_impulse_screen` Step 5 fallback 由 canonical picker 改 active judgment
  preferred(無 ⇒ None,excluded_reason=`no_recent_correction_no_judgment`;
  Step 1-4 域內語意不動;active 判讀以 db.query 版 `_fetch_active_judgments`
  查,表缺/失敗視為無)
- **前端**:`/waves` 第三段 `dossier` 接線(`waves.ts` 型別;anchor→scenario
  對映走 dossier 候選的 anchor_key+id,前端**不重算錨定鍵**);V1 卡 —
  Overview/Detail **不再呼叫 pickDefaultScenario**(函式保留供 V2 鏡射對):
  有 active judgment ⇒ 焦點 accepted[preferred] + ⚓ 判讀 badge + ScenarioList
  accepted 高亮,無 ⇒ 候選平權**不預選**;「選取 → 錨定」= Detail 錨定鈕 →
  `POST /judgments`(`judgments.ts`:single/preferred,invalidation 預填候選
  InvalidateScenario triggers、rule_refs 預填 passed_rules;422 → 拒絕原因 +
  legal keys 顯示);V2 `WaveCell` ⚓ judged 記號;`client.ts` 加 `apiPost`;
  順修:`frontend` devDeps 補 `@types/node`(tsconfig `types:["node"]` 既有
  宣告,fresh clone 下 tsc 才可解析)
- tests:J2 diff 全矩陣 19(intact/invalidated×4 trigger 型/absorbed/vanished×2
  cause/多錨最差/狀態列拷貝)、emitter 18(重寫;picker-golden 檔隨 picker 刪除,
  stale-gate 覆蓋併入)、track1 70(judgment/aggregate/degrade 三路徑 + B3 一致性
  改走 judgment 路徑)、wave_summary 44(judgment-first 3);前端 vitest 146 /
  svelte-check 0 err / tsc 0 err
- 驗收:`grep -rn _pick_primary src mcp_server` = **0**;pytest tests/
  **1084 passed**(xfail 2 隨 picker-golden 檔汰換);cargo 未動(Phase 2 基線)
