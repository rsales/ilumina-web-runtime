//! `xtask serve <discs> [port]`: a tiny local HTTP server for the browser
//! player (std only, GET only, bound to 127.0.0.1).
//!
//! * `/`, `/index.html`, `/app.js` → `port/web/`
//! * `/pkg/web.wasm` → `port/target/wasm32-unknown-unknown/release/web.wasm`
//! * `/discs/<path>` → the user's discs, only for paths in the release
//!   manifest (the wasm side verifies each hash again).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub const WASM: &str = "target/wasm32-unknown-unknown/release/web.wasm";

#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    File(PathBuf, &'static str),
    NotFound,
}

/// Decode `%XX` escapes (paths only; `+` stays as is).
pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Map a request path to a file. `manifest` lists the disc paths allowed.
pub fn route(path: &str, port: &Path, discs: &Path, manifest: &[discs::Entry]) -> Route {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let Some(path) = percent_decode(path) else {
        return Route::NotFound;
    };
    let file = match path.as_str() {
        "/" | "/index.html" => port.join("web/index.html"),
        "/app.js" => port.join("web/app.js"),
        "/pkg/web.wasm" => port.join(WASM),
        p => match p.strip_prefix("/discs/") {
            // Only exact manifest paths: no `..`, no directory listing.
            Some(rel) if manifest.iter().any(|e| e.path == rel) => discs.join(rel),
            _ => return Route::NotFound,
        },
    };
    let ct = content_type(&file);
    Route::File(file, ct)
}

fn respond(mut s: TcpStream, status: &str, ct: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
}

fn handle(s: TcpStream, port: &Path, discs: &Path, manifest: &[discs::Entry]) {
    let mut line = String::new();
    if BufReader::new(&s).read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return;
    };
    if method != "GET" {
        return respond(s, "405 Method Not Allowed", "text/plain", b"GET only");
    }
    match route(path, port, discs, manifest) {
        Route::File(f, ct) => match std::fs::read(&f) {
            Ok(body) => respond(s, "200 OK", ct, &body),
            Err(e) => {
                let msg = if f.ends_with("web.wasm") {
                    format!("{e}: build it with `cargo build --release -p web --target wasm32-unknown-unknown`")
                } else {
                    format!("{e}")
                };
                eprintln!("404 {path}: {msg}");
                respond(s, "404 Not Found", "text/plain", msg.as_bytes())
            }
        },
        Route::NotFound => respond(s, "404 Not Found", "text/plain", b"not found"),
    }
}

pub fn serve(discs: &Path, http_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let port_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = discs::parse_manifest(discs::ILUMINA_GOLD_250_PTBR)?;
    if !port_dir.join(WASM).is_file() {
        eprintln!("warning: {WASM} not found; build it first:");
        eprintln!("  rustup target add wasm32-unknown-unknown");
        eprintln!("  cargo build --release -p web --target wasm32-unknown-unknown");
    }
    let listener = TcpListener::bind(("127.0.0.1", http_port))?;
    println!("iLumina port: http://localhost:{http_port}/  (Ctrl+C to stop)");
    let shared = std::sync::Arc::new((port_dir, discs.to_path_buf(), manifest));
    for s in listener.incoming().flatten() {
        let shared = shared.clone();
        std::thread::spawn(move || handle(s, &shared.0, &shared.1, &shared.2));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Vec<discs::Entry> {
        let z = "0".repeat(64);
        discs::parse_manifest(&format!(
            "{z}  cd1/Program/Files/Home.dxr\n{z}  cd1/Program/Files/Ver Isto.txt\n"
        ))
        .unwrap()
    }

    #[test]
    fn routes_page_wasm_and_manifest_files_only() {
        let (p, d, m) = (Path::new("/port"), Path::new("/discs"), manifest());
        assert_eq!(
            route("/", p, d, &m),
            Route::File("/port/web/index.html".into(), "text/html; charset=utf-8")
        );
        assert_eq!(
            route("/pkg/web.wasm", p, d, &m),
            Route::File(Path::new("/port").join(WASM), "application/wasm")
        );
        assert_eq!(
            route("/discs/cd1/Program/Files/Home.dxr", p, d, &m),
            Route::File(
                "/discs/cd1/Program/Files/Home.dxr".into(),
                "application/octet-stream"
            )
        );
        assert_eq!(
            route("/discs/cd1/Program/Files/Ver%20Isto.txt?x=1", p, d, &m),
            Route::File(
                "/discs/cd1/Program/Files/Ver Isto.txt".into(),
                "application/octet-stream"
            )
        );
        // Anything outside the manifest is refused, including traversal.
        assert_eq!(route("/discs/../../etc/passwd", p, d, &m), Route::NotFound);
        assert_eq!(
            route("/discs/cd1/Program/Files/%2e%2e/x", p, d, &m),
            Route::NotFound
        );
        assert_eq!(route("/Cargo.toml", p, d, &m), Route::NotFound);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b%C3%AD").as_deref(), Some("a bí"));
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%2"), None);
    }
}
