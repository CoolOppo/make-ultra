#![warn(clippy::all)]
#[macro_use]
extern crate serde_derive;
extern crate rayon;
extern crate regex;
extern crate serde;
extern crate serde_regex;
extern crate toml;
extern crate walkdir;

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

mod file_store;
mod rule;

fn main() {
    let rules = rule::read_rules();
    let mut files: Vec<file_store::File> = Vec::new();
    for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
        for rule in rules.values() {
            let name = String::from(entry.file_name().to_string_lossy());
            if rule.from.is_match(&name) {
                let mut yes = true;
                if let Some(x) = &rule.next {
                    for depender in x {
                        if depender.from.is_match(&name) {
                            yes = false;
                        }
                    }
                }
                if yes {
                    let path = String::from(entry.path().to_string_lossy());
                    files.push(file_store::File {
                        name,
                        path,
                        rule: Some(rule),
                    });
                }
            }
        }
    }

    for file in files.iter() {
        let rule = file.rule.unwrap();
        rayon::scope(|s| {
            s.spawn(|_| {
                let command = (rule.command.replace("$i", &file.path))
                    .replace("$o", &rule.from.replace_all(&file.path, &*rule.to));
                println!("{}", command);
                Command::new("cmd")
                    .args(&["/C", &command])
                    .output()
                    .expect("failed to execute process");
            });
        });
    }
    let path = Path::new("files.toml");
    let display = path.display();
    let mut file = match File::create(path) {
        Err(why) => panic!("couldn't create {}: {}", display, why.description()),
        Ok(file) => file,
    };

    match file.write_all(toml::to_string_pretty(&files).unwrap().as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why.description()),
        Ok(_) => println!("successfully wrote to {}", display),
    }
}
