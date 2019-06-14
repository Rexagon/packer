mod config;
mod packer;

use std::env;
use std::path::Path;
use std::io::{self, Write};

use crate::config::*;
use crate::packer::*;

fn app() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();

    let configs_path = Path::new(args[1].as_str());
    if !configs_path.is_file() {
        return Err(format!("Specified config must be a file"));
    }

    // read yaml configs
    let configs = parse_configs(configs_path)?;

    let base_dir = configs_path.parent().unwrap();

    configs.iter()
        .map(|config| pack(base_dir, config))
        .collect::<Result<(), String>>()?;

    Ok(())
}

fn main() {
    if env::args().len() == 1 {
        print_help();
        std::process::exit(-1);
    }

    if let Err(e) = app() {
        io::stderr().write(format!("Error: {}\n", e).as_bytes()).unwrap();
        std::process::exit(-1);
    }
}

fn print_help() {
    println!("packer v0.1\n");
    println!("USAGE:");
    println!("\tpacker [OPTIONS]\n");
    println!("OPTIONS:");
    println!("\t-c, --config <FILE>\t\tSets custom config file");
    println!("\t-o, --output <FILE>\t\tSets custom output file");
}
