
use std::fs::File;
use std::io::prelude::*;
use yaml_rust2::{
    Yaml,
    YamlLoader
};

pub mod client;
pub mod server;

pub fn load_config() -> Yaml {
    let mut f = File::open("../config.yaml").expect("config.yaml file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents).expect("Something went wrong reading the file");
    let config = YamlLoader::load_from_str(&contents).unwrap();
    config[0].clone()
}

