// triggers.rs — Stage 7:失效條件(硬規則逆向轉譯,對齊 traditional_rules.md L15)
//
// v1:整體形態於價格反向**跌破/突破起點(正統原點)**時失效 —— 對應 R1「浪 2 回撤 ≤ 浪 1 之 100%」
// 的逆向(若反向走勢超越形態原點,則該數法被否決,次選即刻升首選)。方向恆定、價位明確。

use crate::candidates::Candidate;
use crate::output::{Direction, Trigger, TriggerKind, TradRuleId};

pub fn build(c: &Candidate) -> Vec<Trigger> {
    let origin = c.pivots[0].price;
    let (kind, verb) = match c.direction {
        Direction::Up => (TriggerKind::PriceBreakBelow, "跌破"),
        Direction::Down => (TriggerKind::PriceBreakAbove, "突破"),
    };
    vec![Trigger {
        kind,
        price: origin,
        rule_reference: TradRuleId::R1Wave2Retracement,
        note: format!("價格反向{verb}形態起點 {:.2} → 本數法失效(L15 規則逆向)", origin),
    }]
}
