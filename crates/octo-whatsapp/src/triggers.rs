//! Triggers stub. Phase 1: read-only empty list. Phase 4 will add the
//! stateful agent-target registry.

#[derive(Debug, Clone, Default)]
pub struct TriggersView {
    pub triggers: Vec<Trigger>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Trigger {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

impl TriggersView {
    pub fn empty() -> Self {
        Self {
            triggers: Vec::new(),
        }
    }
    pub fn list(&self) -> &[Trigger] {
        &self.triggers
    }
    pub fn get(&self, _id: &str) -> Option<&Trigger> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_view() {
        let v = TriggersView::empty();
        assert!(v.list().is_empty());
        assert!(v.get("anything").is_none());
    }
}
