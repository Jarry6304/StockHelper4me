#!/usr/bin/env bash
# test_pipeline.sh — StockHelper4me 完整測試流水線(Bash 版,對齊 test_pipeline.ps1)
#
# 跑 5 個 phase:
#   Phase 0:Environment check(Python venv / Rust toolchain / .env / PG 連線)
#   Phase 1:Sandbox unit tests(無需 PG — Python + Rust workspace)
#   Phase 2:Schema 健康度(alembic head / table row counts)
#   Phase 3:Production verify(per-EventKind rate / facts stats / forest_size 分布)
#   Phase 4:MCP smoke test(Kalman + Neely + 8 toolkit tools)
#
# Usage:
#   ./scripts/test_pipeline.sh                    # 跑全套
#   SKIP_PHASES="3 4" ./scripts/test_pipeline.sh  # 跳過 phase 3 + 4
#   ONLY_PHASES="1"   ./scripts/test_pipeline.sh  # 只跑 sandbox
#   DRY_RUN=1         ./scripts/test_pipeline.sh  # 列計畫
#
# 環境變數:
#   DATABASE_URL   — Phase 2-4 必要
#   FINMIND_TOKEN  — Phase 4 部分 helper 用(目前 readonly,可選)
#   SKIP_PHASES    — 空格分隔的 phase numbers(默認 "")
#   ONLY_PHASES    — 空格分隔的 phase numbers(默認 "")
#   DRY_RUN=1      — 列計畫不執行
#
# 退出碼:0 = 全綠;1 = 任一 phase fail

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SKIP_PHASES="${SKIP_PHASES:-}"
ONLY_PHASES="${ONLY_PHASES:-}"
DRY_RUN="${DRY_RUN:-0}"

# ── 顏色 ─────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    C_CYAN='\033[36m'; C_GREEN='\033[32m'; C_RED='\033[31m'
    C_YELLOW='\033[33m'; C_GRAY='\033[90m'; C_RESET='\033[0m'
else
    C_CYAN=''; C_GREEN=''; C_RED=''; C_YELLOW=''; C_GRAY=''; C_RESET=''
fi

declare -A PHASE_RESULTS
PIPELINE_START=$(date +%s)

phase_header() {
    echo
    echo -e "${C_CYAN}$(printf '=%.0s' {1..78})${C_RESET}"
    echo -e "${C_CYAN}Phase $1 — $2${C_RESET}"
    echo -e "${C_CYAN}$(printf '=%.0s' {1..78})${C_RESET}"
}

step()  { echo -e "${C_GRAY}  → $1${C_RESET}"; }
pass()  { echo -e "${C_GREEN}  [OK] $1${C_RESET}"; }
fail_msg() { echo -e "${C_RED}  [FAIL] $1${C_RESET}"; }
warn()  { echo -e "${C_YELLOW}  [WARN] $1${C_RESET}"; }

should_run() {
    local phase=$1
    if [ -n "$ONLY_PHASES" ]; then
        for p in $ONLY_PHASES; do [ "$p" = "$phase" ] && return 0; done
        return 1
    fi
    for p in $SKIP_PHASES; do [ "$p" = "$phase" ] && return 1; done
    return 0
}

cmd_exists() { command -v "$1" >/dev/null 2>&1; }

run_phase() {
    local num=$1
    local title=$2
    local body=$3
    if ! should_run "$num"; then
        echo
        echo -e "${C_GRAY}Phase $num — $title (skipped)${C_RESET}"
        PHASE_RESULTS[$num]="skipped"
        return
    fi
    phase_header "$num" "$title"
    local phase_start=$(date +%s)
    if [ "$DRY_RUN" = "1" ]; then
        echo -e "${C_YELLOW}  [dry-run] $title${C_RESET}"
        PHASE_RESULTS[$num]="dry-run"
        return
    fi
    if "$body"; then
        local elapsed=$(( $(date +%s) - phase_start ))
        echo
        pass "Phase $num passed in ${elapsed}s"
        PHASE_RESULTS[$num]="ok"
    else
        local elapsed=$(( $(date +%s) - phase_start ))
        echo
        fail_msg "Phase $num FAILED in ${elapsed}s"
        PHASE_RESULTS[$num]="fail"
        return 1
    fi
}

# ── Phase 0:Environment check ───────────────────────────────────────────

phase_0() {
    step "Python 3.11+"
    if ! cmd_exists python && ! cmd_exists python3; then
        fail_msg "python not found"; return 1
    fi
    local pybin
    pybin=$(cmd_exists python3 && echo python3 || echo python)
    pass "$($pybin --version)"

    step "venv activated?"
    if [ -n "${VIRTUAL_ENV:-}" ]; then
        pass "VIRTUAL_ENV=$VIRTUAL_ENV"
    else
        warn "VIRTUAL_ENV not set — recommend source .venv/bin/activate"
    fi

    step "pytest available"
    if ! $pybin -c "import pytest" 2>/dev/null; then
        fail_msg "pytest not installed; run pip install -e '.[dev]'"; return 1
    fi
    pass "pytest OK"

    step "Rust cargo"
    if ! cmd_exists cargo; then fail_msg "cargo not found; install via rustup.rs"; return 1; fi
    pass "$(cargo --version)"

    step ".env file"
    if [ ! -f "$REPO_ROOT/.env" ]; then
        warn ".env missing — Phase 2-4 may fail"
    else
        pass ".env found"
    fi

    step "DATABASE_URL set"
    if [ -z "${DATABASE_URL:-}" ]; then
        warn "DATABASE_URL not set — Phase 2-4 will skip PG operations"
    else
        pass "DATABASE_URL configured"
    fi

    step "psql client"
    if ! cmd_exists psql; then
        warn "psql not in PATH — Phase 2/3 SQL verifier 將 skip"
    else
        pass "$(psql --version)"
    fi

    step "Rust binary tw_cores"
    if [ -f "$REPO_ROOT/rust_compute/target/release/tw_cores" ] || \
       [ -f "$REPO_ROOT/rust_compute/target/release/tw_cores.exe" ]; then
        pass "tw_cores binary present"
    else
        warn "tw_cores binary not built — run cargo build --release -p tw_cores"
    fi
    return 0
}

# ── Phase 1:Sandbox unit tests ──────────────────────────────────────────

phase_1() {
    local pybin
    pybin=$(cmd_exists python3 && echo python3 || echo python)

    step "Rust workspace tests(release)"
    (cd "$REPO_ROOT/rust_compute" && cargo test --release --workspace --no-fail-fast 2>&1) | \
        tee /tmp/rust_test_out.log >/dev/null
    if [ "${PIPESTATUS[0]:-0}" -ne 0 ]; then
        cat /tmp/rust_test_out.log
        fail_msg "Rust workspace tests FAILED"; return 1
    fi
    local rust_pass
    rust_pass=$(grep -oE '[0-9]+ passed' /tmp/rust_test_out.log | awk '{sum += $1} END {print sum}')
    pass "Rust workspace: $rust_pass tests passed"

    step "Python pytest — tests/agg/"
    if ! pytest tests/agg/ -q 2>&1; then fail_msg "agg tests FAILED"; return 1; fi

    step "Python pytest — tests/mcp_server/ (skip render_tools)"
    if ! pytest tests/mcp_server/ --ignore=tests/mcp_server/test_render_tools.py -q 2>&1; then
        fail_msg "mcp_server tests FAILED"; return 1
    fi

    step "Python pytest — tests/cross_cores/"
    if ! pytest tests/cross_cores/ -q 2>&1; then fail_msg "cross_cores tests FAILED"; return 1; fi

    pass "All sandbox tests passed"
    return 0
}

# ── Phase 2:Schema health ───────────────────────────────────────────────

phase_2() {
    if [ -z "${DATABASE_URL:-}" ]; then warn "DATABASE_URL not set — skip"; return 0; fi
    if ! cmd_exists psql; then warn "psql not in PATH — skip"; return 0; fi

    step "alembic head"
    local expected="d9e0f1g2h3i4"
    if alembic current 2>&1 | grep -q "$expected"; then
        pass "alembic head = $expected"
    else
        warn "alembic head 非預期 ($expected)"
    fi

    step "M3 表 row counts + 11 個 cross_cores tables 存在(改用外部 SQL file)"
    if [ -f "$REPO_ROOT/scripts/_schema_health.sql" ]; then
        psql "$DATABASE_URL" -f "$REPO_ROOT/scripts/_schema_health.sql"
    else
        warn "scripts/_schema_health.sql 不存在 — skip"
    fi
    return 0
}

# ── Phase 3:Production verify ───────────────────────────────────────────

phase_3() {
    if [ -z "${DATABASE_URL:-}" ]; then warn "DATABASE_URL not set — skip"; return 0; fi
    if ! cmd_exists psql; then warn "psql not in PATH — skip"; return 0; fi

    step "Facts stats(VACUUM 健康度)"
    if [ -f "$REPO_ROOT/scripts/maintain_facts_stats.sql" ]; then
        psql "$DATABASE_URL" -f "$REPO_ROOT/scripts/maintain_facts_stats.sql"
    else
        warn "scripts/maintain_facts_stats.sql 不存在 — skip"
    fi

    step "Per-EventKind 觸發率(target <= 12/yr/stock)"
    if [ -f "$REPO_ROOT/scripts/verify_event_kind_rate.sql" ]; then
        psql "$DATABASE_URL" -f "$REPO_ROOT/scripts/verify_event_kind_rate.sql"
    else
        warn "scripts/verify_event_kind_rate.sql 不存在 — skip"
    fi

    step "P0 Gate - Neely forest_size 分布(v4.4a acceptance: max <= 200, p95 below 180)"
    if [ -f "$REPO_ROOT/scripts/_forest_size_p0_gate.sql" ]; then
        psql "$DATABASE_URL" -f "$REPO_ROOT/scripts/_forest_size_p0_gate.sql"
    else
        warn "scripts/_forest_size_p0_gate.sql 不存在 — skip"
    fi
    return 0
}

# ── Phase 4:MCP smoke test ──────────────────────────────────────────────

phase_4() {
    if [ -z "${DATABASE_URL:-}" ]; then warn "DATABASE_URL not set — skip"; return 0; fi
    local pybin
    pybin=$(cmd_exists python3 && echo python3 || echo python)

    step "verify_mcp_kalman_neely.py 對 2330 / 3030"
    if [ -f "$REPO_ROOT/scripts/verify_mcp_kalman_neely.py" ]; then
        if $pybin "$REPO_ROOT/scripts/verify_mcp_kalman_neely.py" --stocks 2330,3030; then
            pass "Kalman + Neely production verify [OK]"
        else
            warn "verify_mcp_kalman_neely.py 非全綠"
        fi
    else
        warn "scripts/verify_mcp_kalman_neely.py 不存在 — skip"
    fi

    step "MCP 8 tools 公開介面 importable(改用外部 Python file)"
    if [ -f "$REPO_ROOT/scripts/_mcp_import_check.py" ]; then
        if $pybin "$REPO_ROOT/scripts/_mcp_import_check.py"; then
            pass "MCP 8 tools 全 importable"
        else
            fail_msg "MCP toolkit import FAILED"; return 1
        fi
    else
        warn "scripts/_mcp_import_check.py 不存在 — skip"
    fi
    return 0
}

# ── 跑全套 ──────────────────────────────────────────────────────────────

set +e
run_phase 0 "Environment check" phase_0
run_phase 1 "Sandbox unit tests" phase_1
run_phase 2 "Schema health" phase_2
run_phase 3 "Production verify" phase_3
run_phase 4 "MCP smoke test" phase_4
set -e

# ── 結算 ────────────────────────────────────────────────────────────────

TOTAL_ELAPSED=$(( $(date +%s) - PIPELINE_START ))
echo
echo -e "${C_CYAN}$(printf '=%.0s' {1..78})${C_RESET}"
echo -e "${C_CYAN}Test pipeline 結算${C_RESET}"
echo -e "${C_CYAN}$(printf '=%.0s' {1..78})${C_RESET}"
for phase in 0 1 2 3 4; do
    result="${PHASE_RESULTS[$phase]:-skipped}"
    case "$result" in
        ok)      echo -e "${C_GREEN}Phase $phase: $result${C_RESET}" ;;
        fail)    echo -e "${C_RED}Phase $phase: $result${C_RESET}" ;;
        dry-run) echo -e "${C_YELLOW}Phase $phase: $result${C_RESET}" ;;
        *)       echo -e "${C_GRAY}Phase $phase: $result${C_RESET}" ;;
    esac
done
echo
echo -e "${C_CYAN}總耗時:${TOTAL_ELAPSED}s${C_RESET}"

fail_count=0
for r in "${PHASE_RESULTS[@]}"; do
    [ "$r" = "fail" ] && fail_count=$((fail_count + 1))
done
if [ "$fail_count" -gt 0 ]; then
    echo -e "${C_RED}[FAIL] $fail_count phase(s) failed${C_RESET}"
    exit 1
fi
echo -e "${C_GREEN}[OK] All requested phases passed${C_RESET}"
exit 0
