# Schema 文件總索引 + Authority Order

> **本檔定位**:**所有 schema 相關文件的入口** — 不知道該查哪份時從這裡開始。
> **長度目標**:≤ 200 行,只列「去哪找 / 誰權威 / 怎麼改」,**不抄 schema 內容**。
> **alembic head**:`x3y4z5a6b7c8`(2026-05-11)/ **schema_version**:`3.2`
> **盤點時間**:2026-05-14

---

## 1. 你想做什麼?(決策樹)

| 想做的事 | 該查的文件 |
|---|---|
| **某張表的 PK / 欄位概要 / 上游 / 對應 spec 章節** | [`docs/schema_reference.md`](./schema_reference.md) §2-§7 |
| **某張表的欄位明細 / 語意 / dirty trigger / 來源 dataset** | [`m2Spec/layered_schema_post_refactor.md`](../m2Spec/layered_schema_post_refactor.md) §3-§4 |
| **某 core 讀哪張 Silver / 寫哪三表** | [`docs/cores_schema_map.md`](./cores_schema_map.md) §2-§4 |
| **某 core 的 Params / EventKind / Output 結構** | `m3Spec/{indicator,chip,fundamental,environment}_cores.md` 對應 core 章節 |
| **M3 三表的 PK / Unique / 寫入規約 / params_hash** | [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md) §3 / §六 / §7 |
| **某 collector.toml entry 寫進哪張 Bronze + code path** | [`docs/api_pipeline_reference.md`](./api_pipeline_reference.md) |
| **fresh DB 一鍵建表的 SQL** | [`src/schema_pg.sql`](../src/schema_pg.sql) |
| **schema 怎麼演進到現在這樣(歷史)** | [`alembic/versions/*.py`](../alembic/versions/) + `schema_reference.md` §8 |
| **怎麼加新 migration / 改 schema** | 本檔 §6 |
| **NEELY core 規則 / Stage 1-12 細節** | [`m3Spec/neely_core_architecture.md`](../m3Spec/neely_core_architecture.md) + [`m3Spec/neely_rules.md`](../m3Spec/neely_rules.md) |

---

## 2. 文件權威順序(authority order)

衝突時:**從上往下優先**,上層贏。所有衝突應視為 bug,需修文件對齊。

| 排序 | 來源 | 角色 | 衝突贏家原因 |
|---|---|---|---|
| **1**(最高)| `alembic_version.version_num`(DB 端)+ `alembic/versions/*.py` | **DDL 真實態** | 跑在 PG 上的就是這個 |
| 2 | `src/schema_pg.sql` | fresh DB 一鍵建表 | 應與 alembic head 同步;若不同步是 bug |
| 3 | `m2Spec/layered_schema_post_refactor.md` | Bronze + Silver 規範 | spec 為主,DDL 為輔的對比基準 |
| 4 | `m3Spec/cores_overview.md` + 各 cores spec | M3 規範 | M3 三表 + Core 契約 |
| 5 | `docs/schema_reference.md`(本群)| by-table 速查 | 引用 1-4 而生 |
| 6 | `docs/cores_schema_map.md` | by-core 反查 | 引用 1-4 而生 |
| 7 | `docs/api_pipeline_reference.md` | by-entry 索引 | 引用 1-4 而生 |
| 8 | `docs/schema_master.md`(本檔)| meta 索引 | 不收 schema 內容,只說「去哪找」 |

驗證權威同步:

```sql
-- 1 vs 2(alembic head vs schema_pg.sql)— 若 fresh DB 跑 schema_pg.sql 與跑 alembic upgrade head 結果不同就是 bug
SELECT version_num FROM alembic_version;   -- 應 = 'x3y4z5a6b7c8'

-- 1 vs 5(alembic head 寫進 schema_reference)— 文件 frontmatter
grep "x3y4z5a6b7c8" docs/schema_reference.md   -- 應命中
```

---

## 3. 文件對照表

| 文件路徑 | 行數量級 | 主軸 | 維護頻率 | 對齊 alembic id |
|---|---|---|---|---|
| `alembic/versions/*.py` | 24 個檔,每檔 ~50-200 行 | 每次 migration | 每次 schema 變更 | head `x3y4z5a6b7c8` |
| `src/schema_pg.sql` | ~1268 行 / 258 sections | 完整 DDL | 每次 schema 變更同步 | x3y4z5a6b7c8 |
| `m2Spec/layered_schema_post_refactor.md` | ~1198 行 | by layer 規範 | spec 改動時 | x3y4z5a6b7c8 |
| `m3Spec/cores_overview.md` | ~725 行 | M3 規範 | r4(2026-05-12)|  |
| `m3Spec/{子類}_cores.md` × 5 + neely × 2 | ~365-2657 行 | by core spec | r3-r4 | n/a(spec 不 bind alembic) |
| `docs/schema_reference.md` | ~280 行(v3.2 r1)| by table 速查 | 每次 schema 變更 + 大表新增 | x3y4z5a6b7c8 |
| `docs/cores_schema_map.md` | ~200 行(預計)| by core 反查 | 新增 core 時 + Silver 接點變 | n/a |
| `docs/api_pipeline_reference.md` | ~中 | by entry 索引 | collector.toml 改動 | u0v1w2x3y4z5(v1.26)|
| **`docs/schema_master.md`**(本檔)| ~200 行 | meta 索引 | 文件結構改 / 新文件 | x3y4z5a6b7c8 |

---

## 4. 版本鏈

`schema_version = '3.2'`(寫在 `schema_metadata.value WHERE key='schema_version'`)= 以下 PR 全部落地後的狀態:

| 階段 | 主要 PR | 內容 |
|---|---|---|
| baseline | `0da6e52171b1` | v2.0 PG 初版 schema(M1 era,27 表) |
| v1.7 | `a1b2c3d4e5f6` | `api_sync_progress.status` 5 種 CHECK 補完 |
| v3.2 升版 | `c2d3e4f5g6h7` | schema_version bump '2.0' → '3.2' |
| PR #17 | `d3e4f5g6h7i8` | Bronze events 砍 3 + fwd 加 4 + Rust schema 對齊 3.2 |
| PR #18 | `j9k0l1m2n3o4` | 5 張 v2.0 pivot/pack 表反推 → v3.2 raw Bronze |
| PR #18.5 | `l1m2n3o4p5q6` | 3 張 Bronze refetch schema(holding_shares_per_tw 等)|
| PR #19a | `k0l1m2n3o4p5` | 14 Silver `*_derived` + 3 fwd ALTER 加 dirty 欄 + 14 partial index |
| PR #19c | `o4p5q6r7s8t9` | day_trading_ratio column |
| PR #20 | `n3o4p5q6r7s8` | 15 Bronze → Silver dirty trigger(6 generic functions + 15 trigger) |
| PR #21-B | `p5q6r7s8t9u0` | 3 張新 Bronze:government_bank / total_margin / short_sale_securities |
| PR #R1 | `r7s8t9u0v1w2` | 補 source 欄至 3 張 `_tw` Bronze |
| PR #R2 | `s8t9u0v1w2x3` | 3 張 v2.0 表 RENAME `_legacy_v2` |
| PR #R3 | `t9u0v1w2x3y4` | 3 張 `_tw` Bronze 去後綴升格主名 |
| PR #R4 | `u0v1w2x3y4z5` | collector.toml v2.0 entry name 加 `_legacy` 後綴 |
| PR #22 fix | `v1w2x3y4z5a6` | `price_adjustment_events` dedup trigger(par_value + split 同日) |
| **M3 PR-7** | `w2x3y4z5a6b7` | **M3 三表落地**(indicator_values / structural_snapshots / facts)|
| **Hotfix A1** | **`x3y4z5a6b7c8`** | **目前 head** — financial_statement PK origin_name → type |
| (未落地)PR #R6 | TBD | DROP 3 張 `_legacy_v2` + 5 個 v2.0 `_legacy` entry |

---

## 5. 名詞統一表(跨文件用詞)

| 詞彙 | 涵義 | 出處 |
|---|---|---|
| **Bronze 層** | FinMind raw 收集 | layered §1.1 + cores_overview §4.4 |
| **B0-B6** | Bronze 細分階段(B0 calendar / B1 meta / B2 events / B3 raw price / B4 chip / B5 fundamental / B6 environment) | layered §1.1 |
| **Silver 層** | 衍生計算層 | layered §1.1 |
| **S1-S6** | Silver 細分階段(S1 adjustment Rust / S4 derived_chip / S5 derived_fundamental / S6 derived_environment;S2/S3 預留)| layered §1.1 |
| **`*_derived`** | Silver 衍生表命名慣例(主名後綴) | layered §4 |
| **fwd / 後復權** | `price_*_fwd` 3 張 Rust 算的後復權 K 線 | S1 +CLAUDE.md 關鍵架構決策 |
| **dirty / is_dirty** | Bronze 寫入後標記下游 Silver 需重算 | layered §6 + PR #20 |
| **核 / core** | M3 Cores 層的計算單元(22 個);走 `IndicatorCore` 或 `WaveCore` trait | cores_overview §3 |
| **fact** | 一次「事件型」輸出(寫入 `facts` 表,append-only,人類可讀 statement + 機器 metadata)| cores_overview §六 |
| **indicator_value** | 一次「時序型」輸出(寫入 `indicator_values` 表,每 core × stock × date × params_hash 一 row)| cores_overview §7.1 |
| **structural_snapshot** | 一次「結構快照」輸出(寫入 `structural_snapshots` 表,目前只 neely 用)| cores_overview §7.1 |
| **params_hash** | Output `Params` 用 blake3 + canonical JSON 算的 hash;`indicator_values` / `structural_snapshots` / `facts` 三表 unique constraint 用 | cores_overview §7.4 |
| **保留 stock_id** | Cores 端 Fact 對 market-level 資料用的 sentinel(`_global_` / `_market_` / `_index_taiex_` / `_index_tpex_` / `_index_us_market_` / `_index_business_`) | cores_overview §6.2.1 |
| **Ref 表** | Reference / 維度表(`schema_metadata` / `stock_info_ref` / `trading_date_ref`)| layered §五 |
| **staging 表** | 暫存表,跑完 post-process 清空(`_dividend_policy_staging`)| schema_reference.md §3 |
| **legacy 表** | 退場觀察中(`*_legacy_v2`),PR #R6 後 DROP | schema_reference.md §7 |

---

## 6. 改 schema 標準流程 checklist

任何 schema 改動(ADD / ALTER / DROP / RENAME)必須依序執行 + 同步更新:

```
[1] alembic/versions/<new_revision>.py     # 寫 idempotent migration
    - 用 IF EXISTS / IF NOT EXISTS / DO $$ ... END $$
    - down_revision 指向當前 head
    - revision id 用 12-char 隨機(對齊既有風格)

[2] src/schema_pg.sql                      # fresh DB DDL 同步
    - 確保 schema_pg.sql 跑出來的結果 = alembic upgrade head 跑出來的結果
    - 兩邊 column type / PK / index / trigger / constraint 都要對齊

[3] m2Spec/layered_schema_post_refactor.md # spec 更新
    §3 Bronze / §4 Silver:加新表或更新欄位 / dirty 流向圖

[4] m3Spec/cores_overview.md(M3 三表變動才需)
    §3 trait / §六 邊界 / §7 寫入規約

[5] docs/schema_reference.md(本群)        # by-table 速查
    §2 全表清單 + §3-§7 該層速查 + §8 PR 歷史時序版

[6] docs/cores_schema_map.md(Silver 輸入給 Cores 變動才需)
    §2 大表 + §4 反向索引同步

[7] docs/api_pipeline_reference.md(collector.toml entry 變動才需)
    對應 entry × Bronze 表索引

[8] schema_metadata.schema_version(major version bump 才需)
    bump '3.2' → '3.3';同步:
    - rust_bridge.py:EXPECTED_SCHEMA_VERSION
    - rust_compute/silver_s1_adjustment/src/main.rs assert
    - schema_pg.sql 寫入 INSERT 改新值
    - 本檔 §4 版本鏈加新 PR

[9] 測試
    - alembic upgrade head + alembic downgrade -1 + alembic upgrade head smoke
    - cargo build --workspace --release(若 Rust 端對應 schema)
    - 對應 Silver builder / Core compute 跑 smoke
```

**何時 bump schema_version**:破壞性變更(刪欄 / rename 表 / 改 PK)或結構性新增(新 layer / 新表 group)。純 ADD COLUMN 不 bump。

---

## 7. 已知過期 / 不要讀的文件

| 文件 | 狀態 | 原因 |
|---|---|---|
| `docs/collectors.md` | ⚠️ stale | v2.0 / 2026-04-30;以 Phase 1-6 為主軸,R3 升格 / M3 三表 / PR #21-B 全沒收 |
| `m2Spec/oldm2Spec/collector_schema_consolidated_spec_v3_2.md` | 已歸檔 | v3.2 r1 整合規格,被 `m2Spec/layered_schema_post_refactor.md` 取代 |
| `m2Spec/oldm2Spec/m2_neo_pipeline_spec_r{1,2,3}.md` | 已歸檔 | r1/r2/r3 設計探索,r4 已被 cores_overview.md 取代 |
| `m2/schema_m2_pg.sql` | 已歸檔 | M2 Aggregation Layer 舊設計 schema,M3 已不走此路 |
| 舊 `docs/schema_reference.md`(v2.0)| 已被覆寫 | 想看歷史:`git log -- docs/schema_reference.md` |

---

## 8. Quick links

| 想找 | 路徑 |
|---|---|
| 速查單一表 | [`docs/schema_reference.md`](./schema_reference.md) |
| 表規範 | [`m2Spec/layered_schema_post_refactor.md`](../m2Spec/layered_schema_post_refactor.md) |
| 核反查 | [`docs/cores_schema_map.md`](./cores_schema_map.md) |
| 核規範 | [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md) |
| 核 deep-dive | `m3Spec/{indicator,chip,fundamental,environment,neely}_cores*.md` |
| collector | [`docs/api_pipeline_reference.md`](./api_pipeline_reference.md) |
| DDL | [`src/schema_pg.sql`](../src/schema_pg.sql) |
| migration 歷史 | [`alembic/versions/`](../alembic/versions/) |
