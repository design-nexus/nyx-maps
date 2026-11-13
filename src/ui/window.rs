use crate::location::{GeoLocation, accuracy_text};
use crate::providers::{MapStyle, RouteResult, SearchResult, TravelMode, calculate_route, distance_m, format_distance, format_duration};
use crate::settings;
use crate::settings_dialog;
use crate::storage::{Database, db};
use crate::ui::directions::RouteCard;
use crate::ui::map::MapView;
use crate::ui::places::{self, Library, PlaceCard};
use crate::ui::search::SearchBar;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use nyx_ui::chrome;
use nyx_ui::widgets;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// The room the route card takes at the map's left, kept clear when a route is fitted.
const ROUTE_CARD_ROOM: f64 = 388.0;

pub struct MainWindow {
    pub window: gtk::ApplicationWindow,
    pub overlay: gtk::Overlay,
    pub map_view: MapView,
    pub search_bar: Rc<SearchBar>,
    library: Rc<Library>,
    place_card: Rc<PlaceCard>,
    route_card: Rc<RouteCard>,
    directions_btn: gtk::Button,
    styles: Vec<(MapStyle, gtk::ToggleButton)>,
    attribution_lbl: gtk::Label,
    toast_lbl: gtk::Label,
    toast_timer: Rc<RefCell<Option<glib::SourceId>>>,
    db: Database,
    /// The place card's travel time: answers to older places are dropped.
    travel_request: Cell<u64>,
    /// Fixes from an older Find my location are dropped.
    locate_request: Cell<u64>,
}

impl MainWindow {
    pub fn build(app: &gtk::Application) -> Rc<Self> {
        Rc::new_cyclic(|me: &Weak<MainWindow>| Self::assemble(app, me.clone()))
    }

    fn assemble(app: &gtk::Application, me: Weak<MainWindow>) -> Self {
        let database = db().expect("Failed to initialize maps database");
        let initial_settings = settings::get();

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Maps")
            .default_width(1120)
            .default_height(800)
            .build();
        window.add_css_class("maps-window");
        window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
        chrome::set_title(&window, "Maps", "");

        let overlay = gtk::Overlay::new();
        // Calls back into the window once it's built.
        let with = {
            let me = me.clone();
            move |f: &dyn Fn(&Rc<MainWindow>)| {
                if let Some(w) = me.upgrade() {
                    f(&w);
                }
            }
        };

        // --- Header: the app and the sidebar's toggle, search, Directions, Settings ---
        let header = chrome::app_header(&window, "mark-location-symbolic", "Maps");
        let sidebar_toggle = widgets::bar_button("sidebar-show-symbolic", "Collapse the sidebar (Ctrl+B)");
        header.start.append(&sidebar_toggle);
        header.end.set_spacing(4);

        let directions_btn = gtk::Button::new();
        directions_btn.add_css_class("new-button");
        directions_btn.set_tooltip_text(Some("Directions (Ctrl+R)"));
        let face = widgets::hbox(6);
        let dir_icon = gtk::Image::from_icon_name("maps-directions-symbolic");
        dir_icon.set_pixel_size(16);
        face.append(&dir_icon);
        face.append(&gtk::Label::new(Some("Directions")));
        let dir_key = widgets::label("Ctrl R", "key-hint");
        face.append(&dir_key);
        directions_btn.set_child(Some(&face));
        directions_btn.set_valign(gtk::Align::Center);
        header.end.append(&directions_btn);
        let (overlay_gear, win_gear, db_gear) = (overlay.clone(), window.clone(), database.clone());
        header.end.append(&chrome::settings_button(move || {
            settings_dialog::open(&overlay_gear, &win_gear, db_gear.clone());
        }));

        // --- The map, with its style, locate and zoom on it ---
        let (start_lat, start_lon) = match (initial_settings.last_current_lat, initial_settings.last_current_lon) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => (initial_settings.last_lat, initial_settings.last_lon),
        };
        let cur_style = MapStyle::from_str(&initial_settings.map_style);
        let map_view = MapView::new(start_lat, start_lon, initial_settings.last_zoom, cur_style);
        if initial_settings.last_current_lat.is_some() {
            map_view.set_user_location(start_lat, start_lon, initial_settings.last_current_accuracy, &you_tooltip(&initial_settings));
        }
        {
            let with = with.clone();
            map_view.connect_you_clicked(move || with(&|w| w.show_me()));
        }

        let map_overlay = gtk::Overlay::new();
        map_overlay.set_hexpand(true);
        map_overlay.set_vexpand(true);
        map_overlay.set_child(Some(&map_view.root));

        let style_seg = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        style_seg.add_css_class("segmented");
        style_seg.add_css_class("map-style");
        style_seg.set_halign(gtk::Align::End);
        style_seg.set_valign(gtk::Align::Start);
        style_seg.set_margin_top(12);
        style_seg.set_margin_end(12);
        let mut styles: Vec<(MapStyle, gtk::ToggleButton)> = Vec::new();
        for (style, label) in [(MapStyle::Standard, "Map"), (MapStyle::Topographic, "Terrain"), (MapStyle::Satellite, "Satellite")] {
            let b = gtk::ToggleButton::with_label(label);
            b.add_css_class("segment");
            b.set_tooltip_text(Some(&format!("Show the {} map", style.display_name().to_lowercase())));
            if let Some((_, first)) = styles.first() {
                b.set_group(Some(first));
            }
            b.set_active(style == cur_style);
            let with = with.clone();
            b.connect_toggled(move |b| {
                if b.is_active() {
                    with(&|w| {
                        if w.map_view.current_style() != style {
                            w.set_map_style(style);
                        }
                    });
                }
            });
            style_seg.append(&b);
            styles.push((style, b));
        }
        map_overlay.add_overlay(&style_seg);

        let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
        controls.add_css_class("map-controls");
        controls.set_halign(gtk::Align::End);
        controls.set_valign(gtk::Align::End);
        controls.set_margin_end(12);
        controls.set_margin_bottom(34);
        let map_button = |icon: &str, tip: &str| {
            let b = widgets::icon_button(icon, tip);
            b.add_css_class("map-control-btn");
            b
        };
        let locate_btn = map_button("maps-locate-symbolic", "Find my location");
        locate_btn.set_halign(gtk::Align::End);
        controls.append(&locate_btn);
        let zoom_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        zoom_box.add_css_class("map-zoom");
        zoom_box.set_halign(gtk::Align::End);
        let zoom_in_btn = map_button("maps-plus-symbolic", "Zoom in (+)");
        let zoom_out_btn = map_button("maps-minus-symbolic", "Zoom out (−)");
        let mv = map_view.clone();
        zoom_in_btn.connect_clicked(move |_| mv.zoom_in());
        let mv = map_view.clone();
        zoom_out_btn.connect_clicked(move |_| mv.zoom_out());
        zoom_box.append(&zoom_in_btn);
        zoom_box.append(&zoom_out_btn);
        controls.append(&zoom_box);
        map_overlay.add_overlay(&controls);

        // The tiles' credit, small in the map's corner (their terms ask for it on the map).
        let attribution_lbl = gtk::Label::new(Some(cur_style.attribution()));
        attribution_lbl.add_css_class("map-attribution");
        attribution_lbl.set_halign(gtk::Align::End);
        attribution_lbl.set_valign(gtk::Align::End);
        attribution_lbl.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        attribution_lbl.set_can_target(false);
        map_overlay.add_overlay(&attribution_lbl);

        let toast_lbl = gtk::Label::new(None);
        toast_lbl.add_css_class("maps-toast");
        toast_lbl.set_halign(gtk::Align::Center);
        toast_lbl.set_valign(gtk::Align::End);
        toast_lbl.set_visible(false);
        toast_lbl.set_can_target(false);
        let toast_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let toast: Rc<dyn Fn(&str)> = {
            let (lbl, timer) = (toast_lbl.clone(), toast_timer.clone());
            Rc::new(move |msg: &str| show_toast_on(&lbl, &timer, msg))
        };

        // --- The cards over the map ---
        let place_card = PlaceCard::new(
            database.clone(),
            {
                let with = with.clone();
                move || with(&|w| w.close_place())
            },
            {
                let with = with.clone();
                move |place| with(&|w| w.directions_to(&place))
            },
            {
                let (with, toast) = (with.clone(), toast.clone());
                move |msg: &str| {
                    toast(msg);
                    with(&|w| {
                        w.library.refresh();
                        w.refresh_tile_times();
                    });
                }
            },
        );
        place_card.root.set_margin_start(14);
        place_card.root.set_margin_bottom(34);
        place_card.root.set_visible(false);
        map_overlay.add_overlay(&place_card.root);

        // Directions look for places near you first, else near the map's centre.
        let mv_near = map_view.clone();
        let near_you: crate::ui::search::Near = Rc::new(move || {
            let s = settings::get();
            match (s.last_current_lat, s.last_current_lon) {
                (Some(lat), Some(lon)) => Some((lat, lon)),
                _ => {
                    let (lat, lon, _) = mv_near.get_viewport_state();
                    Some((lat, lon))
                }
            }
        });
        let route_card = RouteCard::new(
            database.clone(),
            near_you,
            {
                let with = with.clone();
                move |route: &RouteResult| {
                    let coords = route.coordinates.clone();
                    with(&|w| w.map_view.show_route(&coords, &w.route_card.destination_name(), ROUTE_CARD_ROOM));
                }
            },
            {
                let with = with.clone();
                move || with(&|w| w.close_route())
            },
        );
        route_card.root.set_margin_start(14);
        route_card.root.set_margin_top(14);
        route_card.root.set_margin_bottom(14);
        route_card.root.set_visible(false);
        if let (Some(lat), Some(lon)) = (initial_settings.last_current_lat, initial_settings.last_current_lon) {
            route_card.set_origin(lat, lon, "My location");
        }
        map_overlay.add_overlay(&route_card.root);

        // --- Search in the header ---
        let search_bar = Rc::new(SearchBar::new(database.clone(), {
            let with = with.clone();
            move |place| with(&|w| w.show_place(&place))
        }));
        let mv_center = map_view.clone();
        search_bar.set_near(Rc::new(move || {
            let (lat, lon, _) = mv_center.get_viewport_state();
            Some((lat, lon))
        }));
        let search_box = gtk::Overlay::new();
        search_box.set_child(Some(&search_bar.root));
        let hint = widgets::label("Ctrl F", "key-hint");
        hint.set_halign(gtk::Align::End);
        hint.set_valign(gtk::Align::Center);
        hint.set_margin_end(10);
        hint.set_can_target(false);
        search_box.add_overlay(&hint);
        let entry = search_bar.entry.clone();
        entry.connect_changed(move |e| hint.set_visible(e.text().is_empty()));
        search_box.set_margin_start(12);
        search_box.set_margin_end(12);
        header.center.append(&search_box);

        // --- The sidebar: Home and Work, Explore nearby, Saved, Recent ---
        let library = Library::new(
            database.clone(),
            {
                let with = with.clone();
                move |place| with(&|w| w.show_place(&place))
            },
            {
                let with = with.clone();
                move |place| with(&|w| w.directions_to(&place))
            },
            {
                let with = with.clone();
                move |category: &str| {
                    let category = category.to_string();
                    with(&|w| w.explore(&category));
                }
            },
            {
                let (with, toast) = (with.clone(), toast.clone());
                move || {
                    with(&|w| w.search_bar.focus());
                    toast("Find a place, then choose Save on its card");
                }
            },
            {
                let toast = toast.clone();
                move |msg: &str| toast(msg)
            },
        );
        library.sidebar.follow_window(&window);
        follow_width(&window, &header.root, &search_bar.entry);
        library.sidebar.set_collapsed(initial_settings.sidebar_collapsed);

        let frame = nyx_ui::frame::AppFrame::new(&header.root);
        frame.set_sidebar(&library.sidebar.root);
        frame.set_page(&map_overlay);
        frame.set_flush(true);
        overlay.set_child(Some(&frame.root));
        overlay.add_overlay(&toast_lbl);
        window.set_child(Some(&overlay));

        // --- Wiring ---
        let lib = library.clone();
        sidebar_toggle.connect_clicked(move |_| {
            lib.sidebar.toggle();
            let collapsed = lib.sidebar.is_collapsed();
            settings::update(|s| s.sidebar_collapsed = collapsed);
        });
        {
            let with = with.clone();
            directions_btn.connect_clicked(move |_| with(&|w| w.toggle_directions()));
        }
        {
            let with = with.clone();
            locate_btn.connect_clicked(move |_| with(&|w| w.locate_user()));
        }
        let key_ctl = gtk::EventControllerKey::new();
        {
            let with = with.clone();
            key_ctl.connect_key_pressed(move |_, key, _, modifier| {
                let handled = Cell::new(false);
                with(&|w| handled.set(w.on_key(key, modifier)));
                if handled.get() { glib::Propagation::Stop } else { glib::Propagation::Proceed }
            });
        }
        window.add_controller(key_ctl);

        let mv_close = map_view.clone();
        window.connect_close_request(move |_| {
            let (lat, lon, zoom) = mv_close.get_viewport_state();
            settings::update(|s| {
                s.last_lat = lat;
                s.last_lon = lon;
                s.last_zoom = zoom;
            });
            glib::Propagation::Proceed
        });

        {
            let with = with.clone();
            glib::idle_add_local_once(move || {
                with(&|w| {
                    w.follow_vector_tiles();
                    w.refresh_tile_times();
                })
            });
        }

        Self {
            window,
            overlay,
            map_view,
            search_bar,
            library,
            place_card,
            route_card,
            directions_btn,
            styles,
            attribution_lbl,
            toast_lbl,
            toast_timer,
            db: database,
            travel_request: Cell::new(0),
            locate_request: Cell::new(0),
        }
    }

    fn on_key(self: &Rc<Self>, key: gdk::Key, modifier: gdk::ModifierType) -> bool {
        let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
        let lower = key.to_lower();
        if ctrl && (lower == gdk::Key::q || lower == gdk::Key::w) {
            self.window.close();
        } else if ctrl && lower == gdk::Key::f {
            self.search_bar.focus();
        } else if ctrl && lower == gdk::Key::b {
            self.library.sidebar.toggle();
            let collapsed = self.library.sidebar.is_collapsed();
            settings::update(|s| s.sidebar_collapsed = collapsed);
        } else if ctrl && lower == gdk::Key::r {
            self.toggle_directions();
        } else if ctrl && key == gdk::Key::comma {
            self.open_settings();
        } else if key == gdk::Key::F1 {
            show_shortcuts();
        } else if key == gdk::Key::plus || key == gdk::Key::equal {
            self.map_view.zoom_in();
        } else if key == gdk::Key::minus {
            self.map_view.zoom_out();
        } else if key == gdk::Key::Escape && self.route_card.root.is_visible() {
            self.close_route();
        } else if key == gdk::Key::Escape && self.place_card.root.is_visible() {
            self.close_place();
        } else {
            return false;
        }
        true
    }

    pub fn open_settings(&self) {
        settings_dialog::open(&self.overlay, &self.window, self.db.clone());
    }

    /// Picks up OpenFreeMap's current tile build (the first run has none yet, so the
    /// Standard map starts as raster), and draws the Standard map again when the theme
    /// changes, as its colours come from it.
    fn follow_vector_tiles(self: &Rc<Self>) {
        let (sender, receiver) = async_channel::bounded(1);
        crate::runtime::spawn(async move {
            let _ = sender.send(crate::providers::refresh_vector_tiles().await).await;
        });
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            if let Ok(Some(_)) = receiver.recv().await
                && let Some(me) = me.upgrade()
                && me.map_view.current_style() == MapStyle::Standard
            {
                me.set_map_style(MapStyle::Standard);
            }
        });
        let me = Rc::downgrade(self);
        let last = std::cell::RefCell::new(nyx_theme::palette());
        nyx_theme::subscribe(&self.window, move || {
            let Some(me) = me.upgrade() else { return };
            let now = nyx_theme::palette();
            if *last.borrow() != now && me.map_view.current_style() == MapStyle::Standard {
                me.set_map_style(MapStyle::Standard);
            }
            *last.borrow_mut() = now;
        });
    }

    /// A place on its card, with its pin on the map.
    pub fn show_place(self: &Rc<Self>, place: &SearchResult) {
        if self.route_card.root.is_visible() {
            self.hide_route();
        }
        self.map_view.center_on(place.lat, place.lon, Some(15.0));
        self.map_view.set_selected_marker(place.lat, place.lon, &place.name);
        self.place_card.set_place(place, &self.db);
        self.place_card.root.set_visible(true);
        self.library.set_active(Some(place));
        self.fetch_travel(place);
    }

    pub fn close_place(&self) {
        self.place_card.root.set_visible(false);
        self.map_view.clear_selected_marker();
        self.library.set_active(None);
    }

    /// How far the place is from you, then how long the trip takes.
    fn fetch_travel(self: &Rc<Self>, place: &SearchResult) {
        let id = self.travel_request.get() + 1;
        self.travel_request.set(id);
        let s = settings::get();
        let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) else {
            self.place_card.set_travel(None, "");
            return;
        };
        let mode = TravelMode::from_str(&s.travel_mode);
        let icon = mode_icon(mode);
        let straight = format_distance(distance_m((lat, lon), (place.lat, place.lon)), &s.units);
        self.place_card.set_travel(Some(&format!("{straight} away")), icon);
        let (sender, receiver) = async_channel::bounded(1);
        let (to, key) = ((place.lat, place.lon), s.geoapify_api_key.clone());
        crate::runtime::spawn(async move {
            let _ = sender.send(calculate_route((lat, lon), to, mode, &key).await).await;
        });
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            if let Ok(Ok(route)) = receiver.recv().await
                && let Some(me) = me.upgrade()
                && me.travel_request.get() == id
            {
                let units = settings::get().units;
                let text = format!("{} · {}", format_duration(route.duration_seconds), format_distance(route.distance_meters, &units));
                me.place_card.set_travel(Some(&text), icon);
            }
        });
    }

    /// How long it takes to get Home and to Work from where you are.
    fn refresh_tile_times(self: &Rc<Self>) {
        let s = settings::get();
        let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) else { return };
        let mode = TravelMode::from_str(&s.travel_mode);
        for label in ["home", "work"] {
            let Ok(Some(place)) = self.db.get_favorite_by_label(label) else { continue };
            let (sender, receiver) = async_channel::bounded(1);
            let key = s.geoapify_api_key.clone();
            crate::runtime::spawn(async move {
                let _ = sender.send(calculate_route((lat, lon), (place.lat, place.lon), mode, &key).await).await;
            });
            let me = Rc::downgrade(self);
            glib::spawn_future_local(async move {
                if let Ok(res) = receiver.recv().await
                    && let Some(me) = me.upgrade()
                {
                    me.library.set_time(label, res.ok().map(|r| format_duration(r.duration_seconds)).as_deref());
                }
            });
        }
    }

    /// The route card, from you to `place`.
    fn directions_to(self: &Rc<Self>, place: &SearchResult) {
        self.route_card.set_destination(place.lat, place.lon, &place.name);
        if self.route_card.origin_coords().is_none() {
            let s = settings::get();
            if let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) {
                self.route_card.set_origin(lat, lon, "My location");
            }
        }
        self.open_route_card();
        self.route_card.calculate();
    }

    fn open_route_card(&self) {
        self.place_card.root.set_visible(false);
        self.route_card.root.set_visible(true);
        self.directions_btn.add_css_class("open");
    }

    /// Directions in the header: to the place on the card, or a new route.
    fn toggle_directions(self: &Rc<Self>) {
        if self.route_card.root.is_visible() {
            self.close_route();
            return;
        }
        match self.place_card.current().filter(|_| self.place_card.root.is_visible()) {
            Some(place) => self.directions_to(&place),
            None => {
                self.open_route_card();
                if !self.route_card.has_destination() {
                    self.route_card.focus_destination();
                }
            }
        }
    }

    fn hide_route(&self) {
        self.route_card.root.set_visible(false);
        self.map_view.clear_route();
        self.directions_btn.remove_css_class("open");
    }

    /// Back from directions: to the place's card if one was open.
    fn close_route(&self) {
        self.hide_route();
        if let Some(place) = self.place_card.current()
            && self.library.has_active()
        {
            self.map_view.set_selected_marker(place.lat, place.lon, &place.name);
            self.place_card.root.set_visible(true);
        }
    }

    /// Explore nearby: places of a kind around the middle of the map, in the search list.
    fn explore(self: &Rc<Self>, category: &str) {
        let (lat, lon, _) = self.map_view.get_viewport_state();
        let label = match category {
            "food" => "Food nearby",
            "groceries" => "Groceries nearby",
            "fuel" => "Fuel nearby",
            "entertainment" => "Things to do nearby",
            "parks" => "Parks nearby",
            _ => "Places nearby",
        };
        let (cat, key) = (category.to_string(), settings::get().geoapify_api_key);
        let (sender, receiver) = async_channel::bounded(1);
        crate::runtime::spawn(async move {
            let _ = sender.send(crate::providers::search_nearby(&cat, lat, lon, 8.0, &key).await).await;
        });
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let (Ok(res), Some(me)) = (receiver.recv().await, me.upgrade()) else { return };
            match res {
                Ok(places) if !places.is_empty() => me.search_bar.show_results(label, places),
                Ok(_) => me.show_toast(&format!("No {} found here", label.trim_end_matches(" nearby").to_lowercase())),
                Err(e) => me.show_toast(&format!("Couldn't look nearby: {e}")),
            }
        });
    }

    pub fn set_map_style(&self, style: MapStyle) {
        let prefs = settings::get();
        self.map_view.set_style(style, &prefs.custom_satellite_url, &prefs.custom_topo_url);
        self.attribution_lbl.set_text(style.attribution());
        for (s, b) in &self.styles {
            if *s == style && !b.is_active() {
                b.set_active(true);
            }
        }
        settings::update(|s| s.map_style = style.as_str().to_string());
    }

    /// Find my location: the map follows each better fix as it comes.
    pub fn locate_user(self: &Rc<Self>) {
        self.show_toast("Locating…");
        let fixes = self.locate();
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Some(res) = fixes.next().await {
                let Some(me) = me.upgrade() else { return };
                match res {
                    Ok(loc) => {
                        me.remember_location(&loc);
                        me.map_view.show_fix(loc.latitude, loc.longitude, loc.accuracy);
                        let units = settings::get().units;
                        me.show_toast(&format!("Located: {}", accuracy_text(loc.accuracy, loc.from_ip, &units).to_lowercase()));
                    }
                    Err(e) => me.show_toast(&format!("Location unavailable: {e}")),
                }
            }
        });
    }

    /// Fixes from the location service, best last; a newer request ends this one's.
    fn locate(self: &Rc<Self>) -> Fixes {
        let id = self.locate_request.get() + 1;
        self.locate_request.set(id);
        let (sender, receiver) = async_channel::unbounded();
        if let Some(fix) = crate::test_hooks::fix() {
            let _ = sender.try_send(Ok(fix));
        } else {
            crate::runtime::spawn(crate::location::locate(sender, crate::location::PATIENCE));
        }
        Fixes { receiver, id, me: Rc::downgrade(self) }
    }

    fn remember_location(self: &Rc<Self>, loc: &GeoLocation) {
        let name = loc.label.clone().unwrap_or_else(|| "Current location".to_string());
        settings::update(|s| {
            s.last_current_lat = Some(loc.latitude);
            s.last_current_lon = Some(loc.longitude);
            s.last_current_label = Some(name);
            s.last_current_accuracy = Some(loc.accuracy);
            s.last_current_from_ip = loc.from_ip;
            s.last_lat = loc.latitude;
            s.last_lon = loc.longitude;
        });
        self.map_view.set_user_location(loc.latitude, loc.longitude, Some(loc.accuracy), &you_tooltip(&settings::get()));
        self.route_card.set_origin(loc.latitude, loc.longitude, "My location");
        self.refresh_tile_times();
    }

    /// Where you are on the card, with how close the fix is.
    pub fn show_me(self: &Rc<Self>) {
        let s = settings::get();
        let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) else { return };
        if self.route_card.root.is_visible() {
            self.hide_route();
        }
        let me = SearchResult { name: "My location".to_string(), formatted_address: String::new(), lat, lon, category: None };
        self.map_view.clear_selected_marker();
        self.place_card.set_place(&me, &self.db);
        self.place_card.set_note(&fix_text(&s));
        self.place_card.root.set_visible(true);
        self.library.set_active(None);
    }

    pub fn check_location_on_startup(self: &Rc<Self>) {
        let fixes = self.locate();
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let mut moved = false;
            while let Some(res) = fixes.next().await {
                let (Ok(loc), Some(me)) = (res, me.upgrade()) else { continue };
                let s = settings::get();
                let changed = match (s.last_current_lat, s.last_current_lon) {
                    (Some(olat), Some(olon)) => distance_m((olat, olon), (loc.latitude, loc.longitude)) > loc.accuracy.max(50.0),
                    _ => true,
                };
                me.remember_location(&loc);
                if changed && !moved {
                    moved = true;
                    me.map_view.show_fix(loc.latitude, loc.longitude, loc.accuracy);
                    me.show_toast(&format!("Location updated: {}", accuracy_text(loc.accuracy, loc.from_ip, &s.units).to_lowercase()));
                }
            }
        });
    }

    pub fn show_toast(&self, message: &str) {
        show_toast_on(&self.toast_lbl, &self.toast_timer, message);
    }

    /// A saved place or a recent search at `at` (within a few metres).
    fn known_place(&self, at: (f64, f64)) -> Option<SearchResult> {
        let near = |lat: f64, lon: f64| distance_m(at, (lat, lon)) < 30.0;
        let saved = self.db.list_favorites().unwrap_or_default().into_iter().find(|f| near(f.lat, f.lon)).map(|f| places::favorite_place(&f));
        saved.or_else(|| {
            self.db.list_recent_searches().unwrap_or_default().into_iter().find(|r| near(r.lat, r.lon)).map(|r| SearchResult {
                name: r.query,
                formatted_address: r.formatted_address,
                lat: r.lat,
                lon: r.lon,
                category: None,
            })
        })
    }

    pub fn open_location(self: &Rc<Self>, lat: f64, lon: f64, label: &str) {
        let place = self.known_place((lat, lon)).unwrap_or_else(|| SearchResult {
            name: label.to_string(),
            formatted_address: String::new(),
            lat,
            lon,
            category: None,
        });
        self.show_place(&place);
    }

    pub fn open_route(self: &Rc<Self>, start: (f64, f64), end: (f64, f64), mode: &str) {
        let s = settings::get();
        let name = |at: (f64, f64)| {
            let you = s.last_current_lat.zip(s.last_current_lon).is_some_and(|here| distance_m(here, at) < 75.0);
            if you {
                "My location".to_string()
            } else {
                self.known_place(at).map(|p| p.name).unwrap_or_else(|| places::coords_text(at.0, at.1))
            }
        };
        self.route_card.set_origin(start.0, start.1, &name(start));
        self.route_card.set_destination(end.0, end.1, &name(end));
        self.route_card.set_mode(TravelMode::from_str(mode));
        self.open_route_card();
        self.route_card.calculate();
    }

    pub fn search_query(&self, query: &str) {
        self.search_bar.set_text(query);
        self.search_bar.search_now();
    }
}

/// One location request's fixes; they stop when a newer request starts or the window goes.
struct Fixes {
    receiver: async_channel::Receiver<anyhow::Result<GeoLocation>>,
    id: u64,
    me: Weak<MainWindow>,
}

impl Fixes {
    async fn next(&self) -> Option<anyhow::Result<GeoLocation>> {
        let fix = self.receiver.recv().await.ok()?;
        let me = self.me.upgrade()?;
        (me.locate_request.get() == self.id).then_some(fix)
    }
}

/// How close your last fix is, in words.
fn fix_text(s: &settings::Settings) -> String {
    match s.last_current_accuracy {
        Some(accuracy) => accuracy_text(accuracy, s.last_current_from_ip, &s.units),
        None => "Last known location".to_string(),
    }
}

fn you_tooltip(s: &settings::Settings) -> String {
    format!("You are here · {}", fix_text(s).to_lowercase())
}

/// Below the sidebar's narrow width the header drops the app's name and the search
/// gets shorter, so Directions and Settings keep their room.
fn follow_width(window: &gtk::ApplicationWindow, header: &impl IsA<gtk::Widget>, search: &gtk::Entry) {
    fn find(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
        if w.has_css_class(class) {
            return Some(w.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(found) = find(&c, class) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }
    let title = find(header.upcast_ref(), "app-title");
    let search = search.clone();
    let last = Cell::new(None::<bool>);
    window.add_tick_callback(move |w, _| {
        let narrow = w.width() > 0 && w.width() < nyx_ui::sidebar::NARROW_WIDTH;
        if last.get() != Some(narrow) {
            last.set(Some(narrow));
            if let Some(t) = &title {
                t.set_visible(!narrow);
            }
            search.set_max_width_chars(if narrow { 22 } else { 34 });
        }
        glib::ControlFlow::Continue
    });
}

fn mode_icon(mode: TravelMode) -> &'static str {
    match mode {
        TravelMode::Drive => "maps-car-symbolic",
        TravelMode::Walk => "maps-walk-symbolic",
        TravelMode::Bicycle => "maps-bike-symbolic",
    }
}

fn show_toast_on(lbl: &gtk::Label, timer: &Rc<RefCell<Option<glib::SourceId>>>, message: &str) {
    lbl.set_text(message);
    lbl.set_visible(true);
    if let Some(src) = timer.borrow_mut().take() {
        src.remove();
    }
    let (l, t) = (lbl.clone(), timer.clone());
    let id = glib::timeout_add_local_once(Duration::from_millis(3500), move || {
        l.set_visible(false);
        *t.borrow_mut() = None;
    });
    *timer.borrow_mut() = Some(id);
}

fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("group-list");
    for (keys, desc) in settings_dialog::SHORTCUTS {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("settings-option");
        let desc_lbl = gtk::Label::new(Some(desc));
        desc_lbl.set_hexpand(true);
        desc_lbl.set_xalign(0.0);
        row.append(&desc_lbl);
        row.append(&widgets::key_caps(keys));
        list.append(&row);
    }
    card.append(&list);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let weak = dialog.downgrade();
    close.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    card.append(&close);
    dialog.present();
}
