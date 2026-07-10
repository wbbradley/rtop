use regex::{Regex, RegexBuilder};

use crate::search::parser::{Field, Query, Term};

/// Which string field a compiled string term matches against.
pub enum StrTarget {
    User,
    Name,
    Cmd,
    Bare,
}

pub enum CompiledTerm {
    Pid(i32),
    Ppid(i32),
    State(char),
    Str {
        field: StrTarget,
        re: Regex,
    },
    /// Uncompilable regex → non-constraining (skipped within its AND-group).
    Invalid,
    /// e.g. `ppid:<non-int>` → never matches (preserves today's behavior).
    Never,
}

/// Compiled parallel of a `Query`. Holds the compiled regexes for string terms;
/// derived state recomputed alongside `Query` on every query change.
pub struct CompiledQuery {
    groups: Vec<Vec<CompiledTerm>>,
    /// Flat union of all string-term regexes, for the renderer's highlighting.
    highlight: Vec<Regex>,
    has_invalid: bool,
    empty: bool,
}

impl CompiledQuery {
    pub fn compile(query: &Query) -> Self {
        let mut groups: Vec<Vec<CompiledTerm>> = Vec::with_capacity(query.groups.len());
        let mut highlight: Vec<Regex> = Vec::new();
        let mut has_invalid = false;

        for group in &query.groups {
            let mut compiled_group: Vec<CompiledTerm> = Vec::with_capacity(group.len());
            for term in group {
                let compiled = match term {
                    Term::Prefixed { field, value } => match field {
                        Field::Pid => match value.parse::<i32>() {
                            Ok(n) => CompiledTerm::Pid(n),
                            Err(_) => CompiledTerm::Never,
                        },
                        Field::Ppid => match value.parse::<i32>() {
                            Ok(n) => CompiledTerm::Ppid(n),
                            Err(_) => CompiledTerm::Never,
                        },
                        Field::State => CompiledTerm::State(first_char(value)),
                        Field::User => {
                            compile_str(StrTarget::User, value, &mut highlight, &mut has_invalid)
                        }
                        Field::Name => {
                            compile_str(StrTarget::Name, value, &mut highlight, &mut has_invalid)
                        }
                        Field::Cmd => {
                            compile_str(StrTarget::Cmd, value, &mut highlight, &mut has_invalid)
                        }
                    },
                    Term::Bare(value) => {
                        compile_str(StrTarget::Bare, value, &mut highlight, &mut has_invalid)
                    }
                };
                compiled_group.push(compiled);
            }
            groups.push(compiled_group);
        }

        let empty = groups.is_empty();
        Self {
            groups,
            highlight,
            has_invalid,
            empty,
        }
    }

    pub fn groups(&self) -> &[Vec<CompiledTerm>] {
        &self.groups
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn highlight_regexes(&self) -> &[Regex] {
        &self.highlight
    }

    pub fn has_invalid(&self) -> bool {
        self.has_invalid
    }
}

/// Compile a string term into an unanchored, case-insensitive-by-default regex.
/// On success push a clone into `highlight` and return `Str`; on failure set
/// `has_invalid` and return `Invalid` (non-constraining).
fn compile_str(
    field: StrTarget,
    pattern: &str,
    highlight: &mut Vec<Regex>,
    has_invalid: &mut bool,
) -> CompiledTerm {
    match RegexBuilder::new(pattern).case_insensitive(true).build() {
        Ok(re) => {
            highlight.push(re.clone());
            CompiledTerm::Str { field, re }
        }
        Err(_) => {
            *has_invalid = true;
            CompiledTerm::Invalid
        }
    }
}

fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::parse;

    #[test]
    fn compile_collects_all_string_regexes() {
        let cq = CompiledQuery::compile(&parse("name:x cmd:y foo"));
        assert_eq!(cq.highlight_regexes().len(), 3);
    }

    #[test]
    fn valid_query_has_no_invalid() {
        let cq = CompiledQuery::compile(&parse("name:fire cmd:profile"));
        assert!(!cq.has_invalid());
    }

    #[test]
    fn partial_regex_flags_invalid() {
        let cq = CompiledQuery::compile(&parse("fire("));
        assert!(cq.has_invalid());
    }
}
