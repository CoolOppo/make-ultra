use hashbrown::HashMap;
use regex::Regex;
use std::{error::Error, fs::File, io::Read, path::Path, process::exit};

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
    #[serde(with = "serde_regex")]
    pub from: Regex,
    pub to: String,
    #[serde(with = "serde_regex")]
    #[serde(default)]
    pub exclude: Option<Regex>,
    pub command: &'static str,
}

impl Rule {
    pub fn get_output<'a>(&self, path: &'a str) -> std::borrow::Cow<'a, str> {
        self.from.replace_all(path, &*self.to)
    }

    pub fn does_match(&self, path: &str) -> bool {
        if self.from.is_match(path) {
            if let Some(x) = &self.exclude {
                if x.is_match(path) {
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
        Err(_why) => {
            println!("ERROR: Couldn't open rule file");
            exit(1);
        }
        Ok(file) => file,
    };
    
    static mut S: String = { String::new() };
    unsafe {
        if let Err(why) = file.read_to_string(&mut S) {
            println!(
                "ERROR: Couldn't read {}: {}",
                path.display(),
                why.description()
            );
            exit(1);
        };

        toml::from_str(&S).unwrap()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generate_rules() {
        #![allow(clippy::trivial_regex)]
        use crate::rule::Rule;
        use regex::Regex;
        use std::{fs::File, io::Write};
        let rules = hashmap! {
            String::from("minify")=> Rule {
                from: Regex::new("(?P<name>.*)\\.js$").unwrap(),
                to: String::from("$name.min.js"),
                command: "terser $i -o $o",
                exclude: Some(Regex::new("\\.min\\.js$").unwrap()),
            },
            String::from("brotli") => Rule {
                from: Regex::new("(?P<name>.*)\\.min\\.js$").unwrap(),
                to: String::from("$name.min.js.br"),
                command: "brotli -f $i",
                exclude: None,
            }
        };
        File::create("rules.toml")
            .unwrap()
            .write_all(toml::to_string_pretty(&rules).unwrap().as_bytes())
            .unwrap();
    }
}
