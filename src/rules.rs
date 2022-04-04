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
    query: Vec<Vec<String>>,
}

impl Rule {
    pub fn check<T>(&self, req: &Request<T>) -> bool {
        self.methods.contains(req.method())
            && (self.paths.iter().any(|path| {
                let p = req.uri().path();
                if let Some(path) = path.strip_suffix('$') {
                    p == path
                } else {
                    p.starts_with(path)
                }
            }))
            && (self
                .headers
                .iter()
                .all(|(name, value)| req.headers().get(name) == Some(value)))
            && (self.query.is_empty() || {
                let query = req.uri().query().and_then(|s| urlencoding::decode(s).ok());
                let params: Option<Vec<&str>> = query.as_deref().map(|s| s.split('&').collect());
                if let Some(params) = params {
                    // At least one query token must match, each one consisting of
                    // one or more parameters.
                    self.query
                        .iter()
                        .any(|query_pairs| query_pairs.iter().all(|q| params.contains(&q.as_str())))
                } else {
                    false
                }
            })
    }
}

pub fn parse_rules<'a>(f: File) -> Result<Vec<Rule>> {
    let reader = BufReader::new(f);
    let mut rules = Vec::new();
    for s in reader.lines() {
        let mut headers = HeaderMap::new();
        let mut paths = Vec::new();
        let mut methods = Vec::new();
        let mut query = Vec::new();
        for token in s?.split_ascii_whitespace() {
            match token.chars().next() {
                Some(c) if c == '/' => paths.push(String::from(token)),
                Some(c) if c == '?' => {
                    let q = &token[1..];
                    query.push(q.split('&').map(String::from).collect());
                }
                _ => {
                    if token.chars().all(|c| char::is_ascii_uppercase(&c)) {
                        http::Method::from_str(token)?;
                        let method = Method::from_str(token)?;
                        methods.push(method);
                    } else {
                        match token.split_once('=') {
                            Some((header, value)) => {
                                headers.insert(
                                    HeaderName::from_str(header)?,
                                    HeaderValue::from_str(value)?,
                                );
                            }
                            None => {
                                return Err(anyhow!("Invalid token: {}", token.clone()));
                            }
                        }
                    }
                }
            }
        }
        let rule = Rule {
            headers,
            methods,
            paths,
            query,
        };
        rules.push(rule);
    }
    Ok(rules)
}
