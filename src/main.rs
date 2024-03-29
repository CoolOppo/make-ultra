#![warn(clippy::all)]

use std::{
    fs::{self, File},
    hash::{BuildHasher, Hasher},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use bincode::{deserialize, serialize};
use cached::cached;
use clap::{crate_version, App, Arg};
use crossbeam::channel::unbounded;
use hashbrown::{hash_map::DefaultHashBuilder, HashMap};
use ignore::WalkBuilder;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use petgraph::{prelude::*, stable_graph::StableDiGraph};
use rayon::prelude::*;

mod config;
mod rule;

const CACHE_PATH: &str = ".make_cache";

lazy_static! {
    static ref MATCHES: clap::ArgMatches<'static> =  clap_setup() ;
    static ref DOT: bool = MATCHES.is_present("dot");
    static ref DRY_RUN: bool = MATCHES.is_present("dry_run");
    static ref FORCE: bool = MATCHES.is_present("force");

    static ref CONFIG: config::Config = config::read_config();
    static ref RULES: &'static Vec<rule::Rule> = &CONFIG.rules;
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
                    println!("WARNING: Invalid {} file", CACHE_PATH);
                    None
                }
            } else if let Ok(bytes) = fs::read(CACHE_PATH) {
                if let Ok(hashes) = deserialize(&bytes) {
                    println!("INFO: Reading old cache file format.");
                    Some(hashes)
                } else {
                    println!("WARNING: Invalid {} file", CACHE_PATH);
                    None
                }
            } else {
                println!("WARNING: Invalid {} file", CACHE_PATH);
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
        .version(crate_version!())
        .get_matches()
}

fn main() {
    lazy_static::initialize(&MATCHES);

    rayon::scope(move |s| {
        let (tx, rx) = unbounded();
        s.spawn(move |_| {
            for folder in &CONFIG.folders {
                WalkBuilder::new(Path::new(folder))
                    .standard_filters(false)
                    .build_parallel()
                    .run(|| {
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
            }
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

        for i in g.node_indices().filter(|n| is_root_node(&g, *n)) {
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

fn is_root_node<N, E>(g: &StableGraph<N, E>, n: NodeIndex) -> bool {
    let incoming_count = g.neighbors_directed(n, Incoming).count();
    incoming_count == 0
        || incoming_count == 1 // Nodes that only have an input from themselves are also roots.
        && g.neighbors_directed(n, Incoming)
        .next()
        .unwrap()
        == n
}

fn run_commands(node: NodeIndex) {
    rayon::scope(move |_| {
        let g = FILE_GRAPH.read();
        let files = FILES.read();

        // We only care about the file's hash if other files are dependent on it
        let should_save_hash = g.edges_directed(node, Outgoing).count() > 0;

        let source_path = &*g[node];
        let should_run_command = if *FORCE {
            true
        } else if let Some(old_hashes) = SAVED_HASHES.as_ref() {
            if let Some(saved_hash) = old_hashes.get(source_path) {
                if let Some(current_hash) = file_hash(source_path) {
                    // If the current hash isn't the same as the saved one,
                    // we should run the command and process the file:
                    current_hash != *saved_hash
                } else {
                    println!("WARNING: Could not read `{}`.", source_path);
                    false
                }
            } else {
                // No saved hash for this file
                true
            }
        } else {
            // No saved hashes at all
            true
        };

        g.edges_directed(node, Outgoing)
            .par_bridge()
            .for_each(|edge| {
                let target_path = &*g[edge.target()];
                let should_process_child;

                if should_run_command {
                    let full_command = split_command(&edge.weight().command);
                    let program = full_command[0];
                    let args = {
                        let mut args = Vec::new();
                        for arg in full_command.iter().skip(1) {
                            args.push(arg.replace("$i", source_path).replace("$o", target_path));
                        }
                        args
                    };

                    println!("{} {}", program, args.join(" "));
                    if *DRY_RUN {
                        should_process_child = true;
                    } else {
                        use std::process::Command;
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
                                should_process_child = false;
                            } else {
                                should_process_child = true;
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
                                should_process_child = false;
                            } else {
                                should_process_child = true;
                            }
                        }
                        if should_save_hash && should_process_child {
                            //                 ^^^^^^^^^^^^^^^^^^^^
                            // We should only update the hash if there was no
                            // error. If we update the hash when an error
                            // occurred, the command will not be rerun if
                            // whatever caused the error on our user's end is
                            // fixed, even though the output file was never
                            // created.
                            update_hash(source_path);
                        }
                    }
                } else if Path::exists(Path::new(target_path)) {
                    // If we never run the command, our child may
                    // still need commands run if it was modified
                    should_process_child = true;
                } else {
                    should_process_child = false;
                }
                if edge.source() != edge.target() && should_process_child {
                    run_commands(*files.get(target_path).unwrap());
                }
            });
    });
}

/// Gets the hash for the file at the given path
fn file_hash(path: &str) -> Option<u64> {
    if let Ok(file_bytes) = fs::read(path) {
        let mut hasher = DefaultHashBuilder::default().build_hasher();
        hasher.write(&file_bytes);
        Some(hasher.finish())
    } else {
        println!("ERROR: Could not read `{}`", path);
        None
    }
}

/// Inserts or overrides the hash for the given path in `NEW_HASHES`
fn insert_hash(path: &str, new_hash: u64) {
    let mut new_hashes = NEW_HASHES.write();
    new_hashes.insert(path.to_string(), new_hash);
}

/// Generates the hash for the given path, inserting to or overriding the stored
/// one in `NEW_HASHES`
fn update_hash(path: &str) -> Option<u64> {
    if let Some(new_hash) = file_hash(path) {
        insert_hash(path, new_hash);
        Some(new_hash)
    } else {
        println!("WARNING: Unable to read `{}` to find its hash.", path);
        None
    }
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

fn get_matching_rules(path: &str) -> Vec<&'static rule::Rule> {
    let mut out = Vec::new();
    for rule in RULES.iter() {
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
        let matching_rules = &matching_rules;
        for rule in matching_rules.iter() {
            let path = &path;
            s.spawn(move |_| {
                let new_file: String;
                let mut should_update_children = false;
                {
                    new_file = rule.get_output(&path).to_string();

                    // "Smart" rule exclusion.
                    // TODO: Make recursive instead of only evaluating the child rules
                    // Imagine we have a rule that would match *.js, turning it into *.min.js.
                    // Now imagine we have another rule that does something to *.min.js files.
                    // Given the file a.min.js, we need to be able to determine that only the
                    // *.min.js rule should run:
                    if matching_rules.len() > 1 {
                        let new_file_rules = get_matching_rules(&new_file);
                        if new_file_rules.eq(matching_rules) {
                            return;
                        }
                    }

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
