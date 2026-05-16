# structural_snapshots Schema Partition Observation

> **建立時間**:2026-05-16(v3.9 Task 3 純研究)
> **目的**:評估 `structural_snapshots` 表是否需要 PostgreSQL 17 partitioning
> **結論先講**:🟢 **目前無需 partition**;預估 5 年內仍遠低於需要 partition 的門檻

---

## 1. 目前 schema(alembic `w2x3y4z5a6b7`)

```sql
CREATE TABLE structural_snapshots (
    stock_id          TEXT NOT NULL,
    snapshot_date     DATE NOT NULL,
    timeframe         TEXT NOT NULL,
    core_name         TEXT NOT NULL,
    source_version    TEXT NOT NULL,
    params_hash       TEXT NOT NULL DEFAULT '',
    snapshot          JSONB NOT NULL,
    derived_from_core TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stock_id, snapshot_date, timeframe, core_name, params_hash)
);

CREATE INDEX idx_structural_snapshots_stock_date_desc
    ON structural_snapshots(stock_id, snapshot_date DESC);

CREATE INDEX idx_structural_snapshots_core
    ON structural_snapshots(core_name, snapshot_date DESC);
```

**未做 partition**(單一 unpartitioned heap)。

---

## 2. Production 規模 + 成長率

### Snapshot 數(寫端)

- **目前 production**(2026-05-14 落地):**1263 stocks × 4 core_names**
  - core_names = `neely_core` + 3 P2 pattern(`support_resistance_core` / `candlestick_pattern_core` / `trendline_core`)
- **每日寫入**:1263 × 4 = ~**5,052 rows/day**(全量 UPSERT)
- **每年寫入**:5052 × 252 trading days ≈ **1.27 M rows/year**
- **5 年累積上限**:約 **6.4 M rows**

### 比較大小參考

| 規模 | rows | 是否需要 partition |
|---|---|---|
| 100K | < 1 年 | ❌ 完全不需要 |
| 1 M | ~1 年 | ❌ 不需要 |
| **10 M** | ~8 年(當前路徑) | 🟡 觀察期 |
| 100 M | (大幅 expand cores 後)| ✅ 開始有 benefit |
| 1 B+ | — | ✅ 必須 |

當前路徑 5 年內最多 ~6.4M rows,**遠低於 partition 必要門檻**。

---

## 3. Query 模式分析

### 主查詢:`fetch_structural_latest`(`src/agg/_db.py:240-265`)

```sql
SELECT DISTINCT ON (core_name, timeframe)
       stock_id, snapshot_date, timeframe, core_name, source_version,
       snapshot, params_hash, derived_from_core
FROM structural_snapshots
WHERE stock_id = $1
  AND snapshot_date <= $2
  [AND core_name IN (...)]
ORDER BY core_name, timeframe, snapshot_date DESC;
```

**Index 命中**:`idx_structural_snapshots_stock_date_desc(stock_id, snapshot_date DESC)` ✅
完整 cover query 的 `WHERE stock_id = ?` + `snapshot_date <= ?` + ORDER BY DESC。

**返回行數**:per stock_id × ~4 core_names × 1 timeframe = **~4 rows / query**。

**預估 latency**(本地 PG 17,SSD):
- < 10K rows index seek + 4 row JSONB return ≈ **< 5 ms**
- 即使 1 year 後 1.27M rows,index seek 仍 ~5-10 ms

### 次查詢:health_check(`src/agg/query.py:340-360`)

`SELECT COUNT(*) FROM structural_snapshots` — full scan,但 health_check 不要求快。

### 寫入模式

每日 batch UPSERT(對齊 `writers.rs:60-89` `ON CONFLICT ... DO UPDATE`):
- PK 命中時 UPDATE source_version / snapshot / created_at
- ~5052 rows/day × ~10 KB JSONB avg ≈ **~50 MB/day raw**(尚未壓縮)
- 帶 `concurrency=32` 並行 INSERT 預估 < 1 分鐘完成

---

## 4. Partition 收益 vs 成本(假設要做時)

### 候選策略 A:RANGE BY snapshot_date(monthly partition)

```sql
CREATE TABLE structural_snapshots (...) PARTITION BY RANGE (snapshot_date);
CREATE TABLE structural_snapshots_2026_05 PARTITION OF structural_snapshots
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
-- ... 每月一個
```

**收益**:
- ✅ 老資料 DROP TABLE 快(對齊 retention policy 砍 > 5 年)
- ✅ 查詢只 scan 對應 partition(若 query 帶日期範圍)
- ⚠️ **當前主查詢 `WHERE stock_id = ? AND snapshot_date <= ?` 無下界**,partition pruning **不命中** — 查全部 partition

**成本**:
- ⚠️ alembic migration 複雜(`ATTACH PARTITION` 或重建表 + 資料搬移)
- ⚠️ partition 數量管理(自動建/砍每月 partition)
- ⚠️ 每查詢多一層 partition 路由 overhead

**結論**:🔴 **不推薦**(query 模式不對,partition pruning miss)

### 候選策略 B:LIST BY core_name(4 partitions)

```sql
CREATE TABLE structural_snapshots (...) PARTITION BY LIST (core_name);
CREATE TABLE structural_snapshots_neely PARTITION OF structural_snapshots
    FOR VALUES IN ('neely_core');
CREATE TABLE structural_snapshots_pattern PARTITION OF structural_snapshots
    FOR VALUES IN ('support_resistance_core', 'candlestick_pattern_core', 'trendline_core');
```

**收益**:
- ✅ 砍 deprecated core 直接 `DROP TABLE`(對齊 cores 數量不變,場景罕見)
- ✅ 各 core 的儲存量 / vacuum 統計獨立
- ⚠️ 主查詢無 `core_name = ?` 過濾(query `WHERE stock_id = ?` 但 ORDER BY DISTINCT ON core_name),partition pruning **不命中**

**結論**:🔴 **不推薦**(query 模式不對 + 收益 < 成本)

### 候選策略 C:綜合 HASH BY stock_id

```sql
CREATE TABLE structural_snapshots (...) PARTITION BY HASH (stock_id);
CREATE TABLE structural_snapshots_p0 PARTITION OF structural_snapshots
    FOR VALUES WITH (modulus 8, remainder 0);
-- ... 8 partitions
```

**收益**:
- ✅ 主查詢 `WHERE stock_id = ?` 直接命中單一 partition(pruning 100%)
- ✅ 並行寫入時不同 partition 不互鎖
- ✅ 線性擴充(增加 partition 數量降低單 partition row count)

**成本**:
- ⚠️ alembic migration 複雜度同 A/B
- ⚠️ partition 數量需 stable(改 modulus 要重建)
- ⚠️ 整表 vacuum 需 per-partition trigger

**結論**:🟡 **未來 expand 至 10+ M rows 後可考慮**(目前 1.27M 不值得)

---

## 5. 預警閾值(觸發評估 partition 的條件)

當下列任一成立,**重啟 partition 評估**:

| 訊號 | 閾值 |
|---|---|
| `fetch_structural_latest` p95 latency | > **100 ms** |
| structural_snapshots total rows | > **10 M** |
| 寫入時 daily batch wall time | > **5 分鐘** |
| vacuum 對 production query 阻塞 | 觀察到顯著 lag |
| disk 使用 | > **50 GB** for structural_snapshots(含 JSONB + indexes)|

**監控建議**:在 `agg.health_check()` 加 `pg_stat_user_tables` 查 row count + `pg_relation_size`(留 follow-up,非本研究範圍)。

---

## 6. 短期行動(本 task 不動程式碼)

✅ **本 task = 純研究結論;不動 alembic / 不動 code**

- 結論文件化(本 doc)+ partition trigger 閾值落地參考
- 未來若 production verify 命中閾值,再開 PR(預估 ~半天 + 資料搬移時段)

---

## 7. References

- alembic `w2x3y4z5a6b7_m3_cores_three_tables.py`(CREATE TABLE 原檔)
- `src/agg/_db.py:240-265`(`fetch_structural_latest` 主 query)
- `rust_compute/cores/system/tw_cores/src/writers.rs:67-89`(UPSERT 寫入)
- PostgreSQL 17 [`Partitioning`](https://www.postgresql.org/docs/17/ddl-partitioning.html) 官方文件
- CLAUDE.md v1.35 + v3.5 production state:1263 stocks × 4 cores ≈ 5K rows/day
