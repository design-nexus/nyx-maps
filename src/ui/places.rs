use crate::providers::SearchResult;
use crate::storage::Database;
use gtk::prelude::*;
use nyx_ui::widgets;
use std::cell::RefCell;
use std::rc::Rc;

pub struct PlaceCard {
    pub root: gtk::Box,
    title_label: gtk::Label,
    address_label: gtk::Label,
    coords_label: gtk::Label,
    category_label: gtk::Label,
    fav_btn: gtk::Button,
    current_place: Rc<RefCell<Option<SearchResult>>>,
}

impl PlaceCard {
    pub fn new(
        db: Database,
        on_back: impl Fn() + 'static,
        on_directions: impl Fn(SearchResult) + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(false);
        root.set_vexpand(true);
        root.set_size_request(320, -1);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .propagate_natural_width(false)
            .hexpand(false)
            .vexpand(true)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.add_css_class("place-card");
        content.add_css_class("panel-body");
        content.set_hexpand(false);
        content.set_size_request(320, -1);

        // Header with Back button and Title
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let back_btn = widgets::bar_button("go-previous-symbolic", "Back to places");
        back_btn.connect_clicked(move |_| on_back());
        header.append(&back_btn);

        let title_label = gtk::Label::new(None);
        title_label.add_css_class("place-title");
        title_label.set_hexpand(false);
        title_label.set_xalign(0.0);
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_label.set_max_width_chars(24);
        header.append(&title_label);
        content.append(&header);

        // Category tag
        let category_label = gtk::Label::new(None);
        category_label.add_css_class("place-desc");
        category_label.set_xalign(0.0);
        category_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        content.append(&category_label);

        // Address
        let address_label = gtk::Label::new(None);
        address_label.add_css_class("place-desc");
        address_label.set_xalign(0.0);
        address_label.set_wrap(true);
        address_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        address_label.set_natural_wrap_mode(gtk::NaturalWrapMode::None);
        address_label.set_max_width_chars(28);
        content.append(&address_label);

        // Coordinates
        let coords_label = gtk::Label::new(None);
        coords_label.add_css_class("dim");
        coords_label.set_xalign(0.0);
        coords_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        content.append(&coords_label);

        // Actions row
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_margin_top(8);

        let directions_btn = gtk::Button::with_label("Directions");
        directions_btn.add_css_class("suggested-action");
        actions.append(&directions_btn);

        let fav_btn = gtk::Button::with_label("★ Save ▾");
        fav_btn.add_css_class("flat");

        let fav_popover = gtk::Popover::new();
        fav_popover.set_parent(&fav_btn);
        let pop_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        pop_box.set_margin_start(4);
        pop_box.set_margin_end(4);
        pop_box.set_margin_top(4);
        pop_box.set_margin_bottom(4);

        let btn_fav_star = gtk::Button::with_label("★ Save as Favorite");
        btn_fav_star.add_css_class("flat");
        let btn_fav_home = gtk::Button::with_label("⌂ Set as Home");
        btn_fav_home.add_css_class("flat");
        let btn_fav_work = gtk::Button::with_label("💼 Set as Work");
        btn_fav_work.add_css_class("flat");

        pop_box.append(&btn_fav_star);
        pop_box.append(&btn_fav_home);
        pop_box.append(&btn_fav_work);
        fav_popover.set_child(Some(&pop_box));

        let fp_clone = fav_popover.clone();
        fav_btn.connect_clicked(move |_| fp_clone.popup());
        actions.append(&fav_btn);

        let share_btn = gtk::Button::with_label("Share");
        share_btn.add_css_class("flat");
        share_btn.set_tooltip_text(Some("Share a link to this place"));
        actions.append(&share_btn);

        content.append(&actions);
        scroller.set_child(Some(&content));
        root.append(&scroller);

        let current_place: Rc<RefCell<Option<SearchResult>>> = Rc::new(RefCell::new(None));
        let cp_share = current_place.clone();
        share_btn.connect_clicked(move |b| {
            if let Some(ref p) = *cp_share.borrow() {
                share_place(p, b);
            }
        });
        let cp_dir = current_place.clone();
        directions_btn.connect_clicked(move |_| {
            if let Some(ref p) = *cp_dir.borrow() {
                on_directions(p.clone());
            }
        });

        let setup_save = |btn: &gtk::Button, label: &'static str, pop: gtk::Popover, cp: Rc<RefCell<Option<SearchResult>>>, db: Database, fb: gtk::Button| {
            btn.connect_clicked(move |_| {
                pop.popdown();
                if let Some(ref p) = *cp.borrow() {
                    let _ = db.add_favorite(&p.name, label, p.lat, p.lon, &p.formatted_address);
                    fb.set_label("★ Saved");
                }
            });
        };

        setup_save(&btn_fav_star, "favorite", fav_popover.clone(), current_place.clone(), db.clone(), fav_btn.clone());
        setup_save(&btn_fav_home, "home", fav_popover.clone(), current_place.clone(), db.clone(), fav_btn.clone());
        setup_save(&btn_fav_work, "work", fav_popover.clone(), current_place.clone(), db.clone(), fav_btn.clone());

        Self {
            root,
            title_label,
            address_label,
            coords_label,
            category_label,
            fav_btn,
            current_place,
        }
    }

    pub fn set_place(&self, place: &SearchResult, is_favorite: bool) {
        *self.current_place.borrow_mut() = Some(place.clone());
        self.title_label.set_text(&place.name);
        self.address_label.set_text(&place.formatted_address);
        self.coords_label.set_text(&format!("{:.5}, {:.5}", place.lat, place.lon));

        if let Some(ref cat) = place.category {
            self.category_label.set_text(cat);
            self.category_label.set_visible(true);
        } else {
            self.category_label.set_visible(false);
        }

        if is_favorite {
            self.fav_btn.set_label("★ Saved");
        } else {
            self.fav_btn.set_label("★ Save ▾");
        }

        self.root.set_visible(true);
    }
}

pub struct PlacesHubView {
    pub root: gtk::Box,
    saved_list: gtk::ListBox,
    recents_list: gtk::ListBox,
    btn_home_chip: gtk::Button,
    btn_work_chip: gtk::Button,
    db: Database,
    on_select_place: Rc<dyn Fn(SearchResult)>,
    on_toast: Rc<dyn Fn(&str)>,
}

impl PlacesHubView {
    pub fn new(
        db: Database,
        on_select_place: impl Fn(SearchResult) + 'static,
        on_open_directions: impl Fn() + 'static,
        on_category_search: impl Fn(&str) + 'static,
        on_toast: impl Fn(&str) + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(false);
        root.set_vexpand(true);
        root.set_size_request(320, -1);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hexpand(false);
        scroller.set_vexpand(true);
        scroller.set_propagate_natural_width(false);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        content.add_css_class("panel-body");
        content.set_hexpand(false);
        content.set_size_request(320, -1);

        // --- 1. QUICK WAYPOINT CHIPS ---
        let chips_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        chips_box.set_hexpand(false);

        let make_chip = |icon: &str, label: &str| -> gtk::Button {
            let btn = gtk::Button::new();
            btn.add_css_class("waypoint-chip");
            btn.set_hexpand(true);
            let h = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            h.set_halign(gtk::Align::Center);
            let img = gtk::Image::from_icon_name(icon);
            img.set_pixel_size(14);
            h.append(&img);
            let lbl = gtk::Label::new(Some(label));
            h.append(&lbl);
            btn.set_child(Some(&h));
            btn
        };

        let btn_home_chip = make_chip("user-home-symbolic", "Home");
        btn_home_chip.set_tooltip_text(Some("Go to Home"));
        let btn_work_chip = make_chip("work-symbolic", "Work");
        btn_work_chip.set_tooltip_text(Some("Go to Work"));
        let btn_route_chip = make_chip("directions-symbolic", "Route");
        btn_route_chip.set_tooltip_text(Some("Plan a route"));
        btn_route_chip.connect_clicked(move |_| on_open_directions());

        chips_box.append(&btn_home_chip);
        chips_box.append(&btn_work_chip);
        chips_box.append(&btn_route_chip);
        content.append(&chips_box);

        // --- 2. EXPLORE NEARBY CATEGORIES ---
        let nearby_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let nearby_title = gtk::Label::new(Some("EXPLORE NEARBY"));
        nearby_title.add_css_class("panel-section-title");
        nearby_title.set_hexpand(true);
        nearby_title.set_xalign(0.0);
        nearby_header.append(&nearby_title);
        content.append(&nearby_header);

        let nearby_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        nearby_box.add_css_class("nearby-chips");
        nearby_box.set_homogeneous(true);
        let on_cat = Rc::new(on_category_search);
        let categories = [
            ("emoji-food-symbolic", "Food", "food"),
            ("emoji-objects-symbolic", "Groceries", "groceries"),
            ("emoji-travel-symbolic", "Fuel", "fuel"),
            ("applications-multimedia-symbolic", "Entertainment", "entertainment"),
            ("emoji-nature-symbolic", "Parks", "parks"),
        ];

        for (icon_name, tooltip, category) in categories {
            let cat_btn = gtk::Button::new();
            let icon = gtk::Image::from_icon_name(icon_name);
            icon.set_pixel_size(16);
            cat_btn.set_child(Some(&icon));
            cat_btn.set_tooltip_text(Some(tooltip));
            cat_btn.add_css_class("nearby-chip-icon");
            let c_query = category.to_string();
            let c_cb = on_cat.clone();
            cat_btn.connect_clicked(move |_| c_cb(&c_query));
            nearby_box.append(&cat_btn);
        }
        content.append(&nearby_box);

        // --- 3. SAVED PLACES SECTION ---
        let saved_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let saved_title = gtk::Label::new(Some("SAVED PLACES"));
        saved_title.add_css_class("panel-section-title");
        saved_title.set_hexpand(true);
        saved_title.set_xalign(0.0);
        saved_header.append(&saved_title);
        content.append(&saved_header);

        let saved_list = gtk::ListBox::new();
        saved_list.add_css_class("group-list");
        content.append(&saved_list);

        // --- 4. RECENT SEARCHES SECTION ---
        let recents_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let recents_title = gtk::Label::new(Some("RECENT SEARCHES"));
        recents_title.add_css_class("panel-section-title");
        recents_title.set_hexpand(true);
        recents_title.set_xalign(0.0);
        recents_header.append(&recents_title);

        let clear_recents_btn = gtk::Button::with_label("Clear");
        clear_recents_btn.add_css_class("flat");
        clear_recents_btn.add_css_class("group-action-btn");
        recents_header.append(&clear_recents_btn);
        content.append(&recents_header);

        let recents_list = gtk::ListBox::new();
        recents_list.add_css_class("group-list");
        content.append(&recents_list);

        scroller.set_child(Some(&content));
        root.append(&scroller);

        let on_sel_rc = Rc::new(on_select_place);
        let on_toast_rc = Rc::new(on_toast);

        let hub = Self {
            root,
            saved_list,
            recents_list,
            btn_home_chip,
            btn_work_chip,
            db,
            on_select_place: on_sel_rc,
            on_toast: on_toast_rc,
        };

        // Wire home & work chips
        hub.bind_chips();

        // Wire clear recents
        let db_clear = hub.db.clone();
        let toast_clear = hub.on_toast.clone();
        let rlist_clear = hub.recents_list.clone();
        clear_recents_btn.connect_clicked(move |_| {
            let _ = db_clear.clear_recent_searches();
            while let Some(child) = rlist_clear.first_child() {
                rlist_clear.remove(&child);
            }
            let empty = gtk::Label::new(Some("No recent searches."));
            empty.add_css_class("dim");
            empty.set_margin_top(8);
            empty.set_margin_bottom(8);
            rlist_clear.append(&empty);
            toast_clear("Cleared recent searches");
        });

        hub.refresh();
        hub
    }

    fn bind_chips(&self) {
        let db_home = self.db.clone();
        let on_sel_home = self.on_select_place.clone();
        let on_toast_home = self.on_toast.clone();
        self.btn_home_chip.connect_clicked(move |_| {
            if let Ok(Some(home)) = db_home.get_favorite_by_label("home") {
                on_sel_home(SearchResult {
                    name: home.name,
                    formatted_address: home.address,
                    lat: home.lat,
                    lon: home.lon,
                    category: Some("home".into()),
                });
            } else {
                on_toast_home("Home not set. Star a place and choose 'Set as Home'!");
            }
        });

        let db_work = self.db.clone();
        let on_sel_work = self.on_select_place.clone();
        let on_toast_work = self.on_toast.clone();
        self.btn_work_chip.connect_clicked(move |_| {
            if let Ok(Some(work)) = db_work.get_favorite_by_label("work") {
                on_sel_work(SearchResult {
                    name: work.name,
                    formatted_address: work.address,
                    lat: work.lat,
                    lon: work.lon,
                    category: Some("work".into()),
                });
            } else {
                on_toast_work("Work not set. Star a place and choose 'Set as Work'!");
            }
        });
    }

    pub fn refresh(&self) {
        self.refresh_saved();
        self.refresh_recents();
    }

    fn refresh_saved(&self) {
        while let Some(child) = self.saved_list.first_child() {
            self.saved_list.remove(&child);
        }

        if let Ok(places) = self.db.list_favorites() {
            if places.is_empty() {
                let empty = gtk::Label::new(Some("No saved places yet."));
                empty.add_css_class("dim");
                empty.set_margin_top(8);
                empty.set_margin_bottom(8);
                self.saved_list.append(&empty);
                return;
            }

            for place in places {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row.add_css_class("search-result-row");

                let icon_name = match place.label.as_str() {
                    "home" => "user-home-symbolic",
                    "work" => "work-symbolic",
                    _ => "starred-symbolic",
                };
                let icon = gtk::Image::from_icon_name(icon_name);
                icon.set_pixel_size(16);
                row.append(&icon);

                let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                info_box.set_hexpand(true);

                let name_lbl = gtk::Label::new(Some(&place.name));
                name_lbl.add_css_class("place-title");
                name_lbl.set_xalign(0.0);
                name_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                name_lbl.set_max_width_chars(22);
                info_box.append(&name_lbl);

                let addr_lbl = gtk::Label::new(Some(&place.address));
                addr_lbl.add_css_class("place-desc");
                addr_lbl.set_xalign(0.0);
                addr_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                addr_lbl.set_max_width_chars(24);
                info_box.append(&addr_lbl);

                row.append(&info_box);

                // Delete button on right
                let del_btn = widgets::bar_button("window-close-symbolic", "Remove");
                del_btn.add_css_class("dim");
                let db_del = self.db.clone();
                let pid = place.id;
                let toast_del = self.on_toast.clone();
                let r_clone = row.clone();
                let sl_clone = self.saved_list.clone();
                del_btn.connect_clicked(move |_| {
                    let _ = db_del.remove_favorite(pid);
                    sl_clone.remove(&r_clone);
                    toast_del("Removed from saved places");
                });
                row.append(&del_btn);

                let pl_clone = place.clone();
                let on_sel = self.on_select_place.clone();
                let gesture = gtk::GestureClick::new();
                gesture.connect_released(move |_, _, _, _| {
                    on_sel(SearchResult {
                        name: pl_clone.name.clone(),
                        formatted_address: pl_clone.address.clone(),
                        lat: pl_clone.lat,
                        lon: pl_clone.lon,
                        category: Some(pl_clone.label.clone()),
                    });
                });
                row.add_controller(gesture);

                self.saved_list.append(&row);
            }
        }
    }

    fn refresh_recents(&self) {
        while let Some(child) = self.recents_list.first_child() {
            self.recents_list.remove(&child);
        }

        if let Ok(recents) = self.db.list_recent_searches() {
            if recents.is_empty() {
                let empty = gtk::Label::new(Some("No recent searches."));
                empty.add_css_class("dim");
                empty.set_margin_top(8);
                empty.set_margin_bottom(8);
                self.recents_list.append(&empty);
                return;
            }

            for rec in recents {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row.add_css_class("search-result-row");

                let icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
                icon.set_pixel_size(16);
                row.append(&icon);

                let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                info_box.set_hexpand(true);

                let name_lbl = gtk::Label::new(Some(&rec.query));
                name_lbl.add_css_class("place-title");
                name_lbl.set_xalign(0.0);
                name_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                name_lbl.set_max_width_chars(22);
                info_box.append(&name_lbl);

                let addr_lbl = gtk::Label::new(Some(&rec.formatted_address));
                addr_lbl.add_css_class("place-desc");
                addr_lbl.set_xalign(0.0);
                addr_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                addr_lbl.set_max_width_chars(24);
                info_box.append(&addr_lbl);

                row.append(&info_box);

                let rc_clone = rec.clone();
                let on_sel = self.on_select_place.clone();
                let gesture = gtk::GestureClick::new();
                gesture.connect_released(move |_, _, _, _| {
                    on_sel(SearchResult {
                        name: rc_clone.query.clone(),
                        formatted_address: rc_clone.formatted_address.clone(),
                        lat: rc_clone.lat,
                        lon: rc_clone.lon,
                        category: Some("Recent Search".into()),
                    });
                });
                row.add_controller(gesture);

                self.recents_list.append(&row);
            }
        }
    }
}

/// A link anyone can open: the place on OpenStreetMap.
pub fn place_url(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=17/{lat:.6}/{lon:.6}")
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
