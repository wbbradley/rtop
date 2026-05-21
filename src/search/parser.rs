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
    /// OR of AND-groups. A process matches the query iff it matches at least one group
    /// (every term in that group). Empty `groups` = empty query = matches everything.
    pub groups: Vec<Vec<Term>>,
    pub auto_select_pid: Option<i32>,
}

pub fn parse(input: &str) -> Query {
    let mut groups: Vec<Vec<Term>> = Vec::new();
    let mut current: Vec<Term> = Vec::new();
    let mut auto_select_pid: Option<i32> = None;

    let close_group = |current: &mut Vec<Term>, groups: &mut Vec<Vec<Term>>| {
        if !current.is_empty() {
            groups.push(std::mem::take(current));
        }
    };

    for tok in input.split_whitespace() {
        let mut first = true;
        for frag in tok.split(',') {
            if !first {
                close_group(&mut current, &mut groups);
            }
            first = false;
            if !frag.is_empty() {
                push_term(frag, &mut current, &mut auto_select_pid);
            }
        }
    }
    close_group(&mut current, &mut groups);

    Query {
        groups,
        auto_select_pid,
    }
}

fn push_term(tok: &str, current: &mut Vec<Term>, auto_select_pid: &mut Option<i32>) {
    if let Some((prefix, value)) = tok.split_once(':')
        && let Some(field) = Field::from_prefix(prefix)
        && !value.is_empty()
    {
        if field == Field::Pid {
            if let Ok(n) = value.parse::<i32>() {
                if auto_select_pid.is_none() {
                    *auto_select_pid = Some(n);
                }
                current.push(Term::Prefixed {
                    field,
                    value: value.to_string(),
                });
                return;
            }
            // Fail open: pid:<not-an-int> folds to bare term.
            current.push(Term::Bare(tok.to_string()));
            return;
        }
        current.push(Term::Prefixed {
            field,
            value: value.to_string(),
        });
        return;
    }
    current.push(Term::Bare(tok.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let q = parse("");
        assert!(q.groups.is_empty());
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn whitespace_only() {
        let q = parse("   \t  ");
        assert!(q.groups.is_empty());
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn single_bare() {
        let q = parse("firefox");
        assert_eq!(q.groups, vec![vec![Term::Bare("firefox".into())]]);
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn user_prefix() {
        let q = parse("user:wbbradley");
        assert_eq!(
            q.groups,
            vec![vec![Term::Prefixed {
                field: Field::User,
                value: "wbbradley".into()
            }]]
        );
    }

    #[test]
    fn pid_int_sets_auto_select() {
        let q = parse("pid:1234");
        assert_eq!(
            q.groups,
            vec![vec![Term::Prefixed {
                field: Field::Pid,
                value: "1234".into()
            }]]
        );
        assert_eq!(q.auto_select_pid, Some(1234));
    }

    #[test]
    fn pid_not_a_number_folds_to_bare() {
        let q = parse("pid:notanumber");
        assert_eq!(q.groups, vec![vec![Term::Bare("pid:notanumber".into())]]);
        assert!(q.auto_select_pid.is_none());
    }

    #[test]
    fn many_terms() {
        let q = parse("cmd:rust user:root foo bar");
        assert_eq!(
            q.groups,
            vec![vec![
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
            ]]
        );
    }

    #[test]
    fn trailing_colon_falls_back_to_bare() {
        let q = parse("name:");
        assert_eq!(q.groups, vec![vec![Term::Bare("name:".into())]]);
    }

    #[test]
    fn embedded_colon_in_value() {
        let q = parse("name:foo:bar");
        assert_eq!(
            q.groups,
            vec![vec![Term::Prefixed {
                field: Field::Name,
                value: "foo:bar".into()
            }]]
        );
    }

    #[test]
    fn unknown_prefix_folds_to_bare() {
        let q = parse("foo:bar");
        assert_eq!(q.groups, vec![vec![Term::Bare("foo:bar".into())]]);
    }

    #[test]
    fn comma_with_whitespace_creates_or_groups() {
        let q = parse("user:root, firefox");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Prefixed {
                    field: Field::User,
                    value: "root".into()
                }],
                vec![Term::Bare("firefox".into())],
            ]
        );
    }

    #[test]
    fn comma_at_token_end_is_separator() {
        let q = parse("name:vim, firefox");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Prefixed {
                    field: Field::Name,
                    value: "vim".into()
                }],
                vec![Term::Bare("firefox".into())],
            ]
        );
    }

    #[test]
    fn comma_at_token_start_is_separator() {
        let q = parse("name:vim ,firefox");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Prefixed {
                    field: Field::Name,
                    value: "vim".into()
                }],
                vec![Term::Bare("firefox".into())],
            ]
        );
    }

    #[test]
    fn comma_inside_bare_token_splits_into_two_groups() {
        let q = parse("bash,dbus-daemon");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Bare("bash".into())],
                vec![Term::Bare("dbus-daemon".into())],
            ]
        );
    }

    #[test]
    fn comma_inside_prefixed_token_splits_and_fragment_does_not_inherit() {
        let q = parse("name:vim,emacs");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Prefixed {
                    field: Field::Name,
                    value: "vim".into()
                }],
                vec![Term::Bare("emacs".into())],
            ]
        );
    }

    #[test]
    fn comma_separated_prefixed_terms_keep_each_prefix() {
        let q = parse("name:vim,name:emacs");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Prefixed {
                    field: Field::Name,
                    value: "vim".into()
                }],
                vec![Term::Prefixed {
                    field: Field::Name,
                    value: "emacs".into()
                }],
            ]
        );
    }

    #[test]
    fn runs_of_commas_collapse_without_whitespace() {
        let expected = vec![vec![Term::Bare("a".into())], vec![Term::Bare("b".into())]];
        assert_eq!(parse("a,,b").groups, expected);
        assert_eq!(parse(",a,b,").groups, expected);
    }

    #[test]
    fn multiple_or_groups() {
        let q = parse("a, b, c");
        assert_eq!(
            q.groups,
            vec![
                vec![Term::Bare("a".into())],
                vec![Term::Bare("b".into())],
                vec![Term::Bare("c".into())],
            ]
        );
    }

    #[test]
    fn lone_comma_token_is_separator() {
        let q = parse("a , b");
        assert_eq!(
            q.groups,
            vec![vec![Term::Bare("a".into())], vec![Term::Bare("b".into())],]
        );
    }

    #[test]
    fn multiple_commas_collapse() {
        let q1 = parse("a ,, b");
        let q2 = parse("a, , b");
        let expected = vec![vec![Term::Bare("a".into())], vec![Term::Bare("b".into())]];
        assert_eq!(q1.groups, expected);
        assert_eq!(q2.groups, expected);
    }

    #[test]
    fn leading_comma_dropped() {
        let q = parse(",a");
        assert_eq!(q.groups, vec![vec![Term::Bare("a".into())]]);
    }

    #[test]
    fn trailing_comma_dropped() {
        let q = parse("a,");
        assert_eq!(q.groups, vec![vec![Term::Bare("a".into())]]);
    }

    #[test]
    fn pid_in_second_or_group_sets_auto_select() {
        let q = parse("firefox, pid:42");
        assert_eq!(q.auto_select_pid, Some(42));
        assert_eq!(q.groups.len(), 2);
    }

    #[test]
    fn first_valid_pid_wins() {
        let q = parse("pid:42, pid:7");
        assert_eq!(q.auto_select_pid, Some(42));
    }
}
