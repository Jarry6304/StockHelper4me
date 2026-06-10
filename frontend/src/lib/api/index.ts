/**
 * API client entry — re-export 所有 endpoint module(讓 caller `import { ... } from '$lib/api'`)。
 */

export {
  ApiError,
  ScenarioForestOverflowError,
  NotFoundError,
  NetworkError,
  apiGet,
  toIsoDate,
  getBaseUrl
} from './client';
export type { FetchOptions } from './client';

export { healthCheck, type HealthResponse } from './health';
export { getNeelyForest, type Timeframe, type GetNeelyForestArgs } from './neely';
export {
  getTraditionalForest,
  type TraditionalForestOutput,
  type TraditionalScenario,
  type GetTraditionalForestArgs
} from './traditional';
export { getWaves, type WavesResponse, type GetWavesArgs } from './waves';
export { getResonance, type GetResonanceArgs } from './resonance';
export { getLevels, type GetLevelsArgs } from './levels';
export { getOhlc, type OhlcRow, type OhlcResponse, type GetOhlcArgs } from './ohlc';
export {
  getScreen,
  ACTIVE_TOOLKITS,
  DISABLED_TOOLKITS,
  type ActiveToolkit,
  type DisabledToolkit,
  type Toolkit,
  type ScreenRow,
  type ScreenResponse,
  type GetScreenArgs
} from './screens';
export { getMarketClimate, type GetClimateArgs } from './climate';
export {
  getWavesSummary,
  type WaveTimeframe,
  type WaveSummaryRow,
  type WavesSummary,
  type GetWavesSummaryArgs
} from './waves_summary';
