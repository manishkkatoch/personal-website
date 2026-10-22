//! A tiny in-memory search over posts, events and the site's own pages.
//! Every query token must match somewhere; titles and tags weigh most.

use crate::{content::Content, date, esc, markdown};

pub struct Doc {
    pub kind: &'static str,
    pub title: String,
    pub href: String,
    pub external: bool,
    pub hint: String,
    title_l: String,
    tags_l: Vec<String>,
    summary_l: String,
    body: String,
    body_l: String,
}

impl Doc {
    fn new(kind: &'static str, title: &str, href: &str, hint: String) -> Doc {
        Doc {
            kind,
            title: title.into(),
            href: href.into(),
            external: href.contains(':'),
            hint,
            title_l: title.to_lowercase(),
            tags_l: Vec::new(),
            summary_l: String::new(),
            body: String::new(),
            body_l: String::new(),
        }
    }

    fn with_text(mut self, tags: &[String], summary: &str, body: String) -> Doc {
        self.tags_l = tags.iter().map(|t| t.to_lowercase()).collect();
        self.summary_l = summary.to_lowercase();
        self.body_l = body.to_lowercase();
        self.body = body;
        self
    }
}

pub struct Hit<'a> {
    pub doc: &'a Doc,
    pub score: f32,
    pub snippet: String,
}

pub struct Index {
    pub docs: Vec<Doc>,
    /// Number of leading docs that are site pages, shown for empty queries.
    pages: usize,
}

impl Index {
    pub fn build(c: &Content, now: i64) -> Index {
        let s = &c.site;
        let mut docs = vec![
            Doc::new("page", "Writing", "writing/", format!("{} posts", c.posts.len())),
            Doc::new("page", "Events", "events/", "talks, workshops, meetups".into()),
        ];
        if !s.calendar.is_empty() {
            docs.push(Doc::new("page", "Book time", &s.calendar, "my calendar".into()));
        }
        docs.push(Doc::new("page", "Home", "./", s.name.clone()));
        let pages = docs.len();
        docs.push(Doc::new("link", "Subscribe to events", "events.ics", "calendar feed".into()));
        docs.push(Doc::new("link", "RSS feed", "feed.xml", "writing, by subscription".into()));
        if !s.email.is_empty() {
            docs.push(Doc::new("link", "Email", &format!("mailto:{}", s.email), s.email.clone()));
        }
        for l in &s.links {
            docs.push(Doc::new("link", &l.label, &l.url, l.url.clone()));
        }
        for p in &c.posts {
            let href = format!("writing/{}/", p.slug);
            docs.push(Doc::new("post", &p.title, &href, date::relative(p.date, now)).with_text(
                &p.tags,
                &p.summary,
                markdown::plain(&p.body),
            ));
        }
        for e in &c.events {
            let href = if e.url.is_empty() { "events/".to_string() } else { e.url.clone() };
            let hint = format!("{} · {}", date::relative(e.start, now), e.location);
            let mut tags = vec![e.kind.clone()];
            tags.extend(e.location.split([',', ' ']).map(str::to_string));
            docs.push(Doc::new("event", &e.title, &href, hint).with_text(&tags, &e.location, markdown::plain(&e.body)));
        }
        Index { docs, pages }
    }

    pub fn query(&self, q: &str, limit: usize) -> Vec<Hit<'_>> {
        let q = q.to_lowercase();
        let tokens: Vec<&str> = q.split_whitespace().collect();
        if tokens.is_empty() {
            return self.docs[..self.pages].iter().map(|doc| Hit { doc, score: 0.0, snippet: String::new() }).collect();
        }
        let mut hits: Vec<Hit> = self
            .docs
            .iter()
            .filter_map(|doc| {
                let mut score = 0.0;
                for t in &tokens {
                    let s = score_token(doc, t);
                    if s == 0.0 {
                        return None;
                    }
                    score += s;
                }
                if doc.kind == "page" {
                    score += 2.0;
                }
                Some(Hit { doc, score, snippet: snippet(doc, tokens[0]) })
            })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(limit);
        hits
    }
}

fn score_token(doc: &Doc, t: &str) -> f32 {
    let mut s = 0.0;
    if let Some(i) = doc.title_l.find(t) {
        s += 10.0;
        if i == 0 || !doc.title_l.as_bytes()[i - 1].is_ascii_alphanumeric() {
            s += 5.0;
        }
    }
    if doc.tags_l.iter().any(|g| g == t) {
        s += 6.0;
    } else if doc.tags_l.iter().any(|g| g.starts_with(t)) {
        s += 3.0;
    }
    if doc.summary_l.contains(t) {
        s += 3.0;
    }
    s += doc.body_l.matches(t).take(5).count() as f32;
    if s == 0.0 && t.len() >= 2 && subsequence(t, &doc.title_l) {
        s += 2.0;
    }
    s
}

fn subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

/// A short excerpt around the first body match, with the match marked.
fn snippet(doc: &Doc, t: &str) -> String {
    // Byte offsets only line up when lowercasing kept every char's width.
    let text = if doc.body.len() == doc.body_l.len() { &doc.body } else { &doc.body_l };
    let Some(pos) = doc.body_l.find(t) else { return String::new() };
    let (end_match, mut a, mut b) = (pos + t.len(), pos.saturating_sub(50), (pos + t.len() + 90).min(text.len()));
    while !text.is_char_boundary(a) {
        a += 1;
    }
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    if !text.is_char_boundary(pos) || !text.is_char_boundary(end_match) {
        return esc(&text[a..b]);
    }
    if a > 0 {
        a = text[a..pos].find(' ').map_or(a, |i| a + i + 1);
    }
    format!(
        "{}{}<mark>{}</mark>{}{}",
        if a > 0 { "…" } else { "" },
        esc(&text[a..pos]),
        esc(&text[pos..end_match]),
        esc(&text[end_match..b]),
        if b < text.len() { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::load;

    fn index() -> Index {
        let c = load(&[
            ("site.md", "---\nname: Ada\ncalendar: https://cal.example/ada\n---\n"),
            ("posts/2026-01-02-rust.md", "---\ntitle: Notes on Rust\ntags: rust\n---\nOwnership is a discipline."),
            ("posts/2026-01-03-wasm.md", "---\ntitle: WebAssembly field\n---\nRust compiles to wasm. Ownership again."),
        ]);
        Index::build(&c, 0)
    }

    #[test]
    fn ranks_titles_over_bodies() {
        let idx = index();
        let hits = idx.query("rust", 5);
        assert_eq!(hits[0].doc.title, "Notes on Rust");
        assert_eq!(hits[1].doc.title, "WebAssembly field");
    }

    #[test]
    fn requires_every_token_and_marks_snippets() {
        let idx = index();
        let hits = idx.query("ownership again", 5);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("<mark>Ownership</mark>"));
    }

    #[test]
    fn empty_query_suggests_pages_and_fuzzy_finds_calendar() {
        let idx = index();
        assert!(idx.query("  ", 5).iter().all(|h| h.doc.kind == "page"));
        assert_eq!(idx.query("bktm", 5)[0].doc.title, "Book time");
    }
}
