use crate::rule::Rule;
use serde_derive::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_folders")]
    pub folders: Vec<String>,

    #[serde(rename(deserialize = "rule"))]
    pub rules: Vec<Rule>,
}

fn default_folders() -> Vec<String> {
    vec![String::from(".")]
}

pub fn read_config() -> Config {
    let contents =
        fs::read_to_string("makeultra.toml").expect("ERROR: Could not read makeultra.toml.");
    toml::from_str(&contents).unwrap()
}
