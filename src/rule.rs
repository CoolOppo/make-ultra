use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use regex::Regex;

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
    #[serde(with = "serde_regex")]
    pub from: Regex,
    pub to: String,
    #[serde(with = "serde_regex")]
    #[serde(default)]
    pub exclude: Option<Regex>,
    pub command: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub next: HashMap<String, Rule>,
}

impl Rule {
    pub fn get_output(&self, path: &str) -> String {
        String::from(self.from.replace_all(path, &*self.to))
    }

    pub fn does_match(&self, path: &str) -> bool {
        if self.from.is_match(path) {
            if let Some(x) = &self.exclude {
                if x.is_match(path) {
                    return false;
                }
            }
            let x = &self.next;
            for depender in x.values() {
                if depender.from.is_match(&path) {
                    return false;
                }
            }
            return true;
        }
        false
    }
}

impl PartialEq for Rule {
    fn eq(&self, other: &Rule) -> bool {
        self.from.as_str() == other.from.as_str()
            && self.to == other.to
            && self.command == other.command
    }
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
        #![allow(clippy::trivial_regex)]
        use crate::rule::Rule;
        use regex::Regex;
        use std::fs::File;
        use std::io::Write;
        let rules = hashmap! {
            String::from("minify")=> Rule {
                from: Regex::new("(?P<name>.*)\\.js$").unwrap(),
                to: String::from("$name.min.js"),
                command: String::from("terser $i -o $o"),
                exclude: Some(Regex::new("\\.min\\.js$").unwrap()),
                next: hashmap! {
                    String::from("gzip") => Rule {
                        from: Regex::new("(?P<name>.*)\\.min\\.js$").unwrap(),
                        to: String::from("$name.min.js.gz"),
                        command: String::from("zopfli $i"),
                        exclude: None,
                        next: hashmap!{},
                    },
                }
            },
            String::from("brotli") => Rule {
                from: Regex::new("(?P<name>.*)\\.min\\.js$").unwrap(),
                to: String::from("$name.min.js.br"),
                command: String::from("brotli -f $i"),
                exclude: None,
                next: hashmap!{},
            }
        };
        File::create("rules.toml")
            .unwrap()
            .write_all(toml::to_string_pretty(&rules).unwrap().as_bytes())
            .unwrap();
    }
}
