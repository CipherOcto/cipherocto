//! Rules engine stub. Phase 1: read-only empty list. Phase 4 will introduce
//! `arc_swap::ArcSwap<Ruleset>`, the matcher pool, and the rule_draft →
//! rule_approved flow.

#[derive(Debug, Clone, Default)]
pub struct RulesView {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

impl RulesView {
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }
    pub fn list(&self) -> &[Rule] {
        &self.rules
    }
    pub fn get(&self, _id: &str) -> Option<&Rule> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_view() {
        let v = RulesView::empty();
        assert!(v.list().is_empty());
        assert!(v.get("anything").is_none());
    }
}
