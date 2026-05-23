"""Phase 2 — promote report_date to real Bronze columns

對齊 user v0.3 區間預測 spec(本 session)+ plan 文件 phase 2。
完整規劃見 /root/.claude/plans/stockhelper4me-serene-thacker.md。

3 張 fundamental Bronze 表加 `report_date DATE` 欄,讓 PIT 層(src/pit/
fundamental.py)能準確判斷「as-of-T 時這筆資料是否已公布」,而不必走
fact_date + heuristic lag 的概率猜測。

設計:
- `monthly_revenue`:GENERATED column from existing `create_time`
  (FinMind 已給的 publish timestamp,raw TEXT 欄,PR #18.5 hotfix 紀錄)。
  STORED + 防禦性 CASE 處理空字串 / 非法格式(回 NULL,由 PIT fallback)。
- `financial_statement` / `business_indicator_tw`:plain DATE,heuristic
  backfill(T+45 / T+27 對齊 _lookahead.py 既有 lag 表)。後續 FinMind probe
  確認 source column(see scripts/probe_finmind_report_date.py)再切真實值。
- 3 張表都加 idx_<table>_report_date 索引(PIT 層常用 WHERE)。

PIT 層 fallback chain(`src/pit/fundamental.py` 升級):
  1. row.report_date 非 NULL → 用此
  2. NULL → fact_date + heuristic lag(11 / 27 / 45)

注意:本 PR **不**動 `_lookahead.py` —— 它讀 `facts.metadata.report_date`
(非 Bronze)。fact-producing cores(revenue_core / business_indicator_core /
financial_statement_core)未來可從 Bronze report_date 讀後寫進 metadata
但屬另一條工作線,本 PR 不含。

Revision ID: b8c9d0e1f2g3
Revises: a7b8c9d0e1f2
Create Date: 2026-05-23
"""

from alembic import op


revision = 'b8c9d0e1f2g3'
down_revision = 'a7b8c9d0e1f2'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # ─────────────────────────────────────────────────────────────────────
    # 1. monthly_revenue:GENERATED from create_time(STORED)
    # ─────────────────────────────────────────────────────────────────────
    # FinMind create_time 是 TIMESTAMPTZ-shaped TEXT(e.g. "2024-01-15 10:30:00")。
    # PR #18.5 hotfix m2n3o4p5q6r7 確認 Bronze 用 TEXT 收(因某些 row 是 "")。
    #
    # PG STORED generated column **要求 IMMUTABLE expression**:
    #   - text::DATE cast 用 DateStyle GUC → STABLE,不能用
    #   - to_date(text, 'YYYY-MM-DD') 顯式格式 → IMMUTABLE,可用
    #
    # CASE 第二支防線濾掉非「YYYY-MM-DD」開頭字串,避免 to_date 對非法輸入噴錯;
    # NULL / "" / 非法 → 結果 NULL,PIT 層 fallback heuristic(date + 11 天)補。
    op.execute(
        """
        ALTER TABLE monthly_revenue
            ADD COLUMN IF NOT EXISTS report_date DATE
            GENERATED ALWAYS AS (
                CASE
                    WHEN create_time IS NULL OR create_time = '' THEN NULL
                    WHEN create_time !~ '^\\d{4}-\\d{2}-\\d{2}' THEN NULL
                    ELSE to_date(substring(create_time, 1, 10), 'YYYY-MM-DD')
                END
            ) STORED
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_monthly_revenue_report_date
            ON monthly_revenue (report_date)
        """
    )

    # ─────────────────────────────────────────────────────────────────────
    # 2. financial_statement:plain DATE + T+45 heuristic backfill
    # ─────────────────────────────────────────────────────────────────────
    # FinMind TaiwanStockFinancialStatements / BalanceSheet / CashFlowsStatement
    # 目前 collector.toml field_rename = {} 沒抓任何 publish-date 欄。
    # 走 heuristic 直到 scripts/probe_finmind_report_date.py 跑出真實欄名。
    op.execute(
        """
        ALTER TABLE financial_statement
            ADD COLUMN IF NOT EXISTS report_date DATE
        """
    )
    op.execute(
        """
        UPDATE financial_statement
           SET report_date = date + INTERVAL '45 days'
         WHERE report_date IS NULL
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_financial_statement_report_date
            ON financial_statement (report_date)
        """
    )

    # ─────────────────────────────────────────────────────────────────────
    # 3. business_indicator_tw:plain DATE + T+27 heuristic backfill
    # ─────────────────────────────────────────────────────────────────────
    # 國發會景氣指標每月 27 號左右發佈上個月資料(_lookahead.py 既有約定)。
    op.execute(
        """
        ALTER TABLE business_indicator_tw
            ADD COLUMN IF NOT EXISTS report_date DATE
        """
    )
    op.execute(
        """
        UPDATE business_indicator_tw
           SET report_date = date + INTERVAL '27 days'
         WHERE report_date IS NULL
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_business_indicator_report_date
            ON business_indicator_tw (report_date)
        """
    )


def downgrade() -> None:
    op.execute("DROP INDEX IF EXISTS idx_business_indicator_report_date")
    op.execute("ALTER TABLE business_indicator_tw DROP COLUMN IF EXISTS report_date")

    op.execute("DROP INDEX IF EXISTS idx_financial_statement_report_date")
    op.execute("ALTER TABLE financial_statement DROP COLUMN IF EXISTS report_date")

    op.execute("DROP INDEX IF EXISTS idx_monthly_revenue_report_date")
    op.execute("ALTER TABLE monthly_revenue DROP COLUMN IF EXISTS report_date")
