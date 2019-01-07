use crate::rule;

#[derive(Debug)]
pub struct File<'a> {
    pub path: String,
    pub rule: &'a rule::Rule,
}
