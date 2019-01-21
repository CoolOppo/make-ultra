#![warn(clippy::all)]

extern crate ignore;
#[macro_use]
extern crate lazy_static;
#[allow(unused_imports)] // The warning is WRONG!
#[macro_use]
extern crate maplit;
extern crate parking_lot;
extern crate rayon;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate petgraph;
extern crate serde_regex;
extern crate toml;
use ignore::WalkBuilder;
use parking_lot::Mutex;
use petgraph::stable_graph::StableDiGraph;
use std::collections::HashMap;

use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
mod file;
mod rule;

// use std::process::Command;
// use petgraph::graphmap::DiGraphMap;

lazy_static! {
    static ref RULES: std::collections::HashMap<std::string::String, rule::Rule> =
        rule::read_rules();
    static ref FILES: Mutex<HashMap<Arc<String>, petgraph::prelude::NodeIndex>> =
        Mutex::new(HashMap::new());
    static ref FILE_GRAPH: Mutex<StableDiGraph<Arc<String>, &'static rule::Rule>> =
        Mutex::new(StableDiGraph::new());
}

fn main() {
    let mut walk_builder = WalkBuilder::new(Path::new("."));
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
    rayon::scope(move |s| {
        for path in rx.iter() {
            s.spawn(move |_| {
                generate_children(path);
            });
        }
    });
    {
        use petgraph::dot::{Config, Dot};
        use std::fs::File;
        use std::io::Write;
        let fg = FILE_GRAPH.lock();

        let mut file = File::create(Path::new("test.dot")).unwrap();
        file.write_all(format!("{:?}", Dot::with_config(&*fg, &[Config::EdgeNoLabel])).as_bytes())
            .unwrap();
    }
}

/// Adds `path` to the DAG if it was not already in it, then recursively adds
/// all children that did not exist yet to the DAG as well.
fn generate_children(path: String) {
    rayon::scope(|s| {
        for rule in RULES.values() {
            let path = &path;
            s.spawn(move |_| {
                if rule.does_match(&path) {
                    let mut new_file: String;
                    let mut should_update_children = false;
                    {
                        let mut files = FILES.lock();
                        let mut file_graph = FILE_GRAPH.lock();
                        let node = if let Some(node_index) = files.get(path) {
                            *node_index
                        } else {
                            let path = Arc::new(path.clone());
                            let file_node = file_graph.add_node(Arc::clone(&path));
                            files.insert(Arc::clone(&path), file_node);
                            file_node
                        };

                        new_file = rule.get_output(&path);
                        let new_file_node = if let Some(node_index) = files.get(&new_file) {
                            *node_index
                        } else {
                            should_update_children = true;
                            let new_file = Arc::new(new_file.clone());
                            let file_node = file_graph.add_node(Arc::clone(&new_file));
                            files.insert(Arc::clone(&new_file), file_node);
                            file_node
                        };

                        file_graph.update_edge(node, new_file_node, rule);
                    }

                    if should_update_children {
                        generate_children(new_file);
                    }
                }
            });
        }
    });
}

// fn make_children<'a>(
//     file: Arc<file::File>,
//     rules: &'a HashMap<std::string::String, rule::Rule>,
//     dag: Arc<Mutex<DiGraphMap<&'a str, ()>>>,
// ) {
//     let out_path = file.rule.get_output(&file);

//     let command = (file.rule.command.replace("$i", &file.path)).replace("$o", &out_path);
//     println!("{}", command);
//     //    Command::new("cmd")
//     //        .args(&["/C", &command])
//     //        .output()
//     //        .expect("failed to execute process");
//     file.rule.next.par_iter().for_each(|(_, x)| {
//         let out_file = Arc::new(file::File {
//             path: out_path.clone(),
//             rule: x,
//         });
//         let dagc = Arc::clone(&dag);
//         {
//             let mut dag2 = dagc.lock();
//             // dag2.add_edge(file.path, "y", ());
//         }
//         make_children(out_file, rules, Arc::clone(&dag));
//     });
//     rules
//         .par_iter()
//         .filter(|(_, rule)| rule != &file.rule && rule.does_match(&file.path))
//         .map(|(_, rule)| rule)
//         .for_each(|x| {
//             let out_file = Arc::new(file::File {
//                 path: out_path.clone(),
//                 rule: x,
//             });
//             make_children(out_file, rules, Arc::clone(&dag));
//         });
// }
