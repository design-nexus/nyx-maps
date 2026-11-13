use crate::settings;
use crate::storage::Database;
use crate::ui::widgets::{self, Page};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};

const GROUPS: &[(&str, &str, &str)] = &[
    ("Providers", "preferences-system-network-symbolic", "Geoapify API key, tile servers"),
    ("Navigation", "emblem-favorite-symbolic", "Units, travel modes"),
    ("Places", "user-home-symbolic", "Home, work, history"),
];

/// Every shortcut, for the card's Keyboard group.
pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl", "F"], "Focus search bar"),
    (&["Ctrl", "B"], "Collapse the sidebar"),
    (&["Ctrl", "R"], "Open directions"),
    (&["Ctrl", ","], "Open settings"),
    (&["+"], "Zoom in"),
    (&["-"], "Zoom out"),
    (&["F1"], "Show keyboard shortcuts"),
    (&["Esc"], "Close the card or the search results"),
    (&["Ctrl", "Q"], "Quit application"),
];

const SPEC: Spec = Spec {
    groups: GROUPS,
    search_text: None,
    standard: Some(Standard {
        app: "maps",
        about: About {
            name: "Maps",
            icon: "io.github.design_nexus.nyx.Maps",
            version: env!("CARGO_PKG_VERSION"),
            description: "Maps, places and directions.",
            licence: "MIT",
            website: "https://github.com/design-nexus/nyx-maps",
            issues: "https://github.com/design-nexus/nyx-maps/issues",
            credits: &[("OpenStreetMap", "map data"), ("Nominatim and Geoapify", "search"), ("OSRM", "directions")],
        },
        shortcuts: SHORTCUTS,
        reduce_motion: || settings::get().reduce_motion,
        set_reduce_motion: |on| settings::update(|s| s.reduce_motion = on),
    }),
};

pub fn open(overlay: &gtk::Overlay, win: &gtk::ApplicationWindow, db: Database) {
    let db_c = db.clone();
    settings_card::open(overlay, win, SPEC, move || {
        let p = widgets::page("settings");
        build_content(&p, &db_c);
        (p.root.upcast(), p.body)
    });
}

fn build_content(p: &Page, db: &Database) {
    providers_group(p);
    navigation_group(p);
    places_group(p, db);
}

fn providers_group(p: &Page) {
    let group = p.group("Providers");
    let current = settings::get();

    // Geoapify API key entry
    let key_entry = gtk::Entry::new();
    key_entry.set_text(&current.geoapify_api_key);
    key_entry.set_placeholder_text(Some("Enter 32-character API key"));
    key_entry.set_width_chars(28);
    key_entry.add_css_class("maps-search-entry");

    key_entry.connect_changed(|e| {
        let text = e.text().trim().to_string();
        settings::update(|s| s.geoapify_api_key = text);
    });

    let row = widgets::row(
        "Geoapify API key",
        "Used for location search and routing (3,000 free credits/day).",
        Some(key_entry.upcast_ref()),
    );
    group.add(&row);

    // Custom Satellite Tile URL
    let sat_entry = gtk::Entry::new();
    sat_entry.set_text(&current.custom_satellite_url);
    sat_entry.set_placeholder_text(Some("https://.../{z}/{x}/{y} (optional)"));
    sat_entry.set_width_chars(28);
    sat_entry.add_css_class("maps-search-entry");

    sat_entry.connect_changed(|e| {
        let text = e.text().trim().to_string();
        settings::update(|s| s.custom_satellite_url = text);
    });

    let row = widgets::row(
        "Custom satellite tiles",
        "Tile URL template. Leave blank to use default Esri World Imagery.",
        Some(sat_entry.upcast_ref()),
    );
    group.add(&row);

    // Custom Topo Tile URL
    let topo_entry = gtk::Entry::new();
    topo_entry.set_text(&current.custom_topo_url);
    topo_entry.set_placeholder_text(Some("https://.../{z}/{x}/{y}.png (optional)"));
    topo_entry.set_width_chars(28);
    topo_entry.add_css_class("maps-search-entry");

    topo_entry.connect_changed(|e| {
        let text = e.text().trim().to_string();
        settings::update(|s| s.custom_topo_url = text);
    });

    let row = widgets::row(
        "Custom topographic tiles",
        "Tile URL template. Leave blank to use default OpenTopoMap.",
        Some(topo_entry.upcast_ref()),
    );
    group.add(&row);
}

fn navigation_group(p: &Page) {
    let group = p.group("Navigation");
    let current = settings::get();

    let (row, _) = widgets::choice_row("Default travel mode", "Mode used when Maps next opens.",
        vec![("drive".into(), "Drive".into()), ("walk".into(), "Walk".into()), ("bicycle".into(), "Cycle".into())],
        &current.travel_mode, |id| settings::update(|s| s.travel_mode = id));
    group.add(&row);
    let (row, _) = widgets::switch_row(
        "Guess my location from my IP address",
        "When the location service can't answer. Your IP address goes to ipwho.is or ipapi.co.",
        current.ip_location,
        |on| settings::update(|s| s.ip_location = on),
    );
    group.add(&row);
    let (row, _) = widgets::choice_row("Map style", "Style used when Maps next opens. You can also change it on the map.",
        vec![("standard".into(), "Map".into()), ("topographic".into(), "Terrain".into()), ("satellite".into(), "Satellite".into())],
        &current.map_style, |id| settings::update(|s| s.map_style = id));
    group.add(&row);
    let unit_options = vec![
        ("imperial".into(), "Imperial (miles, feet)".into()),
        ("metric".into(), "Metric (kilometers, meters)".into()),
    ];

    let (row, _) = widgets::choice_row(
        "Measurement units",
        "Unit system used for distances and route instructions.",
        unit_options,
        &current.units,
        |val| {
            settings::update(|s| s.units = val);
        },
    );
    group.add(&row);
}

fn places_group(p: &Page, db: &Database) {
    let group = p.group("Places");

    saved_place_row(&group, db, "home", "Home");
    saved_place_row(&group, db, "work", "Work");
    let db_c = db.clone();
    let (row, _) = widgets::button_row(
        "Clear recent searches",
        "Remove all past search history from the local database.",
        "Clear",
        move |btn| {
            let _ = db_c.clear_recent_searches();
            btn.set_sensitive(false);
            btn.set_label("Cleared");
        },
    );
    group.add(&row);
}


fn saved_place_row(group: &widgets::Group, db: &Database, label: &'static str, title: &str) {
    let database = db.clone();
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.add_css_class("dim");
    let message = status.clone();
    // Addresses near you (or where the map was) come first.
    let near = std::rc::Rc::new(|| {
        let s = settings::get();
        Some(s.last_current_lat.zip(s.last_current_lon).unwrap_or((s.last_lat, s.last_lon)))
    });
    let picker = crate::ui::search::SearchBar::field(db.clone(), "Search an address", near, move |place| {
        match database.add_favorite(&place.name, label, place.lat, place.lon, &place.formatted_address) {
            Ok(_) => message.set_text("Saved"),
            Err(_) => message.set_text("Could not save this place"),
        }
    });
    if let Ok(Some(place)) = db.get_favorite_by_label(label) {
        picker.entry.set_text(&place.address);
    }
    group.add(&widgets::row(title, "Choose a search result to save this address.", Some(picker.root.upcast_ref())));
    group.add(&status);
}
