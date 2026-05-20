# business_indicator_core empty-series 修復 plan(下個 session)

> **狀態**:📋 待動工(下個 session)
> **發現於**:2026-05-20 Fusion Layer P0-P2 production verify
> **嚴重度**:🟡 中 — `market_dashboard` 7 個 component 缺 1;非 crash,graceful 降級

## 一、問題

Fusion Layer `market_dashboard` 回 `component_count: 6`(應為 7),`missing:
['business_indicator_core']`。

診斷已知:
- `indicator_values` 內 `business_indicator_core` 共 7 筆,`value->'series'` 全部
  `jsonb_array_length = 0`(空 series)。
- Silver `business_indicator_derived` 有 **87 rows**,`leading_indicator` **87/87 非 null**。
- 即:**資料在 Silver,但 core 產不出 series** — `business_indicator_core.compute`
  的 `filter_map` 把 87 個點全丟,或 loader 根本撈不到。

**這是 environment core / 資料 pipeline 的 bug,不是 Fusion Layer 的問題** —
`market_dashboard` 對空 series graceful 降級進 `missing` 是正確行為。

## 二、候選根因(3 個,需診斷確認)

1. **Loader stock_id filter**
   `environment_loader::load_business_indicator` SQL 寫死 `WHERE stock_id = '_market_'`。
   若 Silver 那 87 rows 的 `stock_id` ≠ `_market_` → loader 回 0 rows → 空 series。

2. **Loader date filter**
   同 SQL `AND date >= (CURRENT_DATE - $1::int)`。business_indicator 是**月頻**;
   若資料 stale(最後一筆 `date` 太舊)或傳入 `lookback_days` 太小 → 全被濾掉。

3. **monitoring_color 字串不匹配**
   `business_indicator_core::compute` 的 `filter_map` 末段
   `MonitoringColor::from_label(color_str)?` 只收
   `blue / yellow_blue / green / yellow_red / red`。若 Silver 寫的是別的字串
   (中文 / 大寫 / 數字代碼)→ 每個點都在這行被 `?` 丟掉。
   `leading_indicator` 87/87 非 null 卻整批沒了 → **高度懷疑是這條,或
   coincident / lagging / monitoring 某中間欄全 null**。

## 三、診斷步驟(下個 session 第一步,blocking)

```sql
-- (a) stock_id 與 date 範圍
SELECT DISTINCT stock_id FROM business_indicator_derived;
SELECT MIN(date), MAX(date), COUNT(*) FROM business_indicator_derived;

-- (b) 各欄 non-null count(leading 已知 87/87)
SELECT COUNT(coincident_indicator) AS coincident,
       COUNT(lagging_indicator)    AS lagging,
       COUNT(monitoring)           AS monitoring,
       COUNT(monitoring_color)     AS color
FROM business_indicator_derived;

-- (c) monitoring_color 實際值 vs from_label 接受集合
SELECT DISTINCT monitoring_color FROM business_indicator_derived;
```

## 四、修法(依診斷結果擇一)

| 診斷結果 | 修法 | 檔案 |
|---|---|---|
| (a) stock_id ≠ `_market_` | 修 loader filter,或修寫 Silver 的 builder 讓它寫 `_market_` | `environment_loader/src/lib.rs` 或 `src/silver/builders/` |
| (b) date 太舊 / lookback 太小 | 月頻不該用 `CURRENT_DATE - lookback`;改不設 date 下限或用足量 lookback | `environment_loader::load_business_indicator` |
| (c) monitoring_color 字串不符 | 對齊 `from_label` 接受集合(改 Silver builder 寫標準字串,或 `from_label` 加 alias) | `business_indicator_core/src/lib.rs` 或 Silver builder |
| 某中間欄全 null | Silver builder / Bronze 收集缺漏 — 屬資料 pipeline | `src/silver/builders/` + Bronze collector |

## 五、驗證

- 修完 `tw_cores run-all --write`(或只跑 environment cores)。
- 確認 `business_indicator_core events > 0` 且 `indicator_values.value->'series'` 非空。
- `market_dashboard('YYYY-MM-DD')` → `component_count: 7`,`missing` 不再含
  `business_indicator_core`。

## 六、不在範圍

- Fusion Layer `market_dashboard` 本身不用改 — graceful 降級正確。
- business_indicator 的 Bronze 收集若整段沒跑過,屬獨立的 collector 工作。
