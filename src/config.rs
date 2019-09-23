use crate::rule::Rule;

#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_folders")]
    folders: Vec<String>,

    #[serde(rename(deserialize = "rule"))]
    rules: Vec<Rule>,
}

fn default_folders() -> Vec<String> {
    vec![String::from(".")]
}
