use yaml_rust::{YamlLoader, Yaml};

#[derive(Debug)]
pub struct Config {
    pub output: String,
    pub content: Vec<ContentItem>,
}

#[derive(Debug)]
pub enum ContentItem {
    Unnamed {
        pattern: String
    },
    Named {
        name: String,
        pattern: String,
    },
}

pub fn parse_configs(path: &str) -> Result<Vec<Config>, String> {
    let docs = {
        let config = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(_) => return Err(format!("Unable to open config file"))
        };

        match YamlLoader::load_from_str(config.as_str()) {
            Ok(data) => data,
            Err(_) => return Err(format!("Unable to parse config file"))
        }
    };

    docs.iter().map(parse_document).collect()
}


enum ParsedContentItem {
    Single(ContentItem),
    Multiple(Vec<ContentItem>),
}

fn parse_document(doc: &Yaml) -> Result<Config, String> {
    let output = match doc["output"].as_str() {
        Some(path) => path,
        None => return Err(format!("'output' must be specified in config"))
    };

    let content = match doc["content"].as_vec() {
        Some(arr) => arr,
        None => return Err(format!("'content' must be specified in config as array"))
    };

    let content: Result<Vec<ContentItem>, String> = content.into_iter()
        .map(|it| {
            use ParsedContentItem::*;

            parse_content_item(it).map(|value| {
                match value {
                    Single(item) => vec![item].into_iter(),
                    Multiple(items) => items.into_iter(),
                }
            })
        })
        .flat_map(|it| {
            let (v, r) = match it {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(Err(e)))
            };

            v.into_iter()
                .flatten()
                .map(|item| Ok(item))
                .chain(r)
        })
        .collect();

    Ok(Config {
        output: String::from(output),
        content: content?,
    })
}

fn parse_content_item(content_item: &Yaml) -> Result<ParsedContentItem, String> {
    use ContentItem::*;

    fn parse_named((key, value): (&Yaml, &Yaml)) -> Result<ContentItem, String> {
        let key = match key.as_str() {
            Some(key) => key,
            None => return Err(format!("Named content item key must be a string"))
        };

        let value = match value.as_str() {
            Some(value) => value,
            None => return Err(format!("Named content item value must be a string pattern"))
        };

        Ok(ContentItem::Named {
            name: String::from(key),
            pattern: String::from(value),
        })
    }

    if let Some(pattern) = content_item.as_str() {
        return Ok(ParsedContentItem::Single(Unnamed{
            pattern: String::from(pattern)
        }));
    } else if let Some(map) = content_item.as_hash() {
        let mut iter = map.iter().peekable();

        let first = match iter.next() {
            Some(item) => parse_named(item),
            None => return Err(format!("Empty content rows are not allowed"))
        };

        if iter.peek().is_none() {
            return Ok(ParsedContentItem::Single(first?));
        }

        let res = std::iter::once(first)
            .chain(iter.map(parse_named))
            .collect::<Result<Vec<ContentItem>, String>>();

        return Ok(ParsedContentItem::Multiple(res?));
    }

    Err(format!("Unable to parse content item"))
}
