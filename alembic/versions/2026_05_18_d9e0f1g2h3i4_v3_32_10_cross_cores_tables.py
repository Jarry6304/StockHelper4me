"""v3.32 — 10 new cross_cores factor ranked tables

對齊 plan v3.32:Toolkit A 3 個 / Toolkit B 3 個 / Toolkit C 3 個 / Layer 5 1 個 =
10 個 cross_cores builder 各自一張 `*_ranked_derived` 表(monthly_trigger 是
signals_derived,事件性質)。

表結構對齊 magic_formula_ranked_derived(`y4z5a6b7c8d9`)pattern:
- PK (market, stock_id, date)
- 1-2 個主要 metric 欄 + rank 欄 + is_top_n boolean + excluded_reason
- detail JSONB 給 builder-specific metadata
- is_dirty / dirty_at(預留 dirty queue;orchestrator 暫不消費)

Refs:
  - Chen-Chou-Hsieh 2023 JFM(persistent_momentum)
  - Hung-Lu-Yang 2025 RQFA(revenue_momentum)
  - Sias 2004 RFS / 周賓凰-池祥麟 2014(institutional_concert)
  - Piotroski 2000 JAR(f_score)
  - Ang et al 2009 JFE(low_volatility / long_term_low_vol)
  - Novy-Marx 2013 JFE(industry_adj_gp)
  - Boudoukh 2007 JF(dividend_yield)
  - Jegadeesh-Titman 1993 JF(mom_12_1)

Revision ID: d9e0f1g2h3i4
Revises: c8d9e0f1g2h3
Create Date: 2026-05-18
"""
from alembic import op

revision = "d9e0f1g2h3i4"
down_revision = "c8d9e0f1g2h3"
branch_labels = None
depends_on = None


# 共用 builder rank 表 schema 模板。各 builder 加自己 metric 欄外其餘相同。
_BASE_TAIL = """
    is_top_n          BOOLEAN NOT NULL DEFAULT FALSE,
    excluded_reason   TEXT,
    detail            JSONB,
    is_dirty          BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at          TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
"""


def upgrade() -> None:
    # === Toolkit A:Monthly ===

    # A1 persistent_momentum:6M 連續 2M top decile,skip 1M,hold 6M
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS persistent_momentum_ranked_derived (
            market            TEXT NOT NULL,
            stock_id          TEXT NOT NULL,
            date              DATE NOT NULL,
            return_6m         NUMERIC(10, 6),
            return_12m_1m     NUMERIC(10, 6),
            persistent_months INTEGER,
            momentum_rank     INTEGER,
            universe_size     INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_persistent_momentum_top
            ON persistent_momentum_ranked_derived (market, date, momentum_rank)
            WHERE is_top_n = TRUE
    """)

    # A2 revenue_momentum:月營收 YoY top decile + 過去 3M YoY 連續正
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS revenue_momentum_ranked_derived (
            market               TEXT NOT NULL,
            stock_id             TEXT NOT NULL,
            date                 DATE NOT NULL,
            revenue_yoy_latest   NUMERIC(10, 4),
            consecutive_positive INTEGER,
            revenue_rank         INTEGER,
            universe_size        INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_revenue_momentum_top
            ON revenue_momentum_ranked_derived (market, date, revenue_rank)
            WHERE is_top_n = TRUE
    """)

    # A3 institutional_concert:20D 三大法人同向 + foreign 累積
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS institutional_concert_ranked_derived (
            market                  TEXT NOT NULL,
            stock_id                TEXT NOT NULL,
            date                    DATE NOT NULL,
            concert_days            INTEGER,
            foreign_cumulative_20d  NUMERIC(20, 2),
            shares_outstanding      NUMERIC(20, 0),
            cumulative_pct          NUMERIC(10, 6),
            concert_rank            INTEGER,
            universe_size           INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_institutional_concert_top
            ON institutional_concert_ranked_derived (market, date, concert_rank)
            WHERE is_top_n = TRUE
    """)

    # === Toolkit B:Quarterly ===

    # B1 f_score:Piotroski 9 條件加總
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS f_score_ranked_derived (
            market         TEXT NOT NULL,
            stock_id       TEXT NOT NULL,
            date           DATE NOT NULL,
            f_score        INTEGER,
            profitability  INTEGER,
            leverage       INTEGER,
            efficiency     INTEGER,
            score_rank     INTEGER,
            universe_size  INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_f_score_top
            ON f_score_ranked_derived (market, date, f_score DESC)
            WHERE is_top_n = TRUE
    """)

    # B2 low_volatility:252D 報酬 std bottom quintile
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS low_volatility_ranked_derived (
            market         TEXT NOT NULL,
            stock_id       TEXT NOT NULL,
            date           DATE NOT NULL,
            std_252d       NUMERIC(10, 6),
            vol_rank       INTEGER,
            universe_size  INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_low_vol_top
            ON low_volatility_ranked_derived (market, date, vol_rank)
            WHERE is_top_n = TRUE
    """)

    # B3 industry_adj_gp:(Rev - COGS) / Total Assets − 同產業中位數
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS industry_adj_gp_ranked_derived (
            market               TEXT NOT NULL,
            stock_id             TEXT NOT NULL,
            date                 DATE NOT NULL,
            gross_profitability  NUMERIC(10, 6),
            industry             TEXT,
            industry_median_gp   NUMERIC(10, 6),
            industry_adj_gp      NUMERIC(10, 6),
            gp_rank              INTEGER,
            universe_size        INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_industry_adj_gp_top
            ON industry_adj_gp_ranked_derived (market, date, gp_rank)
            WHERE is_top_n = TRUE
    """)

    # === Toolkit C:Annual ===

    # C1 long_term_low_vol:36M 日報酬 std
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS long_term_low_vol_ranked_derived (
            market         TEXT NOT NULL,
            stock_id       TEXT NOT NULL,
            date           DATE NOT NULL,
            std_36m        NUMERIC(10, 6),
            vol_rank       INTEGER,
            universe_size  INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_long_term_low_vol_top
            ON long_term_low_vol_ranked_derived (market, date, vol_rank)
            WHERE is_top_n = TRUE
    """)

    # C2 dividend_yield:殖利率 ≥ 4% + 12M 報酬 > -20% + 5y 至少 3y 配息
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS dividend_yield_ranked_derived (
            market               TEXT NOT NULL,
            stock_id             TEXT NOT NULL,
            date                 DATE NOT NULL,
            dividend_yield_pct   NUMERIC(10, 4),
            return_12m_pct       NUMERIC(10, 4),
            payout_years_5y      INTEGER,
            yield_rank           INTEGER,
            universe_size        INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_dividend_yield_top
            ON dividend_yield_ranked_derived (market, date, yield_rank)
            WHERE is_top_n = TRUE
    """)

    # C3 mom_12_1:12M-1M cumulative return
    op.execute(f"""
        CREATE TABLE IF NOT EXISTS mom_12_1_ranked_derived (
            market         TEXT NOT NULL,
            stock_id       TEXT NOT NULL,
            date           DATE NOT NULL,
            return_12m_1m  NUMERIC(10, 6),
            mom_rank       INTEGER,
            universe_size  INTEGER,
            {_BASE_TAIL}
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_mom_12_1_top
            ON mom_12_1_ranked_derived (market, date, mom_rank)
            WHERE is_top_n = TRUE
    """)

    # === Layer 5:Monthly Trigger Overlay ===

    # 不是 ranked 表(訊號性質)。每 row 是一個 trigger event。
    op.execute("""
        CREATE TABLE IF NOT EXISTS monthly_trigger_signals_derived (
            market            TEXT NOT NULL,
            stock_id          TEXT NOT NULL,
            date              DATE NOT NULL,
            trigger_type      TEXT NOT NULL,            -- 'positive' | 'negative'
            revenue_yoy_pct   NUMERIC(10, 4),
            institutional_20d NUMERIC(20, 2),
            shares_outstanding NUMERIC(20, 0),
            institutional_pct NUMERIC(10, 6),
            action_hint       TEXT,                     -- 'increase_20pct' | 'decrease_50pct'
            detail            JSONB,
            is_dirty          BOOLEAN NOT NULL DEFAULT FALSE,
            dirty_at          TIMESTAMPTZ,
            PRIMARY KEY (market, stock_id, date, trigger_type)
        )
    """)
    op.execute("""
        CREATE INDEX IF NOT EXISTS idx_monthly_trigger_date
            ON monthly_trigger_signals_derived (market, date, trigger_type)
    """)


def downgrade() -> None:
    for table in (
        "monthly_trigger_signals_derived",
        "mom_12_1_ranked_derived",
        "dividend_yield_ranked_derived",
        "long_term_low_vol_ranked_derived",
        "industry_adj_gp_ranked_derived",
        "low_volatility_ranked_derived",
        "f_score_ranked_derived",
        "institutional_concert_ranked_derived",
        "revenue_momentum_ranked_derived",
        "persistent_momentum_ranked_derived",
    ):
        op.execute(f"DROP TABLE IF EXISTS {table}")
