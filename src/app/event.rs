#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Search,
    Tree,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Search => "search",
            Focus::Tree => "tree",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_label_distinct() {
        let s = Focus::Search.label();
        let t = Focus::Tree.label();
        assert!(!s.is_empty());
        assert!(!t.is_empty());
        assert_ne!(s, t);
    }
}
