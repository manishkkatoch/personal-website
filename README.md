# field

A personal site that is one WebAssembly binary.

Content, routing, templates, search and a few thousand particles all live in
a Rust crate compiled to `wasm32`. A thin JavaScript shim draws the points
with WebGL and forwards pointer, keys and scroll. The same crate also runs
natively at build time and pre-renders every page to plain HTML, so the site
works without JavaScript and is fully crawlable.

## What is unusual about it

- **The menu is a sentence.** "Manish Katoch *writes*, *speaks*, and keeps
  *a calendar open*." The verbs are the links. Hovering one reshapes the
  field before you click.
- **The page is the search box.** Start typing anywhere; `/` also works,
  `Esc` closes. Searches posts, events, links and pages.
- **Every page has a shape.** Home is a breathing sphere. Writing is a stack
  of sheets, one per post; hovering a post slides its sheet out. Events sit
  on a helix of time: past events in ink, future ones in pencil, and a ring
  marks now. The calendar link previews this month: past days are flat,
  future days have volume.
- **Reading progress is a ring.** While you read, the points gather into an
  ellipse around the text. It closes when you reach the end.
- **Time is distance.** Dates say "3 weeks ago" or "in 9 days". Event times
  are shown in the visitor's own zone.
- **The site keeps its owner's hours.** The home page shows your local time.
  After your midnight the field breathes slower and the page gets quieter.
- **Scroll turns the object** when there is nothing to scroll, and empty
  space can be dragged.

It respects `prefers-reduced-motion`, `prefers-color-scheme`, keyboard
navigation, and works with JavaScript or WebGL turned off.

## Make it yours

Everything is in `content/`:

| File | What |
| --- | --- |
| `content/site.md` | name, intro sentence, timezone, city, calendar link, email, social links, and your bio as the body |
| `content/posts/YYYY-MM-DD-slug.md` | posts: `title`, `summary`, `tags`, optional `date`, `draft: true` |
| `content/events/*.md` | events: `title`, `start`, `end`, `location`, `kind`, `url`; write times with their UTC offset, e.g. `2026-11-14T18:30+01:00` |

The sample posts describe the site itself; the sample events are
placeholders, so replace them. Set `timezone`, `city` and `calendar` in
`site.md` before going live.

## Local development

Needs Rust with the wasm target:

```sh
rustup target add wasm32-unknown-unknown
cargo dev
```

Open http://127.0.0.1:8000. Edit anything in `content/`, `src/` or `web/`
and every open tab rebuilds and reloads, keeping its scroll position.
A content edit lands in a second or three. Build errors appear in the
terminal and as an overlay in the browser, and clear once fixed.

- `PORT=8001 cargo dev` picks another port.
- `HOST=0.0.0.0 cargo dev` lets you open it from your phone on the same
  network (the touch interactions are worth checking there).
- `cargo test` runs the unit tests: dates, content, search, ics, views, field.

The dev server (`src/bin/dev.rs`) has no dependencies. It builds the wasm
with a lighter `wasm-dev` profile and writes to `target/site-dev`, so it
never touches `dist/`.

## Production build

```sh
./build.sh                         # writes dist/, fully optimised
python3 -m http.server -d dist     # check it at http://localhost:8000
```

`BASE=/sub/path/` builds for a sub-path, `SITE_URL=https://example.com`
enables absolute links in the feed, canonical tags and a sitemap. If
`wasm-opt` is on the `PATH`, the build uses it.

The build writes, alongside the pages: `feed.xml` (RSS), `events.ics` (a
calendar people can subscribe to), one `.ics` per event, and `404.html`.

## Deploy

`.github/workflows/site.yml` lints, tests, builds and deploys to GitHub
Pages on every push to `main`, and rebuilds daily so pre-rendered pages
stay current. Enable it once under **Settings → Pages → Source: GitHub
Actions**. A custom domain works too; the base path is picked up
automatically.

## Layout

```
content/          markdown: site.md, posts/, events/
build.rs          embeds content/ into the binary
src/content.rs    front matter + content model
src/views.rs      routes and markup (shared by wasm and the generator)
src/field.rs      particle simulation, shapes, camera
src/search.rs     search index
src/ics.rs        iCalendar output
src/wasm.rs       raw wasm ABI (no wasm-bindgen)
src/bin/generate.rs  static site generator
src/bin/dev.rs    local dev server: watch, rebuild, live reload
web/              HTML template, stylesheet, JS shim
```
