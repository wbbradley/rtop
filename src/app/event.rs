#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Search,
    Load,
    Tree,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Search => "search",
            Focus::Load => "load",
            Focus::Tree => "tree",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Rss,
    TimePlus,
    Age,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::Cpu => SortKey::Rss,
            SortKey::Rss => SortKey::TimePlus,
            SortKey::TimePlus => SortKey::Age,
            SortKey::Age => SortKey::Cpu,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Cpu => "cpu",
            SortKey::Rss => "rss",
            SortKey::TimePlus => "time+",
            SortKey::Age => "age",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_key_cycles() {
        assert_eq!(SortKey::Cpu.next(), SortKey::Rss);
        assert_eq!(SortKey::Rss.next(), SortKey::TimePlus);
        assert_eq!(SortKey::TimePlus.next(), SortKey::Age);
        assert_eq!(SortKey::Age.next(), SortKey::Cpu);
    }

    #[test]
    fn sort_key_label() {
        assert_eq!(SortKey::Cpu.label(), "cpu");
        assert_eq!(SortKey::Rss.label(), "rss");
        assert_eq!(SortKey::TimePlus.label(), "time+");
        assert_eq!(SortKey::Age.label(), "age");
    }

    #[test]
    fn focus_label_distinct() {
        let labels = [
            Focus::Search.label(),
            Focus::Load.label(),
            Focus::Tree.label(),
        ];
        for l in labels {
            assert!(!l.is_empty());
        }
        assert_ne!(labels[0], labels[1]);
        assert_ne!(labels[1], labels[2]);
        assert_ne!(labels[0], labels[2]);
    }
}
