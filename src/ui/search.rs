use crate::providers::{SearchResult, distance_m, format_distance, parse_coordinates, search_places_near};
use crate::settings;
use crate::storage::Database;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// Where results should be near (the map's centre, or you).
pub type Near = Rc<dyn Fn() -> Option<(f64, f64)>>;

/// A search field with a list of matching places under it, nearest first. The header's
/// search, the directions panel's start and destination, and Home and Work in Settings.
///
/// The list never takes the keyboard: the field keeps it while the list follows what's
/// typed. Up and Down move through the list, Enter picks, Esc closes it, and a click
/// anywhere but the field or the list closes it too.
pub struct SearchBar {
    pub root: gtk::Box,
    pub entry: gtk::Entry,
    popover: gtk::Popover,
    results_list: gtk::Box,
    current_search_id: Rc<RefCell<u64>>,
    suppress_search: Rc<RefCell<bool>>,
    db: Database,
    on_select: Rc<dyn Fn(SearchResult)>,
    near: Rc<RefCell<Option<Near>>>,
    /// A field (start, destination) shows the picked place's name in the entry.
    fills: bool,
    /// The places in the list now, for Enter.
    shown: Rc<RefCell<Vec<SearchResult>>>,
    /// The place Up and Down are on.
    highlight: Rc<Cell<Option<usize>>>,
    picked: Rc<Cell<bool>>,
}

impl SearchBar {
    /// The header's search.
    pub fn new(db: Database, on_select: impl Fn(SearchResult) + 'static) -> Self {
        let bar = Self::build(db, "Search places and addresses", false, Rc::new(on_select));
        bar.entry.add_css_class("maps-search-entry");
        bar.entry.set_width_chars(16);
        bar.entry.set_max_width_chars(34);
        bar
    }

    /// A start or destination field: picking a place puts its name in the entry.
    pub fn field(db: Database, placeholder: &str, near: Near, on_select: impl Fn(SearchResult) + 'static) -> Self {
        let bar = Self::build(db, placeholder, true, Rc::new(on_select));
        bar.entry.add_css_class("waypoint-entry");
        bar.entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, None);
        bar.entry.set_hexpand(true);
        bar.entry.set_width_chars(12);
        bar.set_near(near);
        bar
    }

    fn build(db: Database, placeholder: &str, fills: bool, on_select: Rc<dyn Fn(SearchResult)>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(placeholder));
        entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, Some("system-search-symbolic"));
        entry.set_icon_activatable(gtk::EntryIconPosition::Primary, false);
        root.append(&entry);

        // Not autohide: an autohide popover moves the focus into itself when it opens
        // and grabs the pointer, so a click on the field would close it.
        let popover = gtk::Popover::new();
        popover.set_parent(&entry);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_autohide(false);
        popover.set_can_focus(false);
        popover.set_has_arrow(false);
        popover.add_css_class("search-popover");

        let pop_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        pop_box.set_size_request(340, -1);
        pop_box.set_margin_start(4);
        pop_box.set_margin_end(4);
        pop_box.set_margin_top(4);
        pop_box.set_margin_bottom(4);

        let results_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        results_list.add_css_class("search-results-list");
        // Scrolled, so the list's smallest height stays small: GTK closes a popover
        // whose content outgrows its surface before the surface is resized, as a
        // longer list of results would.
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(440)
            .child(&results_list)
            .build();
        pop_box.append(&scroller);
        popover.set_child(Some(&pop_box));

        let search_bar = Self {
            root,
            entry,
            popover,
            results_list,
            current_search_id: Rc::new(RefCell::new(0u64)),
            suppress_search: Rc::new(RefCell::new(false)),
            db,
            on_select,
            near: Rc::new(RefCell::new(None)),
            fills,
            shown: Rc::new(RefCell::new(Vec::new())),
            highlight: Rc::new(Cell::new(None)),
            picked: Rc::new(Cell::new(false)),
        };

        search_bar.setup_signals();
        search_bar
    }

    /// Results nearest to what `near` gives come first.
    pub fn set_near(&self, near: Near) {
        *self.near.borrow_mut() = Some(near);
    }

    fn picker(&self) -> Picker {
        self.clone_parts().picker(&self.near)
    }

    fn setup_signals(&self) {
        let search_id = self.current_search_id.clone();
        let suppress = self.suppress_search.clone();
        let popover = self.popover.clone();
        let near = self.near.clone();
        let picked = self.picked.clone();
        let me = self.clone_parts();

        self.entry.connect_changed(move |e| {
            // Text set by the app (one change can be two signals), or typed somewhere else:
            // no search. Only what's typed into the focused field searches.
            if *suppress.borrow() || !focused(e) {
                return;
            }
            picked.set(false);
            me.set_highlight(None);

            let text = e.text().trim().to_string();
            if text.is_empty() {
                *search_id.borrow_mut() += 1;
                popover.popdown();
                return;
            }

            // Direct coordinates check - can center without waiting for network debounce
            if let Some((lat, lon)) = parse_coordinates(&text) {
                let place = SearchResult {
                    name: format!("{:.5}, {:.5}", lat, lon),
                    formatted_address: format!("Latitude: {:.5}, Longitude: {:.5}", lat, lon),
                    lat,
                    lon,
                    category: Some("Coordinate".to_string()),
                };
                me.picker(&near).show(vec![place]);
                return;
            }

            let next_id = {
                let mut id = search_id.borrow_mut();
                *id += 1;
                *id
            };
            let id_check = search_id.clone();
            let picker = me.picker(&near);
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                if *id_check.borrow() != next_id {
                    return;
                }
                picker.search(text, Some((id_check, next_id)));
            });
        });

        // Enter with the list closed picks the first place it had.
        let shown = self.shown.clone();
        let parts = self.clone_parts();
        let near = self.near.clone();
        self.entry.connect_activate(move |_| {
            let first = shown.borrow().first().cloned();
            if let Some(place) = first {
                parts.picker(&near).pick(place);
            }
        });

        // Ahead of the field's own keys, so Up, Down, Enter and Esc work the list.
        let keys = gtk::EventControllerKey::new();
        keys.set_name(Some("maps-search-keys"));
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let parts = self.clone_parts();
        let near = self.near.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if parts.on_key(key, &near) { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        });
        self.entry.add_controller(keys);

        // A click on the field brings back the list Esc closed.
        let reopen = gtk::GestureClick::new();
        reopen.set_name(Some("maps-search-reopen"));
        reopen.set_propagation_phase(gtk::PropagationPhase::Capture);
        let parts = self.clone_parts();
        reopen.connect_pressed(move |_, _, _, _| {
            parts.reopen();
        });
        self.entry.add_controller(reopen);

        // While the list is open: a click outside the field, or the window losing the
        // keyboard, closes it.
        let watch: Rc<RefCell<Option<OutsideWatch>>> = Rc::new(RefCell::new(None));
        let (w, entry) = (watch.clone(), self.entry.clone());
        self.popover.connect_show(move |pop| {
            if w.borrow().is_none()
                && let Some(root) = entry.root()
            {
                *w.borrow_mut() = Some(OutsideWatch::new(root.upcast(), &entry, pop));
            }
        });
        self.popover.connect_closed(move |_| {
            if let Some(w) = watch.borrow_mut().take() {
                w.stop();
            }
        });
    }

    fn clone_parts(&self) -> Parts {
        Parts {
            entry: self.entry.clone(),
            popover: self.popover.clone(),
            list: self.results_list.clone(),
            on_select: self.on_select.clone(),
            db: self.db.clone(),
            fills: self.fills,
            suppress: self.suppress_search.clone(),
            shown: self.shown.clone(),
            highlight: self.highlight.clone(),
            picked: self.picked.clone(),
        }
    }

    pub fn focus(&self) {
        self.entry.grab_focus();
    }

    /// Set the text without searching for it.
    pub fn set_text(&self, text: &str) {
        set_quietly(&self.entry, &self.suppress_search, text);
        self.popover.popdown();
    }

    pub fn text(&self) -> String {
        self.entry.text().trim().to_string()
    }

    /// True once a place from the list is in the field (until it's edited).
    pub fn has_pick(&self) -> bool {
        self.picked.get()
    }

    /// Mark the field as holding a known place (set from elsewhere: a place card,
    /// My Location).
    pub fn set_picked(&self, on: bool) {
        self.picked.set(on);
    }

    /// Search for what's typed now and show the list to choose from (the field takes
    /// the focus, as the list belongs to it).
    pub fn search_now(&self) {
        let text = self.text();
        if !text.is_empty() {
            self.entry.grab_focus();
            self.picker().search(text, None);
        }
    }

    pub fn show_results(&self, query_label: &str, places: Vec<SearchResult>) {
        self.set_text(query_label);
        *self.current_search_id.borrow_mut() += 1;
        self.entry.grab_focus();
        self.picker().show(places);
    }

    pub fn select_place(&self, place: SearchResult) {
        (self.on_select)(place);
    }
}

/// Closes a list on clicks outside its field, and when the window goes to the back.
struct OutsideWatch {
    root: gtk::Widget,
    click: gtk::GestureClick,
    active: Option<glib::SignalHandlerId>,
}

impl OutsideWatch {
    fn new(root: gtk::Widget, entry: &gtk::Entry, popover: &gtk::Popover) -> Self {
        let click = gtk::GestureClick::new();
        click.set_name(Some("maps-search-outside"));
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let (entry_w, pop_w) = (entry.downgrade(), popover.downgrade());
        click.connect_pressed(move |g, _, x, y| {
            let (Some(entry), Some(pop), Some(root)) = (entry_w.upgrade(), pop_w.upgrade(), g.widget()) else { return };
            // Clicks in the list's own surface aren't outside.
            if let Some(event) = g.current_event()
                && event.surface() != root.native().and_then(|n| n.surface())
            {
                return;
            }
            let inside = root.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|hit| hit == *entry.upcast_ref::<gtk::Widget>() || hit.is_ancestor(&entry));
            if !inside {
                pop.popdown();
            }
        });
        root.add_controller(click.clone());
        let active = root.downcast_ref::<gtk::Window>().map(|win| {
            let pop_w = popover.downgrade();
            win.connect_is_active_notify(move |win| {
                if !win.is_active()
                    && let Some(pop) = pop_w.upgrade()
                {
                    pop.popdown();
                }
            })
        });
        Self { root, click, active }
    }

    fn stop(self) {
        self.root.remove_controller(&self.click);
        if let Some(id) = self.active {
            self.root.disconnect(id);
        }
    }
}

#[derive(Clone)]
struct Parts {
    entry: gtk::Entry,
    popover: gtk::Popover,
    list: gtk::Box,
    on_select: Rc<dyn Fn(SearchResult)>,
    db: Database,
    fills: bool,
    suppress: Rc<RefCell<bool>>,
    shown: Rc<RefCell<Vec<SearchResult>>>,
    highlight: Rc<Cell<Option<usize>>>,
    picked: Rc<Cell<bool>>,
}

impl Parts {
    fn picker(&self, near: &Rc<RefCell<Option<Near>>>) -> Picker {
        Picker { parts: self.clone(), near: near.borrow().as_ref().and_then(|n| n()) }
    }

    /// Up and Down through the list, Enter to pick, Esc to close; true if used.
    fn on_key(&self, key: gdk::Key, near: &Rc<RefCell<Option<Near>>>) -> bool {
        let open = self.popover.is_visible();
        let count = self.shown.borrow().len();
        match key {
            gdk::Key::Escape if open => {
                self.popover.popdown();
                true
            }
            gdk::Key::Down | gdk::Key::KP_Down if !open => self.reopen(),
            gdk::Key::Down | gdk::Key::KP_Down if count > 0 => {
                self.set_highlight(Some(self.highlight.get().map_or(0, |i| (i + 1).min(count - 1))));
                true
            }
            gdk::Key::Up | gdk::Key::KP_Up if open && count > 0 => {
                self.set_highlight(self.highlight.get().and_then(|i| i.checked_sub(1)));
                true
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter if open && count > 0 => {
                let i = self.highlight.get().unwrap_or(0).min(count - 1);
                let place = self.shown.borrow()[i].clone();
                self.picker(near).pick(place);
                true
            }
            _ => false,
        }
    }

    /// Show the list again for what's still typed; true if it opened.
    fn reopen(&self) -> bool {
        let open = !self.popover.is_visible()
            && !self.picked.get()
            && !self.shown.borrow().is_empty()
            && !self.entry.text().trim().is_empty();
        if open {
            self.popover.popup();
        }
        open
    }

    fn set_highlight(&self, at: Option<usize>) {
        self.highlight.set(at);
        let scroller = self.list.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>();
        let mut row = self.list.first_child();
        let mut i = 0;
        while let Some(r) = row {
            if Some(i) == at {
                r.add_css_class("highlighted");
                if let (Some(s), Some(bounds)) = (&scroller, r.compute_bounds(&self.list)) {
                    s.vadjustment().clamp_page(bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
                }
            } else {
                r.remove_css_class("highlighted");
            }
            row = r.next_sibling();
            i += 1;
        }
    }
}

/// One search's list: fetching, showing and picking a place.
#[derive(Clone)]
struct Picker {
    parts: Parts,
    near: Option<(f64, f64)>,
}

impl Picker {
    /// Fetch places for `text`; a stale answer (`id` moved on) is dropped.
    fn search(self, text: String, id: Option<(Rc<RefCell<u64>>, u64)>) {
        let api_key = settings::get().geoapify_api_key;
        let near = self.near;
        let (sender, receiver) = async_channel::bounded(1);
        crate::runtime::spawn(async move {
            let _ = sender.send(search_places_near(&text, &api_key, near).await).await;
        });
        glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else { return };
            if let Some((current, mine)) = &id
                && *current.borrow() != *mine
            {
                return;
            }
            match result {
                Ok(places) if !places.is_empty() => self.show(places),
                _ => self.show_empty(),
            }
        });
    }

    fn clear(&self) {
        while let Some(child) = self.parts.list.first_child() {
            self.parts.list.remove(&child);
        }
        self.parts.highlight.set(None);
    }

    fn show_empty(&self) {
        if !focused(&self.parts.entry) {
            return;
        }
        self.clear();
        self.parts.shown.borrow_mut().clear();
        let note = gtk::Label::new(Some("No places found. Try another name or an address."));
        note.add_css_class("place-desc");
        note.set_margin_top(8);
        note.set_margin_bottom(8);
        self.parts.list.append(&note);
        self.open();
    }

    /// Open the list, at its new size if it's open already: the compositor keeps an
    /// open popup at the size it first had, so a new size means opening it again.
    fn open(&self) {
        let pop = &self.parts.popover;
        if pop.is_visible() {
            let (_, natural, _, _) = pop.measure(gtk::Orientation::Vertical, pop.width());
            if natural == pop.height() {
                return;
            }
            pop.popdown();
        }
        pop.popup();
    }

    fn show(&self, places: Vec<SearchResult>) {
        // The list opens only under the field being typed in.
        if !focused(&self.parts.entry) {
            return;
        }
        self.clear();
        let units = settings::get().units;
        for place in &places {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("search-result-row");

            let icon = gtk::Image::from_icon_name(if place.category.as_deref() == Some("Coordinate") {
                "maps-locate-symbolic"
            } else {
                "maps-pin-symbolic"
            });
            icon.set_pixel_size(16);
            icon.set_valign(gtk::Align::Start);
            icon.set_margin_top(2);
            row.append(&icon);

            let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
            text_box.set_hexpand(true);
            let name_lbl = gtk::Label::new(Some(&place.name));
            name_lbl.add_css_class("place-title");
            name_lbl.set_xalign(0.0);
            name_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text_box.append(&name_lbl);
            let kind = place
                .category
                .as_deref()
                .filter(|c| *c != "Coordinate" && !c.contains('.'))
                .map(|c| {
                    let words = c.replace('_', " ");
                    let mut chars = words.chars();
                    chars.next().map(|f| f.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
                });
            let detail = match kind {
                Some(k) if !place.formatted_address.is_empty() => format!("{k} · {}", place.formatted_address),
                Some(k) => k,
                None => place.formatted_address.clone(),
            };
            let addr_lbl = gtk::Label::new(Some(&detail));
            addr_lbl.add_css_class("place-desc");
            addr_lbl.set_xalign(0.0);
            addr_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text_box.append(&addr_lbl);
            row.append(&text_box);

            if let Some(here) = self.near {
                let away = gtk::Label::new(Some(&format_distance(distance_m(here, (place.lat, place.lon)), &units)));
                away.add_css_class("place-distance");
                away.set_valign(gtk::Align::Start);
                row.append(&away);
            }

            let gesture = gtk::GestureClick::new();
            let (me, pl) = (self.clone(), place.clone());
            gesture.connect_released(move |_, _, _, _| me.pick(pl.clone()));
            row.add_controller(gesture);
            self.parts.list.append(&row);
        }
        *self.parts.shown.borrow_mut() = places;
        self.open();
    }

    fn pick(&self, place: SearchResult) {
        self.parts.popover.popdown();
        let _ = self.parts.db.add_recent_search(&place.name, &place.formatted_address, place.lat, place.lon);
        if self.parts.fills {
            set_quietly(&self.parts.entry, &self.parts.suppress, &place.name);
            self.parts.entry.set_position(-1);
        }
        self.parts.picked.set(true);
        (self.parts.on_select)(place);
    }
}

/// The field has the keyboard focus (its text part does, inside the entry).
fn focused(entry: &gtk::Entry) -> bool {
    entry.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN)
}

/// Set `entry`'s text with searching held off for the whole change.
fn set_quietly(entry: &gtk::Entry, suppress: &Rc<RefCell<bool>>, text: &str) {
    *suppress.borrow_mut() = true;
    entry.set_text(text);
    *suppress.borrow_mut() = false;
}
