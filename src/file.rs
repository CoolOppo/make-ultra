use crate::rule;
use std::path::Path;
use std::process::Command;
#[derive(Debug)]
pub struct File<'a> {
    pub path: &'a Path,
    pub rule: &'a rule::Rule,
}

impl File<'_> {
    pub fn run_commands(&self) {
        Command::new("cmd")
            .args(&["/C", &self.rule.command])
            .output()
            .expect("failed to execute process");
    }
}
