use crate::providers::SearchResult;
use crate::storage::{Database, FavoritePlace};
use gtk::prelude::*;
use nyx_ui::menu;
use nyx_ui::sidebar;
use nyx_ui::widgets;
use std::cell::RefCell;
use std::rc::Rc;

/// What a place is, in words ("Café", "Theatre"), from its search category.
pub fn kind_label(category: Option<&str>) -> Option<String> {
    let c = category?.rsplit('.').next()?.trim();
    if c.is_empty() || ["coordinate", "home", "work", "favorite", "recent search"].contains(&c.to_lowercase().as_str()) {
        return None;
    }
    let words = c.replace('_', " ");
    let mut chars = words.chars();
    chars.next().map(|f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// The parts of an address other than the place's own name, house number and street
/// joined ("118 Front Street", "Old Town", …).
fn address_parts(place: &SearchResult) -> Vec<String> {
    let parts: Vec<&str> =
        place.formatted_address.split(',').map(str::trim).filter(|p| !p.is_empty() && *p != place.name).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        if part.chars().all(|c| c.is_ascii_digit()) && i + 1 < parts.len() && out.is_empty() {
            out.push(format!("{part} {}", parts[i + 1]));
            i += 2;
        } else {
            out.push(part.to_string());
            i += 1;
        }
    }
    out
}

/// The street address, short: "118 Front Street".
pub fn short_address(place: &SearchResult) -> Option<String> {
    address_parts(place).into_iter().next().filter(|a| !a.starts_with("Latitude"))
}

/// The part of town or the city: "Old Town".
pub fn area(place: &SearchResult) -> Option<String> {
    address_parts(place).into_iter().nth(1).filter(|a| !a.chars().all(|c| c.is_ascii_digit()))
}

/// Coordinates as people read them: "41.2034, −72.9147".
pub fn coords_text(lat: f64, lon: f64) -> String {
    format!("{lat:.4}, {lon:.4}").replace('-', "\u{2212}")
}

/// The saved places that are this place (by name or where it is).
pub fn saved_as(db: &Database, place: &SearchResult) -> Vec<FavoritePlace> {
    db.list_favorites()
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.name == place.name || ((f.lat - place.lat).abs() < 1e-5 && (f.lon - place.lon).abs() < 1e-5))
        .collect()
}

pub fn favorite_place(f: &FavoritePlace) -> SearchResult {
    SearchResult {
        name: f.name.clone(),
        formatted_address: f.address.clone(),
        lat: f.lat,
        lon: f.lon,
        category: Some(f.label.clone()),
    }
}

/// The place picked on the map: a card over the map's corner with what it is, how
/// far, Directions, Save and Share, and its coordinates.
pub struct PlaceCard {
    pub root: gtk::Box,
    title: gtk::Label,
    sub: gtk::Label,
    travel: gtk::Box,
    travel_icon: gtk::Image,
    travel_label: gtk::Label,
    address: gtk::Box,
    address_label: gtk::Label,
    coords: gtk::Label,
    save_icon: gtk::Image,
    save_label: gtk::Label,
    save_btn: gtk::Button,
    current: Rc<RefCell<Option<SearchResult>>>,
}

impl PlaceCard {
    pub fn new(
        db: Database,
        on_close: impl Fn() + 'static,
        on_directions: impl Fn(SearchResult) + 'static,
        on_saved: impl Fn(&str) + 'static,
    ) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("map-card");
        root.add_css_class("place-card");
        root.set_halign(gtk::Align::Start);
        root.set_valign(gtk::Align::End);
        root.set_size_request(340, -1);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let names = gtk::Box::new(gtk::Orientation::Vertical, 2);
        names.set_hexpand(true);
        let title = widgets::label("", "card-title");
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title.set_max_width_chars(26);
        names.append(&title);
        let sub = widgets::label("", "card-sub");
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        names.append(&sub);
        head.append(&names);
        let close = widgets::icon_button("maps-close-symbolic", "Close (Esc)");
        close.add_css_class("card-close");
        close.set_valign(gtk::Align::Start);
        close.connect_clicked(move |_| on_close());
        head.append(&close);
        root.append(&head);

        // How far, and where.
        let facts = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        facts.add_css_class("card-facts");
        let fact = |icon: &str| {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let img = gtk::Image::from_icon_name(icon);
            img.set_pixel_size(14);
            b.append(&img);
            let l = widgets::label("", "");
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            b.append(&l);
            (b, img, l)
        };
        let (travel, travel_icon, travel_label) = fact("maps-car-symbolic");
        travel.set_visible(false);
        facts.append(&travel);
        let (address, _, address_label) = fact("maps-pin-symbolic");
        address.set_hexpand(true);
        facts.append(&address);
        root.append(&facts);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.add_css_class("card-actions");
        let directions = card_button("maps-directions-symbolic", "Directions");
        directions.add_css_class("suggested-action");
        directions.set_tooltip_text(Some("Directions to here"));
        actions.append(&directions);

        let save_btn = gtk::Button::new();
        save_btn.add_css_class("card-button");
        save_btn.add_css_class("save-button");
        let save_face = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let save_icon = gtk::Image::from_icon_name("maps-star-symbolic");
        save_icon.set_pixel_size(15);
        save_icon.add_css_class("save-star");
        save_face.append(&save_icon);
        let save_label = gtk::Label::new(Some("Save"));
        save_face.append(&save_label);
        let chevron = gtk::Image::from_icon_name("maps-chevron-down-symbolic");
        chevron.set_pixel_size(12);
        chevron.add_css_class("chevron");
        save_face.append(&chevron);
        save_btn.set_child(Some(&save_face));
        save_btn.set_tooltip_text(Some("Save this place, or make it Home or Work"));
        actions.append(&save_btn);

        let share = card_button("maps-share-symbolic", "Share");
        share.set_tooltip_text(Some("Share a link to this place"));
        actions.append(&share);
        root.append(&actions);

        let coords_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        coords_row.add_css_class("card-coords");
        let coords = widgets::label("", "");
        coords_row.append(&coords);
        let copy = widgets::icon_button("maps-copy-symbolic", "Copy the coordinates");
        copy.add_css_class("copy-button");
        coords_row.append(&copy);
        root.append(&coords_row);

        let current: Rc<RefCell<Option<SearchResult>>> = Rc::new(RefCell::new(None));
        let card = Rc::new(Self {
            root,
            title,
            sub,
            travel,
            travel_icon,
            travel_label,
            address,
            address_label,
            coords,
            save_icon,
            save_label,
            save_btn: save_btn.clone(),
            current: current.clone(),
        });

        let cp = current.clone();
        directions.connect_clicked(move |_| {
            if let Some(p) = cp.borrow().clone() {
                on_directions(p);
            }
        });
        let cp = current.clone();
        share.connect_clicked(move |b| {
            if let Some(ref p) = *cp.borrow() {
                share_place(p, b);
            }
        });
        let cp = current.clone();
        copy.connect_clicked(move |b| {
            if let Some(ref p) = *cp.borrow() {
                b.clipboard().set_text(&format!("{:.6}, {:.6}", p.lat, p.lon));
                b.set_tooltip_text(Some("Copied"));
            }
        });

        let on_saved: Rc<dyn Fn(&str)> = Rc::new(on_saved);
        let weak = Rc::downgrade(&card);
        save_btn.connect_clicked(move |b| {
            let Some(card) = weak.upgrade() else { return };
            let Some(place) = card.current.borrow().clone() else { return };
            let saved = saved_as(&db, &place);
            let has = |label: &str| saved.iter().any(|f| f.label == label);
            let save = |label: &'static str, done: &'static str| {
                let (db, place, card, on_saved) = (db.clone(), place.clone(), Rc::downgrade(&card), on_saved.clone());
                move || {
                    let _ = db.add_favorite(&place.name, label, place.lat, place.lon, &place.formatted_address);
                    if let Some(card) = card.upgrade() {
                        card.show_saved(&db);
                    }
                    on_saved(done);
                }
            };
            let mut entries = vec![
                menu::item("Save to Saved places", save("favorite", "Saved")).icon("maps-star-symbolic").enabled(!has("favorite")),
                menu::item("Set as Home", save("home", "Set as Home")).icon("maps-home-symbolic").enabled(!has("home")),
                menu::item("Set as Work", save("work", "Set as Work")).icon("maps-work-symbolic").enabled(!has("work")),
            ];
            if !saved.is_empty() {
                let (db, card, on_saved) = (db.clone(), Rc::downgrade(&card), on_saved.clone());
                entries.push(menu::separator());
                entries.push(
                    menu::item("Remove from Saved", move || {
                        for f in &saved {
                            let _ = db.remove_favorite(f.id);
                        }
                        if let Some(card) = card.upgrade() {
                            card.show_saved(&db);
                        }
                        on_saved("Removed from saved places");
                    })
                    .danger(),
                );
            }
            menu::popup_below(b, entries);
        });
        card
    }

    pub fn set_place(&self, place: &SearchResult, db: &Database) {
        *self.current.borrow_mut() = Some(place.clone());
        self.title.set_text(&place.name);
        let sub = [kind_label(place.category.as_deref()), area(place)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        self.sub.set_text(&sub);
        self.sub.set_visible(!sub.is_empty());
        match short_address(place) {
            Some(a) => {
                self.address_label.set_text(&a);
                self.address.set_tooltip_text(Some(&place.formatted_address));
                self.address.set_visible(true);
            }
            None => self.address.set_visible(false),
        }
        self.coords.set_text(&coords_text(place.lat, place.lon));
        self.set_travel(None, "");
        self.show_saved(db);
    }

    /// How far away, and how long it takes ("7 min · 1.9 mi"); `None` hides it.
    pub fn set_travel(&self, text: Option<&str>, icon: &str) {
        match text {
            Some(t) => {
                self.travel_label.set_text(t);
                if !icon.is_empty() {
                    self.travel_icon.set_icon_name(Some(icon));
                }
                self.travel.set_visible(true);
            }
            None => self.travel.set_visible(false),
        }
    }

    pub fn current(&self) -> Option<SearchResult> {
        self.current.borrow().clone()
    }

    /// The line under the name, in place of the kind and area.
    pub fn set_note(&self, text: &str) {
        self.sub.set_text(text);
        self.sub.set_visible(!text.is_empty());
    }

    fn show_saved(&self, db: &Database) {
        let Some(place) = self.current() else { return };
        let saved = saved_as(db, &place);
        let has = |label: &str| saved.iter().any(|f| f.label == label);
        let (icon, text) = if has("favorite") {
            ("maps-star-filled-symbolic", "Saved")
        } else if has("home") {
            ("maps-home-symbolic", "Home")
        } else if has("work") {
            ("maps-work-symbolic", "Work")
        } else {
            ("maps-star-symbolic", "Save")
        };
        self.save_icon.set_icon_name(Some(icon));
        self.save_label.set_text(text);
        if saved.is_empty() {
            self.save_btn.remove_css_class("saved");
        } else {
            self.save_btn.add_css_class("saved");
        }
    }
}

fn card_button(icon: &str, text: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("card-button");
    let face = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let img = gtk::Image::from_icon_name(icon);
    img.set_pixel_size(15);
    face.append(&img);
    face.append(&gtk::Label::new(Some(text)));
    b.set_child(Some(&face));
    b
}

/// Home or Work in the sidebar: its icon on its colour, how long it takes to get
/// there, its name.
struct Tile {
    button: gtk::Button,
    time: gtk::Label,
}

fn tile(icon: &str, name: &str, swatch: &str) -> Tile {
    let button = gtk::Button::new();
    button.add_css_class("place-tile");
    button.set_hexpand(true);
    let col = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let disc = gtk::CenterBox::new();
    disc.add_css_class("tile-disc");
    disc.add_css_class(&format!("section-{swatch}"));
    let img = gtk::Image::from_icon_name(icon);
    img.set_pixel_size(14);
    disc.set_center_widget(Some(&img));
    disc.set_size_request(24, 24);
    disc.set_valign(gtk::Align::Center);
    top.append(&disc);
    let time = widgets::label("—", "tile-time");
    time.add_css_class("compact-hide");
    time.set_ellipsize(gtk::pango::EllipsizeMode::End);
    top.append(&time);
    col.append(&top);
    let label = widgets::label(name, "tile-name");
    label.add_css_class("compact-hide");
    col.append(&label);
    button.set_child(Some(&col));
    button.update_property(&[gtk::accessible::Property::Label(name)]);
    Tile { button, time }
}

/// The swatches saved places take in turn.
const SAVED_SWATCHES: [&str; 8] = ["coral", "green", "teal", "pink", "violet", "amber", "red", "blue"];

/// The sidebar's content: Home and Work, Explore nearby, Saved and Recent.
pub struct Library {
    pub sidebar: Rc<sidebar::Sidebar>,
    home: Tile,
    work: Tile,
    saved: gtk::Box,
    recents: gtk::Box,
    db: Database,
    on_select: Rc<dyn Fn(SearchResult)>,
    on_directions: Rc<dyn Fn(SearchResult)>,
    on_toast: Rc<dyn Fn(&str)>,
    active: RefCell<Option<SearchResult>>,
}

impl Library {
    pub fn new(
        db: Database,
        on_select: impl Fn(SearchResult) + 'static,
        on_directions: impl Fn(SearchResult) + 'static,
        on_category: impl Fn(&str) + 'static,
        on_add: impl Fn() + 'static,
        on_toast: impl Fn(&str) + 'static,
    ) -> Rc<Self> {
        let bar = sidebar::Sidebar::new();
        bar.root.add_css_class("maps-sidebar");

        let tiles = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tiles.add_css_class("place-tiles");
        tiles.set_homogeneous(true);
        let home = tile("maps-home-symbolic", "Home", "swatch-blue");
        let work = tile("maps-work-symbolic", "Work", "swatch-amber");
        tiles.append(&home.button);
        tiles.append(&work.button);
        bar.append(&tiles);

        bar.append(&rail_divider(sidebar::ACCENT_SECTION));
        let explore = sidebar::group_label("Explore nearby", true);
        sidebar::set_section(&explore, sidebar::ACCENT_SECTION);
        bar.append(&explore);
        let on_category = Rc::new(on_category);
        for (icon, title, category) in [
            ("maps-food-symbolic", "Food", "food"),
            ("maps-groceries-symbolic", "Groceries", "groceries"),
            ("maps-fuel-symbolic", "Fuel", "fuel"),
            ("maps-ticket-symbolic", "Things to do", "entertainment"),
            ("maps-park-symbolic", "Parks", "parks"),
        ] {
            let item = sidebar::nav_item(icon, title, &format!("{title} near the middle of the map"));
            let cb = on_category.clone();
            item.button.connect_clicked(move |_| cb(category));
            bar.append(&item.button);
        }

        bar.append(&rail_divider("swatch-green"));
        let saved_head = sidebar::group_label_with_add("Saved", false, "Save a place: find it, then choose Save", on_add);
        sidebar::set_section(&saved_head, "swatch-green");
        bar.append(&saved_head);
        let saved = gtk::Box::new(gtk::Orientation::Vertical, 1);
        bar.append(&saved);

        bar.append(&rail_divider("swatch-violet"));
        let recent_head = sidebar::group_label("Recent", false);
        sidebar::set_section(&recent_head, "swatch-violet");
        let clear = gtk::Button::with_label("Clear");
        clear.add_css_class("flat");
        clear.add_css_class("group-clear");
        clear.set_tooltip_text(Some("Clear recent searches"));
        clear.set_valign(gtk::Align::End);
        recent_head.append(&clear);
        bar.append(&recent_head);
        let recents = gtk::Box::new(gtk::Orientation::Vertical, 1);
        bar.append(&recents);

        let lib = Rc::new(Self {
            sidebar: bar,
            home,
            work,
            saved,
            recents,
            db,
            on_select: Rc::new(on_select),
            on_directions: Rc::new(on_directions),
            on_toast: Rc::new(on_toast),
            active: RefCell::new(None),
        });

        for (label, name) in [("home", "Home"), ("work", "Work")] {
            let t = if label == "home" { &lib.home } else { &lib.work };
            let weak = Rc::downgrade(&lib);
            t.button.connect_clicked(move |_| {
                let Some(lib) = weak.upgrade() else { return };
                match lib.db.get_favorite_by_label(label) {
                    Ok(Some(f)) => (lib.on_directions)(favorite_place(&f)),
                    _ => (lib.on_toast)(&format!("{name} isn't set: open a place, then choose Save ▸ Set as {name}")),
                }
            });
        }
        let weak = Rc::downgrade(&lib);
        clear.connect_clicked(move |_| {
            let Some(lib) = weak.upgrade() else { return };
            let _ = lib.db.clear_recent_searches();
            lib.refresh();
            (lib.on_toast)("Cleared recent searches");
        });
        let weak = Rc::downgrade(&lib);
        lib.sidebar.connect_compact(move |compact| {
            if let Some(lib) = weak.upgrade() {
                apply_rail(&lib.sidebar.root, compact);
            }
        });

        lib.refresh();
        lib
    }

    /// The place on the card, marked in Saved.
    pub fn set_active(&self, place: Option<&SearchResult>) {
        *self.active.borrow_mut() = place.cloned();
        self.refresh();
    }

    pub fn has_active(&self) -> bool {
        self.active.borrow().is_some()
    }

    /// Travel time to Home or Work ("12 min"), or `None` when it isn't known.
    pub fn set_time(&self, label: &str, time: Option<&str>) {
        let t = if label == "home" { &self.home } else { &self.work };
        t.time.set_text(time.unwrap_or("—"));
    }

    pub fn refresh(&self) {
        let (home, work) = (self.db.get_favorite_by_label("home").ok().flatten(), self.db.get_favorite_by_label("work").ok().flatten());
        for (t, name, place) in [(&self.home, "Home", &home), (&self.work, "Work", &work)] {
            t.button.set_tooltip_text(Some(&match place {
                Some(p) => format!("Directions to {name}: {}", p.name),
                None => format!("{name} isn't set"),
            }));
            if place.is_none() {
                t.time.set_text("—");
            }
        }

        clear(&self.saved);
        let favorites: Vec<FavoritePlace> =
            self.db.list_favorites().unwrap_or_default().into_iter().filter(|f| f.label == "favorite").collect();
        if favorites.is_empty() {
            self.saved.append(&empty_note("No saved places yet"));
        }
        let active = self.active.borrow().clone();
        for (i, f) in favorites.iter().enumerate() {
            let place = favorite_place(f);
            let row = place_row(None, Some(SAVED_SWATCHES[i % SAVED_SWATCHES.len()]), &f.name);
            if active.as_ref().is_some_and(|a| a.name == f.name || ((a.lat - f.lat).abs() < 1e-5 && (a.lon - f.lon).abs() < 1e-5)) {
                row.add_css_class("active");
            }
            let (sel, p) = (self.on_select.clone(), place.clone());
            row.connect_clicked(move |_| sel(p.clone()));
            let (dir, db, toast, id, p) = (self.on_directions.clone(), self.db.clone(), self.on_toast.clone(), f.id, place);
            let saved = self.saved.clone();
            menu::on_right_click(&row, move |w, x, y| {
                let (dir, p, db, toast, saved, row) = (dir.clone(), p.clone(), db.clone(), toast.clone(), saved.clone(), w.clone());
                menu::popup_at(
                    w,
                    x,
                    y,
                    vec![
                        menu::item("Directions", move || dir(p.clone())).icon("maps-directions-symbolic"),
                        menu::separator(),
                        menu::item("Remove from Saved", move || {
                            let _ = db.remove_favorite(id);
                            saved.remove(&row);
                            toast("Removed from saved places");
                        })
                        .danger(),
                    ],
                );
            });
            self.saved.append(&row);
        }

        clear(&self.recents);
        let recents = self.db.list_recent_searches().unwrap_or_default();
        if recents.is_empty() {
            self.recents.append(&empty_note("No recent searches"));
        }
        for r in recents {
            let row = place_row(Some("maps-history-symbolic"), None, &r.query);
            row.set_tooltip_text(Some(&r.query));
            let place = SearchResult {
                name: r.query.clone(),
                formatted_address: r.formatted_address.clone(),
                lat: r.lat,
                lon: r.lon,
                category: None,
            };
            let sel = self.on_select.clone();
            row.connect_clicked(move |_| sel(place.clone()));
            self.recents.append(&row);
        }
        self.sidebar.refresh();
        apply_rail(&self.sidebar.root, self.sidebar.is_compact());
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn empty_note(text: &str) -> gtk::Label {
    let l = widgets::label(text, "nav-empty");
    l.add_css_class("compact-hide");
    l
}

/// A nav row for a place: a history icon, or a dot in its swatch.
fn place_row(icon: Option<&str>, swatch: Option<&str>, title: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("nav-item");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    if let Some(icon) = icon {
        let image = gtk::Image::from_icon_name(icon);
        image.add_css_class("nav-icon");
        content.append(&image);
    }
    if let Some(swatch) = swatch {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("place-dot");
        dot.add_css_class(&format!("section-swatch-{swatch}"));
        dot.set_valign(gtk::Align::Center);
        content.append(&dot);
    }
    let label = widgets::label(title, "nav-label");
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.add_css_class("compact-hide");
    content.append(&label);
    button.set_child(Some(&content));
    button.set_tooltip_text(Some(title));
    button.update_property(&[gtk::accessible::Property::Label(title)]);
    button
}

/// In the icon rail, a short bar in a section's colour where its heading was.
fn rail_divider(section: &str) -> gtk::Separator {
    let bar = gtk::Separator::new(gtk::Orientation::Horizontal);
    bar.add_css_class("nav-divider");
    bar.add_css_class("compact-show");
    bar.add_css_class(&format!("section-{section}"));
    bar.set_visible(false);
    bar
}

/// What the shared sidebar's compact mode doesn't cover: the rail's bars, and Home
/// and Work stacked as squares around their discs.
fn apply_rail(root: &gtk::Box, compact: bool) {
    fn walk(w: &gtk::Widget, compact: bool) {
        if w.has_css_class("compact-show") {
            w.set_visible(compact);
        }
        if let Some(tiles) = w.downcast_ref::<gtk::Box>().filter(|b| b.has_css_class("place-tiles")) {
            tiles.set_orientation(if compact { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
        }
        if w.has_css_class("place-tile") {
            w.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, compact);
            child = c.next_sibling();
        }
    }
    walk(root.upcast_ref(), compact);
}

/// Nyx Share's Share Sheet for a link to the place.
fn share_place(place: &SearchResult, button: &gtk::Button) {
    let url = place_url(place.lat, place.lon);
    let button = button.clone();
    nyx_core::cmd::background(
        move || {
            use nyx_share_client::{Client, Content, ShareOptions};
            let mut content = Content::new();
            content.url(url);
            let options = ShareOptions { app_name: Some("Maps".into()), ..Default::default() };
            Client::connect().and_then(|c| c.share(&content, &options)).map(|_| ()).map_err(|e| e.to_string())
        },
        move |result: Result<(), String>| {
            if let Err(e) = result {
                button.set_tooltip_text(Some(&format!("Couldn't share: {e}")));
                eprintln!("nyx-maps: couldn't share: {e}");
            }
        },
    );
}

/// A link anyone can open: the place on OpenStreetMap.
pub fn place_url(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=17/{lat:.6}/{lon:.6}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(name: &str, address: &str, category: Option<&str>) -> SearchResult {
        SearchResult { name: name.into(), formatted_address: address.into(), lat: 0.0, lon: 0.0, category: category.map(Into::into) }
    }

    #[test]
    fn addresses_read_short() {
        let p = place(
            "Ryman Auditorium",
            "Ryman Auditorium, 116, Rep. John Lewis Way North, Downtown, Nashville, Tennessee, 37219, United States",
            Some("theatre"),
        );
        assert_eq!(short_address(&p).as_deref(), Some("116 Rep. John Lewis Way North"));
        assert_eq!(area(&p).as_deref(), Some("Downtown"));
        assert_eq!(kind_label(p.category.as_deref()).as_deref(), Some("Theatre"));

        let photon = place("Frist Art Museum", "919 Broadway, Nashville, Tennessee, 37203, United States", Some("museum"));
        assert_eq!(short_address(&photon).as_deref(), Some("919 Broadway"));
        assert_eq!(area(&photon).as_deref(), Some("Nashville"));
        assert_eq!(kind_label(Some("catering.fast_food")).as_deref(), Some("Fast food"));
        assert_eq!(kind_label(Some("favorite")), None);
        assert_eq!(short_address(&place("36.1, -86.7", "Latitude: 36.1, Longitude: -86.7", Some("Coordinate"))), None);
    }

    #[test]
    fn coordinates_use_a_real_minus() {
        assert_eq!(coords_text(41.20341, -72.91472), "41.2034, \u{2212}72.9147");
    }
}
