# wave_judgment_loop — 證據 → 判讀 → 錨定迴路(2026-08-31)

> spec:`m3Spec/wave_judgment_loop.md` + 前置 `m3Spec/neely_ch6_gate_running_fix.md`(M0)。
> 本檔逐 phase 追記;production 全市場重跑與首批判讀見文末 runbook。

## 拍版紀錄(2026-08-30)

1. M0(neely 1.2.0)併入本 branch;M1–M4 全部 code 一次做;M5(J3 報告)另案
2. `/stocks/{id}/waves` **additive**:response = `{neely, traditional, dossier}`,raw 兩鍵不動(V1 圖表零斷源);MCP `neely_forecast` 照 spec 全換 dossier(`primary_scenario`/`scenario_count`/`scenario_staleness` 三鍵刪除)
3. 判讀寫入 = `POST /judgments`(web API 首個寫端點)+ CLI `judgment submit`,同一驗證器
4. spec 未列 pick 站(V2 cell `wave_summary.py`、`wave_impulse_screen` fallback)**一併 judgment-aware**:有 active judgment 用 accepted[preferred],無則回現行表現層預設

## Phase 進度

| Phase | 內容 | 狀態 |
|---|---|---|
| 0 | spec 落地(m3Spec 兩檔 + 本檔 + INDEX) | ✅ |
| 1 | M0:Ch6 閘接回 ladder + Running b>a+c(neely 1.2.0) | ⬜ |
| 2 | E1 assumptions/E4 ambiguity/E2 robust + reverse_logic 退場(neely 1.3.0) | ⬜ |
| 3 | J1 wave_judgments 表 + anchor_key + dossier 讀路徑切換 | ⬜ |
| 4 | 判讀驗證器 + CLI/POST 寫路徑 + neely-judgment skill | ⬜ |
| 5 | S3 下游(track1/emitter/V2/wave_impulse)+ J2 diff + refresh hook + 前端 | ⬜ |
| 6 | 收尾(CLAUDE.md 輪替 / runbook 定稿) | ⬜ |
