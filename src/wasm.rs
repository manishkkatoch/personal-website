//! The raw wasm ABI the browser shim talks to. No bindgen: strings go in
//! through `alloc` and come back through `out_ptr`/`out_len`; the field's
//! buffers are read straight out of linear memory.

use crate::{
    content::Content,
    date, esc,
    field::{Field, Params, Shape},
    json,
    search::Index,
    views::{self, Ctx},
};
use std::cell::UnsafeCell;

struct App {
    content: Content,
    field: Field,
    index: Index,
    out: String,
}

struct Global(UnsafeCell<Option<App>>);
// The module is single-threaded; nothing is shared across threads.
unsafe impl Sync for Global {}
static APP: Global = Global(UnsafeCell::new(None));

fn app() -> &'static mut App {
    unsafe { (*APP.0.get()).as_mut().expect("boot() first") }
}

unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("")
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must come from [`alloc`] with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    app().out.as_ptr()
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    app().out.len()
}

/// Loads the embedded content and lays out the field.
/// `tz` is the visitor's UTC offset in minutes, for "today" on the calendar.
#[no_mangle]
pub extern "C" fn boot(now_ms: f64, tz: i32, points: u32) {
    let content = Content::embedded();
    let now = (now_ms / 1000.0) as i64;
    let field = Field::new(points.clamp(500, 20_000) as usize, params(&content, now, tz));
    let index = Index::build(&content, now);
    unsafe { *APP.0.get() = Some(App { content, field, index, out: String::new() }) };
}

fn params(c: &Content, now: i64, tz: i32) -> Params {
    let (lo, hi) = c.events.iter().fold((now, now), |(lo, hi), e| (lo.min(e.start), hi.max(e.start)));
    let pad = ((hi - lo) / 10).max(30 * date::DAY);
    let (lo, hi) = (lo - pad, hi + pad);
    let at = |t: i64| (t - lo) as f32 / (hi - lo) as f32;
    let today = date::civil(now, tz);
    Params {
        sheets: c.posts.len(),
        events: c.events.iter().map(|e| at(e.start)).collect(),
        now: at(now),
        first_weekday: date::weekday(date::days_from_civil(today.y, today.m, 1)),
        month_days: date::days_in_month(today.y, today.m),
        today: today.d,
    }
}

#[no_mangle]
pub extern "C" fn site() {
    let a = app();
    let s = &a.content.site;
    a.out = format!(r#"{{"name":{},"timezone":{},"city":{}}}"#, json(&s.name), json(&s.timezone), json(&s.city));
}

/// Renders a route (path relative to the site base) and morphs the field.
/// # Safety
/// `ptr`/`len` must describe a valid UTF-8 buffer.
#[no_mangle]
pub unsafe extern "C" fn route(ptr: *const u8, len: usize, now_ms: f64) {
    let a = app();
    let ctx = Ctx { now: (now_ms / 1000.0) as i64, live: true };
    let v = views::render(&a.content, views::resolve(input(ptr, len)), &ctx);
    a.field.set_base(v.shape);
    a.out = format!(
        r#"{{"title":{},"description":{},"shape":{},"html":{}}}"#,
        json(&v.title),
        json(&v.description),
        v.shape as u32,
        json(&v.html)
    );
}

/// Searches everything; returns `{count, html}` with one link per hit.
/// # Safety
/// `ptr`/`len` must describe a valid UTF-8 buffer.
#[no_mangle]
pub unsafe extern "C" fn search(ptr: *const u8, len: usize) {
    let a = app();
    let q = input(ptr, len);
    let hits = a.index.query(q, 8);
    let mut html = String::new();
    for (i, h) in hits.iter().enumerate() {
        let d = h.doc;
        let target = if d.external { r#" target="_blank" rel="noopener""# } else { "" };
        html += &format!(
            r#"<a href="{}" class="hit{}" role="option" data-i="{}"{}><span class="kind">{}</span><span class="t">{}</span><span class="h">{}</span>{}</a>"#,
            esc(&d.href),
            if i == 0 { " on" } else { "" },
            i,
            target,
            d.kind,
            esc(&d.title),
            esc(&d.hint),
            if h.snippet.is_empty() { String::new() } else { format!(r#"<span class="snip">{}</span>"#, h.snippet) },
        );
    }
    if hits.is_empty() {
        html = format!(r#"<p class="none">Nothing for “{}”. The field is listening, though.</p>"#, esc(q));
    }
    a.out = format!(r#"{{"count":{},"html":{}}}"#, hits.len(), json(&html));
}

#[no_mangle]
pub extern "C" fn field_count() -> usize {
    app().field.n
}

#[no_mangle]
pub extern "C" fn field_resize(w: f32, h: f32) {
    app().field.resize(w, h);
}

#[no_mangle]
pub extern "C" fn field_pointer(x: f32, y: f32, on: u32) {
    app().field.pointer(x, y, on != 0);
}

#[no_mangle]
pub extern "C" fn field_drag(dx: f32, dy: f32, held: u32) {
    app().field.drag(dx, dy, held != 0);
}

#[no_mangle]
pub extern "C" fn field_spin(d: f32) {
    app().field.spin(d);
}

/// Previews `shape` with `group` lit; shape 255 returns to the route's own.
#[no_mangle]
pub extern "C" fn field_hover(shape: u32, group: u32) {
    let s = if shape == 255 { None } else { Some(Shape::from_u32(shape)) };
    app().field.preview(s, group.min(u16::MAX as u32) as u16);
}

#[no_mangle]
pub extern "C" fn field_progress(p: f32) {
    app().field.set_progress(p);
}

#[no_mangle]
pub extern "C" fn field_energy(e: f32) {
    app().field.set_energy(e);
}

/// Steps the simulation; returns a pointer to `n * [x, y, z, alpha]`.
#[no_mangle]
pub extern "C" fn field_step(dt: f32) -> *const f32 {
    let f = &mut app().field;
    f.step(dt);
    f.positions().as_ptr()
}

/// Camera distance, so point size and depth fade stay constant on any screen.
#[no_mangle]
pub extern "C" fn field_distance() -> f32 {
    app().field.distance()
}

/// Pointer to the 16-float column-major model-view-projection matrix.
#[no_mangle]
pub extern "C" fn field_mvp() -> *const f32 {
    app().field.mvp().as_ptr()
}
