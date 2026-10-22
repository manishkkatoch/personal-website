//! Local development: builds the site, serves it, rebuilds whenever anything
//! under `content/`, `src/` or `web/` changes, and reloads open tabs.
//! Build errors show up in the terminal and as an overlay in the browser.
//!
//! cargo dev                 (alias for `cargo run --bin dev`)
//! PORT=8000 HOST=127.0.0.1  (HOST=0.0.0.0 to open it from a phone)

use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{self, Command},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

const WATCH: &[&str] = &["content", "src", "web", "build.rs", "Cargo.toml"];
const WASM_PROFILE: &str = "wasm-dev";

/// Injected into every HTML page: reloads on rebuild (keeping the scroll
/// position), shows build errors, and reloads after the server restarts.
const CLIENT: &str = r#"<script>
(() => {
  const y = sessionStorage.getItem('field:y');
  if (y !== null) {
    sessionStorage.removeItem('field:y');
    addEventListener('load', () => setTimeout(() => scrollTo(0, +y), 80));
  }
  const reload = () => { sessionStorage.setItem('field:y', scrollY); location.reload(); };
  let opened = false, box;
  const es = new EventSource('/__dev');
  es.onopen = () => { if (opened) reload(); opened = true; };
  es.addEventListener('reload', reload);
  es.addEventListener('failed', (e) => {
    box ??= document.body.appendChild(document.createElement('pre'));
    box.style.cssText = 'position:fixed;left:16px;right:16px;bottom:16px;z-index:99;margin:0;max-height:50vh;overflow:auto;padding:14px 16px;border-radius:4px;background:#1b1b1a;color:#f0cfc2;font:12px/1.5 ui-monospace,Menlo,monospace;white-space:pre-wrap';
    box.textContent = e.data;
  });
})();
</script>"#;

#[derive(Default)]
struct State {
    clients: Vec<TcpStream>,
    error: Option<String>,
}

type Shared = Arc<Mutex<State>>;

struct Paths {
    site: PathBuf,
    next: PathBuf,
    wasm: PathBuf,
}

fn main() {
    env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
    let target = PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()));
    let paths = Paths {
        site: target.join("site-dev"),
        next: target.join("site-dev.next"),
        wasm: target.join("wasm32-unknown-unknown").join(WASM_PROFILE).join("field.wasm"),
    };
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("PORT").unwrap_or_else(|_| "8000".into());
    let listener = TcpListener::bind(format!("{host}:{port}")).unwrap_or_else(|e| {
        eprintln!("field: cannot listen on {host}:{port}: {e} (try PORT=8001 cargo dev)");
        process::exit(1);
    });

    let state = Shared::default();
    let paths = Arc::new(paths);
    rebuild(&state, &paths);
    println!("field: http://{host}:{port}/  (watching content/, src/, web/; ctrl-c to stop)");

    {
        let (state, paths) = (state.clone(), paths.clone());
        thread::spawn(move || watch(&state, &paths));
    }
    for stream in listener.incoming().flatten() {
        let (state, paths) = (state.clone(), paths.clone());
        thread::spawn(move || {
            let _ = serve(stream, &state, &paths.site);
        });
    }
}

/// Modification times of every watched file; any difference means rebuild.
fn snapshot() -> Vec<(PathBuf, SystemTime)> {
    fn walk(p: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
        let Ok(meta) = fs::metadata(p) else { return };
        if meta.is_dir() {
            for entry in fs::read_dir(p).into_iter().flatten().flatten() {
                walk(&entry.path(), out);
            }
        } else if let Ok(t) = meta.modified() {
            out.push((p.to_path_buf(), t));
        }
    }
    let mut out = Vec::new();
    for w in WATCH {
        walk(Path::new(w), &mut out);
    }
    out.sort();
    out
}

fn watch(state: &Shared, paths: &Paths) {
    let mut last = snapshot();
    let mut idle = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(250));
        let now = snapshot();
        if now != last {
            // Let editors finish writing (save-via-rename, formatters) first.
            thread::sleep(Duration::from_millis(150));
            last = snapshot();
            rebuild(state, paths);
        } else if idle.elapsed() > Duration::from_secs(15) {
            // Drop tabs that have gone away.
            state.lock().unwrap().clients.retain_mut(|c| c.write_all(b": ping\n\n").is_ok());
            idle = Instant::now();
        }
    }
}

fn rebuild(state: &Shared, paths: &Paths) {
    let started = Instant::now();
    print!("field: building… ");
    let _ = io::stdout().flush();
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let wasm_args = ["build", "--quiet", "--lib", "--profile", WASM_PROFILE, "--target", "wasm32-unknown-unknown"];
    let next = paths.next.to_string_lossy();
    let wasm = paths.wasm.to_string_lossy();
    let gen_args = ["run", "--quiet", "--bin", "generate", "--", "--out", &next, "--wasm", &wasm, "--base", "/"];

    for args in [&wasm_args[..], &gen_args[..]] {
        let failure = match Command::new(&cargo).args(args).output() {
            Ok(o) if o.status.success() => continue,
            Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
            Err(e) => format!("could not run cargo: {e}"),
        };
        let mut msg = failure;
        if msg.contains("may not be installed") {
            msg += "\n\nhint: rustup target add wasm32-unknown-unknown";
        }
        println!("failed\n{msg}\n");
        let mut s = state.lock().unwrap();
        broadcast(&mut s, "failed", &msg);
        s.error = Some(msg);
        return;
    }

    let _ = fs::remove_dir_all(&paths.site);
    if let Err(e) = fs::rename(&paths.next, &paths.site) {
        println!("failed\ncould not swap in the new build: {e}");
        return;
    }
    println!("done in {:.1}s", started.elapsed().as_secs_f32());
    let mut s = state.lock().unwrap();
    s.error = None;
    broadcast(&mut s, "reload", "");
}

fn broadcast(s: &mut State, event: &str, data: &str) {
    let mut msg = format!("event: {event}\n");
    for line in data.lines() {
        msg += &format!("data: {line}\n");
    }
    if data.is_empty() {
        msg += "data:\n";
    }
    msg += "\n";
    s.clients.retain_mut(|c| c.write_all(msg.as_bytes()).is_ok());
}

fn serve(mut stream: TcpStream, state: &Shared, site: &Path) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");

    if path == "/__dev" {
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n: field\n\n",
        )?;
        let mut s = state.lock().unwrap();
        s.clients.push(stream);
        if let Some(e) = s.error.clone() {
            broadcast(&mut s, "failed", &e);
        }
        return Ok(());
    }

    let rel = path.trim_start_matches('/');
    let mut file = site.join(rel);
    if rel.split('/').any(|p| p == "..") {
        file = site.join("404.html");
    } else if file.is_dir() {
        file = file.join("index.html");
    }
    let (status, file) = if file.is_file() { ("200 OK", file) } else { ("404 Not Found", site.join("404.html")) };

    let (status, mut body) = match fs::read(&file) {
        Ok(b) => (status, b),
        Err(_) => {
            // No successful build yet: show the error and wait for a fix.
            let why = state.lock().unwrap().error.clone().unwrap_or_else(|| "Building…".into());
            let page = format!(
                "<!doctype html><meta charset=utf-8><title>field</title><pre>{}</pre></body>",
                field::esc(&why)
            );
            ("503 Service Unavailable", page.into_bytes())
        }
    };
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("html");
    let ext = if status.starts_with("503") { "html" } else { ext };
    if ext == "html" {
        let html = String::from_utf8_lossy(&body);
        body = match html.rfind("</body>") {
            Some(i) => format!("{}{CLIENT}{}", &html[..i], &html[i..]),
            None => format!("{html}{CLIENT}"),
        }
        .into_bytes();
    }
    let kind = match ext {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "xml" => "application/xml; charset=utf-8",
        "ics" => "text/calendar; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "text/plain; charset=utf-8",
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}
