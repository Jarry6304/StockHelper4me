"""
logger_setup.py
----------------
Collector 全域日誌初始化模組。

設計原則：
- 每次執行產生一個日誌檔：logs/collector_YYYYMMDD.log
- 同一天多次執行：追加（append）至同一檔案，不覆蓋
- 同時輸出至 stdout，便於即時觀察
- 使用 Python 標準 logging 模組，無額外依賴
- 由 collector.toml 的 global.log_level 控制級別
"""

import logging
from pathlib import Path
from datetime import date


def setup_logger(log_dir: str, log_level: str) -> logging.Logger:
    """
    初始化 Collector 全域 Logger。
    應在程式啟動時呼叫一次，後續各模組透過
    logging.getLogger("collector.<module_name>") 取用。

    Args:
        log_dir:   日誌目錄路徑（相對或絕對皆可）
        log_level: 日誌級別字串，如 "DEBUG" / "INFO" / "WARNING" / "ERROR"

    Returns:
        根 logger（name="collector"）
    """
    # 建立日誌目錄（如不存在）
    log_path = Path(log_dir)
    log_path.mkdir(parents=True, exist_ok=True)

    # 日誌檔以日期命名，同一天多次執行皆追加至同一檔
    log_file = log_path / f"collector_{date.today().strftime('%Y%m%d')}.log"

    # 將字串轉換為 logging 級別常數，預設 INFO
    level = getattr(logging, log_level.upper(), logging.INFO)

    # 統一的日誌格式：時間 [級別] 模組名稱: 訊息
    formatter = logging.Formatter(
        fmt="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    # 取得根 logger（子模組透過 "collector.xxx" 繼承此設定）
    root_logger = logging.getLogger("collector")
    root_logger.setLevel(level)

    # 清除舊 handler，避免重複初始化時產生重複輸出
    root_logger.handlers.clear()

    # Handler 1：寫入日誌檔（追加模式）
    file_handler = logging.FileHandler(log_file, encoding="utf-8", mode="a")
    file_handler.setFormatter(formatter)
    root_logger.addHandler(file_handler)

    # Handler 2：同時輸出至 stdout（即時觀察用）
    stream_handler = logging.StreamHandler()
    stream_handler.setFormatter(formatter)
    root_logger.addHandler(stream_handler)

    root_logger.info(f"Logger initialized. level={log_level}, file={log_file}")
    return root_logger
