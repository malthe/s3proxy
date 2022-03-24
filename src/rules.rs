use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use http::{header::HeaderName, Method};
use http::{HeaderMap, HeaderValue};
use hyper::Request;

pub struct Rule {
    headers: HeaderMap,
    methods: Vec<Method>,
    paths: Vec<String>,
}

impl Rule {
    pub fn check<T>(&self, req: &Request<T>) -> bool {
        self.methods.contains(req.method())
            && (self
                .paths
                .iter()
                .any(|path| req.uri().path().starts_with(path)))
            && (self
                .headers
                .iter()
                .any(|(name, value)| req.headers().get(name) == Some(value)))
    }
}

pub fn parse_rules<'a>(f: File) -> Result<Vec<Rule>> {
    let reader = BufReader::new(f);
    let mut rules = Vec::new();
    for s in reader.lines() {
        let mut headers = HeaderMap::new();
        let mut paths = Vec::new();
        let mut methods = Vec::new();
        for token in s?.split_ascii_whitespace() {
            match token.split_once('=') {
                Some((header, value)) => {
                    headers.insert(HeaderName::from_str(header)?, HeaderValue::from_str(value)?);
                }
                None => match token.chars().next() {
                    Some(c) => {
                        if c.is_ascii_uppercase() {
                            http::Method::from_str(token)?;
                            let method = Method::from_str(token)?;
                            methods.push(method);
                        } else if c == '/' {
                            paths.push(String::from(token));
                        } else {
                            return Err(anyhow!("Invalid token: {}", token.clone()));
                        }
                    }
                    None => {
                        unreachable!();
                    }
                },
            }
        }
        let rule = Rule {
            headers,
            methods,
            paths,
        };
        rules.push(rule);
    }
    Ok(rules)
}
