use std::convert::Infallible;
use std::fs::File;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{anyhow, Result as R};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode};
use hyper_rustls::HttpsConnectorBuilder;
use log::{debug, info, warn};
use serde::Deserialize;

use crate::rules::{parse_rules, Rule};

mod rules;

const PORT: u16 = 8080;
const POOL_TIMEOUT: u64 = 30;
const RULES_PATH: &str = "rules.txt";
const CONNECT_TIMEOUT: u64 = 1000;
const READ_TIMEOUT: u64 = 5000;
const WRITE_TIMEOUT: u64 = 5000;

#[derive(Deserialize)]
struct Config {
    bind_address: Option<String>,
    connect_timeout: Option<u64>,
    read_timeout: Option<u64>,
    write_timeout: Option<u64>,
    port: Option<u16>,
    rules_path: Option<String>,
    s3_url: String,
    s3_account_key: String,
    s3_account_secret: String,
    s3_region: String,
}

fn aws_sign_v4<B>(config: &Config, req: &mut Request<B>) -> R<()> {
    let parsed = url::Url::parse(&config.s3_url)?;
    let url = format!(
        "{}{}?{}",
        config.s3_url,
        req.uri().path(),
        req.uri().query().unwrap_or("")
    );
    let url = url.strip_suffix("?").unwrap_or(&url);
    let datetime = chrono::Utc::now();
    let method = req.method().clone();
    let headers = req.headers_mut();
    let content_type = headers.remove(http::header::CONTENT_TYPE);
    let content_length = headers.remove(http::header::CONTENT_LENGTH);
    headers.clear();
    if let Some(content_type) = content_type {
        headers.insert(http::header::CONTENT_TYPE, content_type);
    }
    if let Some(content_length) = content_length {
        headers.insert(http::header::CONTENT_LENGTH, content_length);
    }
    headers.insert(
        "X-Amz-Date",
        datetime.format("%Y%m%dT%H%M%SZ").to_string().parse()?,
    );
    headers.insert("X-Amz-Content-Sha256", "UNSIGNED-PAYLOAD".parse()?);
    let host = parsed.host_str().unwrap_or("").parse()?;
    headers.insert("host", host);
    let s = aws_sign_v4::AwsSign::new(
        method.as_str(),
        url,
        &datetime,
        &headers,
        &config.s3_region,
        &config.s3_account_key,
        &config.s3_account_secret,
    );
    let signature = s.sign();
    headers.insert("authorization", signature.parse()?);
    *req.uri_mut() = http::Uri::from_str(url)?;
    Ok(())
}

fn err_response(status_code: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status_code)
        .body(Body::empty())
        .unwrap()
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
}

#[tokio::main]
pub async fn main() -> R<()> {
    pretty_env_logger::init();
    let config = envy::from_env::<Config>()?;
    let rules_path = config.rules_path.as_deref().unwrap_or(RULES_PATH);
    let rules = match File::open(rules_path) {
        Ok(f) => {
            let rules = parse_rules(f)?;
            info!("Parsed {} rule(s) from {}", rules.len(), rules_path);
            Some(rules)
        }
        Err(e) => {
            if config.rules_path.is_some() {
                return Err(anyhow!("{}: {}", e, rules_path));
            } else {
                info!("{}: {}; running in unrestricted mode", rules_path, e);
            }
            None
        }
    };

    let config: &'static Config = Box::leak(Box::new(config));
    let rules: &'static Option<Vec<Rule>> = Box::leak(Box::new(rules));

    let connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .build();

    let mut connector = hyper_timeout::TimeoutConnector::new(connector);
    let get_timeout =
        |config: Option<u64>, default: u64| Some(Duration::from_millis(config.unwrap_or(default)));
    connector.set_connect_timeout(get_timeout(config.connect_timeout, CONNECT_TIMEOUT));
    connector.set_read_timeout(get_timeout(config.read_timeout, READ_TIMEOUT));
    connector.set_write_timeout(get_timeout(config.write_timeout, WRITE_TIMEOUT));

    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(POOL_TIMEOUT))
        .build::<_, hyper::Body>(connector);

    let make_svc = make_service_fn(move |_conn| {
        let client = client.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |mut req| {
                let client = client.clone();

                async move {
                    debug!("{}: {}", req.method(), req.uri().to_string());

                    if let Some(rules) = rules {
                        if !(rules.iter().any(|r| r.check(&req))) {
                            return Ok::<_, hyper::Error>(err_response(StatusCode::UNAUTHORIZED));
                        }
                    }

                    aws_sign_v4(config, &mut req).unwrap_or_else(|e| {
                        warn!("Unable to sign request: {}", e);
                    });

                    client
                        .request(req)
                        .await
                        .map(|resp| {
                            if !resp.status().is_success() {
                                debug!("Unexpected status code: {}", resp.status());
                            }
                            resp
                        })
                        .or_else(|err| {
                            warn!("Error getting request: {}", err);
                            Ok::<_, hyper::Error>(err_response(StatusCode::BAD_REQUEST))
                        })
                }
            }))
        }
    });
    let addr = match &config.bind_address {
        Some(s) => s.parse()?,
        None => ([0, 0, 0, 0], config.port.unwrap_or(PORT)).into(),
    };
    let server = Server::bind(&addr).serve(make_svc);
    info!("Listening on http://{}", addr);

    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;

    Ok(())
}
