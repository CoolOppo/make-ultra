use std::collections::HashMap;
use std::io::Read;
use std::fs::File;
use std::path::Path;
use std::error::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
    pub from: String,
    pub to: String,
    pub dependers: Vec<Rule>,
}

pub fn read_rules() -> HashMap<String,Rule> {
    let path = Path::new("rules.json");

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

        let mut rules = HashMap::<String, Rule>::new();
        rules.insert(
            String::from("Rust Files"),
            Rule {
                from: String::from("(?P<name>.*)\\.js$"),
                to: String::from("<name>.min.js"),
                dependers: vec![Rule {
                    from: String::from("(?P<name>.*)\\.min\\.js$"),
                    to: String::from("<name>.min.js.gz"),
                    dependers: vec![],
                }],
            },
        );
        File::create("rules.json")
            .unwrap()
            .write_all(serde_json::to_string_pretty(&rules).unwrap().as_bytes())
            .unwrap();
        File::create("rules.toml")
            .unwrap()
            .write_all(toml::to_string_pretty(&rules).unwrap().as_bytes())
            .unwrap();
        File::create("rules.ron")
            .unwrap()
            .write_all(
                ron::ser::to_string_pretty(
                    &rules,
                    ron::ser::PrettyConfig {
                        ..Default::default()
                    },
                )
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
    }
}
