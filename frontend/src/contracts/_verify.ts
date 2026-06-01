// 編譯期檢查:re-export 強制 tsc 解析整個生成型別樹(drift / 不一致 → tsc 報錯)。
export type { NeelyCoreOutput } from "./neely/NeelyCoreOutput";
export type { Scenario } from "./neely/Scenario";
export type { RuleId } from "./neely/RuleId";
export type { LevelsFusion, ResonanceFusion, ClimateFusion } from "./fusion";
// #2 — screens base + 10 metric subtypes(per-toolkit 具名,前端依 toolkit narrow)
export type {
  ScreenResponse,
  ScreenRowBase,
  ScreenRowMagicFormula,
  ScreenRowPersistentMomentum,
  ScreenRowRevenueMomentum,
  ScreenRowInstitutionalConcert,
  ScreenRowFScore,
  ScreenRowLowVolatility,
  ScreenRowIndustryAdjGp,
  ScreenRowLongTermLowVol,
  ScreenRowDividendYield,
  ScreenRowMom12_1,
} from "./fusion";
// #7 — 個股入口 autocomplete StockRef
export type { StockRef } from "./fusion";
// #4 — OHLC 切片(改名 PriceBar/PriceSeries 避開既有 ts-rs OhlcvBar/OhlcvSeries)
export type { PriceBar, PriceSeries } from "./fusion";
// #3 — Kalman ts-rs 衍生(6 個 struct/enum,feature-gated → 治本契約)
export type { KalmanFilterOutput } from "./kalman/KalmanFilterOutput";
export type { KalmanPoint } from "./kalman/KalmanPoint";
export type { KalmanHorizonOutput } from "./kalman/KalmanHorizonOutput";
export type { KalmanEvent } from "./kalman/KalmanEvent";
export type { KalmanEventKind } from "./kalman/KalmanEventKind";
export type { Regime } from "./kalman/Regime";
