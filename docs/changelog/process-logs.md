# Process logs — verify chain / 流水線 / backlog triage

> 自 CLAUDE.md 機械搬移(2026-06-10 P0-1 拆分),內文未重寫;版本索引見 [INDEX.md](INDEX.md)。

## 2026-05-18 整日 verify chain(快速入口)

今日 4 commit 全部 push 上 `claude/continue-previous-work-xdKrl`:
- **v3.29** `4184d04` risk_alert `_parse_severity` 加 `處置 / 注意` broad pattern
- **v3.30** `7f8d877` Kalman series-last-entry path fix + render tools 暫隱藏
- **v3.31** `7b2eb98` MCP toolkit 9 → 4 consolidation(stock_snapshot)+ Kalman/Neely verify pipeline
- **v3.32** `a365240` 10 new cross_cores builders + 4 MCP toolkit screens

完整 verify chain 走 6 phase。**Phase A SQL diagnostic 是 blocking**(v3.32 F-Score
+ industry_adj_gp + dividend_yield 都需先確認 Bronze 資料);若 Phase A 全綠才走
Phase B-F。詳見下方 §「下班後 verify 流水線」段(可從 git pull 直接拉這個版本看)。

---

## 待辦 backlog + schema drift 修(2026-06-04)

> forecast 校準批次優化(我 2026-06-03 的 conformalize/settle 批次化)與 main v4.36
> 「DB sync 全面並行化」**撞同一個 `conformalize_batch`**;user 拍版**取 main 並行版、
> 捨我的批次版**(merge 時 forecast code 全回 main)。本分支只留:① schema fresh-init
> drift 修 ② 下列 backlog。

**schema 修(已落地本分支)**:`wave_impulse_screen_derived`(table + index,v4.26
migration `g3h4i5j6k7l8` 建)原 `schema_pg.sql` 漏(連 main 都缺)→ 補回 verbatim
DDL,令純檔案 fresh-init(`psql -f schema_pg.sql`)不再漏表。alembic head `j6k7l8m9n0o1`。

### 待辦 backlog(2026-06-03 拍版)

**① API — 待做**:對外 API 擴充列為待做(等 user 給範圍/端點規格)。現況 `src/web_api/`
(v4.32 唯讀 FastAPI passthrough)已有 `neely/forest` / `levels` / `resonance` /
`snapshot/{core}` / `market/climate` / `ohlc` / `kalman/series` / `screens/{toolkit}` +
PR #123 加的 `traditional/forest` / `waves`。動工前 user 給 scope。

**② Traditional Wave 整批驗證 — 待驗**(DB-bound,沙箱跑不了;下方 runbook user 本機跑):

> **⚠️ P0-Gate 校準(2026-06-04 production 揭露)**:`monowave_epsilon` 預設 0.0
> (不過濾)→ 單股 traditional-debug 實測 **135s**(neely ~0.5s,慢 ~270×)→ 全市場
> run-all 連線池餓死(`pool timed out` / `LIMIT 51` 查詢 2344s)。修法:**預設改 0.03**
> (3% 反轉雜訊門檻,把 base 砍到 neely 量級)+ 4 旋鈕 env 覆寫(免重編 sweep):
> `TRAD_MONOWAVE_EPSILON` / `TRAD_ROUND_BEAM_SIZE` / `TRAD_MAX_DEGREE_LEVELS` /
> `TRAD_FOREST_MAX_SIZE`。先單股 sweep 定 epsilon 再跑全市場(且 `--concurrency 6~8`)。

```powershell
git pull
alembic upgrade head        # → j6k7l8m9n0o1(traditional_snapshots 已建)
cd rust_compute; cargo build --release -p tw_cores; cd ..

# 0. P0-Gate epsilon sweep(單股計時 + forest summary;目標 ~1-2s/股、forest ≤ 200 不過併)
Measure-Command { .\rust_compute\target\release\tw_cores.exe traditional-debug --stock-id 3363 }   # 預設 0.03
$env:TRAD_MONOWAVE_EPSILON='0.02'; .\rust_compute\target\release\tw_cores.exe traditional-debug --stock-id 3363
$env:TRAD_MONOWAVE_EPSILON='0.05'; .\rust_compute\target\release\tw_cores.exe traditional-debug --stock-id 3363
Remove-Item Env:\TRAD_MONOWAVE_EPSILON   # 定好值後清掉(或 set 成選定值再跑 run-all)

# 1. 整批寫入(全 universe × daily/weekly/monthly;run-all 自動含 traditional_core)
#    ⚠️ 降並行避免連線池餓死(pool = concurrency+4):
.\rust_compute\target\release\tw_cores.exe run-all --write --concurrency 8
#    ⚠️ 看 elapsed:每股多跑 traditional × 3 tf(與 neely 同量級);爆 → workflow toml
#       關 traditional_core 或調 forest_max_size / pivot

# 2. P0-Gate forest size 分布(對齊 neely 1264-stock 慣例)
psql $env:DATABASE_URL -c "
SELECT timeframe, COUNT(*) AS stocks,
       PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(forest->'scenario_forest')) AS p50,
       PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(forest->'scenario_forest')) AS p95,
       MAX(jsonb_array_length(forest->'scenario_forest')) AS max_n
  FROM traditional_snapshots GROUP BY timeframe ORDER BY timeframe;"
#    驗收:max ≤ forest_max_size(200 cap);p95 不過碎。爆/過碎 → monowave_epsilon 調大
#    (去雜訊)/ round_beam_size / max_degree_levels 調控(v3 §「待 user P0-Gate 校準」)

# 3. 整批覆蓋率(每 tf 是否每股都有 row)
psql $env:DATABASE_URL -c "
SELECT timeframe, COUNT(DISTINCT stock_id) AS n_stocks
  FROM traditional_snapshots GROUP BY timeframe ORDER BY timeframe;"

# 4. 端口 spot-check
#    API : uvicorn web_api.app:app
#          → curl 'localhost:8000/stocks/3363/traditional/forest?timeframe=daily'
#          → curl 'localhost:8000/stocks/3363/waves?as_of=<date>'
#    MCP : traditional_wave_forest('3363')
#    Streamlit:「🌲 Traditional Wave」tab(picker by preference_score)

# 5. 沙箱已綠、DB-bound 留本機
pytest tests/web_api/test_api.py tests/mcp_server/test_traditional.py
```

---

## Backlog triage — gov_bank / FastAPI 砍除 + 2A/2B 依賴更正(2026-05-27,planning only)

接 v4.27 PR #104 收尾後 user 對四項 backlog 候選逐一決議。**本段純 planning,無 commit / 無 code**。

### 四項 backlog 決議

| Item | 決議 | 理由 |
|---|---|---|
| **1a gov_bank_net Core 消費** | ❌ 砍 | 訊號品質先天低:公開即時(國安基金進場是頭版)、集中稀疏(僅少數權值股非零)、lagging contrarian floor 非領先 alpha。成本最重(新 crate + 2 EventKind + market gate + intervention lumpy calibration),CP 值最差。chip_loader load 了好幾版 0 core 消費本身即訊號 |
| **1c FastAPI thin wrap** | ❌ 砍 | 無 remote consumer;Streamlit in-process 直呼 agg、MCP 已服務 Claude Desktop。speculative plumbing |
| **2a wave_impulse calibrate** | ✅ 唯一可馬上動 | 不碰 forecast_log,只讀 structural_snapshots + price_daily_fwd。唯一 gate = structural_snapshots 歷史深度 |
| **2b dual_track_resonance 視覺層** | 🔁 重新歸類 | 非 calibration backlog,卡在 M8 fusion 全市場化 sprint 後面(見下) |

### 2A 程式確認(可動工,不必等 production)

- **PIT 價格重建穩**:`pit.ohlcv.asof_close_series(asof_t)`(Bronze raw + adjustment events 篩 ≤ asof_t,Rust S1 mirror)
- **structural_snapshots append-only** 逐日保留(PK 含 `snapshot_date` + idx `(stock_id, snapshot_date DESC)`)→ 歷史 forest 理論上都在
- **`_fetch_structural_snapshots` 現抓 latest**(`DISTINCT ON ... ORDER BY snapshot_date DESC`);改 as-of-T 只需加 `WHERE snapshot_date <= asof`,trivial
- **`backtest.py` 的 `forecast_fn(series, T, h, c)` 餵價格序列非 forest** → 兩條路:
  - **Path A**:用保留 snapshot 加 `WHERE`,replay screen 跑歷史 — 前提是 snapshot 已有足夠歷史深度
  - **Path B**:每歷史 T 跑 neely_core(rust subprocess) on asof series → forest → screen,今天就能跑、不必等,但需專用 harness 且重(~15ms × 股 × 日)

**下一步:跑 count query 定生死**
```sql
SELECT count(DISTINCT snapshot_date), min(snapshot_date), max(snapshot_date)
  FROM structural_snapshots WHERE core_name='neely_core';
```
有深度 → Path A;沒深度 → Path B

**待 calibrate 5 門檻**:
- `RECENT_DAYS=14`
- `RR_MIN=1.5`
- `MAX_UPSIDE_MULTIPLE=2.0`
- `CORRECTION_BOTTOM_BUFFER=0.03`
- `MIN_UPSIDE_PCT=0.03`

**注意**:1-2 週 production 只夠 **hygiene calibration**(count / RR 分布 / 無 RR>20 異常),**predictive calibration**(門檻是否真篩出到 target)需 forward outcome 數週~數月 或 PIT backtest。

### 2B 依賴更正(M8 衝突)

- M8 sprint 做的是 **Track 2 上游**(chip/macro/fundamental forecast cores + Bates-Granger fusion → forecast_log bands),但 production fusion 只跑 ~8 verify 檔(後擴 6 檔 apples-to-apples)。證據:CLAUDE.md 既載「3030 全 divergence,track2 缺 band,不在 M8 verify 8 stocks 內」
- 故 resonance 對全市場跑必然大量 divergence — **不是事件稀疏,是 fusion 沒對全市場跑**
- **logic 層其實已完成**:`resonance.py` / `_shared.py` / `track1` / `track2`,MCP tool #12 已註冊,fusion + mcp_server 測試齊備。缺的只有視覺層
- 2B 真正 blocker = **M8 fusion 全市場化**(獨立 sprint,需每檔 backtest 建 Bates-Granger 權重),再 structural_snapshots 深度

### ⚠ 更正前一輪錯誤判斷

前述「2A 與 2B 共用同一 data clock」錯誤。實際:
- **2A 僅依賴 structural_snapshots 深度**
- **2B 多一層 M8 fusion 全市場化 gate**

兩者**不同步**。

---

## 下班後 verify 流水線(2026-05-18 整日 4 commits)

對應今日 v3.29 → v3.32。完整 6 phase,**Phase A 是 blocking gate**(v3.32 SQL
diagnostic)。

### Phase 0:準備環境

```powershell
cd C:\Users\jarry\source\repos\StockHelper4me
(Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned)
& .\.venv\Scripts\Activate.ps1
$env:DATABASE_URL = "postgresql://twstock:twstock@localhost:5432/twstock"

git fetch origin claude/continue-previous-work-xdKrl
git checkout claude/continue-previous-work-xdKrl
git pull
git log --oneline -4
# 預期看到:a365240 v3.32 / 7b2eb98 v3.31 / 7f8d877 v3.30 / 4184d04 v3.29
```

### Phase A:Pre-impl SQL diagnostic(blocking — v3.32)

3 條 SQL 必跑。任一失敗就停下,先補資料再走 Phase B。

```powershell
# A-1:F-Score 9 條件需要的 detail key 是否都在 financial_statement_derived
psql $env:DATABASE_URL -c "
SELECT DISTINCT jsonb_object_keys(detail) AS key
  FROM financial_statement_derived
 WHERE stock_id = '2330' AND type IN ('income','balance','cashflow')
   AND date >= '2024-01-01'
 ORDER BY key
 LIMIT 60;
"
# 期待至少看到:本期淨利(淨損)/ 營業收入合計 / 營業成本合計 /
#               資產總額 / 流動資產 / 流動負債 / 長期借款 / 股本 /
#               營業活動之現金流量
# 若缺 → src/cross_cores/f_score.py 的 KEY_* fallback chain 需要再加新 key

# A-2:industry_category populated %(v3.32 industry_adj_gp 需 ≥ 80%)
psql $env:DATABASE_URL -c "
SELECT COUNT(*) AS total,
       COUNT(industry_category) AS non_null,
       ROUND((COUNT(industry_category)::numeric / COUNT(*) * 100), 2) AS pct
  FROM stock_info_ref
 WHERE market = 'TW' AND delisting_date IS NULL;
"
# 期待 pct ≥ 80;< 80 → industry_adj_gp 可上但 industry median 不穩,留意 narrative

# A-3:valuation_daily_derived.dividend_yield populated %(v3.32 dividend_yield 用)
psql $env:DATABASE_URL -c "
SELECT (COUNT(dividend_yield)::numeric / COUNT(*) * 100)::numeric(5,2) AS pct,
       COUNT(*) AS total,
       COUNT(dividend_yield) AS non_null
  FROM valuation_daily_derived
 WHERE date = (SELECT MAX(date) FROM valuation_daily_derived) AND market = 'TW';
"
# 期待 pct ≥ 90;< 90 → dividend_yield builder 會有較多 no_yield_data row
```

### Phase B:alembic + 跑 cross_cores phase 8 全市場(v3.32)

```powershell
# B-1:升級 schema
alembic upgrade head
# 期待 head: d9e0f1g2h3i4

# B-2:驗證 11 張新表存在
psql $env:DATABASE_URL -c "
SELECT tablename FROM pg_tables
 WHERE schemaname='public'
   AND (tablename LIKE '%_ranked_derived' OR tablename LIKE 'monthly_trigger%')
 ORDER BY tablename;
"
# 期待 11 張(magic_formula_ranked + v3.32 10 張)

# B-3:跑全市場 cross_cores phase 8(11 個 builder 一起跑)
python src/main.py cross_cores phase 8 --full-rebuild
# 期待 11 個 builder 全部 status=ok;rows_written 規模 ~1100-1300/builder
# (Layer 5 monthly_trigger 可能 < 100 因為 trigger 性質稀疏)

# 想單獨跑 1 個 builder:
# python src/main.py cross_cores phase 8 --builder f_score
```

### Phase C:資料驗證(v3.32 spot-check)

```powershell
# C-1:per builder row count
psql $env:DATABASE_URL -c "
SELECT 'magic_formula' AS b, COUNT(*) FROM magic_formula_ranked_derived WHERE date = (SELECT MAX(date) FROM magic_formula_ranked_derived)
UNION ALL SELECT 'persistent_momentum', COUNT(*) FROM persistent_momentum_ranked_derived WHERE date = (SELECT MAX(date) FROM persistent_momentum_ranked_derived)
UNION ALL SELECT 'revenue_momentum', COUNT(*) FROM revenue_momentum_ranked_derived WHERE date = (SELECT MAX(date) FROM revenue_momentum_ranked_derived)
UNION ALL SELECT 'institutional_concert', COUNT(*) FROM institutional_concert_ranked_derived WHERE date = (SELECT MAX(date) FROM institutional_concert_ranked_derived)
UNION ALL SELECT 'f_score', COUNT(*) FROM f_score_ranked_derived WHERE date = (SELECT MAX(date) FROM f_score_ranked_derived)
UNION ALL SELECT 'low_volatility', COUNT(*) FROM low_volatility_ranked_derived WHERE date = (SELECT MAX(date) FROM low_volatility_ranked_derived)
UNION ALL SELECT 'industry_adj_gp', COUNT(*) FROM industry_adj_gp_ranked_derived WHERE date = (SELECT MAX(date) FROM industry_adj_gp_ranked_derived)
UNION ALL SELECT 'long_term_low_vol', COUNT(*) FROM long_term_low_vol_ranked_derived WHERE date = (SELECT MAX(date) FROM long_term_low_vol_ranked_derived)
UNION ALL SELECT 'dividend_yield', COUNT(*) FROM dividend_yield_ranked_derived WHERE date = (SELECT MAX(date) FROM dividend_yield_ranked_derived)
UNION ALL SELECT 'mom_12_1', COUNT(*) FROM mom_12_1_ranked_derived WHERE date = (SELECT MAX(date) FROM mom_12_1_ranked_derived)
UNION ALL SELECT 'monthly_trigger', COUNT(*) FROM monthly_trigger_signals_derived WHERE date = (SELECT MAX(date) FROM monthly_trigger_signals_derived)
ORDER BY b;
"

# C-2:每 builder eligible (excluded_reason IS NULL) %
psql $env:DATABASE_URL -c "
SELECT 'f_score' AS b,
       COUNT(*) AS total,
       COUNT(*) FILTER (WHERE excluded_reason IS NULL) AS eligible,
       ROUND(COUNT(*) FILTER (WHERE excluded_reason IS NULL)::numeric / COUNT(*) * 100, 1) AS pct
  FROM f_score_ranked_derived WHERE date = (SELECT MAX(date) FROM f_score_ranked_derived)
UNION ALL SELECT 'dividend_yield',
       COUNT(*), COUNT(*) FILTER (WHERE excluded_reason IS NULL),
       ROUND(COUNT(*) FILTER (WHERE excluded_reason IS NULL)::numeric / COUNT(*) * 100, 1)
  FROM dividend_yield_ranked_derived WHERE date = (SELECT MAX(date) FROM dividend_yield_ranked_derived);
"
# F-Score:eligible % 反映實際過 F-Score ≥ 7 的股數
# Dividend Yield:eligible % 反映過 hard filter 的股數(yield ≥ 4% + 12M return > -20% + 5y ≥ 3y 配息)

# C-3:跨 toolkit 重疊 stock 觀察(分散度)
psql $env:DATABASE_URL -c "
WITH a AS (SELECT stock_id FROM persistent_momentum_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM persistent_momentum_ranked_derived)),
     b AS (SELECT stock_id FROM f_score_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM f_score_ranked_derived)),
     c AS (SELECT stock_id FROM long_term_low_vol_ranked_derived
            WHERE is_top_n AND date = (SELECT MAX(date) FROM long_term_low_vol_ranked_derived))
SELECT
  (SELECT COUNT(*) FROM a) AS a_count,
  (SELECT COUNT(*) FROM b) AS b_count,
  (SELECT COUNT(*) FROM c) AS c_count,
  (SELECT COUNT(*) FROM (SELECT * FROM a INTERSECT SELECT * FROM b) x) AS a_inter_b,
  (SELECT COUNT(*) FROM (SELECT * FROM a INTERSECT SELECT * FROM c) x) AS a_inter_c,
  (SELECT COUNT(*) FROM (SELECT * FROM b INTERSECT SELECT * FROM c) x) AS b_inter_c;
"
# 期待跨 toolkit 交集 ≤ 30%(若 > 50% 表 toolkit 高度重疊 → 分散效果差)
```

### Phase D:Kalman / Neely 既有 verify(v3.30 + v3.31)

```powershell
# D-1:Python wrapper 跑 production data
python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030
# 期待 Kalman + Neely 全 [OK]
#   Stock  Kalman    Neely     Notes
#   2330   [OK]      [OK]      K:smoothed=~2200 velocity=非0 | N:price=2265 waves=5
#   3030   [OK]      [OK]      ...

# D-2:SQL spot-check 直看 Rust 寫進 DB
psql $env:DATABASE_URL -v stock=2330 -f scripts/verify_mcp_kalman_neely.sql
# 期待:
#   Phase 1 (Kalman):series_len > 1500;latest_kalman_state 含 raw_close /
#                    smoothed_price / velocity / uncertainty / regime / date
#   Phase 2 (Neely): scenario_count > 0;w1_start / w1_end 揭露 anchor 日期
```

### Phase E:MCP server 8 個 tool 對話內測

```powershell
# Claude Desktop 重啟讓它 reconnect MCP server,然後對話內測:
python -m mcp_server  # 開 stdio

# v3.31 4 個既有 tools
#   "2330 Neely 預測"                   → neely_forecast
#   "2330 Kalman 趨勢"                  → kalman_trend
#   "今天 magic formula top 30"          → magic_formula_screen
#   "2330 完整快照"                     → stock_snapshot(6-in-1)

# v3.32 4 個新 cross-stock factor screens
#   "今天 monthly screen top 30"         → monthly_screen(Toolkit A)
#   "今天 quarterly screen"              → quarterly_screen(Toolkit B)
#   "今天 annual low risk screen"        → annual_low_risk_screen(Toolkit C)
#   "今天 monthly trigger scan"          → monthly_trigger_scan(Layer 5)

# v3.29 應該回到正常(不再「未分類」)
#   "3030 的 risk_alert 狀態"           → severity = "disposition"
#                                          severity_label = "處置股(分盤撮合)"
#                                          (走 stock_snapshot.risk_alert)
```

### Phase F:Python tests + 環境健康

```powershell
# F-1:既有 + v3.32 new tests 全綠
pytest tests/cross_cores/ tests/mcp_server/ tests/agg/ --ignore=tests/mcp_server/test_render_tools.py -v
# 期待 165 passed / 1 skipped(render 缺 fastmcp 是 pre-existing)

# F-2:Rust workspace test(若想完整跑;v3.32 0 Rust 改動,可選)
cd rust_compute
cargo test --release --workspace --no-fail-fast 2>&1 | tail -5
cd ..
# 期待 443 passed / 0 failed
```

### 退出碼判定

| Phase | 條件 | 行動 |
|---|---|---|
| A-1 | F-Score 9 key 全在 | ✅ 過 |
| A-1 | 缺 1-2 key | 🟡 修 KEY_* fallback chain 後繼續 |
| A-2 | industry_category pct ≥ 80% | ✅ 過 |
| A-2 | < 80% | 🟡 industry_adj_gp 仍可上但 narrative 標 caveat |
| A-3 | dividend_yield pct ≥ 90% | ✅ 過 |
| A-3 | < 90% | 🟡 dividend_yield builder 多 no_yield_data row |
| B-3 | 11 builder 全 status=ok | ✅ 過,進 C |
| B-3 | 任 builder 失敗 | ❌ 看 logs 找 root cause |
| C-3 | 跨 toolkit 交集 ≤ 30% | ✅ 分散 OK |
| C-3 | 交集 > 50% | 🟡 toolkit 設計重疊,留意 |
| D | Kalman + Neely 全 [OK] | ✅ |
| D | 任一 [FAIL] | 看提示 — v3.30 path fix / v3.28 regex / tw_cores 重算 |
| E | 8 個 tool 都有正確 response | ✅ |
| F-1 | 165 passed | ✅ |

---

