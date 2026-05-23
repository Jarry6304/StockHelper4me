"""Phase 2 — promote report_date to real Bronze columns

對齊 user v0.3 區間預測 spec(本 session)+ plan 文件 phase 2。
完整規劃見 /root/.claude/plans/stockhelper4me-serene-thacker.md。

3 張 fundamental Bronze 表加 `report_date DATE` 欄,讓 PIT 層(src/pit/
fundamental.py)能準確判斷「as-of-T 時這筆資料是否已公布」,而不必走
fact_date + heuristic lag 的概率猜測。

設計:
- `monthly_revenue`:plain DATE + BEFORE INSERT/UPDATE trigger,從 `create_time`
  TEXT 派生(FinMind 給的 publish timestamp,raw 留原始 TEXT;PR #18.5 hotfix
  紀錄某些 row 是 "")。**不**用 GENERATED column 因為 PG IMMUTABLE 限制 ——
  `text::DATE` cast 走 DateStyle GUC、`to_date(text,fmt)` 走 lc_time GUC,
  兩者都是 STABLE 不是 IMMUTABLE,放 GENERATED 會被 PG 拒。trigger 路徑沒有
  IMMUTABLE 限制(plpgsql 可用 STABLE/VOLATILE 函式)→ 用 trigger。
- `financial_statement` / `business_indicator_tw`:plain DATE,heuristic
  backfill(T+45 / T+27 對齊 _lookahead.py 既有 lag 表)。User probe 確認
  FinMind 對這兩個 dataset 不暴露 publish-date 欄(scripts/probe_finmind_report_date.py
  跑出 2026-05-23),heuristic 是 best-available ground truth,永久保留。
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
    # 1. monthly_revenue:plain DATE + BEFORE INSERT/UPDATE trigger
    # ─────────────────────────────────────────────────────────────────────
    # FinMind create_time 是 TIMESTAMPTZ-shaped TEXT(e.g. "2024-01-15 10:30:00"),
    # PR #18.5 hotfix m2n3o4p5q6r7 確認 Bronze 用 TEXT 收(某些 row 是 "")。
    #
    # PG STORED generated column 要求 IMMUTABLE expression — text::DATE 走
    # DateStyle GUC、to_date(text,fmt) 走 lc_time GUC,**兩者都 STABLE 不 IMMUTABLE**,
    # 放 GENERATED 會被 PG 拒(InvalidObjectDefinition: generation expression
    # is not immutable)。改走 plain column + BEFORE trigger:trigger 沒有
    # IMMUTABLE 限制,可自由用 STABLE/VOLATILE 函式。

    # 1a. 加 plain DATE 欄
    op.execute("ALTER TABLE monthly_revenue ADD COLUMN IF NOT EXISTS report_date DATE")

    # 1b. 一次性 backfill 既有 row
    #     防禦性 CASE 濾掉空字串 / 非法格式 → 結果 NULL,PIT fallback heuristic 補。
    op.execute(
        """
        UPDATE monthly_revenue
           SET report_date = (substring(create_time, 1, 10))::DATE
         WHERE create_time IS NOT NULL
           AND create_time != ''
           AND create_time ~ '^\\d{4}-\\d{2}-\\d{2}'
           AND report_date IS NULL
        """
    )

    # 1c. trigger function:給 INSERT/UPDATE 自動填 report_date
    op.execute(
        """
        CREATE OR REPLACE FUNCTION trg_monthly_revenue_set_report_date()
        RETURNS TRIGGER AS $$
        BEGIN
            IF NEW.create_time IS NOT NULL
               AND NEW.create_time != ''
               AND NEW.create_time ~ '^\\d{4}-\\d{2}-\\d{2}'
            THEN
                NEW.report_date := (substring(NEW.create_time, 1, 10))::DATE;
            ELSE
                NEW.report_date := NULL;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql
        """
    )

    # 1d. 掛 BEFORE INSERT OR UPDATE trigger
    op.execute(
        "DROP TRIGGER IF EXISTS trg_monthly_revenue_report_date ON monthly_revenue"
    )
    op.execute(
        """
        CREATE TRIGGER trg_monthly_revenue_report_date
            BEFORE INSERT OR UPDATE ON monthly_revenue
            FOR EACH ROW
            EXECUTE FUNCTION trg_monthly_revenue_set_report_date()
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
    op.execute("DROP TRIGGER IF EXISTS trg_monthly_revenue_report_date ON monthly_revenue")
    op.execute("DROP FUNCTION IF EXISTS trg_monthly_revenue_set_report_date()")
    op.execute("ALTER TABLE monthly_revenue DROP COLUMN IF EXISTS report_date")
