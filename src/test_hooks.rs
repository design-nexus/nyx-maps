//! Hooks for a test copy (run under NYX_MAPS_APP_ID): a made-up location fix, and
//! typing into a place search with what the field and its list do written to stderr,
//! so neither needs the real keyboard or the location service.

use crate::location::GeoLocation;
use crate::ui::MainWindow;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene};
use std::rc::Rc;
use std::time::Duration;

fn test_copy() -> bool {
    std::env::var_os("NYX_MAPS_APP_ID").is_some()
}

/// NYX_MAPS_TEST_FIX="lat,lon,accuracy[,ip]": where the location service says you are.
pub fn fix() -> Option<GeoLocation> {
    if !test_copy() {
        return None;
    }
    let value = std::env::var("NYX_MAPS_TEST_FIX").ok()?;
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    let num = |i: usize| parts.get(i)?.parse::<f64>().ok();
    Some(GeoLocation { latitude: num(0)?, longitude: num(1)?, accuracy: num(2)?, from_ip: parts.get(3) == Some(&"ip"), label: None })
}

/// NYX_MAPS_TEST_ME=1: the My location card, once the made-up fix is in.
pub fn me_card(win: &Rc<MainWindow>) {
    if !test_copy() || std::env::var_os("NYX_MAPS_TEST_ME").is_none() {
        return;
    }
    let win = win.clone();
    glib::timeout_add_local_once(Duration::from_millis(2500), move || win.show_me());
}

/// NYX_MAPS_TEST_TYPE="cof|fee": types into the header's search (with
/// NYX_MAPS_TEST_TYPE_IN=settings, into Home in Settings), pausing at each "|", then
/// works the list with Up, Down, Esc, Enter and clicks.
pub fn typing(win: &Rc<MainWindow>) {
    if !test_copy() {
        return;
    }
    let Ok(query) = std::env::var("NYX_MAPS_TEST_TYPE") else { return };
    let in_settings = std::env::var("NYX_MAPS_TEST_TYPE_IN").as_deref() == Ok("settings");
    let win = win.clone();
    glib::spawn_future_local(async move {
        let pause = |ms: u64| glib::timeout_future(Duration::from_millis(ms));
        pause(2000).await;
        let entry = if in_settings {
            win.open_settings();
            pause(800).await;
            find_entry(win.window.upcast_ref(), "Search an address")
        } else {
            Some(win.search_bar.entry.clone())
        };
        let Some(entry) = entry else {
            eprintln!("maps-test: no search field found");
            return;
        };
        entry.grab_focus();
        entry.delete_text(0, -1);
        log("focused", &entry);
        for chunk in query.split('|') {
            for ch in chunk.chars() {
                let mut at = entry.text().chars().count() as i32;
                entry.insert_text(&ch.to_string(), &mut at);
                entry.set_position(-1);
                log(&format!("typed {ch:?}"), &entry);
                pause(120).await;
            }
            pause(3500).await;
            log("results", &entry);
        }
        for key in [gdk::Key::Down, gdk::Key::Down, gdk::Key::Down, gdk::Key::Up] {
            press(&entry, key);
            log(&format!("key {}", key.name().unwrap_or_default()), &entry);
        }
        pause(600).await;
        dump_list(&entry);
        pause(3400).await;
        click(&entry, &entry);
        log("clicked the field", &entry);
        press(&entry, gdk::Key::Escape);
        log("key Escape", &entry);
        press(&entry, gdk::Key::Down);
        log("key Down (list closed)", &entry);
        click(&entry, &win.map_view.root);
        log("clicked the map", &entry);
        click(&entry, &entry);
        log("clicked the field", &entry);
        press(&entry, gdk::Key::Down);
        press(&entry, gdk::Key::Down);
        press(&entry, gdk::Key::Return);
        log("Down, Down, Enter", &entry);
    });
}

/// NYX_MAPS_TEST_DUMP=path.png: the open list drawn to a file, with where the field
/// sits in the window (a headless output doesn't draw popups).
fn dump_list(entry: &gtk::Entry) {
    let Ok(path) = std::env::var("NYX_MAPS_TEST_DUMP") else { return };
    let mut child = entry.first_child();
    while let Some(c) = child {
        if let Some(pop) = c.downcast_ref::<gtk::Popover>()
            && let Some(renderer) = pop.renderer()
        {
            let paintable = gtk::WidgetPaintable::new(Some(pop));
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, pop.width() as f64, pop.height() as f64);
            if let Some(node) = snapshot.to_node() {
                let _ = renderer.render_texture(&node, None).save_to_png(&path);
            }
            let root = entry.root().map(|r| r.upcast::<gtk::Widget>());
            let at = root.and_then(|r| entry.compute_bounds(&r));
            eprintln!(
                "maps-test: dumped the list ({}x{}); field at {:?}",
                pop.width(),
                pop.height(),
                at.map(|b| (b.x(), b.y(), b.width(), b.height()))
            );
        }
        child = c.next_sibling();
    }
}

fn find_entry(w: &gtk::Widget, placeholder: &str) -> Option<gtk::Entry> {
    if let Some(e) = w.downcast_ref::<gtk::Entry>()
        && e.placeholder_text().as_deref() == Some(placeholder)
    {
        return Some(e.clone());
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(found) = find_entry(&c, placeholder) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

fn controller<T: IsA<gtk::EventController> + IsA<glib::Object>>(w: &impl IsA<gtk::Widget>, name: &str) -> Option<T> {
    let list = w.observe_controllers();
    (0..list.n_items()).filter_map(|i| list.item(i).and_downcast::<T>()).find(|c| c.name().as_deref() == Some(name))
}

/// The key reaches the field's own key handler, as a real key press does first.
fn press(entry: &gtk::Entry, key: gdk::Key) {
    if let Some(keys) = controller::<gtk::EventControllerKey>(entry, "maps-search-keys") {
        let used = keys.emit_by_name::<bool>("key-pressed", &[&key.into_glib(), &0u32, &gdk::ModifierType::empty()]);
        if !used && matches!(key, gdk::Key::Return) {
            entry.emit_activate();
        }
    }
}

/// A click in the middle of `target`: the window's outside-click check, then the
/// field's own when it lands there.
fn click(entry: &gtk::Entry, target: &impl IsA<gtk::Widget>) {
    let Some(root) = entry.root() else { return };
    let root: &gtk::Widget = root.upcast_ref();
    let middle = graphene::Point::new(target.width() as f32 / 2.0, target.height() as f32 / 2.0);
    let Some(at) = target.compute_point(root, &middle) else { return };
    if let Some(outside) = controller::<gtk::GestureClick>(root, "maps-search-outside") {
        outside.emit_by_name::<()>("pressed", &[&1i32, &(at.x() as f64), &(at.y() as f64)]);
    }
    if target.as_ref() == entry.upcast_ref::<gtk::Widget>()
        && let Some(reopen) = controller::<gtk::GestureClick>(entry, "maps-search-reopen")
    {
        reopen.emit_by_name::<()>("pressed", &[&1i32, &0f64, &0f64]);
    }
}

fn log(step: &str, entry: &gtk::Entry) {
    let focus = entry.root().and_then(|r| r.focus());
    let in_field = focus.is_some_and(|f| f == *entry.upcast_ref::<gtk::Widget>() || f.is_ancestor(entry));
    let mut popover = None;
    let mut child = entry.first_child();
    while let Some(c) = child {
        if let Some(p) = c.downcast_ref::<gtk::Popover>() {
            popover = Some(p.clone());
        }
        child = c.next_sibling();
    }
    let rows = popover.as_ref().and_then(|p| p.child()).and_then(|b| find_class(&b, "search-results-list"));
    let (mut count, mut highlighted, mut first) = (0, None, None);
    let mut row = rows.and_then(|r| r.first_child());
    while let Some(r) = row {
        if r.has_css_class("highlighted") {
            highlighted = Some(count);
        }
        if first.is_none() {
            first = row_title(&r);
        }
        count += 1;
        row = r.next_sibling();
    }
    let surface = popover.as_ref().and_then(|p| p.surface()).map(|s| {
        let at = s.downcast_ref::<gdk::Popup>().map(|p| (p.position_x(), p.position_y()));
        format!("mapped={} {}x{} at={at:?}", s.is_mapped(), s.width(), s.height())
    });
    eprintln!(
        "maps-test: {step}: text={:?} focus_in_field={in_field} list_open={} rows={count} highlighted={highlighted:?} first={first:?} surface={surface:?}",
        entry.text(),
        popover.is_some_and(|p| p.is_visible()),
    );
}

fn find_class(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if w.has_css_class(class) {
        return Some(w.clone());
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(found) = find_class(&c, class) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

fn row_title(w: &gtk::Widget) -> Option<String> {
    if let Some(l) = w.downcast_ref::<gtk::Label>()
        && l.has_css_class("place-title")
    {
        return Some(l.text().to_string());
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(t) = row_title(&c) {
            return Some(t);
        }
        child = c.next_sibling();
    }
    None
}
