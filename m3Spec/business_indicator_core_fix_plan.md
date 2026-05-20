# business_indicator_core empty-series 修復 plan

> **狀態**:✅ code 層修復(2026-05-20)— 候選根因 3(monitoring_color 字串不匹配)
>            已修;production verify 待 user 本機跑(無 DB 的 session 無法跑 §三 SQL)。
> **發現於**:2026-05-20 Fusion Layer P0-P2 production verify
> **嚴重度**:🟡 中 — `market_dashboard` 7 個 component 缺 1;非 crash,graceful 降級

## 〇、已修(2026-05-20)

候選根因 **3(monitoring_color 字串不匹配)** 為從 code + schema 文件即可確認的真實
缺陷,已修:

- **Schema 契約**:`src/schema_pg.sql:213` + `m2Spec/layered_schema_post_refactor.md
  §3.3` 兩處獨立文件都記 `business_indicator_tw.monitoring_color` 存
  `R / YR / G / YB / B`。
- **Bug**:`business_indicator_core::MonitoringColor::from_label` 原本只收
  `blue / yellow_blue / green / yellow_red / red` 英文全名。`field_mapper` 對此
  dataset 不做值轉換(只 rename leading/coincident/lagging),Bronze→Silver 原值
  直通。故若 Bronze 存的是文件契約的 `R/YR/G/YB/B`,`from_label` 每點回 `None`,
  `compute` 的 `filter_map` 把整批 series 丟光 → 空 series。
- **修法**(`rust_compute/cores/environment/business_indicator_core/src/lib.rs`):
  `from_label` 改為同時接受縮寫 `B/YB/G/YR/R`(schema 契約)、英文全名(既有,
  向下相容)、國發會中文燈號 `藍/黃藍/綠/黃紅/紅`(可帶「燈」字尾);英文 / 縮寫
  大小寫不敏感 + 前後空白容忍。+2 unit test(`monitoring_color_from_label_abbrev_and_chinese`
  / `compute_accepts_abbreviated_monitoring_color`)。
- 附帶修 `environment_loader/src/lib.rs` `BusinessIndicatorRaw.monitoring_color`
  誤導性註解(原寫只收英文全名)。

**仍待 user 本機 production verify**(下方 §三 SQL + §五):本 session 環境無 DB,
無法跑診斷確認 Bronze 實際字串值,也無法排除候選 1 / 2。若 verify 後 series 仍空,
依 §三 SQL 結果走 §四(候選 1 stock_id / 候選 2 date 太舊 / 某中間欄全 null)。

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
