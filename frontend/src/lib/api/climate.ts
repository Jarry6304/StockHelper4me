import type { ClimateFusion } from '$contracts/fusion';
import { apiGet, toIsoDate, type FetchOptions } from './client';

export interface GetClimateArgs {
  asOf: Date | string;
}

export function getMarketClimate(
  args: GetClimateArgs,
  opts?: FetchOptions
): Promise<ClimateFusion> {
  const qs = new URLSearchParams({ as_of: toIsoDate(args.asOf) });
  return apiGet<ClimateFusion>(`/market/climate?${qs}`, opts);
}
