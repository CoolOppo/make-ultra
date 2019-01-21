use crate::rule;

#[derive(Debug, PartialEq)]
pub struct File<'a> {
    pub path: String,
    pub rule: &'a rule::Rule,
}
