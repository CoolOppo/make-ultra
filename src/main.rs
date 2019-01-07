#![warn(clippy::all)]
#[macro_use]
extern crate maplit;

extern crate rayon;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate ignore;
extern crate serde_regex;
extern crate toml;

use std::collections::HashMap;
use std::process::Command;

use ignore::WalkBuilder;
use rayon::prelude::*;
use std::sync::mpsc::channel;
mod file;
mod file_store;
mod rule;

fn main() {
    let rules = rule::read_rules();
    let mut files: Vec<file::File> = Vec::new();
    let walk_builder = WalkBuilder::new("./");
    let (tx, rx) = channel();
    walk_builder.build_parallel().run(move || {
        let tx2 = tx.clone();
        Box::new(move |entry_gg| {
            let entry = match entry_gg {
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
                let mut is_top_level = true;
                let x = &rule.next;
                for depender in x.values() {
                    if depender.from.is_match(&path) {
                        is_top_level = false;
                    }
                }

                if is_top_level {
                    files.push(file::File {
                        path: String::from(&*path),
                        rule: &rule,
                    });
                }
            }
        }
    }

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
    Command::new("cmd")
        .args(&["/C", &command])
        .output()
        .expect("failed to execute process");
    if !file.rule.next.is_empty() {
        file.rule.next.par_iter().for_each(|(_, x)| {
            let out_file = file::File {
                path: out_path.clone(),
                rule: x,
            };
            runthi(&out_file, rules);
        });
    } /*else {
          for x in rules.values() {}
      }*/
}
