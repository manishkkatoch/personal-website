//! Writes the static site: every route pre-rendered from the same views the
//! wasm module renders live, plus feeds, calendar files and assets.
//!
//! generate --out dist --wasm path/to/field.wasm [--base /] [--site-url https://example.com]

use field::{
    content::Content,
    date, esc, ics,
    views::{self, Ctx, Route},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Args {
    out: PathBuf,
    wasm: PathBuf,
    web: PathBuf,
    base: String,
    site_url: String,
}

fn args() -> Args {
    let mut a = Args {
        out: "dist".into(),
        wasm: "target/wasm32-unknown-unknown/release/field.wasm".into(),
        web: "web".into(),
        base: "/".into(),
        site_url: String::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let v = it.next().unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--out" => a.out = v.into(),
            "--wasm" => a.wasm = v.into(),
            "--web" => a.web = v.into(),
            "--base" => a.base = v,
            "--site-url" => a.site_url = v,
            _ => panic!("unknown flag {flag}"),
        }
    }
    if !a.base.starts_with('/') {
        a.base.insert(0, '/');
    }
    if !a.base.ends_with('/') {
        a.base.push('/');
    }
    // Only the origin matters; the base path is added separately.
    if let Some((scheme, rest)) = a.site_url.split_once("://") {
        a.site_url = format!("{scheme}://{}", rest.split('/').next().unwrap_or(rest));
    }
    a
}

fn write(path: &Path, data: impl AsRef<[u8]>) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(path, data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// FNV-1a, for cache-busting asset URLs.
fn fingerprint(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in parts.iter().flat_map(|p| p.iter()) {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!("{:08x}", h as u32)
}

fn main() {
    let a = args();
    let c = Content::embedded();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let ctx = Ctx { now, live: false };

    let template = fs::read_to_string(a.web.join("index.html")).expect("web/index.html");
    let app = fs::read(a.web.join("app.js")).expect("web/app.js");
    let css = fs::read(a.web.join("style.css")).expect("web/style.css");
    let wasm = fs::read(&a.wasm).expect("build the wasm module first");
    let version = fingerprint(&[&app, &css, &wasm]);

    if a.out.exists() {
        fs::remove_dir_all(&a.out).unwrap();
    }
    let abs = |rel: &str| format!("{}{}{}", a.site_url, a.base, rel);

    let mut pages: Vec<(String, Route)> =
        vec![(String::new(), Route::Home), ("writing/".into(), Route::Writing), ("events/".into(), Route::Events)];
    pages.extend(c.posts.iter().map(|p| (format!("writing/{}/", p.slug), Route::Post(&p.slug))));
    pages.push(("404".into(), Route::NotFound));

    for (path, route) in &pages {
        let v = views::render(&c, *route, &ctx);
        let canonical = if a.site_url.is_empty() || path == "404" {
            String::new()
        } else {
            format!(r#"<link rel="canonical" href="{}">"#, esc(&abs(path)))
        };
        let html = template
            .replace("{{base}}", &esc(&a.base))
            .replace("{{title}}", &esc(&v.title))
            .replace("{{description}}", &esc(&v.description))
            .replace("{{name}}", &esc(&c.site.name))
            .replace("{{canonical}}", &canonical)
            .replace("{{v}}", &version)
            .replace("{{shape}}", &(v.shape as u32).to_string())
            .replace("{{view}}", &v.html);
        let file = if path == "404" { a.out.join("404.html") } else { a.out.join(path).join("index.html") };
        write(&file, html);
    }

    write(&a.out.join("app.js"), &app);
    write(&a.out.join("style.css"), &css);
    write(&a.out.join("field.wasm"), &wasm);
    write(&a.out.join(".nojekyll"), "");

    write(&a.out.join("events.ics"), ics::feed(&c, now, &a.site_url));
    for e in &c.events {
        write(&a.out.join(format!("events/{}.ics", e.slug)), ics::single(&c, e, now, &a.site_url));
    }

    let mut items = String::new();
    for p in &c.posts {
        let link = abs(&format!("writing/{}/", p.slug));
        items += &format!(
            "<item><title>{}</title><link>{}</link><guid>{}</guid><pubDate>{}</pubDate><description>{}</description></item>\n",
            esc(&p.title),
            esc(&link),
            esc(&link),
            date::rfc2822(p.date),
            esc(&field::markdown::to_html(&p.body)),
        );
    }
    write(
        &a.out.join("feed.xml"),
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<rss version=\"2.0\"><channel><title>{}</title><link>{}</link><description>{}</description>\n{}</channel></rss>\n",
            esc(&c.site.name),
            esc(&abs("")),
            esc(&c.site.description),
            items
        ),
    );

    if !a.site_url.is_empty() {
        let urls: String = pages
            .iter()
            .filter(|(p, _)| p != "404")
            .map(|(p, _)| format!("<url><loc>{}</loc></url>\n", esc(&abs(p))))
            .collect();
        write(
            &a.out.join("sitemap.xml"),
            format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{urls}</urlset>\n"),
        );
        if a.base == "/" {
            write(&a.out.join("robots.txt"), format!("User-agent: *\nAllow: /\nSitemap: {}\n", abs("sitemap.xml")));
        }
    }

    println!(
        "wrote {} pages, {} events, wasm {} KB, to {}",
        pages.len(),
        c.events.len(),
        wasm.len() / 1024,
        a.out.display()
    );
}
