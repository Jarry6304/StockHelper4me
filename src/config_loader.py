"""
config_loader.py
-----------------
TOML 設定檔解析與驗證模組。

負責：
1. 讀取 collector.toml 與 stock_list.toml
2. 驗證各欄位合法性（7 條規則）
3. 提供型別化的設定物件供其他模組使用
"""

import logging
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger("collector.config_loader")

# 合法的 param_mode 值
VALID_PARAM_MODES = {
    "all_market",       # 不需 data_id；start_date (+ end_date)
    "all_market_no_id", # 同上，語意明示「無 data_id」
    "per_stock",        # data_id + start_date + end_date
    "per_stock_no_end", # data_id + start_date（無 end_date）
    "per_stock_fixed",  # 同 per_stock 但 data_id 來自 fixed_ids，不走 stock_list
}

# Phase 0：trading_calendar 預載入（其他 Phase 的 trading_dates 過濾依賴此表）
# Phase 4 在 phase_executor 特殊處理，不使用 [[api]] 定義
VALID_PHASES = {0, 1, 2, 3, 5, 6}


# =============================================================================
# 資料類別：API 設定項目
# =============================================================================

@dataclass
class ApiConfig:
    """對應 collector.toml 中一個 [[api]] entry 的設定"""
    name: str
    dataset: str
    param_mode: str
    target_table: str
    phase: int
    enabled: bool
    is_backer: bool
    segment_days: int
    notes: str = ""

    # 選填欄位
    event_type: str | None = None
    field_rename: dict[str, str] = field(default_factory=dict)
    detail_fields: list[str] = field(default_factory=list)
    computed_fields: list[str] = field(default_factory=list)
    # v1.4 後 fixed_ids 為主要欄位名；fixed_stock_ids 保留作 alias 供舊版讀取
    fixed_ids: list[str] | None = None
    fixed_stock_ids: list[str] | None = None
    merge_strategy: str | None = None
    post_process: str | None = None
    # 個別 API 可覆寫 global.backfill_start_date（例：減資資料 2020 起即可）
    backfill_start_override: str | None = None

    # Phase E 新增：聚合策略與財報類型
    # aggregation：指定 aggregators.py 中的聚合策略
    #   "pivot_institutional"        → 三大法人 per-stock pivot
    #   "pivot_institutional_market" → 全市場三大法人 pivot
    #   "pack_financial"             → 財報科目打包（需搭配 stmt_type）
    #   "pack_holding_shares"        → 股權分散表打包
    aggregation: str | None = None
    # stmt_type：財報類型，僅 aggregation="pack_financial" 需要
    #   "income" | "balance" | "cashflow"
    stmt_type: str | None = None


@dataclass
class RateLimitConfig:
    """Rate limit 設定"""
    calls_per_hour: int
    burst_size: int
    cooldown_on_429_sec: int
    min_interval_ms: int


@dataclass
class RetryConfig:
    """重試策略設定"""
    max_attempts: int
    backoff_base_sec: int
    backoff_max_sec: int
    retry_on_status: list[int]


@dataclass
class GlobalConfig:
    """collector.toml [global] 區塊"""
    db_path: str
    log_dir: str
    log_level: str
    token: str
    backfill_start_date: str
    rust_binary_path: str
    rate_limit: RateLimitConfig
    retry: RetryConfig


@dataclass
class ExecutionConfig:
    """collector.toml [execution] 區塊"""
    mode: str          # "backfill" | "incremental"
    phases: list[int]
    start_date: str
    resume: bool


@dataclass
class CollectorConfig:
    """全域設定容器，整合 global + execution + api registry"""
    global_cfg: GlobalConfig
    execution: ExecutionConfig
    apis: list[ApiConfig]


# =============================================================================
# 資料類別：stock_list.toml
# =============================================================================

@dataclass
class StockListConfig:
    """stock_list.toml 全部設定"""
    source_mode: str           # "db" | "file" | "both"
    market_type: list[str]
    exclude_etf: bool
    exclude_warrant: bool
    exclude_tdr: bool
    exclude_delisted: bool
    min_listing_days: int
    static_ids: list[str]      # [stocks].ids 靜態清單
    dev_enabled: bool


# =============================================================================
# 主要載入函式
# =============================================================================

def load_collector_config(config_path: str = "config/collector.toml") -> CollectorConfig:
    """
    載入並驗證 collector.toml。

    Args:
        config_path: TOML 檔案路徑

    Returns:
        CollectorConfig 物件

    Raises:
        FileNotFoundError: 設定檔不存在
        ValueError: 驗證失敗
    """
    path = Path(config_path)
    if not path.exists():
        raise FileNotFoundError(f"找不到設定檔：{config_path}")

    with open(path, "rb") as f:
        raw: dict[str, Any] = tomllib.load(f)

    # 解析各區塊
    global_cfg = _parse_global(raw)
    execution  = _parse_execution(raw)
    apis       = _parse_apis(raw.get("api", []))

    config = CollectorConfig(
        global_cfg=global_cfg,
        execution=execution,
        apis=apis,
    )

    # 執行驗證規則
    _validate(config)

    logger.info(
        f"Config loaded. apis={len(apis)}, "
        f"mode={execution.mode}, phases={execution.phases}"
    )
    return config


def load_stock_list_config(
    stock_list_path: str = "config/stock_list.toml",
) -> StockListConfig:
    """
    載入 stock_list.toml。

    Args:
        stock_list_path: TOML 檔案路徑

    Returns:
        StockListConfig 物件
    """
    path = Path(stock_list_path)
    if not path.exists():
        raise FileNotFoundError(f"找不到股票清單設定：{stock_list_path}")

    with open(path, "rb") as f:
        raw: dict[str, Any] = tomllib.load(f)

    source   = raw.get("source", {})
    filt     = raw.get("filter", {})
    stocks   = raw.get("stocks", {})
    dev      = raw.get("dev", {})

    return StockListConfig(
        source_mode      = source.get("mode", "db"),
        market_type      = filt.get("market_type", ["twse", "otc"]),
        exclude_etf      = filt.get("exclude_etf", False),
        exclude_warrant  = filt.get("exclude_warrant", True),
        exclude_tdr      = filt.get("exclude_tdr", True),
        exclude_delisted = filt.get("exclude_delisted", True),
        min_listing_days = filt.get("min_listing_days", 30),
        static_ids       = stocks.get("ids", []),
        dev_enabled      = dev.get("enabled", False),
    )


# =============================================================================
# 私有輔助函式
# =============================================================================

def _parse_global(raw: dict) -> GlobalConfig:
    """解析 [global] 區塊"""
    g  = raw.get("global", {})
    rl = g.get("rate_limit", {})
    rt = g.get("retry", {})

    return GlobalConfig(
        db_path              = g.get("db_path", "data/tw_stock.db"),
        log_dir              = g.get("log_dir", "logs"),
        log_level            = g.get("log_level", "INFO"),
        token                = g.get("token", ""),
        backfill_start_date  = g.get("backfill_start_date", "2019-01-01"),
        rust_binary_path     = g.get(
            "rust_binary_path",
            "rust_compute/target/release/tw_stock_compute",
        ),
        rate_limit=RateLimitConfig(
            calls_per_hour      = rl.get("calls_per_hour", 1600),
            burst_size          = rl.get("burst_size", 5),
            cooldown_on_429_sec = rl.get("cooldown_on_429_sec", 120),
            min_interval_ms     = rl.get("min_interval_ms", 2250),
        ),
        retry=RetryConfig(
            max_attempts     = rt.get("max_attempts", 3),
            backoff_base_sec = rt.get("backoff_base_sec", 5),
            backoff_max_sec  = rt.get("backoff_max_sec", 60),
            retry_on_status  = rt.get("retry_on_status", [429, 500, 502, 503, 504]),
        ),
    )


def _parse_execution(raw: dict) -> ExecutionConfig:
    """解析 [execution] 區塊，依 mode 讀取對應子區塊"""
    exec_raw = raw.get("execution", {})
    mode     = exec_raw.get("mode", "backfill")

    sub = exec_raw.get(mode, {})
    return ExecutionConfig(
        mode       = mode,
        phases     = sub.get("phases", [1, 2, 3, 4, 5, 6]),
        start_date = sub.get("start_date", raw.get("global", {}).get("backfill_start_date", "2019-01-01")),
        resume     = sub.get("resume", True),
    )


def _parse_apis(api_list: list[dict]) -> list[ApiConfig]:
    """
    將 [[api]] 陣列解析為 ApiConfig 清單。

    向後相容：fixed_ids 與 fixed_stock_ids 會互相填補，下游程式只看任一即可。
    """
    results = []
    for entry in api_list:
        # fixed_ids 是主要欄位名（v1.4+），fixed_stock_ids 是舊名（v1.2/v1.3）
        fixed_ids       = entry.get("fixed_ids")
        fixed_stock_ids = entry.get("fixed_stock_ids")
        canonical_fixed = fixed_ids or fixed_stock_ids

        cfg = ApiConfig(
            name             = entry["name"],
            dataset          = entry["dataset"],
            param_mode       = entry["param_mode"],
            target_table     = entry["target_table"],
            phase            = entry["phase"],
            enabled          = entry.get("enabled", True),
            is_backer        = entry.get("is_backer", False),
            segment_days     = entry.get("segment_days", 0),
            notes            = entry.get("notes", ""),
            event_type       = entry.get("event_type"),
            field_rename     = entry.get("field_rename", {}),
            detail_fields    = entry.get("detail_fields", []),
            computed_fields  = entry.get("computed_fields", []),
            fixed_ids        = canonical_fixed,
            fixed_stock_ids  = canonical_fixed,
            merge_strategy   = entry.get("merge_strategy"),
            post_process     = entry.get("post_process"),
            aggregation      = entry.get("aggregation"),
            stmt_type        = entry.get("stmt_type"),
            backfill_start_override = entry.get("backfill_start_override"),
        )
        results.append(cfg)
    return results


def _validate(config: CollectorConfig) -> None:
    """
    驗證規則（共 7 條）：
    1. dataset 不可重複（除非 target_table 不同）
    2. phase 值必須在合法範圍內
    3. param_mode 必須是四種之一
    4. per_stock 類型若無 fixed_stock_ids，必須依賴 stock_list
    5. 有 computed_fields 的必須包含 adjustment_factor（Phase 2）
    6. segment_days 為 0 或正整數
    7. field_rename 中以 _ 開頭的 value 會導向 detail JSON（僅記錄）
    """
    errors: list[str] = []

    # 規則 1：dataset 唯一性（同 dataset 不同 target_table 除外）
    seen: dict[str, str] = {}  # dataset -> target_table
    for api in config.apis:
        key = api.dataset
        if key in seen and seen[key] != api.target_table:
            errors.append(
                f"規則1：dataset '{api.dataset}' 重複，"
                f"且 target_table 不同（{seen[key]} vs {api.target_table}）"
            )
        seen[key] = api.target_table

    for api in config.apis:
        # 規則 2：phase 合法範圍
        if api.phase not in VALID_PHASES:
            errors.append(
                f"規則2：{api.name} 的 phase={api.phase} 不合法，"
                f"必須是 {sorted(VALID_PHASES)} 之一"
            )

        # 規則 3：param_mode 合法值
        if api.param_mode not in VALID_PARAM_MODES:
            errors.append(
                f"規則3：{api.name} 的 param_mode='{api.param_mode}' 不合法，"
                f"必須是 {VALID_PARAM_MODES} 之一"
            )

        # 規則 4：per_stock 類型需有股票來源
        if (
            api.param_mode in ("per_stock", "per_stock_no_end")
            and not api.fixed_stock_ids
        ):
            # 依賴 stock_list.toml，此處只記錄，不報錯
            pass

        # 規則 5：有 computed_fields 的 Phase 2 API 必須含 adjustment_factor
        if api.computed_fields and api.phase == 2:
            if "adjustment_factor" not in api.computed_fields:
                errors.append(
                    f"規則5：{api.name}（Phase 2）有 computed_fields "
                    f"但缺少 adjustment_factor"
                )

        # 規則 6：segment_days 必須是 0 或正整數
        if api.segment_days < 0:
            errors.append(
                f"規則6：{api.name} 的 segment_days={api.segment_days} 不合法，"
                f"必須 >= 0"
            )

        # 規則 8：per_stock_fixed 必須有非空 fixed_ids
        if api.param_mode == "per_stock_fixed" and not api.fixed_ids:
            errors.append(
                f"規則8：{api.name} 為 per_stock_fixed 但未提供 fixed_ids（或舊欄名 fixed_stock_ids）"
            )

        # 規則 7：field_rename value 以 _ 開頭者記錄（供 field_mapper 識別）
        for src, dest in api.field_rename.items():
            if dest.startswith("_"):
                logger.debug(
                    f"規則7：{api.name}.field_rename[{src}] → {dest}（將導向 detail JSON）"
                )

    if errors:
        msg = "Config 驗證失敗：\n" + "\n".join(f"  - {e}" for e in errors)
        logger.error(msg)
        raise ValueError(msg)

    logger.info(f"Config 驗證通過。共 {len(config.apis)} 個 API entry")
