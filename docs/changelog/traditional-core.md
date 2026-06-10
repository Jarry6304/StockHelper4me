# Traditional Core v2 / v3 歷程(Frost & Prechter 波浪 vertical)

> 自 CLAUDE.md 機械搬移(2026-06-10 P0-1 拆分),內文未重寫;版本索引見 [INDEX.md](INDEX.md)。

## Traditional Core v2 — Phase 1:獨立引擎 + 產品閘(2026-06-02)

User 直送 Traditional Core v2 spec(與 Neely **完全解耦、並排不整合**的傳統派 Frost & Prechter
EWP 波浪引擎)。規則層 `m3Spec/traditional_rules.md`(R1–R13 + 9 級 Degree + §A–D)。Phase 1 落地
**引擎 + 產品閘 harness**,過閘(forest 可讀性)再做 Phase 2 plumbing。分支 `claude/gracious-mendel-QeG15`。

### 鎖定設計(對齊 v2,推翻 r1「共用 WaveCore + structural_snapshots」)
- 新 crate `rust_compute/cores/wave/traditional_core/`(workspace 39 → **40 crates**)
- entry = **純函式** `traditional_core::run(series, config)`,**不 impl `WaveCore`**(不走 trait dispatch)
- **不 dep `neely_core` / `ohlcv_loader`**(後者 re-export neely `OhlcvSeries` 會耦合)→ 自帶
  `loader.rs` 直讀 Silver `price_*_fwd`;`cargo tree -p traditional_core | grep neely` = **空** ✅
- 僅 reuse `fact_schema::Timeframe` + `core_registry`(平台 util,皆無 neely dep,非 wave 型別)
- forest **不選 primary**;無 confidence/composite_score;`preference_score = guidelines + qualifiers`

### 8-stage pipeline(對齊 v2 `references/engine.md`)
pivot(ATR×k)→ candidates(+形態假設 HypoKind)→ 硬 Validator → classifier → guidelines → fibonacci → triggers → degree+forest。
- **硬規則(淘汰,pivot-level 可評估)**:R1 / R3 / R4(Impulse+Diagonal)、R5(僅 Impulse)、R9(僅 Diagonal)。
- **Deferred(不淘汰,v1 honest scoping)**:R2 `[待查證]`;**R6/R7/R8/R11 子浪細分需遞迴子浪分解 = v2 深度**
  ← 正是產品閘要揭露的引擎深度問題之一。
- R5/R9 分流(關鍵):浪 4 重疊浪 1 → Impulse 被 R5 淘汰、**同幾何 Diagonal 合法**(對齊 R8 rev2 擴張引導對角)。
- Fib 永不淘汰(僅計指引 + 出 `expected_fib_zones`,從正統終點量起)。

### 產品閘 harness(`tw_cores` 子命令,不寫 DB)
```powershell
git pull
cd rust_compute && cargo build -p tw_cores && cd ..
.\rust_compute\target\debug\tw_cores.exe traditional-debug --stock-id 3363   # 或 release build
```
印 forest summary(pivots / candidates / pass-rej / forest size + overflow / 每 scenario
`structure_label` + degree + `preference_score` + invalidation + fib zones)。**user 本機對手標股
(3363 + 補充清單)目測 forest 是否 ≤ forest_max_size(200)且人能讀、pivot(`swing_atr_multiplier=3.0`)
是否合理 → 決定引擎是否做完 / 調 pivot。過閘才進 Phase 2。**

### 沙箱驗證
- `cargo test -p traditional_core` **14 passed / 0 failed / 0 warning**(R1/R3/R4/R5/R9 各 1 +
  R5-impulse-vs-diagonal 分流 + pivot/candidates/degree/config + run() smoke + insufficient + overflow)
- `cargo build -p tw_cores` 綠;`tw_cores list-cores` 顯示 `traditional_core v0.1.0 [Wave / P3]`
- `cargo tree -p traditional_core` 零 `neely_core` / `ohlcv_loader`(解耦不變式)

### Phase 2 — 全層落地(2026-06-02,user 拍版「直接繼續,爆了再說」跳過產品閘)
- **2a 儲存**:alembic `j6k7l8m9n0o1` 建 `traditional_snapshots`(**自有表,非** structural_snapshots;
  PK `(stock_id, timeframe, params_hash)`;`forest`/`diagnostics` JSONB;snapshot 即 read model)
  + `schema_pg.sql` mirror。alembic head `i5j6k7l8m9n0` → **`j6k7l8m9n0o1`**。
- **2b 編排**:tw_cores `dispatch_traditional()`(純函式 run() → `write_traditional_snapshot()`
  ON CONFLICT 覆寫)+ `run_stock_cores` multi-tf loop(daily→daily/weekly/monthly,filter
  `is_enabled("traditional_core")`)→ `run-all --write` 自動含 traditional(refresh chain 免改)。
- **2c API**:`/stocks/{id}/traditional/forest`(passthrough)+ `/stocks/{id}/waves` 邊緣組裝
  (`{neely, traditional}` 並排,字串拼接不 deserialize、無 consensus)。
- **2d MCP**:`traditional_wave_forest`(**14th tool**)+ `mcp_server/_traditional.py`(forest 摘要,
  top_scenarios 依 preference_score,不選 primary)。
- **2e Dashboard**:`dashboards/charts/traditional_wave.py`(複製 neely zigzag/fib render)+
  aggregation.py「🌲 Traditional Wave」tab(自有 fetch traditional_snapshots,picker by preference_score)。

沙箱驗證:`cargo build -p tw_cores` 綠;全 Python `py_compile` 綠;新 tests web_api +4 / mcp_server +2
(pytest/fastapi/fastmcp 沙箱未裝 → 隨 DB-bound 部分留 user 本機 / CI 跑)。

### user 本機 runbook(下個 session)
```powershell
git pull
alembic upgrade head        # i5j6k7l8m9n0 → j6k7l8m9n0o1(建 traditional_snapshots)
cd rust_compute; cargo build --release -p tw_cores; cd ..
.\rust_compute\target\release\tw_cores.exe run-all --write --stocks 3363   # 寫 traditional_snapshots
psql $env:DATABASE_URL -c "SELECT stock_id,timeframe,jsonb_array_length(forest->'scenario_forest') AS n FROM traditional_snapshots;"
# API:uvicorn web_api.app:app → curl 'localhost:8000/stocks/3363/traditional/forest?timeframe=daily'
#                              → curl 'localhost:8000/stocks/3363/waves?as_of=2026-05-30'
# MCP:traditional_wave_forest('3363')  /  Streamlit「🌲 Traditional Wave」tab
pytest tests/web_api/test_api.py tests/mcp_server/test_traditional.py
```

🟡 wall time:`run-all` 每股多跑 traditional × 3 timeframe(與 neely 同量級);若爆 → workflow toml
關 `traditional_core` 或調 forest_max_size/pivot。🟢 解耦:0 碰 structural_snapshots / neely facts;
0 既有 code 邏輯改。Rollback:`git revert` + `alembic downgrade -1`。

> **產品閘(spec 待議 #1)未跑**:user 選擇直接續做。forest 可讀性 / 大小(尤其 R6/R7/R8/R11
> 子浪細分仍 Deferred → 可能 forest 偏碎)待 production verify;爆了回頭調 pivot 或補遞迴分解。

### v3 — 忠於原書多度數 fractal 引擎(2026-06-02,user 拍版「做到完、忠於原書」)

User 推翻 v1「R6/R7/R8/R11 標 Deferred + 單股判 forest」框法:**子浪細分要真執行(忠於原書)、
驗收看多股分布(非單股)**。引擎核心重做為**由下而上逐度數 compaction**,子浪細分 = 建構約束。

- **核心**:degree-N 形態的 children mode 必須符合(Impulse 5-3-5-3-5 / Zigzag 5-3-5 / Ending 對角全3…)。
  遞迴在資料解析度觸底(monowave 就是線,degree 0→1 子浪不可見 → R6 進 deferred;degree≥2 → **HARD 淘汰**)。
- **L9 骨幹**:actionary ≠ 永遠是 5(Ending 對角浪 1/3/5 雖 actionary 卻細分為 3)。
- **新模組**(M1 `75513b9` + M2):monowave / mode(L9 表)/ node(EngineNode)/ rules(R1/R3/R4/R5/R9 幾何)/
  patterns/{impulse,diagonal,zigzag,flat,triangle,**combination**} / compaction(round+beam+dedup+degree ceiling)/ scenario。
  `lib.rs::run` 重接 monowave→compaction→scenario;**output.rs wire contract 0 改**(WaveNode.children 已遞迴)
  → Phase 2 plumbing(table/API/MCP/dashboard)全沿用。M2 combination = Double/Triple Three + Double/Triple Zigzag + R12。
- **沙箱驗證**:`cargo test -p traditional_core` **29 passed**(headline:degree≥2 子浪模式錯/浪3 非 Impulse → 硬淘汰;
  degree-0 → R6 deferred;R12 三角僅最終組件);`cargo build -p tw_cores` 綠;`cargo tree` 零 neely。
- **⚠️ 待 user P0-Gate 校準**:monowave 不過濾(faithful)→ 長序列 base 多 → compaction beam/clone 成本高。
  production `run-all` 前/後看 forest p50/p95/max + elapsed_ms,**`monowave_epsilon` 調大**(去雜訊)/ `round_beam_size`
  / `max_degree_levels` 調控(對齊 neely P0-Gate 1264-stock 慣例)。forest 仍不選 primary、cap 只當安全網。
  v1 模組現況(2026-06 audit 覆核):`pivot.rs` / `validator/` / `classifier/` 已從 disk 刪除;
  `candidates` / `guidelines` / `fibonacci` / `triggers` / `degree` / `scenario` 已被 v3 `scenario::assemble`
  吸收、全 reachable(`lib.rs::run` Stage 5-8),**非死碼**,無待清 cleanup。

---

