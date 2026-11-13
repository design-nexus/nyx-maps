use crate::providers::{RouteResult, TravelMode, Turn, calculate_route, format_distance, format_duration};
use crate::settings;
use crate::storage::Database;
use crate::ui::search::{Near, SearchBar};
use gtk::glib;
use gtk::prelude::*;
use nyx_ui::widgets;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Directions as a card over the map: how you travel, from where to where (with
/// the swap between them), how long it takes, and every turn.
pub struct RouteCard {
    pub root: gtk::Box,
    origin: Rc<SearchBar>,
    dest: Rc<SearchBar>,
    modes: Vec<(TravelMode, gtk::ToggleButton)>,
    mode: Rc<Cell<TravelMode>>,
    status: gtk::Label,
    summary: gtk::Box,
    time: gtk::Label,
    detail: gtk::Label,
    steps: gtk::Box,
    origin_at: Rc<RefCell<Option<(f64, f64)>>>,
    dest_at: Rc<RefCell<Option<(f64, f64)>>>,
    on_route: Box<dyn Fn(&RouteResult)>,
    db: Database,
    /// Answers to older requests are dropped.
    request: Rc<Cell<u64>>,
    me: Rc<RefCell<Weak<RouteCard>>>,
}

impl RouteCard {
    /// `near` says where searches should look first (you, or the map's centre).
    pub fn new(
        db: Database,
        near: Near,
        on_route: impl Fn(&RouteResult) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("map-card");
        root.add_css_class("route-card");
        root.set_halign(gtk::Align::Start);
        root.set_valign(gtk::Align::Start);
        root.set_size_request(360, -1);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.add_css_class("route-head");
        let title = widgets::label("Directions", "route-title");
        title.set_hexpand(true);
        head.append(&title);
        let close = widgets::icon_button("maps-close-symbolic", "Close (Esc)");
        close.add_css_class("card-close");
        close.connect_clicked(move |_| on_close());
        head.append(&close);
        root.append(&head);

        // Drive, Walk, Cycle.
        let seg = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        seg.add_css_class("segmented");
        seg.add_css_class("route-modes");
        seg.set_homogeneous(true);
        let initial = TravelMode::from_str(&settings::get().travel_mode);
        let mut modes: Vec<(TravelMode, gtk::ToggleButton)> = Vec::new();
        for (mode, icon) in [
            (TravelMode::Drive, "maps-car-symbolic"),
            (TravelMode::Walk, "maps-walk-symbolic"),
            (TravelMode::Bicycle, "maps-bike-symbolic"),
        ] {
            let b = gtk::ToggleButton::new();
            b.add_css_class("segment");
            let face = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            face.set_halign(gtk::Align::Center);
            let img = gtk::Image::from_icon_name(icon);
            img.set_pixel_size(15);
            face.append(&img);
            face.append(&gtk::Label::new(Some(mode.display_name())));
            b.set_child(Some(&face));
            if let Some((_, first)) = modes.first() {
                b.set_group(Some(first));
            }
            b.set_active(mode == initial);
            seg.append(&b);
            modes.push((mode, b));
        }
        root.append(&seg);

        // From and to, with a rail between their marks and the swap beside them.
        let ends = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        ends.add_css_class("route-ends");
        let rail = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rail.add_css_class("route-rail");
        let from_dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        from_dot.add_css_class("rail-from");
        from_dot.set_halign(gtk::Align::Center);
        rail.append(&from_dot);
        let line = gtk::Box::new(gtk::Orientation::Vertical, 0);
        line.add_css_class("rail-line");
        line.set_vexpand(true);
        line.set_halign(gtk::Align::Center);
        rail.append(&line);
        let to_pin = gtk::Image::from_icon_name("maps-pin-filled-symbolic");
        to_pin.set_pixel_size(15);
        to_pin.add_css_class("rail-to");
        rail.append(&to_pin);
        ends.append(&rail);

        let fields = gtk::Box::new(gtk::Orientation::Vertical, 6);
        fields.set_hexpand(true);
        let origin_at: Rc<RefCell<Option<(f64, f64)>>> = Rc::new(RefCell::new(None));
        let dest_at: Rc<RefCell<Option<(f64, f64)>>> = Rc::new(RefCell::new(None));
        let weak_cell: Rc<RefCell<Weak<RouteCard>>> = Rc::new(RefCell::new(Weak::new()));
        let (oa, wc) = (origin_at.clone(), weak_cell.clone());
        let origin = Rc::new(SearchBar::field(db.clone(), "Start (My location)", near.clone(), move |p| {
            *oa.borrow_mut() = Some((p.lat, p.lon));
            if let Some(card) = wc.borrow().upgrade() {
                card.calculate();
            }
        }));
        fields.append(&origin.root);
        let (da, wc) = (dest_at.clone(), weak_cell.clone());
        let dest = Rc::new(SearchBar::field(db.clone(), "Choose a destination", near, move |p| {
            *da.borrow_mut() = Some((p.lat, p.lon));
            if let Some(card) = wc.borrow().upgrade() {
                card.calculate();
            }
        }));
        fields.append(&dest.root);
        ends.append(&fields);
        let swap = widgets::icon_button("maps-swap-symbolic", "Swap start and destination");
        swap.add_css_class("swap-button");
        ends.append(&swap);
        root.append(&ends);

        let status = widgets::label("", "route-status");
        status.set_wrap(true);
        status.set_visible(false);
        root.append(&status);

        let summary = gtk::Box::new(gtk::Orientation::Vertical, 1);
        summary.add_css_class("route-summary");
        let time = widgets::label("", "route-time");
        summary.append(&time);
        let detail = widgets::label("", "route-detail");
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        summary.append(&detail);
        summary.set_visible(false);
        root.append(&summary);

        let steps = gtk::Box::new(gtk::Orientation::Vertical, 0);
        steps.add_css_class("route-steps");
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .vexpand(true)
            .child(&steps)
            .build();
        root.append(&scroller);

        let card = Rc::new(Self {
            root,
            origin,
            dest,
            modes,
            mode: Rc::new(Cell::new(initial)),
            status,
            summary,
            time,
            detail,
            steps,
            origin_at,
            dest_at,
            on_route: Box::new(on_route),
            db,
            request: Rc::new(Cell::new(0)),
            me: weak_cell.clone(),
        });
        *weak_cell.borrow_mut() = Rc::downgrade(&card);

        for (mode, b) in &card.modes {
            let (weak, mode) = (Rc::downgrade(&card), *mode);
            b.connect_toggled(move |b| {
                if let Some(card) = weak.upgrade()
                    && b.is_active()
                    && card.mode.get() != mode
                {
                    card.mode.set(mode);
                    card.calculate();
                }
            });
        }
        let weak = Rc::downgrade(&card);
        swap.connect_clicked(move |_| {
            let Some(card) = weak.upgrade() else { return };
            let (o_text, d_text) = (card.origin.text(), card.dest.text());
            let (o_pick, d_pick) = (card.origin.has_pick(), card.dest.has_pick());
            card.origin.set_text(&d_text);
            card.dest.set_text(&o_text);
            card.origin.set_picked(d_pick);
            card.dest.set_picked(o_pick);
            let (o, d) = (*card.origin_at.borrow(), *card.dest_at.borrow());
            *card.origin_at.borrow_mut() = d;
            *card.dest_at.borrow_mut() = o;
            card.calculate();
        });
        // Editing a field forgets the place it held.
        for (bar, at) in [(card.origin.clone(), card.origin_at.clone()), (card.dest.clone(), card.dest_at.clone())] {
            let b = bar.clone();
            bar.entry.connect_changed(move |_| {
                if !b.has_pick() {
                    *at.borrow_mut() = None;
                }
            });
        }
        card
    }

    pub fn set_origin(&self, lat: f64, lon: f64, text: &str) {
        self.origin.set_text(text);
        self.origin.set_picked(true);
        *self.origin_at.borrow_mut() = Some((lat, lon));
    }

    pub fn set_destination(&self, lat: f64, lon: f64, text: &str) {
        self.dest.set_text(text);
        self.dest.set_picked(true);
        *self.dest_at.borrow_mut() = Some((lat, lon));
    }

    pub fn origin_coords(&self) -> Option<(f64, f64)> {
        *self.origin_at.borrow()
    }

    pub fn destination_name(&self) -> String {
        self.dest.text()
    }

    pub fn has_destination(&self) -> bool {
        !self.dest.text().is_empty()
    }

    pub fn focus_destination(&self) {
        self.dest.focus();
    }

    pub fn set_mode(&self, mode: TravelMode) {
        self.mode.set(mode);
        for (m, b) in &self.modes {
            if *m == mode {
                b.set_active(true);
            }
        }
    }

    fn say(&self, text: &str) {
        self.status.set_text(text);
        self.status.set_visible(!text.is_empty());
    }

    /// Find the route between the two ends, once both are there.
    pub fn calculate(&self) {
        let mut o_str = self.origin.text();
        let d_str = self.dest.text();
        if o_str.is_empty() {
            let s = settings::get();
            if let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) {
                self.set_origin(lat, lon, "My location");
                o_str = "My location".into();
            }
        }
        if o_str.is_empty() || d_str.is_empty() {
            self.say(if d_str.is_empty() { "Choose where you're going." } else { "Choose where you're starting from." });
            return;
        }
        // A name that isn't a picked place: show the places it could be, rather than
        // guessing the first one.
        for (text, coords, bar, what) in
            [(&o_str, *self.origin_at.borrow(), &self.origin, "start"), (&d_str, *self.dest_at.borrow(), &self.dest, "destination")]
        {
            if coords.is_none() && !is_known_point(text) {
                bar.search_now();
                self.say(&format!("Pick the {what} from the list."));
                return;
            }
        }

        self.say("Finding the route…");
        let id = self.request.get() + 1;
        self.request.set(id);
        let (sender, receiver) = async_channel::bounded(1);
        let mode = self.mode.get();
        let api_key = settings::get().geoapify_api_key;
        let (o_at, d_at) = (*self.origin_at.borrow(), *self.dest_at.borrow());
        let db = self.db.clone();
        crate::runtime::spawn(async move {
            let origin = match o_at {
                Some(c) => Some(c),
                None => resolve_point(&o_str, &api_key, &db).await,
            };
            let dest = match d_at {
                Some(c) => Some(c),
                None => resolve_point(&d_str, &api_key, &db).await,
            };
            let res = match (origin, dest) {
                (Some(start), Some(end)) => calculate_route(start, end, mode, &api_key).await,
                _ => Err(anyhow::anyhow!("Couldn't find those places.")),
            };
            let _ = sender.send(res).await;
        });
        let request = self.request.clone();
        let this = self.me.borrow().clone();
        glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else { return };
            let Some(card) = this.upgrade() else { return };
            if request.get() != id {
                return;
            }
            match result {
                Ok(route) => {
                    card.display_route(&route);
                    (card.on_route)(&route);
                }
                Err(e) => {
                    card.say(&format!("No route: {e}"));
                    card.summary.set_visible(false);
                    clear(&card.steps);
                }
            }
        });
    }

    pub fn display_route(&self, route: &RouteResult) {
        self.say("");
        let units = settings::get().units;
        self.time.set_text(&format_duration(route.duration_seconds));
        let distance = format_distance(route.distance_meters, &units);
        self.detail.set_text(&match route.main_road() {
            Some(road) => format!("{distance} · via {road}"),
            None => distance,
        });
        self.summary.set_visible(true);

        clear(&self.steps);
        let dest = self.dest.text();
        for step in &route.steps {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("route-step");
            if step.turn == Turn::Arrive {
                row.add_css_class("arrive");
            }
            let img = gtk::Image::from_icon_name(step.turn.icon());
            img.set_pixel_size(18);
            img.set_valign(gtk::Align::Center);
            row.append(&img);
            let text = if step.turn == Turn::Arrive && !dest.is_empty() {
                step.instruction.replace("your destination", &dest)
            } else {
                step.instruction.clone()
            };
            let t = widgets::label(&text, "step-text");
            t.set_hexpand(true);
            t.set_wrap(true);
            t.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            t.set_natural_wrap_mode(gtk::NaturalWrapMode::None);
            t.set_max_width_chars(30);
            row.append(&t);
            if step.distance_meters > 0.0 {
                let d = widgets::label(&format_distance(step.distance_meters, &units), "step-distance");
                d.set_valign(gtk::Align::Center);
                row.append(&d);
            }
            self.steps.append(&row);
        }
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// Words and forms the route can find without a list: you, coordinates, home and work.
fn is_known_point(text: &str) -> bool {
    let t = text.trim();
    ["my location", "current location", "here", "me", "home", "work"].iter().any(|w| t.eq_ignore_ascii_case(w))
        || crate::providers::parse_coordinates(t).is_some()
}

async fn resolve_point(text: &str, api_key: &str, db: &Database) -> Option<(f64, f64)> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("my location")
        || trimmed.eq_ignore_ascii_case("current location")
        || trimmed.eq_ignore_ascii_case("here")
        || trimmed.eq_ignore_ascii_case("me")
    {
        let s = settings::get();
        if let (Some(lat), Some(lon)) = (s.last_current_lat, s.last_current_lon) {
            return Some((lat, lon));
        }
        if let Ok(loc) = crate::location::request_current_location(std::time::Duration::from_secs(6)).await {
            settings::update(|s| {
                s.last_current_lat = Some(loc.latitude);
                s.last_current_lon = Some(loc.longitude);
                s.last_current_label = loc.label.clone();
                s.last_current_accuracy = Some(loc.accuracy);
                s.last_current_from_ip = loc.from_ip;
            });
            return Some((loc.latitude, loc.longitude));
        }
    }

    if let Some(coords) = crate::providers::parse_coordinates(trimmed) {
        return Some(coords);
    }

    if trimmed.eq_ignore_ascii_case("home") {
        if let Ok(Some(p)) = db.get_favorite_by_label("home") {
            return Some((p.lat, p.lon));
        }
    } else if trimmed.eq_ignore_ascii_case("work")
        && let Ok(Some(p)) = db.get_favorite_by_label("work")
    {
        return Some((p.lat, p.lon));
    }

    if let Ok(results) = crate::providers::search_places(trimmed, api_key).await
        && let Some(first) = results.into_iter().next()
    {
        return Some((first.lat, first.lon));
    }
    None
}
