# StockHelper4me — Web 前端原型

SvelteKit + Vite + TypeScript + Plotly.js,消費 Golden L3 唯讀 API。

## 安裝 + 開發

需要 Node.js >= 18。

```bash
cd frontend
npm install
npm run dev
```

開 `http://localhost:5173`。

## 後端依賴

需要本機跑 FastAPI(`uvicorn web_api.app:app` from repo root)。
v0.1 走 dev proxy:`vite.config.ts` 內 `/api/*` → `http://localhost:8000/*`。
直接呼叫 absolute URL 走 CORS(repo `src/web_api/cors.py` 已配置)。

## 結構

```
frontend/
├── package.json
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── app.html
│   ├── app.css            # 全域 CSS variables (設計 token)
│   ├── app.d.ts
│   ├── contracts/         # ts-rs + pydantic2ts 自動生成型別(不要手改)
│   ├── lib/               # 共用模組
│   │   ├── api/           # API client 薄包裝
│   │   └── components/    # Svelte components
│   └── routes/
│       ├── +layout.svelte # 全站頁頂
│       ├── +layout.ts     # SSR 關 (SPA)
│       ├── +page.svelte   # 首頁 landing
│       ├── stocks/[id]/   # V1 個股 WAVE 卡
│       └── screens/[toolkit]/  # V2 跨股篩選表
```

## 視圖

- **V1 個股 WAVE 卡** (`/stocks/[id]`) — Neely ∥ 傳統 forest 並排,State 1 總覽 → State 2 詳情 → State 1b 無法判斷
- **V2 跨股篩選表** (`/screens/[toolkit]`) — magic_formula / f_score 等 7 個因子 toolkit

## 設計約束(對齊 spec L1-L8 / CL1-CL6)

- forest **無 primary / 無百分比**,僅按 `power_rating` + 計數 + Certainty 排序
- Neely / 傳統 **不合併**,並排呈現
- `insufficient_data` / `compaction_timeout` → 顯式「無法判斷」,不繪假圖
- Track2 統計帶為**獨立軌**並排
- V2 wave 欄 = **summary-only**,無 forest 圖,sparkline + label + 方向 + 情境數 + Certainty

## Build

```bash
npm run build       # → build/(adapter-static SPA + fallback index.html)
npm run preview     # 本機預覽 build
```

## Test

```bash
npm run check       # svelte-check tsc
npm test            # vitest 單元測
```
