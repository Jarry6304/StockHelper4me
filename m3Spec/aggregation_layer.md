# Aggregation Layer 規格 r1

> **狀態**:r1(2026-05-14 立稿)
> **層級**:System Core(對齊 `cores_overview.md` §8.6)
> **路徑**:即時請求路徑(對 batch 計算路徑 M3 Cores 而言)
> **原則**:**並排呈現,不整合**(對齊 §九 / §十一)
> **依賴**:`facts` / `indicator_values` / `structural_snapshots` + Silver 主數據表
> **不依賴**:Rust core 模組(避免循環);純讀 PG layer

## 目錄

1. [本層定位](#一本層定位)
2. [介面 vs 實作](#二介面-vs-實作)
3. [資料源](#三資料源)
4. [as_of(date) API](#四as_ofdate-api)
5. [時間對齊規則](#五時間對齊規則)
6. [Look-ahead bias 防衛](#六look-ahead-bias-防衛)
7. [跨 stock 與 market-level 並排](#七跨-stock-與-market-level-並排)
8. [Output 結構](#八output-結構)
9. [典型 use case](#九典型-use-case)
10. [Cache 策略](#十cache-策略)
11. [實作分階段](#十一實作分階段)
12. [非範圍](#十二非範圍)

---

## 一、本層定位

對齊 `cores_overview.md`:
- §8.6:列為 **"System Core"**(`aggregation_layer`)
- §九:**「並排呈現,不整合」**
- §10.0:**「即時請求路徑核心」**
- §十一:跨 Core 訊號(如 TTM Squeeze)由本層並排呈現,使用者自行連結

**本層不做**:
- 跨 Core 訊號推論(如「布林收進 Keltner 內 = Squeeze」)
- 機率 / score 整合(如「foreign_holding 加 institutional 加總分」)
- 即時計算(讀 batch 已產出的 facts 表)

**本層只做**:
- 從 PG 三表 + Silver fwd 表撈 raw 數據
- 時間軸對齊(daily 主軸 + monthly/quarterly fold-forward)
- Look-ahead bias 防衛(`report_date` 後生效)
- 跨 stock + market-level 並排組裝

---

## 二、介面 vs 實作

### 2.1 漸進式設計

對齊 user 2026-05-14 決策:
- **Phase B-1**:本規格 r1(本檔)
- **Phase B-2**:Python lib MVP(`agg/` package)
- **Phase B-3**:Streamlit dashboard demo
- **未來(若需)**:FastAPI thin wrap 對外網站化

Python lib 永遠是基礎層;FastAPI / Streamlit / GUI 都是 thin wrapper。

### 2.2 對外 API surface

```python
# agg/query.py
from datetime import date
from dataclasses import dataclass

@dataclass
class AsOfSnapshot:
    stock_id: str
    as_of: date
    facts: list[dict]                    # 期間內 facts(已過 look-ahead 防衛)
    indicator_latest: dict[str, dict]    # 各 indicator 最新值
    structural: dict[str, dict]          # neely scenario_forest 等
    market: dict[str, list[dict]]        # market-level facts 並排
    metadata: dict                       # query 參數記錄

def as_of(
    db_url: str,
    stock_id: str,
    as_of: date,
    cores: list[str] | None = None,
    lookback_days: int = 90,
    include_market: bool = True,
    timeframes: list[str] | None = None,
) -> AsOfSnapshot: ...
```

`as_of()` 是核心入口;backtest 與即時查詢共用此介面,look-ahead bias 防衛集中一處。

---

## 三、資料源

### 3.1 三張 M3 表

| 表 | PK | 用途 | 主要欄位 |
|---|---|---|---|
| `indicator_values` | (stock_id, value_date, timeframe, source_core, params_hash) | 時序數值(MACD/RSI/KD/...) | `value` JSONB |
| `structural_snapshots` | (stock_id, snapshot_date, timeframe, core_name, params_hash) | 結構快照(neely Forest) | `snapshot` JSONB |
| `facts` | id BIGSERIAL + uq_facts_dedup | append-only 事件 Fact | `statement`, `metadata` JSONB |

### 3.2 Silver fwd 表(輔助)

當 use case 需要原始價量(`as_of` 顯示時間軸圖表時)讀:
- `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`

Aggregation 不寫入 fwd 表 — 只讀。

### 3.3 不讀的表

- Bronze raw 表(`price_daily` 等)— Silver 層責任,Aggregation 只看「業務層」
- `*_derived` Silver 衍生表 — 已被 cores 消費,Aggregation 不重複讀
- `_legacy_v2` — deprecated 路徑

---

## 四、`as_of(date)` API

### 4.1 為何強制 `as_of`

每次查詢都帶 `as_of: date` 參數,做兩件事:

1. **回測語意**:回到 2024-01-15 看那天能看到什麼,disable 2024-01-16 以後的 facts
2. **Look-ahead bias 防衛**:`financial_statement` 公告日 T+45 才能用,Aggregation 強制過濾

不允許「查最新值」隱含 as_of=today — 強制顯式參數降低錯誤。

### 4.2 預設 lookback

- daily facts:`lookback_days` 預設 90 天
- monthly fact(revenue):3 個月(3 個發布週期)
- quarterly fact(financial_statement):2 季
- structural_snapshots:當日的快照(無 lookback)
- indicator_values:最後 1 筆(每 source_core 一筆;value 內 series 已含 90 天時序)

### 4.3 cores 子集

`cores: list[str] | None` 指定只撈某些 cores 的 facts。`None` = 全部 23 cores。
對齊 PR-9b workflow toml dispatch 同款設計(可重用同一 toml 過濾)。

---

## 五、時間對齊規則

對齊 `chip_cores.md` §2.3 + `fundamental_cores.md` §2.3 + `environment_cores.md` §三:

### 5.1 主軸 = daily

`as_of: date` 永遠是日級別。所有非日級資料 fold-forward 對齊。

### 5.2 不同 timeframe 對齊規則

| 來源 timeframe | 對齊行為 |
|---|---|
| daily | 直接對齊(`fact_date == as_of` 或 `<= as_of`) |
| weekly | shareholder_core 等;取 `fact_date <= as_of` 最近一筆 |
| monthly | revenue_core / business_indicator_core;**`report_date <= as_of`** 後最近一筆 |
| quarterly | financial_statement_core;`fact_date + 45_days <= as_of` 後最近一筆 |

### 5.3 fold-forward 原則

非日級事件「發布日後生效」:
- 4 月公布 3 月營收:`as_of = 4/30` 才看得到 3 月 revenue fact
- Q1 公布 Q4 財報:`as_of = 5/15` 才看得到 Q4 financial fact(T+45)

**fact_date 是業務日,report_date 是發布日**;Aggregation 用 `report_date`(或 `publish_date`)過濾,不是 `fact_date`。

---

## 六、Look-ahead bias 防衛

### 6.1 防衛點集中

所有 fact 過濾都在 `agg/_lookahead.py` 一處:

```python
def is_visible_at(fact: dict, as_of: date) -> bool:
    metadata = fact.get("metadata", {})
    # monthly fact:看 report_date
    if "report_date" in metadata:
        return date.fromisoformat(metadata["report_date"]) <= as_of
    # quarterly financial_statement:fact_date + 45 天
    if fact["source_core"] == "financial_statement_core":
        publish = date.fromisoformat(fact["fact_date"]) + timedelta(days=45)
        return publish <= as_of
    # daily fact:直接看 fact_date
    return date.fromisoformat(fact["fact_date"]) <= as_of
```

### 6.2 各 core 的 report_date 來源

| Core | fact_date | report_date 在哪 |
|---|---|---|
| revenue_core | 該月最後交易日 | `metadata.report_date`(發布日,通常該月後 10 號) |
| business_indicator_core | 該月最後交易日 | `metadata.report_date`(發布日,通常該月後 27 號) |
| financial_statement_core | 該季最後交易日 | **無 report_date 欄,fact_date + 45 天 fallback** |
| 其他 daily fact | 當日 | 不適用(daily 無 look-ahead) |

### 6.3 已知限制

- `financial_statement_core` 沒記錄真實發布日,用 T+45 sliding window 過濾;若有公司提前 / 延遲公告,可能對齊偏差 ±15 天
- `report_date` 在 facts.metadata 內,需 JSONB query 過濾;perf 可接受,但寫法比一般 SQL 過濾繁瑣

---

## 七、跨 stock 與 market-level 並排

### 7.1 保留字慣例(對齊 `cores_overview.md` §6.2.1)

| 保留字 | 用途 | source_core 範例 |
|---|---|---|
| `_index_taiex_` | TAIEX 加權指數 | taiex_core, neely_core(指數本身) |
| `_index_us_market_` | 美股 SPY / VIX | us_market_core |
| `_index_business_` | 景氣指標 | business_indicator_core |
| `_market_` | 市場層級籌碼 | market_margin_core |
| `_global_` | 全球性指標 | exchange_rate_core, fear_greed_core |

### 7.2 並排組裝邏輯

```python
def fetch_market_facts(db, as_of: date, lookback_days: int = 90) -> dict:
    """
    回傳:
    {
        "_index_taiex_":     [facts...],
        "_index_us_market_": [facts...],
        "_global_":          [facts...],   # exchange_rate + fear_greed
        "_market_":          [facts...],
        "_index_business_":  [facts...],
    }
    """
    market_stock_ids = [
        "_index_taiex_", "_index_us_market_",
        "_global_", "_market_", "_index_business_",
    ]
    rows = db.fetch_facts(
        stock_ids=market_stock_ids,
        as_of=as_of,
        lookback_days=lookback_days,
    )
    return groupby(rows, key=lambda r: r["stock_id"])
```

UI 端決定如何呈現「個股 facts」+「市場 facts」並排(時間軸上下對映 / sidebar 顯示等)。

### 7.3 個股之間不做交叉

Aggregation 一次查詢只看單一個股 + 全市場(透過保留字)。

**不做**:把 2330 facts 跟 2317 facts 比對(配對策略由上層 UI 決定);個股之間數量爆量 1700 × 1700 個 pair 不可能 ad-hoc。

---

## 八、Output 結構

### 8.1 AsOfSnapshot dataclass

```python
@dataclass
class AsOfSnapshot:
    stock_id: str
    as_of: date

    # 事件層 — 期間內所有 facts(已過 look-ahead 防衛)
    # 排序:fact_date DESC, source_core ASC
    facts: list[FactRow]

    # 時序層 — 各 indicator core 最新一筆 indicator_values
    # key = source_core, value = {value_date, value: {...}}
    indicator_latest: dict[str, IndicatorRow]

    # 結構層 — neely scenario_forest 等;當日 snapshot_date == as_of 那筆
    # key = core_name, value = {snapshot: {...}, derived_from_core: ...}
    structural: dict[str, StructuralRow]

    # 市場並排 — 5 個保留字 stock_id 的 facts
    market: dict[str, list[FactRow]]

    # query 參數記錄
    metadata: QueryMetadata
```

### 8.2 serialize 友善

所有 dataclass 用 `@dataclass(slots=True)` + 提供 `to_dict()` / `to_json()`。
FastAPI 之後 wrap 直接 `pydantic` 化即可,不需重寫。

### 8.3 pandas DataFrame 友善

`AsOfSnapshot.facts_df() -> pd.DataFrame` 把 facts list flatten 成 DataFrame:
- 欄位:`fact_date / source_core / statement / kind`(從 metadata 抽)`/ ...`
- 配 Jupyter 用最自然

---

## 九、典型 use case

### 9.1 個股深度查詢

```python
snap = as_of(db_url, "2330", date(2026, 5, 1))
# 看 2330 在 2026-05-01 那天能看到什麼:
# - 期間 facts:GoldenCross / RsiOversold / LargeNetBuy / ...
# - 各 indicator 最新值:MACD line/signal/histogram
# - structural snapshot:neely Forest 主要 scenario
# - 市場並排:TAIEX 同期 facts,VIX 區間,景氣燈號
```

### 9.2 回測迭代

```python
for d in pd.date_range("2024-01-01", "2024-12-31", freq="W"):
    snap = as_of(db_url, "2330", d.date(), lookback_days=30)
    # 計算當週的 signal,模擬交易
```

### 9.3 跨 indicator combine(教學層;不在 Aggregation 內整合)

```python
snap = as_of(db_url, "2330", today, cores=["bollinger_core", "keltner_core"])
boll = snap.indicator_latest["bollinger_core"]["value"]
keltner = snap.indicator_latest.get("keltner_core")  # 若 P3 後加
# user 自己判斷是否 Squeeze
```

### 9.4 篩 stock list

```python
# 找出今天有 RsiOversold 的所有股票
candidate = find_facts_today(db_url, today, source_core="rsi_core", kind="RsiOversold")
# 對每個 candidate 再呼叫 as_of() 看深度
```

### 9.5 個股 + 大盤環境連動

```python
snap = as_of(db_url, "2330", today, include_market=True)
# UI 把 2330 facts 與 snap.market["_index_taiex_"] facts 並排顯示時間軸
```

---

## 十、Cache 策略

### 10.1 MVP:ad-hoc 不 cache

對齊 user 2026-05-14 決策:

- facts 表 ~4.4M rows;單股 90 天 lookback 約 ~500-2000 facts
- 走 `idx_facts_stock_date_desc` index,query ~50-100ms 可接受
- 寫 lib 時不引入 cache layer

### 10.2 後續若 perf 不夠

選項(留 follow-up):
- **PG materialized view**(per-stock-as-of 預算)— 複雜度高,動態 `as_of` 不適合
- **functools.lru_cache** Python 端 in-memory(短期 sessions cache)
- **Redis / Memcached**(若做網站對外)

不在 r1 範圍。

---

## 十一、實作分階段

### Phase B-2:Python lib(`agg/` package)

範圍:
- `agg/__init__.py`:expose `as_of`, `AsOfSnapshot`
- `agg/query.py`:`as_of()` 入口
- `agg/_lookahead.py`:`is_visible_at()` 防衛
- `agg/_market.py`:`fetch_market_facts()` 5 保留字組合
- `agg/_serialize.py`:dataclass + `to_dict()` / `to_json()` / DataFrame
- `agg/_db.py`:reuse `src/db.py:create_writer()` connection

接口套件層:加進 `pyproject.toml`:
```toml
[tool.setuptools.packages.find]
include = ["silver*", "bronze*", "agg*"]
```

測試:
- 5 個 use case 各一個 pytest
- mock PG 用合成 facts 資料
- 至少一個 integration test(對 dev DB)

### Phase B-3:Streamlit dashboard

範圍(MVP):
- 一個 page,選股(2330 預設)+ date picker
- 4 區塊並排:
  - **時間軸**(`price_daily_fwd` + facts 標註)
  - **Indicator 最新值表**
  - **Neely scenario forest 摘要**(主要 scenario power_rating)
  - **市場環境**(TAIEX / VIX 同期 facts)
- 走 `as_of()` 拉資料,本機 `streamlit run dashboard.py`

不做(留 follow-up):
- 多用戶 / auth / deploy
- 圖表互動細節(zoom / brush selection)
- 跨股對比(個股之間 pair)

### Phase B-4(未來):FastAPI thin wrap

需要時補(對外網站化前):
- `agg_api/main.py`:FastAPI app
- `/as_of/{stock_id}` endpoint thin wrap `agg.as_of()`
- Auth / rate limit / monitoring 屬「網站工程」非「資料工程」,獨立規格

---

## 十二、非範圍

對齊 `cores_overview.md` §11 / §13 / §十四:

| 主題 | 為何不在本層 | 替代處理 |
|---|---|---|
| 跨 Core 訊號(TTM Squeeze) | 違反零耦合 | 教學文件 / UI 層 |
| 機率 / score 整合 | Neely Core 已撤掉 confidence | 不引入 |
| 即時計算 indicator | M3 Cores batch 路徑責任 | 走 cores 重跑 |
| Bronze raw 表讀取 | Silver 層責任 | 不讀 |
| 個股 × 個股 cross | 1700² 量級 | UI 決定要不要做 |
| Push 即時通知 | 工程層責任 | 獨立 notification service |
| 寫入 facts | Aggregation 只讀 | M3 Cores `tw_cores run-all --write` |

---

## 附錄 A:參考資料

| 文件 | 段落 |
|---|---|
| `m3Spec/cores_overview.md` | §8.6 / §九 / §十一 / §10.0 / §6.2.1 保留字 |
| `m3Spec/chip_cores.md` | §2.3 batch 17:30 排程 / §六 跨 Chip 處理 |
| `m3Spec/fundamental_cores.md` | §2.3 fold-forward 規則 |
| `m3Spec/environment_cores.md` | §三 並排 / §八 business_indicator report_date |
| alembic `w2x3y4z5a6b7` | 三表 schema 落地 |
| `src/silver/orchestrator.py` | dirty queue pattern reference |

---

## 附錄 B:Version history

| Version | Date | Changes |
|---|---|---|
| r1 | 2026-05-14 | 立稿 — Phase B 設計討論 + 落地藍圖 |
