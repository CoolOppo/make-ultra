pub fn say_hi() {
    println!("Hi");
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MyFile {
	pub name: String,
	pub path: String
}
