//! The resolver against a server of our own: a local DNS over HTTPS server
//! with its own certificate authority, and a file server only it knows the
//! name of.
//!
//! Everything here is local, so the tests say the same thing on a machine
//! with a network and on one without: the chosen resolver is asked, the
//! answer comes back, and a download goes through it.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::*;

/// The address the fake resolver answers with.
const ANSWER: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

/// The name only the fake resolver knows.
const NAME: &str = "download.test";

/// The certificate authority and the server certificate it signs.
struct Authority {
    /// The authority's certificate, as the client is told to trust it.
    root: Vec<u8>,
    /// The server's certificate chain and key, for the TLS server.
    chain: Vec<rustls_pki_types::CertificateDer<'static>>,
    key: rustls_pki_types::PrivateKeyDer<'static>,
}

/// A certificate authority of our own, and a server certificate for
/// `127.0.0.1` it signs.
fn authority() -> Authority {
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).expect("ca params");
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_key = rcgen::KeyPair::generate().expect("ca key");
    let ca_cert = ca.self_signed(&ca_key).expect("ca certificate");

    let leaf = rcgen::CertificateParams::new(vec!["127.0.0.1".to_string()]).expect("leaf params");
    let leaf_key = rcgen::KeyPair::generate().expect("leaf key");
    let issuer = rcgen::Issuer::from_params(&ca, ca_key);
    let leaf_cert = leaf.signed_by(&leaf_key, &issuer).expect("leaf certificate");

    Authority {
        root: ca_cert.der().to_vec(),
        chain: vec![leaf_cert.der().clone(), ca_cert.der().clone()],
        key: rustls_pki_types::PrivateKeyDer::try_from(leaf_key.serialize_der())
            .expect("the key in the form rustls wants"),
    }
}

/// The DNS over HTTPS server: HTTP/2 over TLS, answering every query with
/// the same address, the way a resolver that knows one name would.
async fn spawn_doh_server(authority: &Authority) -> SocketAddr {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("the server's TLS versions")
    .with_no_client_auth()
    .with_single_cert(authority.chain.clone(), authority.key.clone_key())
    .expect("the server's certificate");
    config.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("the address");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(stream) = acceptor.accept(stream).await else {
                    return;
                };
                // A connection that goes wrong is the client's business; the
                // test asks the resolver and reads its answer.
                let _ = serve_h2(stream).await;
            });
        }
    });
    address
}

/// Answer one HTTP/2 connection: every request gets the same DNS answer.
async fn serve_h2(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut connection = h2::server::handshake(stream).await?;
    while let Some(request) = connection.accept().await {
        let (mut request, mut respond) = request?;
        let mut body = Vec::new();
        while let Some(chunk) = request.body_mut().data().await {
            body.extend_from_slice(&chunk?);
        }
        let response = http::Response::builder()
            .status(200)
            .header("content-type", "application/dns-message")
            .body(())
            .unwrap();
        let mut send = respond.send_response(response, false)?;
        send.send_data(bytes::Bytes::from(answer(&body)), true)?;
    }
    Ok(())
}

/// The DNS answer for a query: the question as it came, and one address.
fn answer(query: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // The id of the query, then the flags of a standard answer.
    out.extend_from_slice(&query[..2]);
    out.extend_from_slice(&[0x81, 0x80]);
    out.extend_from_slice(&query[4..6]); // one question
    out.extend_from_slice(&[0x00, 0x01]); // one answer
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // no authority, no extra
    // The question, as it came.
    let end = question_end(query);
    out.extend_from_slice(&query[12..end]);
    // The answer: a pointer to the name, an A record, four bytes of address.
    out.extend_from_slice(&[0xc0, 0x0c]);
    out.extend_from_slice(&[0x00, 0x01]); // A
    out.extend_from_slice(&[0x00, 0x01]); // IN
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x3c]); // 60 seconds
    out.extend_from_slice(&[0x00, 0x04]);
    out.extend_from_slice(&ANSWER.octets());
    out
}

/// Where the question of a query ends: after the name, its type and its class.
fn question_end(query: &[u8]) -> usize {
    let mut at = 12;
    while at < query.len() && query[at] != 0 {
        at += 1 + query[at] as usize;
    }
    // The zero byte that ends the name, then the type and the class.
    at + 5
}

/// A file server that answers every request with one line.
async fn spawn_file_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("the address");
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let body = b"the file";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body).await;
            });
        }
    });
    address
}

/// The resolver for the local server, and the address it is at.
fn local_choice(address: SocketAddr) -> ResolverChoice {
    ResolverChoice {
        resolver: Resolver::over_https(
            "127.0.0.1",
            format!("https://127.0.0.1:{}/dns-query", address.port()),
        ),
        fallback: false,
    }
}

/// The chosen resolver answers, and the answer is the address it was told.
#[tokio::test]
async fn the_chosen_resolver_answers_through_the_encrypted_server() {
    let authority = authority();
    let address = spawn_doh_server(&authority).await;
    let dns = Dns::with_roots(
        &Hosts::default(),
        &local_choice(address),
        vec![authority.root.clone()],
    );

    let found = dns
        .addresses(NAME)
        .await
        .expect("the local resolver answers");
    assert_eq!(found, vec![IpAddr::V4(ANSWER.into())]);
}

/// A download goes through the encrypted resolver: the name is only the local
/// server's to answer, so the file can only arrive if the resolver was asked.
#[tokio::test]
async fn a_download_goes_through_the_encrypted_resolver() {
    let authority = authority();
    let doh = spawn_doh_server(&authority).await;
    let files = spawn_file_server().await;
    let dns = Arc::new(Dns::with_roots(
        &Hosts::default(),
        &local_choice(doh),
        vec![authority.root.clone()],
    ));

    let client = crate::net::client_builder(None, Some(&dns))
        .expect("a builder")
        .build()
        .expect("a client");
    let url = format!("http://{NAME}:{}/file", files.port());
    let answer = client
        .get(&url)
        .send()
        .await
        .expect("the file arrives through the resolver");
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.text().await.expect("the body"), "the file");
}

/// A resolver that cannot be reached fails with the mode in the message and
/// never asks the system's resolver instead.
#[tokio::test]
async fn a_resolver_that_cannot_be_reached_says_so() {
    // TEST-NET-1: never routed, so never answered.
    let choice = ResolverChoice {
        resolver: Resolver::over_https("127.0.0.1", "https://192.0.2.1/dns-query"),
        fallback: false,
    };
    let dns = Dns::new(&Hosts::default(), &choice);
    let error = dns
        .addresses(NAME)
        .await
        .expect_err("the resolver cannot be reached");
    assert!(
        error.contains("could not be reached"),
        "the error says what happened: {error}"
    );
}

/// The local names stay with the system's resolver, whatever was chosen.
#[tokio::test]
async fn a_local_name_stays_with_the_system() {
    let choice = ResolverChoice {
        resolver: Resolver::over_https("127.0.0.1", "https://192.0.2.1/dns-query"),
        fallback: false,
    };
    let dns = Dns::new(&Hosts::default(), &choice);
    let found = dns.addresses("localhost").await.expect("localhost");
    assert!(found.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
}
