---
title: How to write here
summary: Posts and events are markdown files with a little front matter.
tags: meta, guide
---
A post is a markdown file in `content/posts/`, named
`YYYY-MM-DD-slug.md`:

```markdown
---
title: A title
summary: One line for lists and link previews.
tags: comma, separated
---
The body, in markdown. Tables, footnotes and task lists work.
```

An event is a file in `content/events/`. Write the time in the zone the
event happens in, with its offset; visitors see it in their own:

```markdown
---
title: A talk
start: 2026-11-14T18:30+01:00
end: 2026-11-14T19:15+01:00
location: Berlin
kind: talk
url: https://example.com
---
What it is about.
```

Add `draft: true` to keep something out. Push, and the site rebuilds.
