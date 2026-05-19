-- _forest_size_p0_gate.sql — Phase 3 P0 Gate:Neely forest_size 分布
-- 由 test_pipeline.ps1 / test_pipeline.sh 透過 psql -f 載入
--
-- Acceptance(v4.4a 後):
--   - forest_size max ≤ 200(BeamSearchFallback cap 不破)
--   - forest_size p95 < 180
-- 若 max > 200 → 重校 rust_compute/cores/wave/neely_core/src/compaction/beam_search.rs::BeamSearchFallback.k

\echo '== Neely forest_size 分布(p50 / p95 / p99 / max / scenario_count)=='
SELECT
  PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p95,
  PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY jsonb_array_length(snapshot->'scenario_forest')) AS p99,
  MAX(jsonb_array_length(snapshot->'scenario_forest'))                                          AS max_count,
  COUNT(*)                                                                                       AS scenario_count
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND snapshot_date = (SELECT MAX(snapshot_date) FROM structural_snapshots WHERE core_name='neely_core');
