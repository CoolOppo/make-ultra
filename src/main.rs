#![warn(clippy::all)]
extern crate ignore;
#[macro_use]
extern crate maplit;
extern crate rayon;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_regex;
extern crate toml;

use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::channel;

use ignore::WalkBuilder;
use rayon::prelude::*;

mod file;
mod file_store;
mod rule;

fn main() {
    let rules = rule::read_rules();
    let mut files: Vec<file::File> = Vec::new();
    let mut walk_builder = WalkBuilder::new("./");
    let (tx, rx) = channel();
    walk_builder
        .standard_filters(false)
        .build_parallel()
        .run(move || {
            let tx2 = tx.clone();
            Box::new(move |entry| {
                let entry = match entry {
                    Err(_e) => {
                        return ignore::WalkState::Continue;
                    }
                    Ok(e) => e,
                };
                let path = entry.path().to_owned();
                let p = String::from(path.to_string_lossy());
                tx2.send(p).unwrap();
                ignore::WalkState::Continue
            })
        });
    for path in rx.iter() {
        for rule in rules.values() {
            if rule.does_match(&path) {
                files.push(file::File {
                    path: String::from(&*path),
                    rule: &rule,
                });
            }
        }
    }
    // let (file_tx, file_rx) = channel();
    files.par_iter_mut().for_each(|file| {
        runthi(file, &rules);
    });

    //    let path = Path::new("files.toml");
    //    let display = path.display();
    //    let mut file = match File::create(path) {
    //        Err(why) => panic!("couldn't create {}: {}", display, why.description()),
    //        Ok(file) => file,
    //    };
    //
    //    match file.write_all(toml::to_string_pretty(&files).unwrap().as_bytes()) {
    //        Err(why) => panic!("couldn't write to {}: {}", display, why.description()),
    //        Ok(_) => println!("successfully wrote to {}", display),
    //    }
}

fn runthi(file: &file::File, rules: &HashMap<String, rule::Rule>) {
    let out_path = file.rule.get_output(&file);

    let command = (file.rule.command.replace("$i", &file.path)).replace("$o", &out_path);
    println!("{}", command);
    //    Command::new("cmd")
    //        .args(&["/C", &command])
    //        .output()
    //        .expect("failed to execute process");
    file.rule.next.par_iter().for_each(|(_, x)| {
        let out_file = file::File {
            path: out_path.clone(),
            rule: x,
        };
        runthi(&out_file, rules);
    });
    rules
        .par_iter()
        .filter(|(_, rule)| rule != &file.rule && rule.does_match(&file.path))
        .map(|(_, rule)| rule)
        .for_each(|x| {
            let out_file = file::File {
                path: out_path.clone(),
                rule: x,
            };
            runthi(&out_file, rules);
        });
}
