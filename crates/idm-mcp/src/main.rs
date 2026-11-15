//! The binary: standard input and output wired to the server.
//!
//! The assistant starts this program and owns its life, so it says nothing on
//! standard output except the protocol itself.

use std::io::{self, BufReader};

use idm_mcp::client::HttpClient;
use idm_mcp::serve;

fn main() -> io::Result<()> {
    let mut app = HttpClient::new();
    serve(
        BufReader::new(io::stdin().lock()),
        io::stdout().lock(),
        &mut app,
    )
}
