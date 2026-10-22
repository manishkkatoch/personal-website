use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
}

/// Renders markdown to HTML, giving every heading a stable anchor id.
pub fn to_html(md: &str) -> String {
    let mut events: Vec<Event> = Parser::new_ext(md, options()).collect();
    let mut used: Vec<String> = Vec::new();
    for i in 0..events.len() {
        if !matches!(events[i], Event::Start(Tag::Heading { id: None, .. })) {
            continue;
        }
        let mut text = String::new();
        for e in &events[i + 1..] {
            match e {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                _ => {}
            }
        }
        let base = slugify(&text);
        let mut id = base.clone();
        let mut n = 1;
        while used.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        used.push(id.clone());
        if let Event::Start(Tag::Heading { id: slot, .. }) = &mut events[i] {
            *slot = Some(CowStr::from(id));
        }
    }
    let mut out = String::with_capacity(md.len() * 3 / 2);
    html::push_html(&mut out, events.into_iter());
    out
}

/// Text content only, for word counts and search.
pub fn plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for e in Parser::new_ext(md, options()) {
        match e {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak | Event::End(_) => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_get_unique_ids() {
        let html = to_html("# Hello world\n\n## Hello world\n\ntext");
        assert!(html.contains(r#"<h1 id="hello-world">"#));
        assert!(html.contains(r#"<h2 id="hello-world-2">"#));
    }

    #[test]
    fn plain_strips_markup() {
        assert_eq!(plain("# Title\n\nSome *emph* and `code`."), "Title Some emph and code.");
    }
}
