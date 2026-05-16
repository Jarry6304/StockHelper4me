// run_stock_cores.rs — 17 stock-level cores dispatch
// (從 main.rs v3.5 R4 C8 抽出)
//
// 17 cores:1 Wave(neely) + 8 Indicator(P1) + 8 Indicator(P3) + 3 Pattern(P2 structural)
// + 1 VWAP(P3 anchored)+ 5 Chip + 3 Fundamental + 1 Magic Formula + 1 Kalman = 31 cores 實際上。
// 對齊 cores_overview §五 + §十一 multi-domain layout。

use fact_schema::Timeframe;
use sqlx::postgres::PgPool;

use crate::dispatcher::{dispatch_indicator, dispatch_neely, dispatch_structural};
use crate::summary::{loader_err_summary, CoreRunSummary};
use crate::workflow::CoreFilter;

pub async fn run_stock_cores(
    pool: &PgPool,
    stock_id: &str,
    tf: Timeframe,
    write: bool,
    filter: &CoreFilter,
    summary: &mut Vec<CoreRunSummary>,
) {
    // Lookback 上限給足:6 年日線(覆蓋各 indicator warmup × 1.2 + 充足實際 series)
    const STOCK_LOOKBACK_DAYS: i32 = 365 * 6;
    const STOCK_LOOKBACK_MONTHS: i32 = 6 * 12 + 12;
    const STOCK_LOOKBACK_QUARTERS: i32 = 6 * 4 + 4;

    // ---- 1. Wave: neely_core(走 structural_snapshots,不寫 indicator_values)----
    if filter.is_enabled("neely_core") {
        let mut neely_params = neely_core::NeelyCoreParams::default();
        neely_params.timeframe = tf;
        match ohlcv_loader::load_for_neely(pool, stock_id, &neely_params).await {
            Ok(series) => summary.push(dispatch_neely(pool, stock_id, &series, neely_params, write).await),
            Err(e) => summary.push(loader_err_summary(
                "neely_core", stock_id, "load_for_neely", &e,
            )),
        }
    }

    // ---- 2-19. Indicator(P1 8 + P3 8 + P2 pattern 3 = 19)— 共用 OhlcvSeries ----
    // 19 cores 共用 ohlcv,若全 disabled 可整段 skip(節省 1 個 query)
    let any_indicator_enabled = [
        "macd_core", "rsi_core", "kd_core", "adx_core",
        "ma_core", "bollinger_core", "atr_core", "obv_core",
        "williams_r_core", "cci_core", "keltner_core", "donchian_core",
        "vwap_core", "mfi_core", "coppock_core", "ichimoku_core",
        "support_resistance_core", "candlestick_pattern_core", "trendline_core",
    ].iter().any(|n| filter.is_enabled(n));

    if any_indicator_enabled {
        let ohlcv_result = match tf {
            Timeframe::Daily => ohlcv_loader::load_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await,
            Timeframe::Weekly => ohlcv_loader::load_weekly(pool, stock_id, STOCK_LOOKBACK_DAYS / 7).await,
            Timeframe::Monthly => ohlcv_loader::load_monthly(pool, stock_id, STOCK_LOOKBACK_MONTHS).await,
            Timeframe::Quarterly => Err(anyhow::anyhow!(
                "Quarterly 不適用 OHLCV(season 報表專用 Timeframe);stock_level indicator cores 不該帶 Quarterly"
            )),
        };
        match ohlcv_result {
            Ok(ohlcv) => {
                // 每個 indicator core 獨立 dispatch,失敗不阻塞其他
                if filter.is_enabled("macd_core") {
                    summary.push(dispatch_indicator(pool, &macd_core::MacdCore::new(), &ohlcv, macd_core::MacdParams::default(), write).await);
                }
                if filter.is_enabled("rsi_core") {
                    summary.push(dispatch_indicator(pool, &rsi_core::RsiCore::new(), &ohlcv, rsi_core::RsiParams::default(), write).await);
                }
                if filter.is_enabled("kd_core") {
                    summary.push(dispatch_indicator(pool, &kd_core::KdCore::new(), &ohlcv, kd_core::KdParams::default(), write).await);
                }
                if filter.is_enabled("adx_core") {
                    summary.push(dispatch_indicator(pool, &adx_core::AdxCore::new(), &ohlcv, adx_core::AdxParams::default(), write).await);
                }
                if filter.is_enabled("ma_core") {
                    summary.push(dispatch_indicator(pool, &ma_core::MaCore::new(), &ohlcv, ma_core::MaParams::default(), write).await);
                }
                if filter.is_enabled("bollinger_core") {
                    summary.push(dispatch_indicator(pool, &bollinger_core::BollingerCore::new(), &ohlcv, bollinger_core::BollingerParams::default(), write).await);
                }
                if filter.is_enabled("atr_core") {
                    summary.push(dispatch_indicator(pool, &atr_core::AtrCore::new(), &ohlcv, atr_core::AtrParams::default(), write).await);
                }
                if filter.is_enabled("obv_core") {
                    summary.push(dispatch_indicator(pool, &obv_core::ObvCore::new(), &ohlcv, obv_core::ObvParams::default(), write).await);
                }
                // ---- P3 indicator cores ----
                if filter.is_enabled("williams_r_core") {
                    summary.push(dispatch_indicator(pool, &williams_r_core::WilliamsRCore::new(), &ohlcv, williams_r_core::WilliamsRParams::default(), write).await);
                }
                if filter.is_enabled("cci_core") {
                    summary.push(dispatch_indicator(pool, &cci_core::CciCore::new(), &ohlcv, cci_core::CciParams::default(), write).await);
                }
                if filter.is_enabled("keltner_core") {
                    summary.push(dispatch_indicator(pool, &keltner_core::KeltnerCore::new(), &ohlcv, keltner_core::KeltnerParams::default(), write).await);
                }
                if filter.is_enabled("donchian_core") {
                    summary.push(dispatch_indicator(pool, &donchian_core::DonchianCore::new(), &ohlcv, donchian_core::DonchianParams::default(), write).await);
                }
                if filter.is_enabled("mfi_core") {
                    summary.push(dispatch_indicator(pool, &mfi_core::MfiCore::new(), &ohlcv, mfi_core::MfiParams::default(), write).await);
                }
                if filter.is_enabled("coppock_core") {
                    summary.push(dispatch_indicator(pool, &coppock_core::CoppockCore::new(), &ohlcv, coppock_core::CoppockParams::default(), write).await);
                }
                if filter.is_enabled("ichimoku_core") {
                    summary.push(dispatch_indicator(pool, &ichimoku_core::IchimokuCore::new(), &ohlcv, ichimoku_core::IchimokuParams::default(), write).await);
                }
                // ---- P2 pattern cores(共用 ohlcv,寫 structural_snapshots)----
                if filter.is_enabled("support_resistance_core") {
                    summary.push(dispatch_structural(pool, &support_resistance_core::SupportResistanceCore::new(), &ohlcv, support_resistance_core::SupportResistanceParams::default(), write).await);
                }
                if filter.is_enabled("candlestick_pattern_core") {
                    summary.push(dispatch_structural(pool, &candlestick_pattern_core::CandlestickPatternCore::new(), &ohlcv, candlestick_pattern_core::CandlestickPatternParams::default(), write).await);
                }
                // ---- trendline_core(P2,唯一耦合例外)— 跑 neely_core 取 monowave_series 餵入 ----
                if filter.is_enabled("trendline_core") {
                    use fact_schema::WaveCore;
                    let mut tl_neely_params = neely_core::NeelyCoreParams::default();
                    tl_neely_params.timeframe = tf;
                    match neely_core::NeelyCore::new().compute(&ohlcv, tl_neely_params) {
                        Ok(neely_out) => {
                            let tl_input = trendline_core::TrendlineInput {
                                ohlcv: ohlcv.clone(),
                                monowaves: neely_out.monowave_series.clone(),
                            };
                            summary.push(
                                dispatch_structural(
                                    pool,
                                    &trendline_core::TrendlineCore::new(),
                                    &tl_input,
                                    trendline_core::TrendlineParams::default(),
                                    write,
                                )
                                .await,
                            );
                        }
                        Err(e) => summary.push(loader_err_summary(
                            "trendline_core", stock_id, "neely_monowave", &e,
                        )),
                    }
                }
                // ---- vwap_core(P3,需 anchor_date)— 預設用 series 第一個 bar 的日期 ----
                // v1.34 Round 5:empty series(如 ETF 沒 Silver fwd 資料)silent skip 不算 error
                if filter.is_enabled("vwap_core") {
                    let anchor = ohlcv.bars.first().map(|b| b.date);
                    if let Some(anchor_date) = anchor {
                        let mut vwap_params = vwap_core::VwapParams::default();
                        vwap_params.anchor_date = Some(anchor_date);
                        summary.push(
                            dispatch_indicator(
                                pool, &vwap_core::VwapCore::new(), &ohlcv, vwap_params, write,
                            )
                            .await,
                        );
                    } else {
                        // empty series → no work,記 status=skipped 不報 err
                        summary.push(CoreRunSummary {
                            core: "vwap_core".to_string(),
                            stock_id: stock_id.to_string(),
                            status: "skipped".to_string(),
                            events: 0,
                            iv_written: 0,
                            fact_written: 0,
                            elapsed_ms: 0,
                            error: Some("empty_series:no Silver data for stock".to_string()),
                        });
                    }
                }
            }
            Err(e) => {
                for name in [
                    "macd_core", "rsi_core", "kd_core", "adx_core",
                    "ma_core", "bollinger_core", "atr_core", "obv_core",
                    "williams_r_core", "cci_core", "keltner_core", "donchian_core",
                    "vwap_core", "mfi_core", "coppock_core", "ichimoku_core",
                    "support_resistance_core", "candlestick_pattern_core", "trendline_core",
                ] {
                    if filter.is_enabled(name) {
                        summary.push(loader_err_summary(name, stock_id, "load_daily", &e));
                    }
                }
            }
        }
    }

    // ---- 10. day_trading_core ----
    if filter.is_enabled("day_trading_core") {
        match chip_loader::load_day_trading(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &day_trading_core::DayTradingCore::new(), &series,
                    day_trading_core::DayTradingParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "day_trading_core", stock_id, "load_day_trading", &e,
            )),
        }
    }

    // ---- 11. institutional_core ----
    if filter.is_enabled("institutional_core") {
        match chip_loader::load_institutional_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &institutional_core::InstitutionalCore::new(), &series,
                    institutional_core::InstitutionalParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "institutional_core", stock_id, "load_institutional_daily", &e,
            )),
        }
    }

    // ---- 12. margin_core ----
    if filter.is_enabled("margin_core") {
        match chip_loader::load_margin_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &margin_core::MarginCore::new(), &series,
                    margin_core::MarginParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "margin_core", stock_id, "load_margin_daily", &e,
            )),
        }
    }

    // ---- 13. foreign_holding_core ----
    if filter.is_enabled("foreign_holding_core") {
        match chip_loader::load_foreign_holding(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &foreign_holding_core::ForeignHoldingCore::new(), &series,
                    foreign_holding_core::ForeignHoldingParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "foreign_holding_core", stock_id, "load_foreign_holding", &e,
            )),
        }
    }

    // ---- 14. shareholder_core(週頻 — Params::default() timeframe = Weekly)----
    if filter.is_enabled("shareholder_core") {
        match chip_loader::load_holding_shares_per(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &shareholder_core::ShareholderCore::new(), &series,
                    shareholder_core::ShareholderParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "shareholder_core", stock_id, "load_holding_shares_per", &e,
            )),
        }
    }

    // ---- 15. revenue_core(月頻)----
    if filter.is_enabled("revenue_core") {
        match fundamental_loader::load_monthly_revenue(pool, stock_id, STOCK_LOOKBACK_MONTHS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &revenue_core::RevenueCore::new(), &series,
                    revenue_core::RevenueParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "revenue_core", stock_id, "load_monthly_revenue", &e,
            )),
        }
    }

    // ---- 16. valuation_core(日頻)----
    if filter.is_enabled("valuation_core") {
        match fundamental_loader::load_valuation_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &valuation_core::ValuationCore::new(), &series,
                    valuation_core::ValuationParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "valuation_core", stock_id, "load_valuation_daily", &e,
            )),
        }
    }

    // ---- 17. financial_statement_core(季頻)----
    if filter.is_enabled("financial_statement_core") {
        match fundamental_loader::load_financial_statement(pool, stock_id, STOCK_LOOKBACK_QUARTERS).await
        {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &financial_statement_core::FinancialStatementCore::new(), &series,
                    financial_statement_core::FinancialStatementParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "financial_statement_core", stock_id, "load_financial_statement", &e,
            )),
        }
    }

    // ---- 18. magic_formula_core(日頻;v3.4)----
    if filter.is_enabled("magic_formula_core") {
        match fundamental_loader::load_magic_formula_series(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &magic_formula_core::MagicFormulaCore::new(), &series,
                    magic_formula_core::MagicFormulaParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "magic_formula_core", stock_id, "load_magic_formula_series", &e,
            )),
        }
    }

    // ---- 19. kalman_filter_core(日頻;v3.4)----
    if filter.is_enabled("kalman_filter_core") {
        match ohlcv_loader::load_daily(pool, stock_id, STOCK_LOOKBACK_DAYS).await {
            Ok(series) => summary.push(
                dispatch_indicator(pool, &kalman_filter_core::KalmanFilterCore::new(), &series,
                    kalman_filter_core::KalmanFilterParams::default(), write).await,
            ),
            Err(e) => summary.push(loader_err_summary(
                "kalman_filter_core", stock_id, "load_daily", &e,
            )),
        }
    }
}
