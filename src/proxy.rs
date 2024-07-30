use std::collections::HashSet;
use std::fmt::Error;
use std::fs;
use std::hash::Hash;
use std::io::read_to_string;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::LazyLock;

use bytes::Bytes;
use clap::builder::Str;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::client::conn::http1::Builder;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use log::{debug, info};

use tokio::net::{TcpListener, TcpStream};
use tracing::error;
static DOMAIN_LIST: LazyLock<Vec<String>> = LazyLock::new(|| {
    let content = fs::read_to_string("domain_list").expect("Bhai baaler file exist kore naa");

    content
        .lines()
        .map(|line|{
            let mut x = line.replace("\n", "");
            if x.contains("www."){
                x = x.strip_prefix("www.").unwrap().to_string();
            }
            x
        })
        .collect::<Vec<String>>()
});
pub async fn proxy(
    req: Request<hyper::body::Incoming>,
    restrict: bool,
) -> std::result::Result<
    hyper::Response<http_body_util::combinators::BoxBody<Bytes, hyper::Error>>,
    Box<dyn std::error::Error + Send + Sync>,
> {


    let mut uri = req
        .uri()
        .to_string()
        .strip_suffix(":443")
        .unwrap()
        .to_string();

    if uri.contains("www."){
        uri = uri.strip_prefix("www.").unwrap().to_string();
    }

    if !check_allow(restrict,uri){
        // error!("Domains not allowed");
        return Err(Box::from("error"));
    }
    if Method::CONNECT == req.method() {
        if let Some(addr) = host_addr(req.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, addr).await {
                            error!("Server I/O error: {}", e);
                        };
                    }
                    Err(e) => error!("Upgrade Error: {}", e),
                }
            });

            Ok(Response::new(empty()))
        } else {
            eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(full("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        let host = req.uri().host().expect("uri has no host");
        let port = req.uri().port_u16().unwrap_or(80);

        let stream = TcpStream::connect((host, port)).await.unwrap();
        let io = TokioIo::new(stream);

        let (mut sender, conn) = Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                error!("Connection failed: {:?}", err);
            }
        });

        let resp = sender.send_request(req).await?;
        Ok(resp.map(|b| b.boxed()))
    }
}

fn check_allow(restrict: bool, uri: String) -> bool{
    for x in DOMAIN_LIST.iter() {
        if x == &uri {
            if restrict {
                error!("Prohibited domain {x} accessed");
                return false;
            } else {
                debug!("Allowing domain");
                return true
            }
        }
    }
    if !restrict{
        return false;
    }
    return true;
}

fn host_addr(uri: &http::Uri) -> Option<String> {
    uri.authority().and_then(|auth| Some(auth.to_string()))
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn tunnel(upgraded: Upgraded, addr: String) -> std::io::Result<()> {
    // Connect to remote server
    let mut server = TcpStream::connect(addr).await?;
    let mut upgraded = TokioIo::new(upgraded);

    // Proxying data
    let (from_client, from_server) =
        tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

    // Print message when done
    info!(
        "client wrote {} bytes and received {} bytes",
        from_client, from_server
    );

    Ok(())
}
