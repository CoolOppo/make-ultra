#![warn(clippy::all)]
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate walkdir;
#[macro_use]
extern crate lazy_static;
extern crate regex;

use crate::rule::Rule;
use regex::Regex;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

mod file;
mod rule;

fn main() {
    let rules = read_rules();
    let mut files: Vec<file::MyFile> = Vec::new();
    let mut regexes: Vec<(&rule::Rule, Regex)> = Vec::new();
    for rule in rules.iter() {
        regexes.push((
            rule,
            Regex::new(&rule.pattern)
                .expect(&format!("Invalid RegEx in rule \"{}\"", &rule.pattern)),
        ));
    }
    for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
        for (rule, regex) in regexes.iter() {
            let name = String::from(entry.file_name().to_string_lossy());
            if regex.is_match(&name) {
                let path = String::from(entry.path().to_string_lossy());
                // println!("{}", entry.path().to_str().unwrap());
                files.push(file::MyFile { name, path });
            }
        }
    }
    let path = Path::new("files.json");
    let display = path.display();
    let mut file = match File::create(path) {
        Err(why) => panic!("couldn't create {}: {}", display, why.description()),
        Ok(file) => file,
    };

    // Write the `LOREM_IPSUM` string to `file`, returns `io::Result<()>`
    match file.write_all(serde_json::to_string(&files).unwrap().as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why.description()),
        Ok(_) => println!("successfully wrote to {}", display),
    }
}

fn read_rules() -> Vec<Rule> {
    let path = Path::new("rules.json");

    let mut file = match File::open(&path) {
        // The `description` method of `io::Error` returns a string that
        // describes the error
        Err(_why) => panic!("couldn't open rule file"),
        Ok(file) => file,
    };

    // Read the file contents into a string, returns `io::Result<usize>`
    let mut s = String::new();
    if let Err(why) = file.read_to_string(&mut s) {
        panic!("couldn't read {}: {}", path.display(), why.description())
    };

    let rules: Vec<rule::Rule> = serde_json::from_str(&s).unwrap();
    rules
}
