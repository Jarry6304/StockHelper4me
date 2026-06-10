# V2 WAVE 欄真實端點 — 拍版 (a) 批次端點(2026-06-11)

> v4.38 留下的 CL4 拍版:V2 跨股表(`/screens/[toolkit]`)WAVE / 共振欄資料源
> 三選一。user 拍版 **(a) `GET /waves/summary` 批次端點**;(b′)(第 4 Golden L3
> 物化 kind)留作 perf 升級路徑 — 本端點抽取函式可原樣搬進 materialize。
> 0 alembic / 0 Rust / 0 refresh chain 變更。

## 拍版依據(影響分析摘要)

| | (a) 批次端點 ★ | (b) 物化 12 ranked 表 | (b′) 第 4 物化 kind | (c) lazy 單檔 |
|---|---|---|---|---|
| alembic | 0 | 12 migrations | 0 | 0 |
| Python 改動 | 1 router + 1 contract | 12 builders + chain 手術 | 1 builder + 仍要 1 router | 0(前端揹 compute) |
| 新鮮度 | 同 V1 snapshots | **結構性 stale 1 天**(Phase 8 在 cores 前跑) | 正確 | 同 V1 |
| 整表 wave 排序 | 可 | 可 | 可 | 不可 |

關鍵事實:`resonance_fusion` 物化 doc 已含 track1 + findings;neely forest
production 分布 p50=4 / p95=11 scenarios → 30 檔讀時抽取成本可忽略,(b′) 的
預物化是 YAGNI。

## 落地內容

| 層 | 檔案 | 內容 |
|---|---|---|
| 抽取(單一真相源) | `src/fusion/wave_summary.py`(新) | `wave_summary_rows(conn, stock_ids, as_of, timeframe)`:2 條 batch SQL(`DISTINCT ON` + `ANY(%s)`)讀 neely_core + resonance_fusion;每檔抽 label / direction / certainty / scenario_count / sparkline(monowave 尾 ≤10 點歸一化)/ resonance(findings 歸約 strong>basic>divergence>none,single_track→none)/ staleness_days;單檔壞 → insufficient 不炸整批 |
| top scenario 選法 | 同上 | **鏡射前端 `power.ts::pickDefaultScenario`**(含 fb9e166 老化形態修正):未失效 → recency tier → power → passed → days。V2 cell 與 V1 卡 default focus 同一顆 scenario;改一邊必同步另一邊 |
| 端點 | `src/web_api/routers/waves_summary.py`(新)+ app.py 註冊 | `GET /waves/summary?stock_ids=2330,1101&date=…&timeframe=daily`;stock_ids 上限 100 / 空 / 壞 timeframe → 422;無資料股回 `insufficient:true` 不 404 |
| 契約 | `src/web_api/contracts.py` + codegen | `WaveSummaryRow` / `WavesSummary` → `frontend/src/contracts/fusion.ts`(pydantic2ts,純增量 diff) |
| 前端 | `frontend/src/lib/screener/placeholder.ts`(改寫)+ `lib/api/waves_summary.ts`(新) | `fetchWaveDigests()` 批次入口:真端點 → `digestFromRow` 映射(enum 防衛收斂);API 失敗 → 整欄 insufficient 不擋表格;`VITE_WAVE_PLACEHOLDER=1` 保留 fake fallback(PH 角標語意不變) |
| 接線 | `+page.ts` loader → `Screener` → `ScreenerTable` | digests 隨 screen rows 第二跳抓(stock_ids 依賴 rows,無法並行);`WaveCell` 0 改 |

### 與 v4.38 spec 字面的偏差(記錄)

1. spec 範例寫 `POST {stock_ids[]}` — web_api CORS 鎖 GET/OPTIONS 且全 API 唯讀
   → 改 **GET query string**(30 檔 ~200 字元,無長度問題)
2. payload 比 spec 例多 `scenario_count` / `sparkline` / `staleness_days` 3 個
   additive 欄(保 wireframe 完整呈現 + 新鮮度揭露)
3. 「只動 getWaveDigest() 1 個 module」實際 = placeholder.ts 改寫 + loader/props
   3 處 pass-through(digests 需從 loader 下發;cell 與 fake 產生器確實只在 1 個 module)

## 驗證(2026-06-11 sandbox)

- `pytest tests/` **1023 passed**(+38:fusion 抽取 24 + web_api 端點 14)/ 1 pre-existing fail / 2 xfailed
- `npx vitest run` **125 passed**(placeholder 測試改寫:fake 7 保留 + digestFromRow/fetchWaveDigests 6 新)
- `npx svelte-check` 0 errors / 0 warnings;`npm run build` 過
- codegen 重跑 `git diff frontend/src/contracts/` 僅 +16 行新型別(既有型別 0 變更)

### user 本機 runbook

```powershell
git pull
uvicorn web_api.app:app          # 後端
curl.exe "http://127.0.0.1:8000/waves/summary?stock_ids=2330,3030&date=2026-06-11"
cd frontend; npm install; npm run dev
# http://localhost:5173/screens/magic_formula → WAVE 欄真資料(無 PH 角標)
# 抽查:cell 的 label/方向 應與 /stocks/2330 V1 卡 default focus 一致
```

## 待議(不阻塞)

- production 實測若端點 >1s(universe 翻倍等)→ 啟動 (b′):`wave_summary.py`
  抽取函式原樣搬進 `src/fusion/materialize/` 成第 4 kind
- wave_impulse toolkit 的 HTTP 端點(CL3)仍為獨立議題
