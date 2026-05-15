# Magic Formula Core 規格(神奇公式 — Greenblatt 2005)

> **版本**:r1(v3.4 上線時立稿,2026-05-15)
> **位置**:`rust_compute/cores/fundamental/magic_formula_core/`
> **上游 Silver**:`magic_formula_ranked_derived`(由 Python Silver builder
> `magic_formula_ranked.py` 跨股 cross-rank 後寫入)
> **優先級**:P2(對齊 fundamental cores group)

## 一、本文件範圍

定義 `magic_formula_core` 的 trait 實作、Params 預設值、Output 結構、
EventKind、計算策略,以及與其他 fundamental cores 的並排原則。

**不在本文件**:
- 跨股 ranking SQL 邏輯(屬 Silver builder,見 `src/silver/builders/magic_formula_ranked.py`)
- universe filter 規則(industry_category keyword 過濾,亦屬 Silver builder)
- MCP tool `magic_formula_screen` wrapper 細節(屬 `mcp_server/_magic_formula.py`)

## 二、定位

Magic Formula(Greenblatt 2005)是經典量化選股策略,根據兩個比率對 universe
跨股排名,挑選 combined_rank 最低的 top N 持有 1 年:

- **Earnings Yield**(EBIT / EV)— 衡量「便宜程度」
- **Return on Invested Capital**(EBIT / IC)— 衡量「賺錢效率」

Core 本身**只算 per-stock 的 is_top_30 transition**(進入 / 退出 Top30),
跨股 rank 已由 Silver builder 預先計算完成。對齊 cores_overview §四
「Core 層 per-stock 獨立」原則。

### 2.1 跨股工作放 Silver 的理由

Magic Formula rank 需要對 universe(排除金融 + 公用後 ~1400 檔)整體比較。
若放 Core 層會踩 cross-stock state,違反 §四 per-stock 獨立原則。

設計選擇(v3.4 user 拍版 2026-05-15):
- **Silver builder**(Python):跨股一句 SQL window function 算 ey_rank /
  roic_rank / combined_rank;每天 1 row per stock 寫進 `magic_formula_ranked_derived`
- **Rust core**:per-stock 讀 series,比相鄰兩日 `is_top_30` 變化 → 出
  EnteredTop30 / ExitedTop30 facts

對映既有 `valuation_core.market_value_weight` 的同款設計(Silver 算跨股聚合
分母,Core per-stock 算自身權重),架構一致。

## 三、上游 Silver 表

- 表:`magic_formula_ranked_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位(對齊 alembic `y4z5a6b7c8d9` + `src/schema_pg.sql`):
  - `ebit_ttm` `NUMERIC(20,2)` — 過去 4 季營業利益加總(TTM)
  - `market_cap` — close × total_issued
  - `total_debt` — 估:Total Liabilities(短/長期 debt 細分留 follow-up)
  - `cash` — 現金及約當現金
  - `enterprise_value` — market_cap + total_debt - cash
  - `invested_capital` — total_assets - cash(working-capital proxy)
  - `earnings_yield` `NUMERIC(10,6)` — ebit_ttm / enterprise_value
  - `roic` `NUMERIC(10,6)` — ebit_ttm / invested_capital
  - `ey_rank` `INTEGER` — 1..universe_size;NULL for excluded stocks
  - `roic_rank` `INTEGER`
  - `combined_rank` `INTEGER` — ey_rank + roic_rank
  - `universe_size` `INTEGER`
  - `is_top_30` `BOOLEAN` — combined_rank ≤ 30
  - `excluded_reason` `TEXT` — `financial / utility / no_ebit_data /
    no_balance_data / no_market_cap / negative_ebit_or_ev / NULL`
  - `detail` `JSONB`

- 載入器:`shared/fundamental_loader/load_magic_formula_series`
  - 提供 `MagicFormulaSeries { stock_id, points: Vec<MagicFormulaPoint> }`
  - lookback_days param(預設 365 in `tw_cores run-all` dispatch)

## 四、Params

```rust
pub struct MagicFormulaParams {
    pub timeframe: Timeframe,         // Daily(Silver 每天更新 rank)
    pub top_n: i32,                   // 預設 30
}
```

### 4.1 Reference

- top_n=30:Greenblatt, J. (2005). *The Little Book That Beats the Market*.
  Hoboken, NJ: Wiley. Ch.5-7(原版 rank 邏輯 + 持有 1 年)
- OOS 驗證:
  - Larkin, K. (2009). "Magic Formula investing — the long-term evidence."
    SSRN id=1330551(1988-2007 美股 OOS 仍有 alpha)
  - Persson & Selander (2009). Lund Univ. thesis(歐洲市場 valid)
- top_n 範圍 20-30 是 Greenblatt 原文推薦;30 為 user 拍版上限。

## 五、warmup_periods

```rust
fn warmup_periods(&self, _params: &MagicFormulaParams) -> usize { 0 }
```

理由:Silver builder 已維護 rank state,Rust core 只需相鄰兩日比較。

## 六、Output

```rust
pub struct MagicFormulaOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<MagicFormulaSeriesPoint>,
    pub events: Vec<MagicFormulaEvent>,
}

pub struct MagicFormulaSeriesPoint {
    pub date: NaiveDate,
    pub fact_date: NaiveDate,                 // = date(日頻;同 valuation_core 對齊)
    pub earnings_yield: Option<f64>,
    pub roic: Option<f64>,
    pub combined_rank: Option<i32>,
    pub universe_size: Option<i32>,
    pub is_top_30: bool,
    pub excluded_reason: Option<String>,
}

pub struct MagicFormulaEvent {
    pub date: NaiveDate,                      // = fact_date
    pub kind: MagicFormulaEventKind,
    pub metadata: serde_json::Value,
}

pub enum MagicFormulaEventKind {
    EnteredTop30,
    ExitedTop30,
}
```

### 6.1 EventKind 設計(user 拍版 2026-05-15)

只 2 個 transition EventKind,對齊「LLM 只要知道狀態變化」原則:

| EventKind | 條件 | 預估觸發率(per stock/yr) |
|---|---|---|
| `EnteredTop30` | 前一日 is_top_30=false → 今日 true | ~1-2 |
| `ExitedTop30`  | 前一日 is_top_30=true  → 今日 false | ~1-2 |

合計 ~2-4/yr/stock,對齊 v1.32 P2 acceptance ≤ 12/yr/stock 標準。

**不收**(對齊 user 拍版「只 Top30 transition」):
- ❌ Top100 watch list(避免 LLM 誤判 watch list = buy signal)
- ❌ EY / ROIC 個股 zone(EnteredEyHigh > 12%, EnteredRoicExceptional > 30%
  等)— 留 future PR
- ❌ Rank 變動 ≥ 10% 也算 fact(避免每股 ~20/yr 過量)

## 七、計算策略

### 7.1 Transition detection

```rust
for i in 1..series.len() {
    let prev = &series[i - 1];
    let cur  = &series[i];
    if !prev.is_top_30 && cur.is_top_30 {
        events.push(EnteredTop30 { ... });
    } else if prev.is_top_30 && !cur.is_top_30 {
        events.push(ExitedTop30 { ... });
    }
}
```

第 1 個 row 不產 phantom event(無前一日比對)。

### 7.2 Metadata 結構

```json
{
  "earnings_yield":  0.082,
  "roic":            0.31,
  "combined_rank":   157,
  "universe_size":   1432,
  "top_n":           30,
  "excluded_reason": null   // ExitedTop30 才可能有值
}
```

## 八、Fact 範例

| Fact statement | metadata |
|---|---|
| `EnteredTop30 on 2026-05-15` | `{ earnings_yield: 0.082, roic: 0.31, combined_rank: 157, universe_size: 1432, top_n: 30 }` |
| `ExitedTop30 on 2026-08-12` | `{ earnings_yield: 0.06, roic: 0.18, combined_rank: 458, top_n: 30, excluded_reason: null }` |

## 九、Universe filter(Silver builder 端)

對齊 Greenblatt 2005 §六 "Special Industries" — 排除金融保險 + 公用事業。

Silver builder 用 `stock_info_ref.industry_category` keyword match:
- **financial**:含「金融」/「保險」/「銀行」/「證券」/「壽險」keyword
- **utility**:含「電力」/「燃氣」/「自來水」keyword

注意:**「水」太籠統會誤排「水泥工業」**;v3.4 r1 設計用 `自來水` 而非 `水`。

excluded stocks 仍寫一 row 進 Silver,但 `rank` / `is_top_30` 為 NULL / FALSE,
`excluded_reason` 紀錄分類。Rust core 看到 `is_top_30=FALSE` 就不會觸發
EnteredTop30(無 phantom)。

## 十、不收錄的指標(留 future PR)

- ❌ NCAV / Graham Number(Buffett / Graham 另一系列,屬獨立 core)
- ❌ Piotroski F-Score(財務健康度 9 項目評分,獨立 fundamental core)
- ❌ Magnitude of rank change(大幅 rank 變動)— v3.4 r1 不收,facts 量級風險
- ❌ Sector-relative rank — Greenblatt 原版不分產業排名

對齊 cores_overview §四「不抽象」原則:Magic Formula 是獨立策略 core,
不混 NCAV / Piotroski 等其他基本面框架。

## 十一、Production 注意

- Silver builder 第一次跑會對 last 30 days × ~1400 universe × 1 row 寫進
  ~42K rows(對齊 daily granularity);後續 incremental +1700 rows/day。
- detail JSONB key 命名是 production-driven(對齊 financial_statement_core
  的 v1.30 A1 修法);若 user 的 Bronze 命名跟 builder fallback chain 不同,
  該股會被歸 `excluded_reason='no_ebit_data'`。診斷:
  ```sql
  SELECT excluded_reason, COUNT(*) FROM magic_formula_ranked_derived
  WHERE date = (SELECT MAX(date) FROM magic_formula_ranked_derived)
  GROUP BY 1;
  ```
- LLM 端走 MCP `magic_formula_screen(date, top_n=30)`,payload ~5 KB / ~1250 tokens。
