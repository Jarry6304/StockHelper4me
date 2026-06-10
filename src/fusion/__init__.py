"""Fusion Layer — M3 唯一對外資料層(aggregation_layer 的繼任者)。

對齊 m3Spec/fusion_layer.md v1.0(🔒 LOCK)。

雙端口設計:
- `fusion.raw`         — Raw 端口:既有 `as_of()`,並排呈現不整合
                          (對齊 cores_overview §九)。
- `fusion.<module>`    — Integration 端口:跨 core 整合,不引入新規則
                          (snapshot / key_levels / pattern_scan / stop_loss /
                           market_dashboard / market_events / indicator_assembly)。

LLM / MCP Tools / Dashboard / CLI 皆從 Fusion 出口取資料。

公開面(P2-1):底線模組為套件內部;外部一律從本 `__init__` 或 `fusion.raw`
顯式出口 import。`__all__` = 公開契約(名單以實際外部消費者為準,不多收)。
"""

# 公開面(P2-1):picker(scenario 排序 / degree / invalidation 單一真相源)+
# fibonacci 投影 helpers(_forecast 與 dashboards neely_wave 共用)。
# 用 PEP 562 module __getattr__ **轉發而非綁定**,讓既有測試
# `patch("fusion._picker.*", ...)` 對公開出口 caller 持續生效(同 fusion.raw)。
_PICKER_FORWARDS = frozenset({
    "DEGREE_RANK",
    "canonical_is_invalidated",
    "classify_degree_by_years",
    "degree_rank",
    "effective_degree",
    "power_rating_label",
    "power_rating_sign",
    "power_rating_strength",
})
_FIB_FORWARDS = frozenset({
    "TIMEFRAME_FIB_RANGE",
    "extract_invalidation_price",
    "find_closest_zone",
    "project_range",
})


def __getattr__(name: str):
    if name in _PICKER_FORWARDS:
        from fusion import _picker
        return getattr(_picker, name)
    if name in _FIB_FORWARDS:
        from fusion import _fib_projection
        return getattr(_fib_projection, name)
    raise AttributeError(f"module 'fusion' has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__))


__all__ = sorted(_PICKER_FORWARDS | _FIB_FORWARDS)
