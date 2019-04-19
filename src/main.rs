#![warn(clippy::all)]
#![feature(const_string_new)]

#[macro_use]
extern crate cached;

#[macro_use]
extern crate lazy_static;

#[allow(unused_imports)] // The warning is WRONG!
#[macro_use]
extern crate maplit;
#[macro_use]
extern crate serde_derive;

use bincode::{deserialize, serialize};
use clap::{App, Arg};
use crossbeam::channel::unbounded;
use hashbrown::{hash_map::DefaultHashBuilder, HashMap};
use ignore::WalkBuilder;
use parking_lot::RwLock;
use petgraph::{prelude::*, stable_graph::StableDiGraph};
use rayon::prelude::*;
use snap;
use std::{
    fs::{self, File},
    hash::{BuildHasher, Hasher},
    io::{Error, Read, Write},
    path::Path,
    sync::Arc,
};

mod rule;

const CACHE_PATH: &str = ".make_cache";

lazy_static! {
    static ref MATCHES: clap::ArgMatches<'static> = { clap_setup() };
    static ref DOT: bool = MATCHES.is_present("dot");
    static ref DRY_RUN: bool = MATCHES.is_present("dry_run");
    static ref FORCE: bool = MATCHES.is_present("force");

    static ref RULES: HashMap<std::string::String, rule::Rule> = rule::read_rules();
    static ref FILES: RwLock<HashMap<Arc<String>, NodeIndex>> = RwLock::new(HashMap::new());
    static ref FILE_GRAPH: RwLock<StableDiGraph<Arc<String>, &'static rule::Rule>> = RwLock::new(StableDiGraph::new());

    static ref SAVED_HASHES: Option<HashMap<String, u64>> = {
        if let Ok(cache_file) = File::open(CACHE_PATH) {
            let mut reader = snap::Reader::new(cache_file);
            let mut bytes = Vec::new();
            if let Ok(_p) = reader.read_to_end(&mut bytes) {
                if let Ok(hashes) = deserialize(&bytes) {
                    Some(hashes)
                } else {
                    println!("Invalid {} file", CACHE_PATH);
                    None
                }
            } else if let Ok(bytes) = fs::read(CACHE_PATH) {
                if let Ok(hashes) = deserialize(&bytes) {
                    println!("Reading old cache file format.");
                    Some(hashes)
                } else {
                    println!("Invalid {} file", CACHE_PATH);
                    None
                }
            } else {
                println!("Invalid {} file", CACHE_PATH);
                None
            }
        } else {
            None
        }
    };

    static ref NEW_HASHES: RwLock<HashMap<String, u64>> =
    if let Some(hashes) = SAVED_HASHES.as_ref() {
        // Regenerate cache if running a force build
        if !*FORCE {
            RwLock::new(hashes.clone())
        } else {
            RwLock::new(HashMap::new())
        }
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
    lazy_static::initialize(&MATCHES);

    rayon::scope(move |s| {
        let (tx, rx) = unbounded();
        s.spawn(move |_| {
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
                        let path = entry
                            .path()
                            .to_str()
                            .unwrap_or_else(|| {
                                panic!("\"{}\" is not UTF-8", entry.path().to_string_lossy())
                            })
                            .to_string();
                        tx.send(path).unwrap();
                        ignore::WalkState::Continue
                    })
                });
        });

        for path in rx.iter() {
            s.spawn(move |_| {
                generate_children(path);
            });
        }
    });
    if *DOT {
        use petgraph::dot::{Config, Dot};
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
            // Get all root nodes
            (incoming_count == 0
                || incoming_count == 1 // Nodes that only have an input from themselves are also roots.
                && g.neighbors_directed(*n, petgraph::Direction::Incoming)
                .next()
                .unwrap()
                == *n)
        }) {
            update_hash(&*g[i]);
            s.spawn(move |_| {
                run_commands(i);
            });
        }
    });

    let new_hashes = &*NEW_HASHES.write();
    if !new_hashes.is_empty() && !*DRY_RUN {
        let serialized_hashes = serialize(new_hashes).expect("Unable to serialize new hashes.");
        let out_file = File::create(CACHE_PATH).unwrap();
        let mut writer = snap::Writer::new(out_file);
        writer
            .write_all(&serialized_hashes)
            .expect("Unable to save cache file.");
    }
}

fn run_commands(node: NodeIndex) {
    rayon::scope(move |_| {
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
                    let full_command = split_command(edge.weight().command);
                    let program = full_command[0];
                    let args = {
                        let mut args = Vec::new();
                        for arg in full_command.iter().skip(1) {
                            args.push(arg.replace("$i", source_path).replace("$o", target_path));
                        }
                        args
                    };

                    println!("{} {}", program, args.join(" "));
                    if !*DRY_RUN {
                        if cfg!(target_os = "windows") {
                            let out = Command::new("cmd")
                                .arg("/C")
                                .arg(program)
                                .args(&args)
                                .output()
                                .unwrap_or_else(|_| {
                                    panic!("Failed to execute {} {}", program, args.join(" "))
                                });
                            if !out.stderr.is_empty() {
                                println!("{}", std::str::from_utf8(&out.stderr).unwrap());
                            }
                        } else {
                            let out =
                                Command::new(program)
                                    .args(&args)
                                    .output()
                                    .unwrap_or_else(|_| {
                                        panic!("Failed to execute {} {}", program, args.join(" "))
                                    });
                            if !out.stderr.is_empty() {
                                println!("{}", std::str::from_utf8(&out.stderr).unwrap());
                            }
                        }
                        update_hash(target_path);
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
    let mut hasher = DefaultHashBuilder::default().build_hasher();
    hasher.write(&file_bytes);
    Ok(hasher.finish())
}

cached! {
    COMMAND;
    fn split_command(command: &'static str) -> Vec<&'static str> = {
        let mut out = Vec::new();
        for piece in command.split_whitespace(){
            out.push(piece);
        }
        out
    }
}

fn update_hash(path: &str) {
    if let Ok(new_hash) = file_hash(path) {
        let mut new_hashes = NEW_HASHES.write();
        new_hashes.insert(path.to_string(), new_hash);
    } else {
        println!("WARNING: Unable to read `{}` to find its hash.", path);
    }
}

fn get_matching_rules<'a, 'b>(path: &'a str) -> Vec<&'b rule::Rule> {
    let mut out = Vec::new();
    for rule in RULES.values() {
        if rule.does_match(&path) {
            out.push(rule);
        }
    }
    out
}

/// Adds `path` to the DAG if it was not already in it, then recursively adds
/// all children that did not exist yet to the DAG as well.
fn generate_children(path: String) {
    let matching_rules = get_matching_rules(&path);
    rayon::scope(|s| {
        for rule in matching_rules.iter() {
            let path = &path;
            s.spawn(move |_| {
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

                    new_file = rule.get_output(&path).to_string();

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
            });
        }
    });
}
