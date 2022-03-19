use std::convert::Infallible;
use std::time::Duration;
use std::str::FromStr;

use anyhow::{Result as R};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode};
use hyper_rustls::HttpsConnectorBuilder;
use log::{debug, info, warn};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    bind_address: Option<String>,
    s3_url: String,
    s3_account_key: String,
    s3_account_secret: String,
    s3_region: String,
}

const PORT: u16 = 8080;
const POOL_TIMEOUT: u64 = 30;

fn aws_sign_v4<B>(config: &Config, req: &mut Request<B>) -> R<()> {
    let parsed = url::Url::parse(&config.s3_url)?;
    let url = format!("{}{}", config.s3_url, req.uri().path());
    let datetime = chrono::Utc::now();
    let method = req.method().clone();
    let headers = req.headers_mut();
    headers.insert(
        "X-Amz-Date",
        datetime
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
            .parse()?
    );
    headers.insert("X-Amz-Content-Sha256", "UNSIGNED-PAYLOAD".parse()?);
    let host = parsed.host_str().unwrap_or("").parse()?;
    headers.insert("host", host);
    let s = aws_sign_v4::AwsSign::new(
        method.as_str(),
        &url,
        &datetime,
        &headers,
        &config.s3_region,
        &config.s3_account_key,
        &config.s3_account_secret
    );
    let signature = s.sign();
    headers.insert("authorization", signature.parse()?);
    *req.uri_mut() = http::Uri::from_str(&url)?;
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
}

#[tokio::main]
pub async fn main() -> R<()> {
    let config = envy::from_env::<Config>()?;
    let config: &'static Config = Box::leak(Box::new(config));

    pretty_env_logger::init();

    let connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .build();

    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(POOL_TIMEOUT))
        .build::<_, hyper::Body>(connector);

    let make_svc = make_service_fn(move |_conn| {
        let client = client.clone();
        async move {
            Ok::<_, Infallible>(service_fn(
                move |mut req| {
                    debug!("URL: {}", req.uri().to_string());
                    let client = client.clone();
                    aws_sign_v4(config, &mut req).unwrap_or_else(|e| {
                       info!("Unable to sign request: {}", e);
                    });
                    async move {
                        client.request(req).await.or_else(
                            |err| {
                                warn!("Error getting request: {}", err);
                                Ok::<_, hyper::Error>(Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(Body::empty())
                                    .unwrap())
                            }
                        )
                    }
                }
            ))
        }
    });
    let addr = match &config.bind_address {
        Some(s) => s.parse()?,
        None => ([0, 0, 0, 0], PORT).into()
    };
    let server = Server::bind(&addr).serve(make_svc);
    info!("Listening on http://{}", addr);

    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;

    Ok(())
}
