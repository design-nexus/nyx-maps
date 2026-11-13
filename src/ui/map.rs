use crate::providers::{MapStyle, create_renderer};
use gtk::prelude::*;
use libshumate::prelude::*;
use libshumate::{Coordinate, Marker, MarkerLayer, PathLayer, SimpleMap};
use std::cell::RefCell;
use std::rc::Rc;

type Callback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct MapView {
    pub root: SimpleMap,
    marker_layer: MarkerLayer,
    route_layer: PathLayer,
    route_case: PathLayer,
    /// The circle around you that the fix is within.
    accuracy_layer: PathLayer,
    selected_marker: Rc<RefCell<Option<Marker>>>,
    user_marker: Rc<RefCell<Option<Marker>>>,
    on_you: Rc<RefCell<Option<Callback>>>,
    route_start_marker: Rc<RefCell<Option<Marker>>>,
    route_end_marker: Rc<RefCell<Option<Marker>>>,
    current_style: Rc<RefCell<MapStyle>>,
    /// Shumate zoom minus the app's zoom (256 px tiles): 1 for half-size high-DPI tiles.
    zoom_offset: Rc<std::cell::Cell<f64>>,
}

impl MapView {
    pub fn new(initial_lat: f64, initial_lon: f64, initial_zoom: f64, style: MapStyle) -> Self {
        let root = SimpleMap::new();
        root.set_show_zoom_buttons(false);
        if let Some(lic) = root.license() {
            lic.set_visible(false);
        }
        root.set_hexpand(true);
        root.set_vexpand(true);

        let viewport = root.viewport().expect("SimpleMap has viewport");
        viewport.set_location(initial_lat, initial_lon);

        let marker_layer = MarkerLayer::new(&viewport);
        let route_layer = PathLayer::new(&viewport);
        route_layer.set_stroke(true);
        route_layer.set_closed(false);
        let route_case = PathLayer::new(&viewport);
        route_case.set_stroke(true);
        route_case.set_closed(false);

        // The route in the app's accent, on a darker edge so it reads on any map.
        let c = gdk::RGBA::parse(nyx_theme::app_accent("maps").as_str())
            .unwrap_or_else(|_| gdk::RGBA::new(0.35, 0.60, 0.98, 1.0));
        route_layer.set_stroke_color(Some(&gdk::RGBA::new(c.red(), c.green(), c.blue(), 1.0)));
        route_layer.set_stroke_width(6.0);
        route_case.set_stroke_color(Some(&gdk::RGBA::new(c.red() * 0.45, c.green() * 0.45, c.blue() * 0.45, 0.85)));
        route_case.set_stroke_width(10.0);

        let accuracy_layer = PathLayer::new(&viewport);
        accuracy_layer.set_closed(true);
        accuracy_layer.set_fill(true);
        accuracy_layer.set_fill_color(Some(&gdk::RGBA::new(0.24, 0.545, 0.99, 0.13)));
        accuracy_layer.set_stroke(true);
        accuracy_layer.set_stroke_color(Some(&gdk::RGBA::new(0.24, 0.545, 0.99, 0.5)));
        accuracy_layer.set_stroke_width(1.5);

        root.add_overlay_layer(&accuracy_layer);
        root.add_overlay_layer(&route_case);
        root.add_overlay_layer(&route_layer);
        root.add_overlay_layer(&marker_layer);

        let view = Self {
            root,
            marker_layer,
            route_layer,
            route_case,
            accuracy_layer,
            selected_marker: Rc::new(RefCell::new(None)),
            user_marker: Rc::new(RefCell::new(None)),
            on_you: Rc::new(RefCell::new(None)),
            route_start_marker: Rc::new(RefCell::new(None)),
            route_end_marker: Rc::new(RefCell::new(None)),
            current_style: Rc::new(RefCell::new(style)),
            zoom_offset: Rc::new(std::cell::Cell::new(0.0)),
        };

        view.set_style(style, "", "");
        if let Some(vp) = view.root.viewport() {
            vp.set_zoom_level(initial_zoom + view.zoom_offset.get());
        }
        view
    }

    /// Show `style`, keeping the place and scale on screen: sources with smaller tiles
    /// count zoom levels from a different start.
    pub fn set_style(&self, style: MapStyle, custom_sat: &str, custom_topo: &str) {
        *self.current_style.borrow_mut() = style;
        let renderer = create_renderer(style, custom_sat, custom_topo);
        let zoom = self.root.viewport().map(|vp| vp.zoom_level() - self.zoom_offset.get());
        let offset = (256.0 / renderer.tile_size().max(1) as f64).log2();
        self.zoom_offset.set(offset);
        self.root.set_map_source(Some(&renderer));
        if let (Some(vp), Some(z)) = (self.root.viewport(), zoom) {
            vp.set_zoom_level(z + offset);
        }
    }

    pub fn current_style(&self) -> MapStyle {
        *self.current_style.borrow()
    }

    pub fn center_on(&self, lat: f64, lon: f64, zoom: Option<f64>) {
        if let Some(viewport) = self.root.viewport() {
            viewport.set_location(lat, lon);
            if let Some(z) = zoom {
                viewport.set_zoom_level(z + self.zoom_offset.get());
            }
        }
    }

    pub fn set_selected_marker(&self, lat: f64, lon: f64, title: &str) {
        self.clear_selected_marker();
        let marker = pin_marker(lat, lon, title);
        self.marker_layer.add_marker(&marker);
        *self.selected_marker.borrow_mut() = Some(marker);
    }

    pub fn clear_selected_marker(&self) {
        if let Some(m) = self.selected_marker.borrow_mut().take() {
            self.marker_layer.remove_marker(&m);
        }
    }

    /// You on the map: the dot, and around it the circle the fix is within (`accuracy`
    /// in metres; none when it isn't known). `about` is the dot's tooltip.
    pub fn set_user_location(&self, lat: f64, lon: f64, accuracy: Option<f64>, about: &str) {
        if let Some(m) = self.user_marker.borrow_mut().take() {
            self.marker_layer.remove_marker(&m);
        }
        self.accuracy_layer.remove_all();
        if let Some(radius) = accuracy.filter(|r| *r > 0.0) {
            for (plat, plon) in circle(lat, lon, radius) {
                self.accuracy_layer.add_node(&Coordinate::new_full(plat, plon));
            }
        }

        let marker = you_marker(lat, lon);
        if let Some(dot) = marker.child() {
            dot.set_can_target(true);
            dot.set_tooltip_text(Some(about));
            dot.set_cursor_from_name(Some("pointer"));
            let click = gtk::GestureClick::new();
            let on_you = self.on_you.clone();
            click.connect_released(move |_, _, _, _| {
                let f = on_you.borrow().clone();
                if let Some(f) = f {
                    f();
                }
            });
            dot.add_controller(click);
        }
        self.marker_layer.add_marker(&marker);
        *self.user_marker.borrow_mut() = Some(marker);
    }

    /// Clicking your dot.
    pub fn connect_you_clicked(&self, f: impl Fn() + 'static) {
        *self.on_you.borrow_mut() = Some(Rc::new(f));
    }

    /// Centre on a fix, zoomed so the circle it's within fills about half the map.
    pub fn show_fix(&self, lat: f64, lon: f64, accuracy: f64) {
        let height = match self.root.height() {
            h if h > 0 => h as f64,
            _ => 600.0,
        };
        // Metres per pixel at zoom z (256 px tiles): 156543.03 · cos(lat) / 2^z.
        let zoom = ((height / 4.0) * 156_543.03 * lat.to_radians().cos() / accuracy.max(1.0)).log2();
        self.center_on(lat, lon, Some(zoom.clamp(3.0, 16.0)));
    }

    pub fn fit_coordinates(&self, coordinates: &[(f64, f64)]) {
        if coordinates.is_empty() {
            return;
        }
        let mut min_lat = f64::MAX;
        let mut max_lat = f64::MIN;
        let mut min_lon = f64::MAX;
        let mut max_lon = f64::MIN;

        for &(lat, lon) in coordinates {
            if lat < min_lat { min_lat = lat; }
            if lat > max_lat { max_lat = lat; }
            if lon < min_lon { min_lon = lon; }
            if lon > max_lon { max_lon = lon; }
        }

        let center_lat = (min_lat + max_lat) / 2.0;
        let center_lon = (min_lon + max_lon) / 2.0;

        let lat_diff = (max_lat - min_lat).abs();
        let lon_diff = (max_lon - min_lon).abs();
        let max_diff = lat_diff.max(lon_diff);

        let zoom = if max_diff < 0.005 {
            16.0
        } else if max_diff < 0.015 {
            15.0
        } else if max_diff < 0.035 {
            14.0
        } else if max_diff < 0.08 {
            13.0
        } else if max_diff < 0.18 {
            12.0
        } else if max_diff < 0.4 {
            11.0
        } else if max_diff < 0.9 {
            10.0
        } else if max_diff < 2.0 {
            9.0
        } else if max_diff < 4.0 {
            8.0
        } else if max_diff < 8.0 {
            7.0
        } else {
            6.0
        };

        self.center_on(center_lat, center_lon, Some(zoom));
    }

    /// Draw the route from a dot to a pin named `dest`, and fit it in view, `left` px
    /// of the map's left edge kept clear (for the card over it).
    pub fn show_route(&self, coordinates: &[(f64, f64)], dest: &str, left: f64) {
        self.clear_route();
        self.clear_selected_marker();

        if coordinates.is_empty() {
            return;
        }

        for (lat, lon) in coordinates {
            self.route_case.add_node(&Coordinate::new_full(*lat, *lon));
            self.route_layer.add_node(&Coordinate::new_full(*lat, *lon));
        }

        if let Some(first) = coordinates.first() {
            let marker = you_marker(first.0, first.1);
            self.marker_layer.add_marker(&marker);
            *self.route_start_marker.borrow_mut() = Some(marker);
        }
        if let Some(last) = coordinates.last() {
            let marker = pin_marker(last.0, last.1, dest);
            self.marker_layer.add_marker(&marker);
            *self.route_end_marker.borrow_mut() = Some(marker);
        }

        self.fit_coordinates(coordinates);
        if left > 0.0
            && let Some(vp) = self.root.viewport()
        {
            // The fit is for the whole map; the card takes some of it.
            vp.set_zoom_level(vp.zoom_level() - 0.5);
        }
        self.shift_center(left / 2.0);
    }

    /// Move the view so its centre shows `dx` px right of the middle.
    fn shift_center(&self, dx: f64) {
        let (Some(vp), Some(map)) = (self.root.viewport(), self.root.map()) else { return };
        let (w, h) = (map.width() as f64, map.height() as f64);
        if w <= 0.0 || dx == 0.0 {
            return;
        }
        let (lat, lon) = vp.widget_coords_to_location(&map, w / 2.0 - dx, h / 2.0);
        vp.set_location(lat, lon);
    }

    pub fn clear_route(&self) {
        self.route_layer.remove_all();
        self.route_case.remove_all();
        if let Some(m) = self.route_start_marker.borrow_mut().take() {
            self.marker_layer.remove_marker(&m);
        }
        if let Some(m) = self.route_end_marker.borrow_mut().take() {
            self.marker_layer.remove_marker(&m);
        }
    }

    pub fn zoom_in(&self) {
        if let Some(map) = self.root.map() {
            map.zoom_in();
        } else if let Some(vp) = self.root.viewport() {
            let current = vp.zoom_level();
            let max = vp.max_zoom_level() as f64;
            vp.set_zoom_level((current + 1.0).min(max));
        }
    }

    pub fn zoom_out(&self) {
        if let Some(map) = self.root.map() {
            map.zoom_out();
        } else if let Some(vp) = self.root.viewport() {
            let current = vp.zoom_level();
            let min = vp.min_zoom_level() as f64;
            vp.set_zoom_level((current - 1.0).max(min));
        }
    }

    pub fn get_viewport_state(&self) -> (f64, f64, f64) {
        if let Some(vp) = self.root.viewport() {
            (vp.latitude(), vp.longitude(), vp.zoom_level() - self.zoom_offset.get())
        } else {
            (0.0, 0.0, 0.0)
        }
    }
}

/// A pin with its tip on the place and the place's name beside it.
fn pin_marker(lat: f64, lon: f64, title: &str) -> Marker {
    let marker = Marker::new();
    marker.set_location(lat, lon);
    // The pin sits in the middle (the marker is centred on the place): a hidden copy
    // of the name on its left balances the name on its right, and a gap under it
    // as tall as the pin puts its tip on the place.
    let row = gtk::CenterBox::new();
    let ghost = gtk::Label::new(Some(title));
    ghost.add_css_class("map-pin-label");
    ghost.set_opacity(0.0);
    row.set_start_widget(Some(&ghost));
    let pin = gtk::Image::from_icon_name("maps-pin-filled-symbolic");
    pin.set_pixel_size(30);
    pin.add_css_class("map-pin");
    row.set_center_widget(Some(&pin));
    let name = gtk::Label::new(Some(title));
    name.add_css_class("map-pin-label");
    name.set_valign(gtk::Align::Center);
    row.set_end_widget(Some(&name));
    if title.is_empty() {
        ghost.set_visible(false);
        name.set_visible(false);
    }
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    col.append(&row);
    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_size_request(-1, 26);
    col.append(&gap);
    col.set_can_target(false);
    marker.set_child(Some(&col));
    marker
}

/// The points of a circle `radius` metres around a place.
fn circle(lat: f64, lon: f64, radius: f64) -> Vec<(f64, f64)> {
    let angular = (radius / 6_371_000.0).to_degrees();
    let squash = lat.to_radians().cos().max(0.01);
    (0..64)
        .map(|i| {
            let a = i as f64 / 64.0 * std::f64::consts::TAU;
            (lat + angular * a.cos(), lon + angular * a.sin() / squash)
        })
        .collect()
}

/// You: a blue dot in a soft halo.
fn you_marker(lat: f64, lon: f64) -> Marker {
    let marker = Marker::new();
    marker.set_location(lat, lon);
    let halo = gtk::CenterBox::new();
    halo.add_css_class("map-you-halo");
    halo.set_size_request(40, 40);
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("map-you");
    dot.set_halign(gtk::Align::Center);
    dot.set_valign(gtk::Align::Center);
    halo.set_center_widget(Some(&dot));
    halo.set_can_target(false);
    marker.set_child(Some(&halo));
    marker
}
