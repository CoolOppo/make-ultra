use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
    #[serde(with = "serde_regex")]
    pub from: Regex,
    pub to: String,
    pub command: String,
    pub next: Option<Vec<Rule>>
}

pub fn read_rules() -> HashMap<String, Rule> {
    let path = Path::new("rules.toml");

    let mut file = match File::open(&path) {
        Err(_why) => panic!("couldn't open rule file"),
        Ok(file) => file,
    };

    let mut s = String::new();
    if let Err(why) = file.read_to_string(&mut s) {
        panic!("couldn't read {}: {}", path.display(), why.description())
    };

    toml::from_str(&s).unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn generate_rules() {
        use crate::rule::Rule;
        use std::collections::HashMap;
        use std::fs::File;
        use std::io::Write;
        use regex::Regex;
        let mut rules = HashMap::<String, Rule>::new();
        rules.insert(
            String::from("Rust Files"),
            Rule {
                from: Regex::new("(?P<name>.*)\\.js$").unwrap(),
                to: String::from("$name.min.js"),
                command: String::from("terser $i -o $o"),
                next: Some(vec![Rule {
                    from: Regex::new("(?P<name>.*)\\.min\\.js$").unwrap(),
                    to: String::from("$name.min.js.gz"),
                    command: String::from("wsl gzip -k $i"),
                    next: None,
                }]),
            },
        );
        File::create("rules.toml")
            .unwrap()
            .write_all(toml::to_string_pretty(&rules).unwrap().as_bytes())
            .unwrap();
    }
}
