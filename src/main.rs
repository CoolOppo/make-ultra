#![warn(clippy::all)]
#[macro_use]
extern crate maplit;
extern crate rayon;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_regex;
extern crate toml;
extern crate walkdir;

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use walkdir::WalkDir;
use std::process::Command;

mod file;
mod file_store;
mod rule;

fn main() {
    let rules = rule::read_rules();
    let mut files: Vec<file_store::File> = Vec::new();
    for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
        let path = String::from(entry.path().to_string_lossy());

        for rule in rules.values() {
            if rule.from.is_match(&path) {
                let mut is_top_level = true;
                let x = &rule.next;
                for depender in x.values() {
                    if depender.from.is_match(&path) {
                        is_top_level = false;
                    }
                }

                if is_top_level {
                    files.push(file_store::File {
                        path: path.clone(),
                        rule: Some(rule),
                    });
                }
            }
        }
    }

    for file in files.iter().map(|f| file::File { path: Path::new(&f.path), rule: &f.rule.unwrap() }) {
        runthi(file);
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

fn runthi(file: file::File) {
    rayon::scope(|s| {
        s.spawn(|_| {
            let out_path = &String::from(file.rule.from.replace_all(&file.path.to_string_lossy(), &*file.rule.to));

            let command = (file.rule.command.replace("$i", &file.path.to_string_lossy()))
                .replace("$o", out_path);
            println!("{}", command);
            Command::new("cmd")
                .args(&["/C", &command])
                .output()
                .expect("failed to execute process");
            for x in file.rule.next.values() {
                let out_file = file::File {
                    path: &Path::new(out_path),
                    rule: x,
                };
                runthi(out_file);
            }
        });
    });
}