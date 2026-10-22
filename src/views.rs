//! Routes and their markup. The browser renders these from wasm; the
//! generator renders the very same functions to static HTML.

use crate::{
    content::{Content, Event, Post},
    date, esc,
    field::Shape,
    markdown,
};

pub struct Ctx {
    pub now: i64,
    /// Live renders speak in relative time; static ones in absolute dates.
    pub live: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Route<'a> {
    Home,
    Writing,
    Post(&'a str),
    Events,
    NotFound,
}

/// Resolves a path relative to the site base, e.g. `writing/hello/`.
pub fn resolve(path: &str) -> Route<'_> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = path.trim_matches('/');
    let path = path.strip_suffix("index.html").unwrap_or(path).trim_end_matches('/');
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    match (parts.next(), parts.next(), parts.next()) {
        (None, ..) => Route::Home,
        (Some("writing"), None, _) => Route::Writing,
        (Some("writing"), Some(slug), None) => Route::Post(slug),
        (Some("events"), None, _) => Route::Events,
        _ => Route::NotFound,
    }
}

pub struct View {
    pub title: String,
    pub description: String,
    pub shape: Shape,
    pub html: String,
}

pub fn render(c: &Content, route: Route, ctx: &Ctx) -> View {
    let s = &c.site;
    match route {
        Route::Home => {
            View { title: s.name.clone(), description: s.description.clone(), shape: Shape::Sphere, html: home(c, ctx) }
        }
        Route::Writing => View {
            title: format!("Writing — {}", s.name),
            description: format!("Writing by {}.", s.name),
            shape: Shape::Sheets,
            html: writing(c, ctx),
        },
        Route::Events => View {
            title: format!("Events — {}", s.name),
            description: format!("Where {} will be, and has been.", s.name),
            shape: Shape::Helix,
            html: events(c, ctx),
        },
        Route::Post(slug) => match c.post(slug) {
            Some((i, p)) => View {
                title: format!("{} — {}", p.title, s.name),
                description: if p.summary.is_empty() { s.description.clone() } else { p.summary.clone() },
                shape: Shape::Ring,
                html: post(c, i, p, ctx),
            },
            None => render(c, Route::NotFound, ctx),
        },
        Route::NotFound => View {
            title: format!("Nowhere — {}", s.name),
            description: s.description.clone(),
            shape: Shape::Scatter,
            html: format!(
                r#"<section class="lost">{}<h1>Nothing here.</h1><p>The points went looking and came back empty. <a href="./">Start over</a>, or just type.</p></section>"#,
                crumbs(c, &[])
            ),
        },
    }
}

fn crumbs(c: &Content, trail: &[(&str, &str)]) -> String {
    let mut out = format!(r#"<nav class="crumbs"><a href="./" data-shape="0">{}</a>"#, esc(&c.site.name));
    for (label, href) in trail {
        if href.is_empty() {
            out += &format!(r#"<span>{}</span>"#, esc(label));
        } else {
            out += &format!(r#"<a href="{}">{}</a>"#, esc(href), esc(label));
        }
    }
    out + "</nav>"
}

fn when(ts: i64, ctx: &Ctx) -> String {
    if ctx.live {
        format!(
            r#"<time datetime="{}" title="{}">{}</time>"#,
            date::iso_date(ts),
            date::long(ts),
            date::relative(ts, ctx.now)
        )
    } else {
        format!(r#"<time datetime="{}">{}</time>"#, date::iso_date(ts), date::long(ts))
    }
}

/// Renders the intro sentence, turning `[text](target)` into navigation.
fn intro(c: &Content) -> String {
    let mut out = String::new();
    let mut rest = c.site.intro.as_str();
    while let Some(i) = rest.find('[') {
        out += &esc(&rest[..i]);
        let after = &rest[i + 1..];
        let link = after.find("](").and_then(|j| after[j + 2..].find(')').map(|k| (j, j + 2 + k)));
        let Some((j, k)) = link else {
            out.push('[');
            rest = after;
            continue;
        };
        let (text, target) = (esc(&after[..j]), &after[j + 2..k]);
        out += &match target {
            "writing" => format!(r#"<a href="writing/" data-shape="1">{text}</a>"#),
            "events" => format!(r#"<a href="events/" data-shape="2">{text}</a>"#),
            "calendar" => format!(
                r#"<a href="{}" data-shape="4" target="_blank" rel="noopener">{text}</a>"#,
                esc(if c.site.calendar.is_empty() { "events/" } else { &c.site.calendar })
            ),
            url => format!(r#"<a href="{}" rel="noopener">{text}</a>"#, esc(url)),
        };
        rest = &after[k + 1..];
    }
    out + &esc(rest)
}

fn elsewhere(c: &Content) -> String {
    let mut links: Vec<String> = c
        .site
        .links
        .iter()
        .map(|l| format!(r#"<a href="{}" rel="me noopener">{}</a>"#, esc(&l.url), esc(&l.label)))
        .collect();
    if !c.site.email.is_empty() {
        links.push(format!(r#"<a href="mailto:{0}">{0}</a>"#, esc(&c.site.email)));
    }
    links.push(r#"<a href="feed.xml">rss</a>"#.into());
    format!(r#"<nav class="elsewhere">{}</nav>"#, links.join(""))
}

fn home(c: &Content, ctx: &Ctx) -> String {
    let s = &c.site;
    let mut glance = String::new();
    if let Some((i, e)) = c.events.iter().enumerate().find(|(_, e)| e.end >= ctx.now) {
        glance += &format!(
            r#"<li><span class="k">next</span><a href="events/" data-shape="2" data-group="{}">{}</a> <span class="m">{}</span></li>"#,
            i + 1,
            esc(&e.title),
            if ctx.live { date::relative(e.start, ctx.now) } else { date::long(e.start) }
        );
    }
    if let Some(p) = c.posts.first() {
        glance += &format!(
            r#"<li><span class="k">lately</span><a href="writing/{}/" data-shape="1" data-group="1">{}</a> <span class="m">{}</span></li>"#,
            esc(&p.slug),
            esc(&p.title),
            if ctx.live { date::relative(p.date, ctx.now) } else { date::long(p.date) }
        );
    }
    let place = if s.city.is_empty() { String::new() } else { format!("based in {}", esc(&s.city)) };
    format!(
        r#"<section class="home"><h1 class="intro"><span class="name">{}</span> {}</h1><div class="bio">{}</div><ul class="glance">{}</ul><p class="clock" data-clock>{}</p>{}</section>"#,
        esc(&s.name),
        intro(c),
        markdown::to_html(&s.bio),
        glance,
        place,
        elsewhere(c),
    )
}

fn writing(c: &Content, ctx: &Ctx) -> String {
    let mut items = String::new();
    for (i, p) in c.posts.iter().enumerate() {
        items += &format!(
            r#"<li><a href="writing/{}/" data-shape="1" data-group="{}">{}<span class="t">{}</span>{}</a></li>"#,
            esc(&p.slug),
            i + 1,
            when(p.date, ctx),
            esc(&p.title),
            if p.summary.is_empty() { String::new() } else { format!(r#"<span class="s">{}</span>"#, esc(&p.summary)) },
        );
    }
    if items.is_empty() {
        items = r#"<li class="empty">Nothing yet. The stack is waiting.</li>"#.into();
    }
    format!(
        r#"<section class="list writing">{}<ol class="entries">{}</ol><p class="foot"><a href="feed.xml">Subscribe by RSS</a></p></section>"#,
        crumbs(c, &[("writing", "")]),
        items
    )
}

fn event_item(i: usize, e: &Event, ctx: &Ctx) -> String {
    let title = if e.url.is_empty() {
        format!(r#"<span class="t">{}</span>"#, esc(&e.title))
    } else {
        format!(r#"<a class="t" href="{}" rel="noopener">{}</a>"#, esc(&e.url), esc(&e.title))
    };
    let rel = if ctx.live { date::relative(e.start, ctx.now) } else { date::long(e.start) };
    let rel = format!(r#"<span class="rel">{rel}</span>"#);
    let meta: Vec<String> = [&e.kind, &e.location].iter().filter(|s| !s.is_empty()).map(|s| esc(s)).collect();
    let body = if e.body.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="d">{}</div>"#, markdown::to_html(&e.body))
    };
    format!(
        r#"<li data-shape="2" data-group="{}" tabindex="0"><time datetime="{}" data-local>{}</time>{}{}<span class="where">{}</span>{}<a class="ics" href="events/{}.ics" download>add to calendar</a></li>"#,
        i + 1,
        date::iso(e.start),
        date::event(e.start, e.offset),
        rel,
        title,
        meta.join(" · "),
        body,
        esc(&e.slug),
    )
}

fn events(c: &Content, ctx: &Ctx) -> String {
    let (mut ahead, mut behind) = (String::new(), String::new());
    for (i, e) in c.events.iter().enumerate() {
        if e.end >= ctx.now {
            ahead += &event_item(i, e, ctx);
        }
    }
    for (i, e) in c.events.iter().enumerate().rev() {
        if e.end < ctx.now {
            behind += &event_item(i, e, ctx);
        }
    }
    if ahead.is_empty() {
        ahead = r#"<li class="empty">Nothing on the horizon yet.</li>"#.into();
    }
    let book = if c.site.calendar.is_empty() {
        String::new()
    } else {
        format!(
            r#" · <a href="{}" data-shape="4" target="_blank" rel="noopener">or book time with me</a>"#,
            esc(&c.site.calendar)
        )
    };
    let behind = if behind.is_empty() {
        String::new()
    } else {
        format!(r#"<h2 class="label">behind</h2><ol class="entries past">{behind}</ol>"#)
    };
    format!(
        r#"<section class="list events">{}<h2 class="label">ahead</h2><ol class="entries">{}</ol>{}<p class="foot"><a href="events.ics">Subscribe to all of it</a>{}</p></section>"#,
        crumbs(c, &[("events", "")]),
        ahead,
        behind,
        book
    )
}

fn post(c: &Content, i: usize, p: &Post, ctx: &Ctx) -> String {
    let tags = p.tags.iter().map(|t| format!("<span>{}</span>", esc(t))).collect::<String>();
    let mut around = String::new();
    if let Some(newer) = i.checked_sub(1).and_then(|j| c.posts.get(j)) {
        around += &format!(r#"<a href="writing/{}/" rel="next">newer: {}</a>"#, esc(&newer.slug), esc(&newer.title));
    }
    if let Some(older) = c.posts.get(i + 1) {
        around += &format!(r#"<a href="writing/{}/" rel="prev">older: {}</a>"#, esc(&older.slug), esc(&older.title));
    }
    format!(
        r#"<article class="post">{}<h1>{}</h1><p class="meta">{}<span>{} words</span>{}</p><div class="prose">{}</div><footer>{}</footer></article>"#,
        crumbs(c, &[("writing", "writing/")]),
        esc(&p.title),
        when(p.date, ctx),
        p.words,
        tags,
        markdown::to_html(&p.body),
        around
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::load;

    #[test]
    fn resolves_routes() {
        assert_eq!(resolve(""), Route::Home);
        assert_eq!(resolve("/index.html"), Route::Home);
        assert_eq!(resolve("writing/"), Route::Writing);
        assert_eq!(resolve("writing/hello/?x=1"), Route::Post("hello"));
        assert_eq!(resolve("writing/hello/index.html"), Route::Post("hello"));
        assert_eq!(resolve("events"), Route::Events);
        assert_eq!(resolve("nope/deeper/still"), Route::NotFound);
    }

    #[test]
    fn intro_sentence_becomes_navigation() {
        let c = load(&[(
            "site.md",
            "---\nname: Ada\nintro: [writes](writing), [speaks](events) & keeps [a calendar open](calendar).\ncalendar: https://cal.example\n---\n",
        )]);
        let html = intro(&c);
        assert!(html.contains(r#"<a href="writing/" data-shape="1">writes</a>"#));
        assert!(html.contains(r#"href="https://cal.example" data-shape="4""#));
        assert!(html.contains("&amp; keeps"));
    }

    #[test]
    fn unknown_posts_fall_through_to_not_found() {
        let c = load(&[]);
        let v = render(&c, Route::Post("missing"), &Ctx { now: 0, live: true });
        assert_eq!(v.shape, Shape::Scatter);
    }
}
