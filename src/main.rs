mod config;

use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bytes::BufMut;
use libflate::zlib::Encoder;
use globwalk::{GlobWalkerBuilder, DirEntry, WalkError};

use config::*;

#[derive(Debug)]
struct Header<'a> {
    items: Vec<HeaderItem<'a>>
}

impl<'a> Header<'a> {
    fn finalize(header: &mut Header) {
        let mut offset = 0;
    }
}

#[derive(Debug)]
struct HeaderItem<'a> {
    path: PathBuf,
    name: Option<&'a str>,
    data_offset: Option<u64>,
    data_length: Option<u64>,
}

impl<'a> HeaderItem<'a> {
    fn new(path: PathBuf) -> Result<HeaderItem<'a>, String> {
        if let None = path.to_str() {
            return Err(format!("File name is invalid UTF-8 string"));
        }

        Ok(HeaderItem {
            path,
            name: None,
            data_offset: None,
            data_length: None,
        })
    }

    fn length(&self) -> usize {
        let name = match self.name {
            Some(name) => name,
            None => return 0
        };

        std::mem::size_of::<u16>() +                        // name length
            name.as_bytes().len() +                         // name
            std::mem::size_of::<u16>() +                    // path length
            self.path.to_str().unwrap().as_bytes().len() +  // path
            std::mem::size_of::<u64>() +                    // data offset
            std::mem::size_of::<u64>()                      // data length
    }

    fn write(&self, buf: &mut BufMut) {
    }
}

fn create_header<'a>(base_directory: &Path, config: &Config) -> Result<Header<'a>, String> {
    let mut named_items = vec![];

    let items = config.content.iter()
        .filter_map(|item| {
            let pattern = match item {
                ContentItem::Unnamed { pattern } => pattern,
                ContentItem::Named { .. } => {
                    named_items.push(item);
                    return None;
                }
            };

            let glob = GlobWalkerBuilder::new(base_directory, pattern)
                .min_depth(1)
                .build()
                .map_err(|e| {
                    e.to_string()
                });

            Some(glob)
        })
        .flat_map(|glob| {
            let (v, r) = match glob {
                Ok(glob) => (Some(glob.into_iter()), None),
                Err(e) => (None, Some(Err(e)))
            };

            v.into_iter()
                .flatten()
                .map(|entry: Result<DirEntry, WalkError>| {
                    entry.map_err(|e| e.to_string())
                })
                .chain(r)
        })
        .map(|entry| -> Result<HeaderItem, String> {
            Ok(HeaderItem::new(entry?.into_path())?)
        })
        .collect::<Result<Vec<HeaderItem>, String>>()?;

    // TODO: update items and append named

    Ok(Header {
        items
    })
}

fn path_relative_from(path: &Path, base: &Path) -> Option<PathBuf> {
    use std::path::Component;

    if path.is_absolute() != base.is_absolute() {
        if path.is_absolute() {
            Some(PathBuf::from(path))
        } else {
            None
        }
    } else {
        let mut ita = path.components();
        let mut itb = base.components();
        let mut comps: Vec<Component> = vec![];
        loop {
            match (ita.next(), itb.next()) {
                (None, None) => break,
                (Some(a), None) => {
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
                (None, _) => comps.push(Component::ParentDir),
                (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                (Some(a), Some(b)) if b == Component::CurDir => comps.push(a),
                (Some(_), Some(b)) if b == Component::ParentDir => return None,
                (Some(a), Some(_)) => {
                    comps.push(Component::ParentDir);
                    for _ in itb {
                        comps.push(Component::ParentDir);
                    }
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
            }
        }
        Some(comps.iter().map(|c| c.as_os_str()).collect())
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        print_help();
        std::process::exit(-1);
    }

    let config_path = Path::new(args[1].as_str());
    if !config_path.is_file() {
        print_help();
        std::process::exit(-1);
    }

    let base_directory = config_path.parent().unwrap();

    // read yaml config
    let configs = match parse_configs(config_path) {
        Ok(c) => c,
        Err(e) => {
            log_err(e.as_str());
            std::process::exit(-1);
        }
    };

    configs.iter().for_each(|config| {
        println!("Output: {}", config.output);

        let config_base = config.base.as_ref().map(|base| {
            base_directory.join(Path::new(base.as_str()))
        });

        let base_directory = match config_base.as_ref() {
            Some(base) => base.as_path(),
            None => base_directory
        };

        if let Err(e) = create_header(base_directory, config) {
            log_err(e.as_str());
            std::process::exit(-1);
        }
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
