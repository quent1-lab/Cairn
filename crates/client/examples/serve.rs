//! Serveur statique minimal (std uniquement) pour servir le bundle du client
//! en local — utile faute de `python -m http.server`.
//!
//! Usage : cargo run -p cairn-client --example serve -- [dossier] [port]
//! Défaut : crates/client/dist sur le port 8080.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| "crates/client/dist".into());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8080);
    let root = PathBuf::from(root);

    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    println!("Cairn servi sur http://localhost:{port}  (dossier {})", root.display());
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            let _ = handle(s, &root);
        }
    }
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    // Première ligne : "GET /chemin HTTP/1.1".
    let path = req
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/");
    let rel = path.split('?').next().unwrap_or("/").trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    // Anti-traversée : on refuse les chemins remontants.
    let target = root.join(rel);
    if rel.contains("..") || !target.starts_with(root) {
        return respond(&mut stream, 403, "text/plain", b"Forbidden");
    }

    match std::fs::read(&target) {
        Ok(body) => respond(&mut stream, 200, mime(&target), &body),
        Err(_) => respond(&mut stream, 404, "text/plain", b"Not Found"),
    }
}

fn respond(stream: &mut TcpStream, code: u16, mime: &str, body: &[u8]) -> std::io::Result<()> {
    let status = match code {
        200 => "200 OK",
        403 => "403 Forbidden",
        _ => "404 Not Found",
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Type MIME par extension. `.wasm` DOIT être `application/wasm`, sinon le
/// navigateur refuse d'instancier le module.
fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}
