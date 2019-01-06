use crate::rule;
#[derive(Serialize, Deserialize, Debug)]
pub struct File<'a> {
    //    pub name: String,
    pub path: String,
    #[serde(skip)]
    pub rule: Option<&'a rule::Rule>,
}
