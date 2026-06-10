# 架構整備 P0–P2(2026-06-10)

> 低耦合 / 低抽象 / LLM 可讀性三目標修正案,7 項全收尾;0 alembic / 0 schema /
> 0 collector.toml / 0 runtime 行為變更(P1-1 僅型別搬家,輸出位元不變)。
> 規格:user 直送「StockHelper4me 架構整備 P0–P2」+ references/ 7 份;
> 每項一個獨立 commit(rollback 各自 `git revert`)。

## 項目與落地

| ID | 項目 | 落地 |
|---|---|---|
| P0-2 | 過期 docstring 清除 | repo 內 /root/.claude plans 路徑全清(mcp_server / src / dashboards / alembic docstring / kalman_forecast_core / 3 README);「23 cores」改指單一真相源;再發防止 3 條進 CLAUDE.md 禁止事項 |
| P0-1 | CLAUDE.md 三角色拆分 | 8,507 行 → 入口 250 行;docs/changelog/ 7 帶檔 + traditional-core + process-logs + INDEX(79 列);v1.x 與過期段追加 claude_history(⚠️ 標註);內容守恆 −42 行(<60);拆分腳本 scripts/split_claude_md.py |
| P2-3 | README 瘦身 + 歸檔 | 版本橫幅 2,000+ 字牆 → 14 行(最新 1 版 + 指標表 + INDEX 連結);m2Spec/oldm2Spec(548KB)git mv → docs/archive/(含「僅考古」README);全 repo 引用 repoint |
| P1-2 | tools/data.py 拆域檔 | 1,349 行 / 38 函式 → 8 域檔(_shared / wave / snapshot / screens / market / levels / indicators / raw,各 ≤262 行);data.py 轉 70 行 façade;server.py / tests 0 改動;域檔只 import _shared 無環 |
| P2-1 | fusion 公開面正名 | fusion / fusion.raw 兩個 `__init__` 顯式 export(名單 = 實際消費者 grep);外部 22 處 import 改公開路徑;底線模組標「套件內部」 |
| P2-2 | DSN 單一真相源 | 新 src/dsn.py(REPO_ROOT + load_repo_env + resolve_database_url);db.py / fusion / probe 三份副本 delegate;alembic 留鏡像(拍板 B);v4.33 回歸鎖遷移 + 語意表 5 案回歸 |
| P1-1 | OhlcvSeries 下放 fact_schema | `cargo tree -p ohlcv_loader` 零 neely;22 下游 crate 0 檔變更;TS codegen byte-identical(generate.sh Track A 補跑 fact_schema) |

## 記錄的規格偏差(均為修復性,commit message 各有完整 rationale)

1. **P0-1**:v4.37 / v4.35 兩段在歷史 merge 中遺失 `##` 標頭 → 拆分時補回(INDEX 79 = 77 + 2;規格 INDEX 範例本身含 v4.37 列);「Fusion Layer API 規劃落地」無版本號 → 按日期歸 v4.10-v4.19 帶;「完整重跑流程」內容為 SQLite era → 移 claude_history 標 ⚠️
2. **P0-2**:現況清單外的同類 hits(src/forecast / dashboards / alembic docstring / fusion/raw/query.py / kalman_forecast_core)一併修,對齊 repo-wide 驗收 grep
3. **P2-1**:re-export 用 PEP 562 `__getattr__` 轉發而非 eager import — 既有 35 個測試 `patch("fusion.raw._db.get_connection")` 打內部模組,eager 綁定會凍結原函式物件令 patch 打不進(實測 35 failed → 轉發後全綠);`canonical_is_invalidated` / `fetch_market_facts` 為規格名單漏列,grep 補抓
4. **P1-1**:`load_for_neely` 用 `NeelyCore::warmup_periods`(規格現況表只列 line 25 re-export 一處耦合)→ 原樣搬到 tw_cores::helpers(兩 caller 都在該 crate),共用層才真正零領域依賴

## 驗證(2026-06-10 sandbox)

- `cargo test --workspace` **647 passed / 0 failed**(= 改動前基準;P1-1 後 `cargo tree -p ohlcv_loader | grep neely` 空)
- `pytest tests/` **979 passed / 2 xfailed / 1 pre-existing fail**(`test_default_market_is_lowercase_tw`,v4.32 起已記錄,與本案無關;子集 fusion+mcp_server+web_api 527 passed)
- TS codegen 重跑 `git diff frontend/src/contracts/` = 空(含刪檔重生驗證)
- 內容守恆:拆分後總行數 vs 原 CLAUDE.md 差 −42(< 60)

### DB-bound 項(sandbox 無 PG,留 user 本機)

```powershell
git pull
python scripts/verify_mcp_toolkit_v4_29.py --stocks 2330,3030   # 退碼 0(P1-2 façade 驗收)
# P2-2 v4.33 情境回歸:repo 外任意 cwd 起 web_api / MCP / main.py status / streamlit 四入口
alembic upgrade head                                            # 應 no-op(0 migration;自足性回歸)
cd rust_compute; cargo build --release -p tw_cores; cd ..
.\rust_compute\target\release\tw_cores.exe run-all --write --stocks 2330   # P1-1 smoke:輸出 row-identical
.\scripts\test_pipeline.ps1                                     # 全 pipeline 健康度
```
