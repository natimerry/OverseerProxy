mod proxy;

use clap::Parser;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info, trace, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use bytes::Bytes;
use clap_derive::Parser;
use colored::Colorize;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::Frame;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{body::Body, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use hyper::client::conn::http1 as client_http1;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = String::from("0.0.0.0"))]
    host: String,
    #[arg(long, default_value_t = 3000)]
    port: i32,
    #[arg(long, default_value_t = false)]
    restrict: bool,
}
use proxy::proxy;

async fn log(
    req: Request<hyper::body::Incoming>,
    client: SocketAddr,
    restrict: bool,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Box<dyn std::error::Error + Send + Sync>> {
    let path = req.uri().to_string();
    let headers = debug!("{:#?}",&req.headers());
    info!("{} request {}", client.to_string().blue(), path.red());
    return proxy::proxy(req, restrict).await;
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let debug_file =
        tracing_appender::rolling::hourly("./logs/", "debug").with_max_level(tracing::Level::TRACE);

    let warn_file = tracing_appender::rolling::hourly("./logs/", "warnings")
        .with_max_level(tracing::Level::WARN);
    let all_files = debug_file.and(warn_file);

    tracing_subscriber::registry()
        .with(LevelFilter::TRACE)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(all_files)
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(true)
                .with_writer(std::io::stdout.with_max_level(Level::TRACE))
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    let args = Args::parse();
    info!("Running webserver on {}:{}", args.host, args.port);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .expect("unable to parse socket address");
    trace!("SocketAddr: {:#?}", addr);

    let listener = TcpListener::bind(addr).await?;

    trace!("RESTRICT IS {}",args.restrict);
    loop {
        let (stream, client) = listener.accept().await?;
        trace!("{:?}", &stream);

        debug!("Spawning new thread for client {}", client);
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = server_http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, service_fn(move |req| log(req, client, args.restrict)))
                .with_upgrades()
                .await
            {

            }
        });
    }

    unreachable!()
}
