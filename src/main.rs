mod config;

use std::env;
use std::fs::{OpenOptions, File};
use std::convert::TryFrom;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use byteorder::{LE, WriteBytesExt};
use libflate::zlib::Encoder;
use path_slash::{PathBufExt, PathExt};
use globwalk::{GlobWalkerBuilder, DirEntry, WalkError};

use config::*;
use std::error::Error;

#[derive(Debug)]
struct Header {
    items: Vec<HeaderItem>
}

#[derive(Debug)]
struct HeaderItem {
    path: PathBuf,
    name: Option<String>,
    header_offset: Option<usize>,
    data_offset: Option<u64>,
    data_length: Option<u64>,
}

impl HeaderItem {
    fn new(path: PathBuf) -> Result<HeaderItem, String> {
        if let None = path.to_str() {
            return Err(format!("File name is invalid UTF-8 string"));
        }

        Ok(HeaderItem {
            path,
            name: None,
            header_offset: None,
            data_offset: None,
            data_length: None,
        })
    }

    fn length(&self) -> usize {
        let name = match self.name.as_ref() {
            Some(name) => name,
            None => return 0
        };

        std::mem::size_of::<u64>() +                        // data offset
            std::mem::size_of::<u64>() +                    // data length
            std::mem::size_of::<u16>() +                    // name length
            name.as_bytes().len() +                         // name
            std::mem::size_of::<u16>() +                    // path length
            self.path.to_str().unwrap().as_bytes().len()    // path
    }

    fn write(&mut self, pos: usize, file: &mut File) -> Result<usize, String> {
        fn err_mapper(e: impl Error) -> String {
            e.to_string()
        }

        let name = match self.name.as_ref() {
            Some(name) => name,
            None => return Err(format!("Header item name not specified"))
        };

        self.header_offset = Some(pos);

        // TODO: force all operations to be in little endian format

        // write offset
        file.write_u64::<LE>(self.data_offset.unwrap_or(0)).map_err(err_mapper);

        // write length
        file.write_u64::<LE>(self.data_length.unwrap_or(0)).map_err(err_mapper);

        // write name
        let name_length = u16::try_from(name.len())
            .map_err(|_| format!("Name string is too long"))?;

        file.write_u16::<LE>(name_length).map_err(err_mapper);
        file.write_all(name.as_bytes()).map_err(err_mapper);

        // write path
        let path = self.path.to_str().unwrap();
        let path_length = u16::try_from(path.len())
            .map_err(|_| format!("Path string is too long"))?;

        file.write_u16::<LE>(path_length).map_err(err_mapper);
        file.write_all(path.as_bytes()).map_err(err_mapper);

        Ok(pos + self.length())
    }
}

fn create_header(base_directory: &Path, config: &Config) -> Result<Header, String> {
    let mut named_items = vec![];

    let mut items: Vec<HeaderItem> = config.content.iter()
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
        .filter(|entry| {
            if let Ok(entry) = entry {
                entry.path().is_file()
            } else {
                true
            }
        })
        .map(|entry| -> Result<HeaderItem, String> {
            Ok(HeaderItem::new(entry?.into_path())?)
        })
        .collect::<Result<Vec<HeaderItem>, String>>()?;

    // TODO: append named

    for i in 0..items.len() {
        let path = path_relative_from(items[i].path.as_path(), base_directory);
        let path = match path {
            Some(path) => path,
            None => return Err(format!("Unable to create relative path"))
        };

        items[i].name = Some(match path.to_slash() {
            Some(path) => path,
            None => return Err(format!("Unable to convert file name to slashed form"))
        });
    }

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

fn app() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();

    let config_path = Path::new(args[1].as_str());
    if !config_path.is_file() {
        return Err(format!("Specified config must be a file"));
    }

    let base_directory = config_path.parent().unwrap();

    // read yaml config
    let configs = parse_configs(config_path)?;

    configs.iter().map(|config| -> Result<(), String> {
        println!("Output: {}", config.output);

        let config_base = config.base.as_ref().map(|base| {
            base_directory.join(Path::new(base.as_str()))
        });

        let base_directory = match config_base.as_ref() {
            Some(base) => base.as_path(),
            None => base_directory
        };

        let mut header = create_header(base_directory, config)?;

        let output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(base_directory.join(config.output.as_str()));

        let mut output = match output {
            Ok(file) => file,
            Err(e) => return Err(e.to_string())
        };

        // write header
        let mut pos: usize = 0;
        for i in 0..header.items.len() {
            pos = header.items[i].write(pos, &mut output)?;
        }

        for i in 0..header.items.len() {
            println!("{}", header.items[i].name.as_ref().unwrap());

            let mut input_file = OpenOptions::new()
                .read(true)
                .open(header.items[i].path.as_path())
                .map_err(|e| e.to_string())?;

            let mut encoder = Encoder::new(output).unwrap();
            io::copy(&mut input_file, &mut encoder).unwrap();

            output = encoder.finish()
                .into_result().map_err(|_| "Unable to encode data")?;
        }

        Ok(())
    }).collect::<Result<(), String>>()?;

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
