#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Pid,
    Ppid,
    User,
    Name,
    Cmd,
    State,
}

impl Field {
    fn from_prefix(s: &str) -> Option<Self> {
        match s {
            "pid" => Some(Field::Pid),
            "ppid" => Some(Field::Ppid),
            "user" => Some(Field::User),
            "name" => Some(Field::Name),
            "cmd" => Some(Field::Cmd),
            "state" => Some(Field::State),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Prefixed { field: Field, value: String },
    Bare(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
    pub auto_select_pid: Option<i32>,
}

pub fn parse(input: &str) -> Query {
    let mut terms: Vec<Term> = Vec::new();
    let mut auto_select_pid: Option<i32> = None;

    for tok in input.split_whitespace() {
        if let Some((prefix, value)) = tok.split_once(':')
            && let Some(field) = Field::from_prefix(prefix)
            && !value.is_empty()
        {
            if field == Field::Pid {
                if let Ok(n) = value.parse::<i32>() {
                    auto_select_pid = Some(n);
                    terms.push(Term::Prefixed {
                        field,
                        value: value.to_string(),
                    });
                    continue;
                }
                // Fail open: pid:<not-an-int> folds to bare term.
                terms.push(Term::Bare(tok.to_string()));
                continue;
            }
            terms.push(Term::Prefixed {
                field,
                value: value.to_string(),
            });
            continue;
        }
        terms.push(Term::Bare(tok.to_string()));
    }

    Query {
        terms,
        auto_select_pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let q = parse("");
        assert!(q.terms.is_empty());
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn whitespace_only() {
        let q = parse("   \t  ");
        assert!(q.terms.is_empty());
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn single_bare() {
        let q = parse("firefox");
        assert_eq!(q.terms, vec![Term::Bare("firefox".into())]);
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn user_prefix() {
        let q = parse("user:wbbradley");
        assert_eq!(
            q.terms,
            vec![Term::Prefixed {
                field: Field::User,
                value: "wbbradley".into()
            }]
        );
    }

    #[test]
    fn pid_int_sets_auto_select() {
        let q = parse("pid:1234");
        assert_eq!(
            q.terms,
            vec![Term::Prefixed {
                field: Field::Pid,
                value: "1234".into()
            }]
        );
        assert_eq!(q.auto_select_pid, Some(1234));
    }

    #[test]
    fn pid_not_a_number_folds_to_bare() {
        let q = parse("pid:notanumber");
        assert_eq!(q.terms, vec![Term::Bare("pid:notanumber".into())]);
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn many_terms() {
        let q = parse("cmd:rust user:root foo bar");
        assert_eq!(
            q.terms,
            vec![
                Term::Prefixed {
                    field: Field::Cmd,
                    value: "rust".into()
                },
                Term::Prefixed {
                    field: Field::User,
                    value: "root".into()
                },
                Term::Bare("foo".into()),
                Term::Bare("bar".into()),
            ]
        );
    }

    #[test]
    fn trailing_colon_falls_back_to_bare() {
        let q = parse("name:");
        assert_eq!(q.terms, vec![Term::Bare("name:".into())]);
    }

    #[test]
    fn embedded_colon_in_value() {
        let q = parse("name:foo:bar");
        assert_eq!(
            q.terms,
            vec![Term::Prefixed {
                field: Field::Name,
                value: "foo:bar".into()
            }]
        );
    }

    #[test]
    fn unknown_prefix_folds_to_bare() {
        let q = parse("foo:bar");
        assert_eq!(q.terms, vec![Term::Bare("foo:bar".into())]);
    }
}
