use std::fs;
use std::sync::OnceLock;

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::client::conn::http1::Builder;
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use log::{debug, error, info, trace};

use tokio::net::TcpStream;
static DOMAIN_LIST: OnceLock<Vec<String>> = OnceLock::new();

fn get_domain_list() -> &'static Vec<String> {
    DOMAIN_LIST.get_or_init(|| match fs::read_to_string("domain_list") {
        Ok(content) => content
            .lines()
            .map(|line| {
                let mut x = line.replace("\n", "");
                if x.contains("www.") {
                    x = x.strip_prefix("www.").unwrap().to_string();
                }
                x
            })
            .collect(),
        Err(_) => Vec::new(),
    })
}

pub async fn proxy(
    req: Request<hyper::body::Incoming>,
    restrict: bool,
) -> std::result::Result<
    hyper::Response<http_body_util::combinators::BoxBody<Bytes, hyper::Error>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut uri = req.uri().to_string();
    if uri.contains(":443") {
        uri = uri.strip_suffix(":443").unwrap().to_string();
    }
    if uri.contains("www.") {
        uri = uri.strip_prefix("www.").unwrap().to_string();
    }

    if !check_allow(restrict, uri) {
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
            trace!("Received response: \n\t {:#?}", resp);

            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        let host = req.uri().host().expect("uri has no host");
        let port = req.uri().port_u16().unwrap_or(80);

        let stream = TcpStream::connect((host, port)).await;
        if let Err(e) = &stream {
            error!("Error constructing TCP stream: {:?}", e);
        }
        let stream = stream.unwrap();

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
        trace!("Received response: \n\t {:#?}", resp);
        Ok(resp.map(|b| b.boxed()))
    }
}

fn check_allow(restrict: bool, uri: String) -> bool {
    for x in get_domain_list().iter() {
        if x == &uri {
            if restrict {
                error!("Prohibited domain {x} accessed");
                return false;
            } else {
                debug!("Allowing domain");
                return true;
            }
        }
    }
    if !restrict {
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
    let mut server = TcpStream::connect(addr.clone()).await?;
    let mut upgraded = TokioIo::new(upgraded);

    // Proxying data
    let (from_client, from_server) =
        tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

    debug!("Tunneling to {}", addr);
    info!(
        "client wrote {} bytes and received {} bytes",
        from_client, from_server
    );

    Ok(())
}
