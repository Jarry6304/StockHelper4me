#!/usr/bin/env bash
# v4.32 Golden L3 — TS 契約 codegen 入口(治本:型別衍生自唯一真相源,drift 被抓)。
#
#   Track A.1 Rust neely output.rs(NeelyCoreOutput 等 63 型別)→ frontend/src/contracts/neely/*.ts
#   Track A.2 Rust kalman_filter_core(6 型別:Output/Point/HorizonOutput/Event/Kind/Regime)
#                 → frontend/src/contracts/kalman/*.ts  (v4.x #3)
#             (兩 track 皆 feature-gated ts-rs;生產 build 不開 ts feature → 零風險)
#   Track B   Python Web API 契約(web_api/contracts.py 鏡射 .to_dict() / wire shape)
#                 → frontend/src/contracts/fusion.ts
#
# 前置:`pip install -e ".[web]" pydantic-to-typescript` + `npm i -g json-schema-to-typescript typescript`
# 驗證:(cd frontend && "$(npm root -g)/typescript/bin/tsc" --noEmit -p tsconfig.json)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$REPO/frontend/src/contracts"
mkdir -p "$OUT/neely" "$OUT/kalman"

echo "[1/3] Track A.1 — Rust neely output.rs → TS (ts-rs, feature-gated)"
( cd "$REPO/rust_compute" && TS_RS_EXPORT_DIR="$OUT" cargo test --features ts -p neely_core )

echo "[2/3] Track A.2 — Rust kalman_filter_core → TS (ts-rs, feature-gated)"
( cd "$REPO/rust_compute" && TS_RS_EXPORT_DIR="$OUT" cargo test --features ts -p kalman_filter_core )

echo "[3/3] Track B — Python Web API contracts → TS (pydantic2ts)"
( cd "$REPO" && PYTHONPATH=src pydantic2ts --module web_api.contracts --output "$OUT/fusion.ts" )

echo "Done. TS 契約 → $OUT"
echo "Verify: (cd $REPO/frontend && \"\$(npm root -g)/typescript/bin/tsc\" --noEmit -p tsconfig.json)"
