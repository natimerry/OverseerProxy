mod proxy;

use std::net::SocketAddr;
use env_logger::Builder;
use log::{debug, error, info, trace};


use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

struct Args {
    host: String,
    port: i32,
    restrict: bool,
}


impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let mut host = "0.0.0.0".to_string();
        let mut port = 8888;
        let mut restrict = true;
        
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => host = args.next().unwrap_or(host),
                "--port" => port = args.next().and_then(|s| s.parse().ok()).unwrap_or(port),
                "--restrict" => restrict = true,
                _ => {}
            }
        }
        
        Args { host, port, restrict }
    }
}

async fn log(
    req: Request<hyper::body::Incoming>,
    client: SocketAddr,
    restrict: bool,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Box<dyn std::error::Error + Send + Sync>> {
    let path = req.uri().to_string();
    // info!("{} request {}", client, path);
    return proxy::proxy(req, restrict).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Builder::new()
        .filter_level(log::LevelFilter::Info) 
        .parse_env("LOG_LEVEL") 
        .format_timestamp_secs() 
        .format_module_path(false)  
        .format_level(true) 
        .write_style(env_logger::WriteStyle::Auto) 
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
            if let Err(_) = server_http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, service_fn(move |req| log(req, client, args.restrict)))
                .with_upgrades()
                .await
            {
                error!("Error spawning thread");
            }
        });
    }
}

