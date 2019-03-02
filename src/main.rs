#![warn(clippy::all)]

extern crate clap;
extern crate hashbrown;
extern crate ignore;
#[macro_use]
extern crate lazy_static;
#[allow(unused_imports)] // The warning is WRONG!
#[macro_use]
extern crate maplit;
extern crate parking_lot;
extern crate petgraph;
extern crate rayon;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_regex;
extern crate toml;

use bincode::{deserialize, serialize};
use clap::{App, Arg};
use ignore::WalkBuilder;
use parking_lot::RwLock;
use petgraph::stable_graph::StableDiGraph;
use rayon::prelude::*;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs,
    hash::Hasher,
    io::Error,
    path::Path,
    sync::{mpsc::channel, Arc},
};

mod rule;

lazy_static! {
    static ref MATCHES: clap::ArgMatches<'static> = { clap_setup() };
    static ref RULES: HashMap<std::string::String, rule::Rule> = rule::read_rules();
    static ref FILES: RwLock<std::collections::HashMap<Arc<String>, petgraph::prelude::NodeIndex>> =
        RwLock::new(HashMap::new());
    static ref FILE_GRAPH: RwLock<StableDiGraph<Arc<String>, &'static rule::Rule>> =
        RwLock::new(StableDiGraph::new());
    static ref DRY_RUN: bool = MATCHES.is_present("dry_run");
    static ref DOT: bool = MATCHES.is_present("dot");
    static ref FORCE: bool = MATCHES.is_present("force");
    static ref CACHE_PATH: String = String::from(".make_cache");
    static ref SAVED_HASHES: Option<HashMap<String, u64>> = {
        if let Ok(cache_file) = fs::read(&*CACHE_PATH) {
            if let Ok(hashes) = deserialize(&cache_file) {
                Some(hashes)
            } else {
                println!("Invalid .make_cache file");
                None
            }
        } else {
            None
        }
    };
    static ref NEW_HASHES: RwLock<HashMap<String, u64>> =
        if let Some(hashes) = SAVED_HASHES.as_ref() {
            RwLock::new(hashes.clone())
        } else {
            RwLock::new(HashMap::new())
        };
}

fn clap_setup() -> clap::ArgMatches<'static> {
    App::new("make_ultra")
        .arg(
            Arg::with_name("dry_run")
                .help("Print commands, but do not run them")
                .long("dry")
                .short("n"),
        )
        .arg(
            Arg::with_name("dot")
                .help("Prints dot graph to file")
                .long("dot")
                .short("d")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("force")
                .help("Force running on all files")
                .long("force")
                .short("f")
                .takes_value(false),
        )
        .get_matches()
}

fn main() {
    let (tx, rx) = channel();
    WalkBuilder::new(Path::new("."))
        .standard_filters(false)
        .build_parallel()
        .run(move || {
            let tx = tx.clone();
            Box::new(move |entry| {
                let entry = match entry {
                    Err(_) => {
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
    if *DOT {
        use petgraph::dot::{Config, Dot};
        use std::{fs::File, io::Write};
        let fg = FILE_GRAPH.read();

        let mut file = File::create(Path::new(MATCHES.value_of("dot").unwrap())).unwrap();
        file.write_all(format!("{:?}", Dot::with_config(&*fg, &[Config::EdgeNoLabel])).as_bytes())
            .unwrap();
    }
    rayon::scope(move |s| {
        let g = FILE_GRAPH.read();
        for i in g.node_indices().filter(|n| {
            let incoming_count = g
                .neighbors_directed(*n, petgraph::Direction::Incoming)
                .count();
            // Get all nodes with no inputs (roots)
            (incoming_count == 0
                || incoming_count == 1
                    && g.neighbors_directed(*n, petgraph::Direction::Incoming)
                        .next()
                        .unwrap()
                        == *n)
        }) {
            s.spawn(move |_| {
                run_commands(i);
            });
        }
    });

    let new_hashes = &*NEW_HASHES.write();
    if !new_hashes.is_empty() {
        let serialized_hashes = serialize(new_hashes).expect("Unable to serialize new hashes.");
        fs::write(&*CACHE_PATH, serialized_hashes)
            .unwrap_or_else(|_| panic!("Unable to save serialized hashes to `{}`.", &*CACHE_PATH));
    }
}

fn run_commands(node: petgraph::prelude::NodeIndex) {
    rayon::scope(move |_| {
        use petgraph::visit::EdgeRef;
        use rayon::iter::ParallelBridge;
        use std::process::Command;
        let g = FILE_GRAPH.read();
        let files = FILES.read();

        g.edges_directed(node, petgraph::Direction::Outgoing)
            .par_bridge()
            .for_each(|edge| {
                let source_path = &*g[node];
                let target_path = &*g[edge.target()];
                let should_run_command = if *FORCE {
                    true
                } else if let Some(old_hashes) = SAVED_HASHES.as_ref() {
                    if let Some(saved_hash) = old_hashes.get(source_path) {
                        if let Ok(current_hash) = file_hash(source_path) {
                            current_hash != *saved_hash
                        } else {
                            println!(
                                "WARNING: Could not read `{}`. Treating as if dirty.",
                                source_path
                            );
                            true
                        }
                    } else {
                        // No saved hash for this file
                        true
                    }
                } else {
                    // No saved hashes at all
                    true
                };

                if should_run_command {
                    let command = (edge.weight().command.replace("$i", source_path))
                        .replace("$o", target_path);
                    println!("{}", command);
                    if !*DRY_RUN {
                        if cfg!(target_os = "windows") {
                            let out = Command::new("cmd")
                                .args(&["/C", &command])
                                .output()
                                .unwrap_or_else(|_| panic!("Failed to execute {}", command));
                            if !out.stderr.is_empty() {
                                println!("{}", std::str::from_utf8(&out.stderr).unwrap());
                            }
                        } else {
                            let out = Command::new("sh")
                                .arg("-c")
                                .arg(&command)
                                .output()
                                .unwrap_or_else(|_| panic!("Failed to execute {}", command));
                            if !out.stderr.is_empty() {
                                println!("{}", std::str::from_utf8(&out.stderr).unwrap());
                            }
                        };
                        if let Ok(new_hash) = file_hash(target_path) {
                            let mut new_hashes = NEW_HASHES.write();
                            new_hashes.insert(target_path.clone(), new_hash);
                        } else {
                            println!(
                                "WARNING: Unable to read `{}` to find its new hash.",
                                target_path
                            );
                        }
                    }
                }
                if edge.source() != edge.target() {
                    run_commands(*files.get(target_path).unwrap());
                }
            });
    });
}

fn file_hash(path: &str) -> Result<u64, Error> {
    let file_bytes = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    hasher.write(&file_bytes);
    Ok(hasher.finish())
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
                            if path != &new_file {
                                should_update_children = true;
                            }
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
