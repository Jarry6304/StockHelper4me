// 編譯期檢查:re-export 強制 tsc 解析整個生成型別樹(drift / 不一致 → tsc 報錯)。
export type { NeelyCoreOutput } from "./neely/NeelyCoreOutput";
export type { Scenario } from "./neely/Scenario";
export type { RuleId } from "./neely/RuleId";
export type { LevelsFusion, ResonanceFusion, ClimateFusion } from "./fusion";
