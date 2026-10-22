//! Parses the markdown files under `content/` into the site model.
//!
//! Every file is front matter (`key: value` lines between `---` fences)
//! followed by a markdown body.

use crate::{date, markdown};

pub struct Link {
    pub label: String,
    pub url: String,
}

pub struct Site {
    pub name: String,
    pub description: String,
    /// One sentence that doubles as navigation: `[writes](writing)`.
    pub intro: String,
    pub timezone: String,
    pub city: String,
    pub calendar: String,
    pub email: String,
    pub links: Vec<Link>,
    pub bio: String,
}

pub struct Post {
    pub slug: String,
    pub title: String,
    pub date: i64,
    pub tags: Vec<String>,
    pub summary: String,
    pub body: String,
    pub words: usize,
}

pub struct Event {
    pub slug: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    /// UTC offset (minutes) the start time was written in.
    pub offset: i32,
    pub location: String,
    pub url: String,
    pub kind: String,
    pub body: String,
}

pub struct Content {
    pub site: Site,
    /// Newest first.
    pub posts: Vec<Post>,
    /// Soonest first.
    pub events: Vec<Event>,
}

impl Content {
    pub fn embedded() -> Content {
        load(crate::embedded::FILES)
    }

    pub fn post(&self, slug: &str) -> Option<(usize, &Post)> {
        self.posts.iter().enumerate().find(|(_, p)| p.slug == slug)
    }
}

pub fn load(files: &[(&str, &str)]) -> Content {
    let mut site = None;
    let mut posts = Vec::new();
    let mut events = Vec::new();
    for &(path, src) in files {
        let (meta, body) = front(src);
        let get = |k: &str| meta.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        if get("draft").is_some_and(|v| v == "true") {
            continue;
        }
        let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md");
        let slug = get("slug").unwrap_or_else(|| strip_date(stem).to_string());
        if path == "site.md" {
            site = Some(Site {
                name: get("name").unwrap_or_else(|| "Your Name".into()),
                description: get("description").unwrap_or_default(),
                intro: get("intro").unwrap_or_else(|| "[writes](writing) and [speaks](events).".into()),
                timezone: get("timezone").unwrap_or_else(|| "UTC".into()),
                city: get("city").unwrap_or_default(),
                calendar: get("calendar").unwrap_or_default(),
                email: get("email").unwrap_or_default(),
                links: meta
                    .iter()
                    .filter_map(|(k, v)| k.strip_prefix("link.").map(|l| Link { label: l.into(), url: v.clone() }))
                    .collect(),
                bio: body,
            });
        } else if path.starts_with("posts/") {
            let stamp = get("date").and_then(|d| date::parse(&d)).or_else(|| stem.get(0..10).and_then(date::parse));
            posts.push(Post {
                title: get("title").unwrap_or_else(|| slug.clone()),
                date: stamp.map_or(0, |s| s.ts),
                tags: get("tags")
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                    .unwrap_or_default(),
                summary: get("summary").unwrap_or_default(),
                words: markdown::plain(&body).split_whitespace().count(),
                body,
                slug,
            });
        } else if path.starts_with("events/") {
            let Some(start) = get("start").and_then(|s| date::parse(&s)) else { continue };
            let end = get("end").and_then(|s| date::parse(&s)).map_or(start.ts + 3600, |s| s.ts);
            events.push(Event {
                title: get("title").unwrap_or_else(|| slug.clone()),
                start: start.ts,
                end: end.max(start.ts),
                offset: start.offset,
                location: get("location").unwrap_or_default(),
                url: get("url").unwrap_or_default(),
                kind: get("kind").unwrap_or_default(),
                body,
                slug,
            });
        }
    }
    posts.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    events.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.slug.cmp(&b.slug)));
    let site = site.unwrap_or_else(|| load(&[("site.md", "")]).site);
    Content { site, posts, events }
}

/// Splits front matter from the body. Values may be wrapped in quotes.
fn front(src: &str) -> (Vec<(String, String)>, String) {
    let src = src.replace("\r\n", "\n");
    let Some(rest) = src.strip_prefix("---\n") else { return (Vec::new(), src) };
    let Some(end) = rest.find("\n---") else { return (Vec::new(), src.clone()) };
    let meta = rest[..end]
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| {
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
            (k.trim().to_string(), v.to_string())
        })
        .collect();
    let body = &rest[end + 4..];
    let body = body.split_once('\n').map_or("", |(_, b)| b);
    (meta, body.trim().to_string())
}

/// `2026-01-12-hello` -> `hello`
fn strip_date(stem: &str) -> &str {
    let b = stem.as_bytes();
    if b.len() > 11 && b[10] == b'-' && date::parse(&stem[..10]).is_some() {
        &stem[11..]
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_front_matter_and_sorts() {
        let c = load(&[
            ("site.md", "---\nname: Ada\nlink.github: https://github.com/ada\n---\nHello.\n"),
            ("posts/2026-01-02-older.md", "---\ntitle: \"Older: a post\"\ntags: a, b\n---\nOne two three."),
            ("posts/2026-03-04-newer.md", "---\ntitle: Newer\n---\nBody"),
            ("posts/2026-03-05-hidden.md", "---\ntitle: Hidden\ndraft: true\n---\n"),
            ("events/talk.md", "---\ntitle: Talk\nstart: 2026-11-14T18:30+01:00\n---\n"),
            ("events/broken.md", "---\ntitle: No date\n---\n"),
        ]);
        assert_eq!(c.site.name, "Ada");
        assert_eq!(c.site.bio, "Hello.");
        assert_eq!(c.site.links[0].label, "github");
        assert_eq!(c.posts.len(), 2);
        assert_eq!(c.posts[0].slug, "newer");
        assert_eq!(c.posts[1].title, "Older: a post");
        assert_eq!(c.posts[1].tags, ["a", "b"]);
        assert_eq!(c.posts[1].words, 3);
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].end - c.events[0].start, 3600);
    }

    #[test]
    fn embedded_content_loads() {
        let c = Content::embedded();
        assert!(!c.site.name.is_empty());
        assert!(!c.posts.is_empty());
    }
}
