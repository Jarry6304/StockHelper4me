"""Fusion Layer — M3 唯一對外資料層(aggregation_layer 的繼任者)。

對齊 m3Spec/fusion_layer.md v1.0(🔒 LOCK)。

雙端口設計:
- `fusion.raw`         — Raw 端口:既有 `as_of()`,並排呈現不整合
                          (對齊 cores_overview §九)。
- `fusion.<module>`    — Integration 端口:跨 core 整合,不引入新規則
                          (snapshot / key_levels / pattern_scan / stop_loss /
                           market_dashboard / market_events / indicator_assembly)。

LLM / MCP Tools / Dashboard / CLI 皆從 Fusion 出口取資料。
"""
