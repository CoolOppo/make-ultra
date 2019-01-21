#![warn(clippy::all)]
#![allow(dead_code, unused_variables, unused_assignments, unused_imports)]

extern crate clap;
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

use clap::{App, Arg};
use ignore::WalkBuilder;
use parking_lot::{Mutex, RwLock};
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::Topo;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
mod rule;

lazy_static! {
    static ref MATCHES: clap::ArgMatches<'static> = { clap_setup() };
    static ref RULES: std::collections::HashMap<std::string::String, rule::Rule> =
        rule::read_rules();
    static ref FILES: RwLock<HashMap<Arc<String>, petgraph::prelude::NodeIndex>> =
        RwLock::new(HashMap::new());
    static ref FILE_GRAPH: RwLock<StableDiGraph<Arc<String>, &'static rule::Rule>> =
        RwLock::new(StableDiGraph::new());
}

fn clap_setup() -> clap::ArgMatches<'static> {
    App::new("make_ultra")
        .arg(
            Arg::with_name("dry_run")
                .help("Print commands, but do not run them.")
                .long("dry")
                .short("n"),
        )
        .arg(
            Arg::with_name("dot")
                .help("Print dot graph to file.")
                .long("dot")
                .short("d")
                .takes_value(true),
        )
        .get_matches()
}

fn main() {
    let mut walk_builder = WalkBuilder::new(Path::new("."));
    let (tx, rx) = channel();
    walk_builder
        .standard_filters(false)
        .build_parallel()
        .run(move || {
            let tx = tx.clone();
            Box::new(move |entry| {
                let entry = match entry {
                    Err(_e) => {
                        return ignore::WalkState::Continue;
                    }
                    Ok(e) => e,
                };
                let path = entry.path().to_owned();
                let p = String::from(path.to_string_lossy());
                tx.send(p).unwrap();
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
    if MATCHES.is_present("dot") {
        use petgraph::dot::{Config, Dot};
        use std::fs::File;
        use std::io::Write;
        let fg = FILE_GRAPH.read();

        let mut file = File::create(Path::new(MATCHES.value_of("dot").unwrap())).unwrap();
        file.write_all(format!("{:?}", Dot::with_config(&*fg, &[Config::EdgeNoLabel])).as_bytes())
            .unwrap();
    }
    {
        let mut topo_visitor: petgraph::visit::Topo<petgraph::prelude::NodeIndex, _>;
        {
            let g = FILE_GRAPH.read();
            topo_visitor = Topo::new(&*g);
        }
        rayon::scope(move |s| {
            use petgraph::visit::IntoNodeReferences;
            let g = FILE_GRAPH.read();
            for i in g.node_indices().filter(|n| {
                // Get all nodes with no inputs (roots)
                g.neighbors_directed(*n, petgraph::Direction::Incoming)
                    .count()
                    == 0
            }) {
                s.spawn(move |_| {
                    run_commands(i);
                });
            }
        });
    }
}

fn run_commands(node: petgraph::prelude::NodeIndex) {
    rayon::scope(move |s| {
        use rayon::iter::ParallelBridge;
        use std::process::Command;
        let g = FILE_GRAPH.read();
        g.edges_directed(node, petgraph::Direction::Outgoing)
            .par_bridge()
            .for_each(|edge| {
                let g = FILE_GRAPH.read();
                let files = FILES.read();
                let full_command = (edge.weight().command.replace("$i", &*g[node]))
                    .replace("$o", &*g[petgraph::visit::EdgeRef::target(&edge)]);
                println!("{}", full_command);
                if !MATCHES.is_present("dry_run") {
                    let first_space = full_command.find(' ').unwrap();
                    let command = &full_command[..first_space];
                    let args: Vec<&str> = full_command[first_space + 1..].split(' ').collect();
                    Command::new(command)
                        .args(&args)
                        .output()
                        .expect("failed to execute process");
                }
                run_commands(
                    *files
                        .get(&*g[petgraph::visit::EdgeRef::target(&edge)])
                        .unwrap(),
                );
            });
    });
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
                        let mut files = FILES.write();
                        let node = if let Some(node_index) = files.get(path) {
                            *node_index
                        } else {
                            let mut file_graph = FILE_GRAPH.write();
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
                            let mut file_graph = FILE_GRAPH.write();
                            let new_file = Arc::new(new_file.clone());
                            let file_node = file_graph.add_node(Arc::clone(&new_file));
                            files.insert(Arc::clone(&new_file), file_node);
                            file_node
                        };

                        {
                            let mut file_graph = FILE_GRAPH.write();
                            file_graph.update_edge(node, new_file_node, rule);
                        }
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
