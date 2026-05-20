# Fusion Layer 規格 v1.0

> **版本**:v1.0(🔒 LOCK,架構拍板)
> **狀態**:✅ **P0+P1+P2 已落地(2026-05-20)** — 分支 `claude/plan-stockhelper-api-kWh9F`
> / PR #91;落地紀錄見 `CLAUDE.md` §「Fusion Layer — API 規劃落地」。
> **日期**:2026-05-20
> **層級**:System Layer(取代並升級 `aggregation_layer`)
> **基準**:對齊 `api_roadmap_v1.md`、`cores_overview.md` v2.0 r5、`aggregation_layer.md` r4
> **實作位置**:`src/fusion/`(Python lib)
> **路徑**:即時請求路徑層(對 batch 計算路徑 M3 Cores 而言)
> **依賴**:PG 三表(`facts` / `indicator_values` / `structural_snapshots`)+ Silver `*_derived` 表
> **不依賴**:Rust core 模組(避免循環)

## 目錄

1. [本層定位](#一本層定位)
2. [為何需要 Fusion 層](#二為何需要-fusion-層)
3. [整體資料流](#三整體資料流)
4. [雙端口設計](#四雙端口設計)
5. [模組清單](#五模組清單)
6. [上游 Cores 必要改動](#六上游-cores-必要改動)
7. [Schema 改動](#七schema-改動)
8. [關鍵融合流程](#八關鍵融合流程)
9. [設計原則](#九設計原則)
10. [非範圍](#十非範圍)
11. [落地階段](#十一落地階段)
12. [與既有架構的關係](#十二與既有架構的關係)
13. [修訂歷史](#十三修訂歷史)

---

## 一、本層定位

**Fusion Layer = `aggregation_layer` 的繼任者**,不是疊在 agg 之上的新層。把 agg
拆成兩個端口共存:

- **Raw 端口**:沿用 `as_of()`,繼承「並排呈現,不整合」原則(對齊
  `cores_overview.md` §九)。
- **Integration 端口**:新增,允許跨 core 整合(但不引入新規則)。

LLM / MCP Tools / Dashboard / CLI **都從 Fusion 出口取資料**。

> **路徑命名**:模組路徑從 `src/agg/` rename 為 `src/fusion/`。`as_of()` 介面與
> 行為不變,只是收進 `fusion.raw.as_of`。

---

## 二、為何需要 Fusion 層

`api_roadmap_v1.md` 規劃 18 個 API,其中至少 7 個涉及**跨 core 整合**
(`stock_snapshot.technical_summary`、`key_levels`、`pattern_scan`、
`stop_loss_calc`、`market_dashboard`、`market_events`、`indicator_<category>`)。

`aggregation_layer.md` §九明文「並排呈現,不整合」,不能在原 agg 內做這些。
拍板:agg 升級為 fusion,雙端口共存。

---

## 三、整體資料流

LLM / MCP Tools / Dashboard / CLI → **Fusion Layer(唯一對外資料層)**:
- Raw 端口(`as_of`,並排不整合)
- Integration 端口(跨 core 整合,不引入新規則)

兩端口都讀 PG 三表(`facts` / `indicator_values` / `structural_snapshots`)+
Silver `*_derived`。M3 Cores(batch 計算路徑)寫這三表。

---

## 四、雙端口設計

### 4.1 Raw 端口(`fusion.raw`)

完全繼承 `aggregation_layer.md` r3 行為,不做任何整合:`as_of()` /
`as_of_with_ohlc()` / `find_facts_today()` / `health_check()`。

### 4.2 Integration 端口(`fusion.<module>`)

新增端口,專責整合。各模組各自獨立,**不繼承共同 base class**(避免過度抽象)。

| 介面 | 對應 roadmap API |
|---|---|
| `snapshot.stock_snapshot` | A.stock_snapshot(10-in-1) |
| `key_levels.key_levels` | B.key_levels |
| `pattern_scan.pattern_scan` | B.pattern_scan |
| `stop_loss.stop_loss` | B.stop_loss_calc |
| `market_dashboard.market_dashboard` | D.market_dashboard |
| `market_events.market_events` | D.market_events |
| `indicator_assembly.assemble_indicators` | E.indicator_*(5 工具共用) |

---

## 五、模組清單

```
src/fusion/
├── __init__.py
├── raw/                          # = 既有 src/agg/ rename
│   ├── __init__.py / _db.py / _lookahead.py / _market.py / _types.py / query.py
├── _shared.py                    # 共用 helper(severity 對映 / cluster / fact_to_event)
├── snapshot.py                   # A.stock_snapshot(10-in-1 組裝)
├── key_levels.py                 # B.key_levels
├── pattern_scan.py               # B.pattern_scan(with key_levels context)
├── stop_loss.py                  # B.stop_loss_calc
├── market_dashboard.py           # D.market_dashboard
├── market_events.py              # D.market_events
└── indicator_assembly.py         # E.indicator_*(series + events 跨表拼裝)
```

刻意省略:`_base.py` ABC、`orchestrator.py`、plugin/registry —— 模組互不呼叫
(§9 #8),共用走 `_shared.py`。

---

## 六、上游 Cores 必要改動

| # | Core | 改動 |
|---|---|---|
| 1 | `neely_core` | 加 stock-level `flat_fib_zones`(union all scenarios' `expected_fib_zones`)寫進 `NeelyCoreOutput` |
| 2 | `taiex_core` | EventKind 補:`Ma20SlopeFlip` / `Drawdown5pct` / `NewHigh52w` / `NewHighAll` |
| 3 | `fear_greed_core` | EventKind 補:`EnterPanic` / `Drop30In5d`(`EnteredExtremeGreed` 已存在,不重複) |
| 4 | `exchange_rate_core` | EventKind 補:`TwdStrengthenStreak`(`TwdBreak31` 由 `KeyLevelBreakdown` 涵蓋,不重複) |
| 5 | `market_margin_core` | EventKind 補:`Balance5dDrop3pct` |
| 6 | 所有 environment cores | `*Point` struct 加 `percentile_252: f64` |
| 7 | 所有 cores | `produce_facts()` 帶 `severity`(`Fact` struct 加 `severity` field) |

---

## 七、Schema 改動

### 7.1 `facts` 表加 `severity` column

```sql
ALTER TABLE facts ADD COLUMN severity SMALLINT DEFAULT 1 NOT NULL;
-- 1=info, 2=notable, 3=warning, 4=critical
CREATE INDEX idx_facts_severity_date ON facts (severity, fact_date DESC);
```

嚴重度由 cores 寫入時決定,Fusion 不做二次判斷(§9 設計原則 6)。
alembic head:`d9e0f1g2h3i4` → `e0f1g2h3i4j5`。

### 7.2 `neely` `flat_fib_zones`

`NeelyCoreOutput` 加 `flat_fib_zones` 欄,隨整個 output 序列化進
`structural_snapshots.snapshot` JSONB。

---

## 八、關鍵融合流程

- **`key_levels`**:讀 `support_resistance_core` / `trendline_core` facts +
  `neely` `flat_fib_zones` → 1% bucket cluster → 依 source 數(strength)排序。
- **`market_events`**:`facts WHERE source_core ∈ environment_cores AND
  severity >= severity_min` → map 為統一 Event schema。純 SQL filter。
- **`indicator_<category>`**:讀 `indicator_values`(series)+ `facts`(events)
  跨表拼裝。

---

## 九、設計原則

| # | 原則 |
|---|---|
| 1 | 不引入新規則(Integration 端口只整合 cores 既有輸出) |
| 2 | 不疊床架屋(模組間互不繼承,共用走 `_shared.py`) |
| 3 | 無 LLM(API 層純資料/規則) |
| 4 | as_of date 強制 |
| 5 | Look-ahead bias 防衛沿用 raw 端口機制 |
| 6 | severity 由 cores 決定,Fusion 只 filter |
| 7 | percentile 由 cores 寫入時計算 |
| 8 | 整合不可遞迴(integration 模組不互相呼叫,只讀 raw / 三表) |

---

## 十、非範圍

跨 stock cluster / 相關性 / sector rotation(屬 Cross-Stock Cores)、主觀標籤化、
即時 streaming、walk-forward backtest、LLM 推論。

---

## 十一、落地階段

> **✅ P0+P1+P2 全部落地完成(2026-05-20)** — 分支 `claude/plan-stockhelper-api-kWh9F`
> / PR #91。落地紀錄見 `CLAUDE.md` §「Fusion Layer — API 規劃落地」。

### P0 — 基礎升級 ✅

- [x] `src/agg/` rename 為 `src/fusion/raw/`,44 處 import 更新
- [x] `facts.severity` column + index(alembic `e0f1g2h3i4j5`)
- [x] cores `produce_facts()` 帶 severity — 採 Rust `Fact.severity` struct field,
      各 core 自己映射(對齊「不耦合不抽象」,非中央表)
- [x] `cores_overview.md` §8 對齊 + `aggregation_layer.md` r4

### P1 — 整合模組 ✅

- [x] `neely_core` `flat_fib_zones` 落地
- [x] Environment cores EventKind 補齊(taiex / fear_greed / exchange_rate / market_margin)
- [x] Environment cores Point struct 加 `percentile_252`(全 7 個)
- [x] Fusion 模組實作:snapshot / key_levels / pattern_scan / stop_loss /
      market_dashboard / market_events / indicator_assembly + `_shared`
- [x] MCP Tools 10 個新 thin wrapper(toolkit 8 → 18;roadmap §2.3 列 10 新,
      headline「9」為 off-by-one,已校正)

### P2 — 清理收尾 ✅

- [x] `magic_formula_core` — **查證後不搬**:Rust crate 是 live(`tw_cores`
      dispatch,讀 cross_cores 寫的 `magic_formula_ranked_derived` 表產 facts),
      非 dead code。原 spec「搬到 cross_cores」假設有誤,`cores_overview.md` §8.4 已更正。
- [x] `traditional_core` — 確認 vaporware(`cores/wave/` 僅 `neely_core`),
      從 `cores_overview.md` §8.1 / §9 撤下。
- [x] `tests/fusion/` 補完(30 個 mock-based 測試)。

### 已知 follow-up(下個 session)

- `business_indicator_core` series 空 → `market_dashboard` 6/7 component;非
  Fusion bug(graceful 降級正確),診斷 + 修法見
  `m3Spec/business_indicator_core_fix_plan.md`。

---

## 十二、與既有架構的關係

### 12.1 沿用

`as_of()` 介面與行為(進到 `fusion.raw.query.as_of`)、look-ahead bias 防衛、
PG 三表 + Silver、Cores §7.5「子類內 Output 結構同構」。

### 12.2 改動

- `cores_overview.md` §九「並排呈現,不整合」降級為僅適用 raw 端口。
- `aggregation_layer.md` 標記 **r4** — 內容改為「本層併入 `fusion.raw`,新 spec
  見 `fusion_layer.md`」。
- `api_roadmap_v1.md` §9.4「不引入新 Core」鬆綁:允許對既有 cores 加欄位 /
  補 EventKind,但不新增整個 Core。

### 12.3 不影響

Cores 計算 trait、Cross-Stock Cores orchestrator、MCP Tools 對外契約、
Silver / Bronze 層。

---

## 十三、修訂歷史

| 版本 | 日期 | 內容 |
|---|---|---|
| v1.0 | 2026-05-20 | 初稿,Fusion = agg 升級拍板,P0/P1/P2 落地階段定義,雙端口設計 LOCK |
| v1.0(落地)| 2026-05-20 | P0+P1+P2 全部落地;§11 標記完成;P2.1 magic_formula_core 查證為 live(不搬);severity 採 struct field 而非中央表 |
