"""
field_mapper.py
----------------
欄位映射、Schema 驗證與衍生欄位計算模組。

處理流程（依序執行）：
  0. Schema Validation：比對 API 回傳欄位與 field_rename 定義
  1. field_rename：將原始欄位名 rename 為 DB 欄位名
  2. detail JSON：以 _ 開頭的欄位收集進 detail JSON
  3. event_type：若 api_config 有定義則附加
  4. computed_fields：計算衍生欄位（AF、volume_factor、cash/stock_dividend）
  5. 附加 market="TW"、source="finmind" 等固定欄位
"""

import json
import logging
from typing import Any

from config_loader import ApiConfig

logger = logging.getLogger("collector.field_mapper")


class FieldMapper:
    """
    通用欄位映射器。

    每個 api_config 對應一個 FieldMapper 實例，
    transform() 接受原始 API 回傳資料，輸出準備好入庫的 dict list。
    """

    def transform(
        self,
        api_config: ApiConfig,
        raw_records: list[dict[str, Any]],
    ) -> tuple[list[dict[str, Any]], bool]:
        """
        將 API 回傳的原始資料轉換為 DB 可寫入的格式。

        Args:
            api_config:   API 設定（field_rename、computed_fields 等）
            raw_records:  API 原始回傳資料（list of dict）

        Returns:
            (rows, schema_mismatch)：
              rows           — 處理後的資料列 list，準備好交給 DBWriter.upsert()
              schema_mismatch — True 表示回傳欄位與 field_rename 定義不符（已記錄 WARNING）
        """
        if not raw_records:
            return [], False

        # 步驟 0：Schema Validation（以第一筆資料為樣本）
        # 回傳 True 表示欄位與預期不符，由呼叫端決定是否記錄 schema_mismatch
        schema_mismatch = self._validate_schema(api_config, raw_records[0])

        results = []
        for record in raw_records:
            row: dict[str, Any] = {}
            detail: dict[str, Any] = {}

            # 步驟 1 & 2：rename + detail 收集
            for src_key, value in record.items():
                dest_key = api_config.field_rename.get(src_key, src_key)

                if dest_key.startswith("_"):
                    # _ 開頭的欄位導向 detail JSON（去掉前綴底線作為 key）
                    detail[dest_key.lstrip("_")] = value
                else:
                    row[dest_key] = value

            # 步驟 2b：將收集到的 detail 序列化為 JSON
            if detail:
                row["detail"] = json.dumps(detail, ensure_ascii=False)

            # 步驟 3：附加 event_type
            if api_config.event_type:
                row["event_type"] = api_config.event_type

            # 步驟 4：計算衍生欄位
            if api_config.computed_fields:
                self._compute(api_config, row)

            # 步驟 5：附加固定欄位
            row["market"]  = "TW"
            row["source"]  = "finmind"

            results.append(row)

        return results, schema_mismatch

    # =========================================================================
    # Schema Validation（v1.1 新增）
    # =========================================================================

    def _validate_schema(
        self,
        api_config: ApiConfig,
        sample_record: dict[str, Any],
    ) -> bool:
        """
        比對 API 回傳的第一筆資料欄位與 field_rename 定義的來源欄位。

        策略：
        - 缺少必要欄位 → WARNING + 回傳 True（呼叫端標記 schema_mismatch）
        - API 新增未知欄位 → INFO（純資訊，不影響流程）
        - 不 raise exception：API 變動不應阻斷入庫

        Args:
            api_config:    API 設定
            sample_record: 第一筆回傳資料（用於取得欄位集合）

        Returns:
            True 表示有必要欄位缺失（schema mismatch），False 表示正常
        """
        actual_keys   = set(sample_record.keys())
        expected_keys = set(api_config.field_rename.keys())

        # 缺少的來源欄位 → rename/compute 可能失敗，回傳 True 通知呼叫端
        missing = expected_keys - actual_keys
        if missing:
            logger.warning(
                f"[SchemaValidation] {api_config.name}: "
                f"expected fields missing from API response: {missing}. "
                f"Data will be ingested but computed_fields may be incorrect."
            )
            return True

        # API 新增未知欄位 → 純資訊記錄，不視為 mismatch
        known_keys = expected_keys | {"date", "stock_id", "stock_name"}
        novel      = actual_keys - known_keys
        if novel:
            logger.info(
                f"[SchemaValidation] {api_config.name}: "
                f"novel fields detected in API response: {novel}"
            )

        return False

    # =========================================================================
    # Computed Fields 計算
    # =========================================================================

    def _compute(self, api_config: ApiConfig, row: dict[str, Any]) -> None:
        """
        依 api_config.computed_fields 計算衍生欄位。
        規則來自 tw_stock_architecture_review_v1.1 §3.4。

        支援的計算欄位：
          adjustment_factor：價格調整因子（before_price / after_price）
          volume_factor：    成交量調整因子
          cash_dividend：    現金股利（從 dividend 事件拆分）
          stock_dividend：   股票股利（從 dividend 事件拆分）

        Args:
            api_config: API 設定（含 computed_fields 清單）
            row:        已完成 rename 的資料列（in-place 修改）
        """
        fields = api_config.computed_fields

        # ── 計算 adjustment_factor（價格調整因子）
        if "adjustment_factor" in fields:
            bp = row.get("before_price")
            ap = row.get("after_price")
            if bp and ap and float(ap) != 0:
                row["adjustment_factor"] = float(bp) / float(ap)
            else:
                row["adjustment_factor"] = 1.0
                if bp is not None or ap is not None:
                    logger.warning(
                        f"Cannot compute AF: before={bp}, after={ap}. "
                        f"stock={row.get('stock_id')}, date={row.get('date')}"
                    )

        # ── 計算 volume_factor（成交量調整因子）
        if "volume_factor" in fields:
            et = row.get("event_type", "")
            if et == "dividend":
                # 除權息不影響股本，volume_factor = 1.0
                row["volume_factor"] = 1.0
            else:
                # 減資、分割等：成交量因子 = after_price / before_price
                bp = row.get("before_price", 0)
                ap = row.get("after_price", 0)
                row["volume_factor"] = float(ap) / float(bp) if bp != 0 else 1.0

        # ── 計算 cash_dividend / stock_dividend（除權息事件）
        if "cash_dividend" in fields or "stock_dividend" in fields:
            self._split_dividend(row)

    def _split_dividend(self, row: dict[str, Any]) -> None:
        """
        從 detail JSON 拆分 cash_dividend 與 stock_dividend。

        事件子類型（event_subtype）來自 TaiwanStockDividendResult 的
        stock_or_cache_dividend 欄位，經 field_rename 後存入 detail JSON。

        子類型對應：
          "除息" / "息" → 純現金股利
          "除權" / "權" → 純股票股利
          "權息"        → 混合（需 dividend_policy_merge post-process 拆分）
          其他          → 無法判斷，設為 NULL

        Args:
            row: 已完成 rename 的資料列（in-place 修改）
        """
        detail  = json.loads(row.get("detail", "{}"))
        subtype = detail.get("event_subtype", "")
        combined = detail.get("combined_dividend", 0.0)

        if combined is None:
            combined = 0.0

        if subtype in ("除息", "息"):
            row["cash_dividend"]  = float(combined)
            row["stock_dividend"] = 0.0
        elif subtype in ("除權", "權"):
            row["cash_dividend"]  = 0.0
            row["stock_dividend"] = float(combined)
        elif subtype == "權息":
            # 混合事件：此時無法拆分，留 NULL
            # dividend_policy_merge post-process 會補齊
            row["cash_dividend"]  = None
            row["stock_dividend"] = None
        else:
            # 未知子類型，保留 NULL
            row["cash_dividend"]  = None
            row["stock_dividend"] = None
