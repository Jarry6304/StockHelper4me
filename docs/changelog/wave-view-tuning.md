# 波浪視圖調校 — 雲層 live-only / stale 窗錨定 / 收盤背景線(2026-06-11)

> user 回報:web 畫面 (1) 傳統波浪沒有時間軸 (2) Neely 雲層失準,並提出
> 「一口氣全部歷史倒進引擎 → 取到最前面幾年的合適波型;預期舊資料收攏成
> 固定波型再從尾端反推未來」。三路探索(前端渲染 / neely / traditional)後
> user 拍版「1、2、3、4 都做」。

## 根因(探索事實)

- **引擎面(user 猜測大致正確)**:兩引擎固定 lookback(日線 1500 根 ≈ 6 年);
  Neely candidate 滑窗遍歷整段序列任意位置匹配(`candidates/generator.rs:56-112`),
  無尾端錨定、寫 snapshot 無 recency 過濾;compaction 是窮舉聚合全保留,
  **不是**「舊結構定型淘汰」。早年合適波型「也」進 forest(新舊混雜)。
- **雲層失準的直接機制**:State 1 總覽餵 `flat_fib_zones`(全 forest 聯集 —
  原始用途是 fusion key_levels 支撐壓力候選,不是 UI 前瞻雲層),歷史錨定形態
  的價位被畫進 `[topScenario.end, asOf+90d]` 投影窗。
- **傳統「沒有時間軸」體感**:資料與日期軸都在;但 trending 股近一年無
  corrective 形態 → 全 forest stale → 自動擴窗把老形態與 asOf 同框,x 軸攤成
  2+ 年刻度壓扁;且圖上只有稀疏 pivot zigzag,無價格時間背景。

## 拍版與落地(4 個獨立 commit)

| # | 內容 | 改動面 |
|---|---|---|
| 1 | **表現層治理**:`collectLiveFibZones()`(只聯集結尾 ≤180d scenario 的 expected_fib_zones,去重鏡射 Rust `flatten_fib_zones`;全 stale → 隱藏 + 提示)。傳統圖 stale 時窗錨定形態本身 `[start−30d, end+90d]`;範圍 preset 鈕(6m/1.5y/3y/全部);stale 文案移除硬編 2330 | 純 frontend |
| 2 | **`scenario_age_days`**:`/waves/summary` row 加形態年齡(picked scenario `wave_tree.end` 距 as_of;與 staleness_days = snapshot 新鮮度是兩回事);V2 WaveCell age>365d label 變 stale 色,對齊 V1 視覺 | serving + 契約 additive,0 schema |
| 3 | **收盤背景線**:兩張波浪圖最底層鋪後復權收盤淡線 — **重用既有** `GET /stocks/{id}/ohlc` + `getOhlc`(計畫原列新 price 端點,實作時發現已存在,0 後端);loader 並行抓 asOf−2200d,失敗降級無背景 | 純 frontend |
| 4 | **漸進收攏討論稿**:`m3Spec/proposal_progressive_settlement.md`(S1 歷史段定型 + S2 尾端錨定;Reverse Logic / key_levels / facts / PIT 取捨清單)— **未拍版,不動 Rust** | 文件 |

## 與 user 心智模型的對齊聲明

「舊資料收攏成固定波型再反推未來」= NEoWave 漸進分析法,引擎目前沒有 —
本批 1-3 是表現層止血(雲層與預設窗只看 live / 給時間背景),引擎級實現
走選項 4 的 spec 拍版流程(見討論稿 §5 開放問題)。

## 驗證

- sandbox:vitest **137 passed**(+11)/ svelte-check 0 / `npm run build` 過;
  pytest wave_summary + waves_summary **47 passed**(全套見 commit);
  codegen `git diff frontend/src/contracts/` 僅 +1 行(`scenario_age_days`)。
- user 本機 runbook:
  ```powershell
  git pull
  uvicorn web_api.app:app
  cd frontend; npm run dev
  # /stocks/2330(全 stale 案):傳統圖預設窗錨定老形態(軸可讀)+ preset 鈕;
  #   Neely 總覽雲層隱藏 + 「近 180d 無進行中形態」提示;兩圖有淡灰收盤背景線
  # /stocks/3030(近期 Impulse):雲層只剩 live zones,緊貼現價
  curl.exe "http://127.0.0.1:8000/waves/summary?stock_ids=2330,3030&date=<today>"
  #   → row 多 scenario_age_days;/screens/magic_formula stale 股 label 變暗
  ```

## 待議(不阻塞)

- 漸進收攏 spec 拍版(`m3Spec/proposal_progressive_settlement.md` §5)→ 拍版後
  另案動 Rust;traditional_core 同概念分開拍。
