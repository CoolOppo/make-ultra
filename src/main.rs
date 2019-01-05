#![warn(clippy::all)]
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate toml;
extern crate ron;
extern crate walkdir;
extern crate regex;

use std::collections::HashMap;
use crate::rule::Rule;
use regex::Regex;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

mod file_store;
mod rule;

fn main() {
    let rules = rule::read_rules();
    let mut files: Vec<file_store::MyFile> = Vec::new();
    let mut regex_pairs: Vec<(&rule::Rule, Regex)> = Vec::new();
    for rule in rules.values() {
        regex_pairs.push((
            rule,
            Regex::new(&rule.from)
                .unwrap_or_else(|_| panic!("Invalid RegEx in rule \"{}\"", &rule.from)),
        ));
    }
    for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
        for (rule, regex) in regex_pairs.iter() {
            let name = String::from(entry.file_name().to_string_lossy());
            if regex.is_match(&name) {
                let path = String::from(entry.path().to_string_lossy());
                // println!("{}", entry.path().to_str().unwrap());
                files.push(file_store::MyFile { name, path, rule: Some(rule) });
            }
        }
    }
    let path = Path::new("files.json");
    let display = path.display();
    let mut file = match File::create(path) {
        Err(why) => panic!("couldn't create {}: {}", display, why.description()),
        Ok(file) => file,
    };

    match file.write_all(serde_json::to_string(&files).unwrap().as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why.description()),
        Ok(_) => println!("successfully wrote to {}", display),
    }
}


