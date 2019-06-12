mod config;

use std::env;
use std::io::{self, Write};
use std::fs::OpenOptions;

use libflate::zlib::Encoder;

use config::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        print_help();
        std::process::exit(-1);
    }

    // read yaml config
    let configs = match parse_configs(args[1].as_str()) {
        Ok(c) => c,
        Err(e) => {
            log_err(e.as_str());
            std::process::exit(-1);
        }
    };

    configs.iter().for_each(|config| {
        println!("Output: {}", config.output);
        println!("Content: {:#?}\n", config.content);
    });

    // encode file

    let mut input_file = OpenOptions::new()
        .read(true)
        .open(args[1].as_str())
        .expect("Unable to open file");

    let output_file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(format!("{}.pak", args[1]).as_str())
        .expect("Unable to create output file");

    let mut encoder = Encoder::new(output_file).unwrap();
    io::copy(&mut input_file, &mut encoder).unwrap();

    let encoded_data = encoder.finish()
        .into_result().expect("Unable to encode data");
}

fn log_err(error: &str) {
    io::stderr().write(format!("Error: {}\n", error).as_bytes()).unwrap();
}

fn print_help() {
    println!("packer v0.1\n");
    println!("USAGE:");
    println!("\tpacker [OPTIONS]\n");
    println!("OPTIONS:");
    println!("\t-c, --config <FILE>\t\tSets custom config file");
    println!("\t-o, --output <FILE>\t\tSets custom output file");
}
