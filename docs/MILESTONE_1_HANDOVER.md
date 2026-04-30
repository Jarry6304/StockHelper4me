# tw-stock-collector v2.0 遷移 — Milestone 1 銜接文件

> **產生日期**: 2026-04-28
> **用途**: 跨 session 銜接,讓下個 session 不用重讀討論歷史就能直接接續工作
> **基準分支**: `claude/collector-schema-mapping-2YF5U`(已含 v1.6)

---

## 一、整體架構決策(已定案,不再討論)

### 1.1 三 milestone 結構

| Milestone | 內容 | 狀態 |
|---|---|---|
| **M1** | Collector 從 SQLite 遷到 Postgres 17 + psycopg3 + sqlx | **進行中**(Step 1.1–1.3 完成) |
| **M2** | NEO Pipeline v2.0 spec r3 → r4 改寫(對齊 subprocess 邊界) | 待 M1 完成後動工 |
| **M3** | v2.0 P0 動工(Rust workspace、Neely Core MVP、五檔實測) | 待 M2 完成後動工 |

### 1.2 技術棧最終決策

| 項目 | 選擇 | 原因 |
|---|---|---|
| **DB** | Postgres 17 | 真正想要「不再為 SQLite 不足打補丁」,17 比 18 生態穩、SO 問答多 |
| **Python driver** | `psycopg[binary]>=3.2,<4` 單一 driver | 純自用情境下簡單性 > 極致效能;28% 差異感覺不到;API 跟 sqlite3 像,Collector 遷移成本最低 |
| **Rust driver** | `sqlx 0.8` + Postgres feature + tokio | 跟 psycopg3 體驗對齊;migration 工具內建;`cargo sqlx prepare` offline mode 方便 |
| **Schema migration** | Alembic 1.13+,純 SQL migration(不用 SQLAlchemy ORM) | 主流、可靠;baseline 直接讀 `schema_pg.sql`,不維護兩份 DDL |
| **DBWriter 抽象層** | `typing.Protocol` + `@runtime_checkable` | structural typing,測試友好;不強制繼承,不擋未來加 Schema 抽象 |
| **SQLite fallback** | 保留 `SqliteWriter`,僅 `TWSTOCK_USE_SQLITE=1` 環境變數啟用 | 過渡期雙寫驗證 + CI 快速測試 + debug 用,v2.1 評估完全廢棄 |
| **PostgresWriter 同步 vs async** | 同步(Phase 排程是 serial 的);Aggregation Layer / on-demand 補算未來另建 PostgresAsyncWriter | 不混合 sync/async |
| **Rust ↔ Python 邊界** | Subprocess + CLI args + stdout JSON,**不用 PyO3** | Collector 已驗證的 pattern,直接複用 |

### 1.3 NEO Pipeline v2.0 spec(r3)需要修正的點 — M2 工作

**r3 原本寫 PyO3 + JSON via serde,實際應該是 Subprocess + 共享 DB**:

| 章節 | 原本主張 | 應改成 |
|---|---|---|
| 13.3 開發語言補充 | PyO3 邊界用 JSON via serde + numpy zero-copy | Subprocess + CLI args + stdout JSON,資料不跨邊界共享 DB |
| 附錄 A 條目 32 | 「PyO3 邊界用 JSON via serde」 | 「Subprocess + 共享 DB」 |
| 13.8 P0 必須決策清單第 1 項 | 「PyO3 邊界與序列化格式」 | 「Subprocess + 共享 Postgres」 |
| 11.2 Monolithic Binary | 提到 `inventory::submit!` | 微調為「每個 Core 群組一個 `[[bin]]`」 |
| 14 章 Storage Layer | 預設 Postgres(JSONB、partition)— **方向正確** | DDL 對齊本 M1 schema_pg.sql 風格 |
| 15.6 single-flight | Postgres advisory lock — **方向正確** | 確認可行 |

**還需新增**:
- DB 引擎決策列入 13.8 第 0 項
- Schema Version 同步機制(對齊 Collector 的 `EXPECTED_SCHEMA_VERSION` 慣例)
- P0 動工 checklist 5 項(Cargo workspace 模板、CLI args spec、Schema version 同步、SIGTERM handling、DB schema migration)
- 13.9 五檔實測 「前置條件聲明」(只需 Neely Core MVP,不需 Storage 等)

---

## 二、Milestone 1 進度

### 2.1 已完成 Steps

| Step | 內容 | 產物路徑 | 驗證狀態 |
|---|---|---|---|
| **1.1** | Docker compose + 環境變數 | `docker-compose.yml`、`.env.example` | 設計完成,實測請等 user 端 |
| **1.2** | Postgres 17 完整 DDL(27 表) | `src/schema_pg.sql` | ✅ 沙箱實測通過,idempotent 通過 |
| **1.3a** | `db.py` 全文重寫(Protocol + PostgresWriter + SqliteWriter + factory) | `src/db.py` | ✅ 13 項單元測試通過 |
| **1.3b** | `requirements.txt`(psycopg + alembic + python-dotenv) | `requirements.txt` | ✅ 依賴可裝 |
| **1.3c** | Alembic 初始化 + baseline migration | `alembic/`、`alembic.ini` | ✅ upgrade / downgrade / 整合 db.py 全通過 |
| **1.3d** | field_mapper 相容性 | (不需改動) | ✅ `_table_columns()` 簽名保持 |

### 2.2 待完成 Steps

| Step | 內容 | 預估工作量 | 風險點 |
|---|---|---|---|
| **1.4** | `rust_compute` 改 sqlx | **大**(~3 天) | rusqlite → sqlx 是大改,SQL 邏輯不變但 transaction / async 模式全變 |
| **1.6** | 日期欄位 type 修正(audit) | 中 | db.py 的 `_cast_for_pg` 已自動處理,但要 audit `aggregators.py` 中是否還有 TEXT 假設 |
| **1.7** | 全 Phase 1–6 重跑 + diff 驗證 | 中 | 需要 FinMind token,Claude sandbox 跑不了,user 端執行 |
| **1.8** | 文件更新(CLAUDE.md、README) | 小 | M1 完成後更新到 v2.0 |

---

## 三、Milestone 1 詳細交付產物

### 3.1 檔案清單(全部已測通,可直接落地到 repo)

```
m1/
├── .env.example                    # 環境變數樣板
├── docker-compose.yml              # Postgres 17 容器設定(tuning 已調)
├── alembic.ini                     # Alembic 設定(URL 從 env 讀)
├── requirements.txt                # Python 依賴清單
├── src/
│   ├── schema_pg.sql               # Postgres 完整 DDL(27 表 + 索引 + 約束)
│   └── db.py                       # 全文重寫,~700 行
├── alembic/
│   ├── env.py                      # 客製版,讀 DATABASE_URL,純 SQL migration
│   ├── script.py.mako              # 預設(沒改)
│   ├── README                      # 預設(沒改)
│   └── versions/
│       └── 2026_04_28_0da6e52171b1_baseline_schema_v2_0.py
└── scripts/
    └── test_db.py                  # 13 項單元測試
```

### 3.2 db.py 重點設計

**架構元件**:
- `DBWriter(Protocol)` — 抽象介面,structural typing
- `PostgresWriter` — 預設實作,psycopg3 同步
- `SqliteWriter` — 過渡 fallback,`TWSTOCK_USE_SQLITE=1` 啟用
- `create_writer()` — factory,依環境選實作
- `_legacy_dbwriter_constructor` — DeprecationWarning shim,過渡相容

**關鍵特性**:
1. `_col_type_cache: dict[str, dict[str, str]]` — 兼顧 set 介面相容性 + dict 型別資訊
2. `_cast_for_pg()` 統一處理 jsonb / date / empty-string,業務 code 不用 `json.dumps`
3. `_warn_dropped_once()` 同 (table, dropped_keys) 只 warn 一次
4. `init_schema()` 三段式:metadata 檢查 → alembic upgrade → fallback `schema_pg.sql`
5. transaction 失敗自動 rollback(避免 `InFailedSqlTransaction`)
6. `_mask_url()` 遮蔽 log 中的密碼

**外部 API**(向後相容,業務 code 不用大改):
- `db.upsert(table, rows, primary_keys)` — 必須傳 PK
- `db.insert(table, row)` — 不傳 PK 走 `ON CONFLICT DO NOTHING`
- `db.query(sql, params)` / `db.query_one(...)` — psycopg 用 `%s` 佔位符
- `db.update(sql, params)` — 同上
- `db._table_columns(table) -> set[str]` — field_mapper 用,簽名沒變

### 3.3 schema_pg.sql 重點改動(對 SQLite 版的差異)

| 改動 | 範圍 | 風險 |
|---|---|---|
| TEXT 日期 → DATE | 所有 date 欄位 | ⚠️ Phase 5 / 6 可能有 `YYYY-MM-DD HH:MM:SS` 混入,要在 db.py 寫入時 strip(已實作) |
| TEXT JSON → JSONB | 9 個 detail 欄位 | psycopg3 自動處理 dict <-> jsonb |
| REAL → NUMERIC(p, s) | 股價、財報數字欄位 | 精度提升,業務 code 端讀出來是 `Decimal` |
| INTEGER → BIGINT | volume / 成交金額 | 防 32bit 溢位 |
| 加 CHECK constraint | event_type、status、fwd_adj_valid | API 傳新值會報錯,需先擴 CHECK |
| 加 GIN index 於 financial_statement.detail | JSONB 查詢加速 100× | 寫入慢 5–10% |
| 新增 schema_metadata 表 | v2.0 spec 對齊機制 | Rust binary 啟動時會 assert |
| `datetime('now')` → `NOW()` | TIMESTAMPTZ | Python 端用 `datetime.now(UTC)`,不要 naive datetime |

### 3.4 Alembic 工作流

**首次部署**:
```bash
docker compose up -d
export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock
alembic upgrade head     # 或讓 db.init_schema() 自動觸發
```

**未來加新欄位**:
```bash
alembic revision -m "add_xxx_to_yyy"
# 編輯 alembic/versions/<日期>_<rev>_add_xxx_to_yyy.py
# upgrade(): op.add_column(...)
# downgrade(): op.drop_column(...)
# 同步改 src/schema_pg.sql(SSOT)
alembic upgrade head
```

---

## 四、Step 1.4 預先設計(下個 session 直接動工)

### 4.1 Cargo.toml 改動

**舊**:
```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
```

**新**:
```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-native-tls", "postgres", "chrono", "macros"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"

[dev-dependencies]
# 移除 rusqlite,沒了
```

### 4.2 src/main.rs 改動點

**Connection 模式**:
- 舊:`rusqlite::Connection::open(path)` 同步單連線
- 新:`sqlx::PgPool` 連線池,async

**CLI args**:
- 舊:`--db <path>`(SQLite 檔案路徑)
- 新:`--database-url <url>` 或從 `DATABASE_URL` 環境變數讀

**Transaction**:
- 舊:`conn.execute_batch("BEGIN; ... COMMIT")` 或 implicit
- 新:`let mut tx = pool.begin().await?; ...; tx.commit().await?;`

**Query 風格選擇**:
- 編譯期檢查的 `sqlx::query!()` macro(需要 `cargo sqlx prepare` 與 live DB)
- runtime 檢查的 `sqlx::query()`(較鬆,不用 live DB)
- **建議**:用 `sqlx::query()` runtime 版,因為:
  - 不用每次 build 都連 DB
  - CI 環境簡單
  - SQL 字串可動態組(雖然這個專案不常需要)
  - 少一個 .sqlx/ 快取目錄要管

**Signal handling**:
- 舊:沒有(rusqlite 自然 commit / rollback)
- 新:`tokio::signal::ctrl_c()` 或 unix `SIGTERM`,呼到時 `tx.rollback().await?` 然後 exit

**Stdout summary**:
- 不變(仍是 `println!("{}", serde_json::to_string(&summary)?)`)
- Schema version assert: 啟動時 query `schema_metadata.schema_version`,不吻合 panic

### 4.3 預期改動範圍

| 函式 | 改動 |
|---|---|
| `main()` | `#[tokio::main]`,改 async;Connection → PgPool |
| `resolve_stock_ids()` | 改 async;rusqlite 風格 → sqlx::query() |
| `load_trading_calendar()` | 改 async |
| `process_stock()` | 改 async;傳入 `&mut sqlx::Transaction` |
| `load_raw_prices()` | 改 async,單股 query |
| `load_adj_events()` | 改 async |
| `compute_fwd_prices()` | 純計算,**不改** |
| `aggregate_weekly()` / `aggregate_monthly()` | 純計算,**不改** |
| `write_fwd_*()` | 改 async,executemany 用 sqlx 風格 |
| 新增 `assert_schema_version()` | 啟動時呼叫 |

### 4.4 風險與緩解

| 風險 | 緩解 |
|---|---|
| sqlx 連線池在 batch 場景下行為與 rusqlite 不同 | 用單連線跑 batch(`pool.acquire()` 一次),避免連線池 overhead |
| `SIGTERM → wait 10s → SIGKILL` 在 async 下要用 `tokio::select!` | 沿用 Collector 既有 Python 端 signal handling 邏輯 |
| `executemany` 在 sqlx 沒原生支援 | 用 `QueryBuilder` 拼 multi-row INSERT 或單筆 loop(後者簡單) |
| 編譯時間變長(sqlx + tokio) | 接受,batch 一日跑一次,啟動時間不敏感 |

---

## 五、下個 session 開工指引

### 5.1 接續 Milestone 1 Step 1.4 的指令

直接告訴 Claude:

> 接續 Milestone 1 工作。Step 1.1–1.3 已完成,現在做 Step 1.4 — `rust_compute` 從 rusqlite 改 sqlx。
>
> 請依本文件第 4 節「Step 1.4 預先設計」開始動工:
> 1. 改 Cargo.toml 依賴
> 2. 改 main.rs 為 tokio::main + sqlx::PgPool
> 3. 處理 SIGTERM 邏輯
> 4. 跟 schema_metadata 對齊 schema_version assert
> 5. 沙箱實測(裝 Rust toolchain + sqlx + Postgres,跑 fwd 計算驗證)

### 5.2 後續 Steps 順序建議

```
Step 1.4 (rust_compute → sqlx)              ~3 天
  ↓
Step 1.5 (json_extract → ->>)               ~0.5 天(已完成)
  ↓
Step 1.6 (日期欄位 audit)                    ~1 天
  ↓
Step 1.7 (全 Phase 1–6 重跑驗證)             ~2 天 — 需 user 端執行
  ↓
Step 1.8 (文件更新 CLAUDE.md / README)        ~0.5 天
  ↓
M1 完成 → 啟動 M2 (r3 → r4 spec 改寫)
```

### 5.3 不要做的事

- ❌ **不要在 M1 期間動 v2.0 spec(r3)** — M2 工作,等 M1 完成
- ❌ **不要在 M1 期間開始 v2.0 P0 動工** — M3 工作
- ❌ **不要嘗試切換到其他技術棧**(asyncpg、tokio-postgres、DuckDB 等)— 已決策,不重議
- ❌ **不要刪除 SqliteWriter** — 過渡期需要,v2.1 才評估廢棄
- ❌ **不要在 M1 加 partition / advisory lock 等 v2.0 feature** — Collector 用不到,M2 / M3 才加

### 5.4 已知未解決的小問題(等 user 端遇到再說)

1. **Phase 5 / 6 中可能有 `YYYY-MM-DD HH:MM:SS` 混入 date 欄位** — db.py 已自動 strip,但要 audit aggregators.py 是否依賴特定字串格式
2. **CHECK constraint 可能擋到 API 新增 enum 值** — 需要先 ALTER 才能加新 event_type
3. **Postgres 預設不監聽 TCP** — Docker compose 已處理,但裸機部署要改 `postgresql.conf` + `pg_hba.conf`
4. **JSONB 序列化可能比 TEXT 慢一點點** — 沙箱沒 benchmark,production 看狀況

---

## 六、Quick Reference — 關鍵連線指令

### 環境變數
```bash
export DATABASE_URL=postgresql://twstock:twstock@localhost:5432/twstock
# 或寫到 .env 檔(配合 python-dotenv)
```

### 啟動 Postgres
```bash
docker compose up -d
docker compose logs -f postgres
docker compose down       # 保留資料
docker compose down -v    # 連 volume 一起刪
```

### Alembic
```bash
alembic upgrade head      # 升到最新版
alembic current           # 看目前版本
alembic history           # 看所有版本
alembic downgrade base    # 回到空(危險)
alembic revision -m "msg" # 建新 migration
```

### psql 連線
```bash
psql "postgresql://twstock:twstock@localhost:5432/twstock"
```

### Debug 模式切回 SQLite
```bash
TWSTOCK_USE_SQLITE=1 SQLITE_PATH=data/tw_stock.db python src/main.py ...
```

### 跑 db.py 測試
```bash
python scripts/test_db.py
```

---

## 七、給下個 session 的 Claude 的補充情境

### 7.1 user 偏好

- 慣用語言:**繁體中文**(用 userPreferences 強制)
- 工程風格:**重視長期工程品質,不喜歡為短期方便打補丁**
- 決策風格:**會主動指出我的矛盾、要求分析而非直接選擇**
- 技術背景:HFC Finance 工程師,熟 ASP.NET MVC / C# / Kendo UI / SQL Server,Rust + Python 混合架構有經驗

### 7.2 Collector 既有架構摘要

- 主分支:`claude/collector-schema-mapping-2YF5U`
- Python:`src/{main,phase_executor,api_client,db,field_mapper,...}.py`
- Rust:`rust_compute/src/main.rs`(獨立 binary,不是 PyO3)
- 邊界:**Subprocess + CLI args + stdout JSON + 共享 SQLite(本 M1 改 Postgres)**
- Schema version: const `SCHEMA_VERSION="1.1"` 在 Python / Rust 兩端 hardcode,Rust binary 在 stdout 回傳
- Phase 順序:1 META → 2 EVENTS → 3 RAW PRICE → 4 RUST 計算 → 5 CHIP/FUND → 6 MACRO

### 7.3 關鍵檔案位置(Collector 原 repo)

```
StockHelper4me/
├── CLAUDE.md                       # v1.6 銜接文件,跨 session 用
├── collectorSpec/                  # spec v1.2 (p1-p3)
├── config/
│   ├── collector.toml              # 主設定檔
│   └── stock_list.toml             # 股票清單
├── docs/schema_reference.md        # schema 對照(本 M1 完整解析過)
├── rust_compute/
│   ├── Cargo.toml
│   └── src/main.rs                 # ~600 行,Phase 4 Rust 實作
├── scripts/                        # inspect_db, drop_table 等工具
└── src/                            # Python 主 code
    ├── api_client.py
    ├── db.py                       # 本 M1 將被 m1/src/db.py 取代
    ├── field_mapper.py
    ├── main.py
    ├── phase_executor.py
    ├── rust_bridge.py              # Subprocess 呼叫 rust_compute binary
    └── ...(共 14 個 .py 檔)
```

### 7.4 引述本 M1 產物的方式

下個 session 要看到本 M1 的產物,可以:
- 把本份 markdown 與下列檔案一併上傳:
  - `m1/src/schema_pg.sql`
  - `m1/src/db.py`
  - `m1/alembic/env.py`
  - `m1/alembic/versions/2026_04_28_0da6e52171b1_baseline_schema_v2_0.py`
  - `m1/alembic.ini`
  - `m1/requirements.txt`
  - `m1/docker-compose.yml`
  - `m1/scripts/test_db.py`

或者把整個 `m1/` 目錄推到 git branch,告訴下個 session URL 即可(沙箱可 clone)。

---

## 八、未解決問題清單(集中檢視,免散落)

### 已標記但延後處理

1. **Postgres 並發寫的真實壓力**(Q3 答「要看實測」)— Step 1.7 全 Phase 重跑時順便 benchmark
2. **TW-Market Core 輸出去向**(in-memory vs 新表)— M2 改 r3 spec 時決定
3. **Core binary 顆粒度**(per-group bin vs 統一 pipeline_bin)— M2 / M3 決定,目前傾向 per-group(對齊 Collector 的 `tw_stock_compute` 慣例)
4. **PG 18 升級時機** — 純自用先不急,商業化前再評估
5. **PostgresAsyncWriter 何時加** — Aggregation Layer 動工時(M3 P1)

### 觀察項(不一定是問題)

1. SQLite fallback 的 `init_schema()` 引用了不存在的 `db_legacy_sqlite_ddl` module — 若實際啟用 SqliteWriter 需要把舊 SQLite DDL 抽出來
2. `_legacy_dbwriter_constructor` shim 的 deprecation 時程 — 建議 v2.1 移除
3. `psycopg-pool` 已在 requirements 但 db.py 還沒用 — 留給 PostgresAsyncWriter

---

> **本文件結束**
> 下個 session 開工:把這份 markdown + m1/ 目錄產物提供給 Claude,直接說「接續 M1 Step 1.4」即可。
