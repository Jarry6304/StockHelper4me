// run_environment.rs — 5 + 1 environment cores dispatch
// (從 main.rs v3.5 R4 C8 抽出)
//
// 6 environment cores:taiex / us_market / exchange_rate / fear_greed /
// market_margin / business_indicator(月頻)。
// 各 core 失敗只標 loader_err 不中斷其他;對齊 cores_overview §7.5 dirty 契約。

use sqlx::postgres::PgPool;

use crate::dispatcher::dispatch_indicator;
use crate::summary::{loader_err_summary, CoreRunSummary};
use crate::workflow::CoreFilter;

pub async fn run_market_cores(
    pool: &PgPool,
    write: bool,
    filter: &CoreFilter,
    summary: &mut Vec<CoreRunSummary>,
) {
    // 環境 cores 各自 warmup 不一,給 5 年(~1825 天)足量歷史
    const ENV_LOOKBACK_DAYS: i32 = 365 * 5;

    // 1. taiex_core
    if filter.is_enabled("taiex_core") {
        match environment_loader::load_taiex(pool, ENV_LOOKBACK_DAYS).await {
            Ok(series) => {
                let core = taiex_core::TaiexCore::new();
                summary.push(
                    dispatch_indicator(pool, &core, &series, taiex_core::TaiexParams::default(), write)
                        .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "taiex_core", "_index_taiex_", "load_taiex", &e,
            )),
        }
    }

    // 2. us_market_core
    if filter.is_enabled("us_market_core") {
        match environment_loader::load_us_market_combined(pool, ENV_LOOKBACK_DAYS).await {
            Ok(combined) => {
                let core = us_market_core::UsMarketCore::new();
                summary.push(
                    dispatch_indicator(
                        pool, &core, &combined,
                        us_market_core::UsMarketParams::default(),
                        write,
                    )
                    .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "us_market_core", "_index_us_market_", "load_us_market_combined", &e,
            )),
        }
    }

    // 3. exchange_rate_core(USD/TWD,後續 Params currency_pairs 多幣別留 follow-up)
    if filter.is_enabled("exchange_rate_core") {
        match environment_loader::load_exchange_rate(pool, "USD", ENV_LOOKBACK_DAYS).await {
            Ok(series) => {
                let core = exchange_rate_core::ExchangeRateCore::new();
                summary.push(
                    dispatch_indicator(
                        pool, &core, &series,
                        exchange_rate_core::ExchangeRateParams::default(),
                        write,
                    )
                    .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "exchange_rate_core", "_global_", "load_exchange_rate", &e,
            )),
        }
    }

    // 4. fear_greed_core
    if filter.is_enabled("fear_greed_core") {
        match environment_loader::load_fear_greed(pool, ENV_LOOKBACK_DAYS).await {
            Ok(series) => {
                let core = fear_greed_core::FearGreedCore::new();
                summary.push(
                    dispatch_indicator(
                        pool, &core, &series,
                        fear_greed_core::FearGreedParams::default(),
                        write,
                    )
                    .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "fear_greed_core", "_global_", "load_fear_greed", &e,
            )),
        }
    }

    // 5. market_margin_core
    if filter.is_enabled("market_margin_core") {
        match environment_loader::load_market_margin(pool, ENV_LOOKBACK_DAYS).await {
            Ok(series) => {
                let core = market_margin_core::MarketMarginCore::new();
                summary.push(
                    dispatch_indicator(
                        pool, &core, &series,
                        market_margin_core::MarketMarginParams::default(),
                        write,
                    )
                    .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "market_margin_core", "_market_", "load_market_margin", &e,
            )),
        }
    }

    // 6. business_indicator_core(月頻;Silver 端 sentinel `_market_`,Core 端保留字 `_index_business_`)
    if filter.is_enabled("business_indicator_core") {
        match environment_loader::load_business_indicator(pool, ENV_LOOKBACK_DAYS).await {
            Ok(series) => {
                let core = business_indicator_core::BusinessIndicatorCore::new();
                summary.push(
                    dispatch_indicator(
                        pool, &core, &series,
                        business_indicator_core::BusinessIndicatorParams::default(),
                        write,
                    )
                    .await,
                );
            }
            Err(e) => summary.push(loader_err_summary(
                "business_indicator_core", "_index_business_", "load_business_indicator", &e,
            )),
        }
    }

    // 7. commodity_macro_core(v3.21;初版 GOLD;對齊 m3Spec/environment_cores.md §十)
    if filter.is_enabled("commodity_macro_core") {
        let params = commodity_macro_core::CommodityMacroParams::default();
        for commodity in &params.commodities {
            match environment_loader::load_commodity_macro(pool, commodity, ENV_LOOKBACK_DAYS).await {
                Ok(series) => {
                    let core = commodity_macro_core::CommodityMacroCore::new();
                    summary.push(
                        dispatch_indicator(
                            pool, &core, &series, params.clone(), write,
                        )
                        .await,
                    );
                }
                Err(e) => summary.push(loader_err_summary(
                    "commodity_macro_core", "_global_", "load_commodity_macro", &e,
                )),
            }
        }
    }
}
