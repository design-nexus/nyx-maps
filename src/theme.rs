use crate::settings;
pub use nyx_theme::*;

pub fn install_icons() {
    let dir = nyx_core::paths::cache_home().join("nyx-maps/icons");
    let _ = std::fs::create_dir_all(&dir);
    const ICONS: &[(&str, &str)] = &[
        ("directions-symbolic", include_str!("../data/icons/directions-symbolic.svg")),
        ("route-symbolic", include_str!("../data/icons/route-symbolic.svg")),
        ("drive-symbolic", include_str!("../data/icons/drive-symbolic.svg")),
        ("walk-symbolic", include_str!("../data/icons/walk-symbolic.svg")),
        ("bicycle-symbolic", include_str!("../data/icons/bicycle-symbolic.svg")),
        ("work-symbolic", include_str!("../data/icons/work-symbolic.svg")),
        ("layers-symbolic", include_str!("../data/icons/layers-symbolic.svg")),
        ("maps-bike-symbolic", include_str!("../data/icons/maps-bike-symbolic.svg")),
        ("maps-car-symbolic", include_str!("../data/icons/maps-car-symbolic.svg")),
        ("maps-chevron-down-symbolic", include_str!("../data/icons/maps-chevron-down-symbolic.svg")),
        ("maps-close-symbolic", include_str!("../data/icons/maps-close-symbolic.svg")),
        ("maps-copy-symbolic", include_str!("../data/icons/maps-copy-symbolic.svg")),
        ("maps-directions-symbolic", include_str!("../data/icons/maps-directions-symbolic.svg")),
        ("maps-food-symbolic", include_str!("../data/icons/maps-food-symbolic.svg")),
        ("maps-fuel-symbolic", include_str!("../data/icons/maps-fuel-symbolic.svg")),
        ("maps-groceries-symbolic", include_str!("../data/icons/maps-groceries-symbolic.svg")),
        ("maps-history-symbolic", include_str!("../data/icons/maps-history-symbolic.svg")),
        ("maps-home-symbolic", include_str!("../data/icons/maps-home-symbolic.svg")),
        ("maps-locate-symbolic", include_str!("../data/icons/maps-locate-symbolic.svg")),
        ("maps-minus-symbolic", include_str!("../data/icons/maps-minus-symbolic.svg")),
        ("maps-park-symbolic", include_str!("../data/icons/maps-park-symbolic.svg")),
        ("maps-pin-filled-symbolic", include_str!("../data/icons/maps-pin-filled-symbolic.svg")),
        ("maps-pin-symbolic", include_str!("../data/icons/maps-pin-symbolic.svg")),
        ("maps-plus-symbolic", include_str!("../data/icons/maps-plus-symbolic.svg")),
        ("maps-share-symbolic", include_str!("../data/icons/maps-share-symbolic.svg")),
        ("maps-star-filled-symbolic", include_str!("../data/icons/maps-star-filled-symbolic.svg")),
        ("maps-star-symbolic", include_str!("../data/icons/maps-star-symbolic.svg")),
        ("maps-straight-symbolic", include_str!("../data/icons/maps-straight-symbolic.svg")),
        ("maps-swap-symbolic", include_str!("../data/icons/maps-swap-symbolic.svg")),
        ("maps-ticket-symbolic", include_str!("../data/icons/maps-ticket-symbolic.svg")),
        ("maps-turn-left-symbolic", include_str!("../data/icons/maps-turn-left-symbolic.svg")),
        ("maps-turn-right-symbolic", include_str!("../data/icons/maps-turn-right-symbolic.svg")),
        ("maps-walk-symbolic", include_str!("../data/icons/maps-walk-symbolic.svg")),
        ("maps-work-symbolic", include_str!("../data/icons/maps-work-symbolic.svg")),
    ];
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = std::fs::write(&path, svg);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

pub fn install() {
    install_icons();
    nyx_theme::use_design_2();

    nyx_theme::install(Style {
        window_class: "maps-window",
        base_skip: &[],
        css: include_str!("style.css"),
        no_glow_css: "",
        extra_colors: None,
        prefs: || {
            let p = settings::get();
            ThemePrefs {
                mode: p.mode,
                theme: p.theme,
                glow: p.glow,
                reduce_motion: p.reduce_motion,
            }
        },
        set_theme: Some(|mode, theme| {
            settings::update(|p| {
                p.mode = mode;
                p.theme = theme;
            });
        }),
    });
}
