

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
	pub pattern: String,
	pub new_extension: String
}
