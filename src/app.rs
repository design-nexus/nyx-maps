use crate::control;
use crate::integration::{parse_deep_link, DeepLink};
use crate::theme;
use crate::ui::MainWindow;
use gio::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct MapsApp {
    app: gtk::Application,
    main_window: Rc<RefCell<Option<Rc<MainWindow>>>>,
}

impl MapsApp {
    pub fn new() -> Self {
        // NYX_MAPS_APP_ID runs a test copy beside the real one (its own window class).
        let id = std::env::var("NYX_MAPS_APP_ID").unwrap_or_else(|_| "io.github.design_nexus.nyx.Maps".into());
        let app = gtk::Application::builder()
            .application_id(id)
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE | gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        Self {
            app,
            main_window: Rc::new(RefCell::new(None)),
        }
    }

    pub fn run(&self) -> glib::ExitCode {
        let win_ctrl = self.main_window.clone();
        self.app.connect_startup(move |app| {
            theme::install();
            let win_ref = win_ctrl.clone();
            nyx_core::control::serve_async(app, control::tools(), move |name, args, reply| {
                control::handle(name, args, win_ref.clone(), reply);
            });
        });

        let win_ref = self.main_window.clone();
        self.app.connect_activate(move |application| {
            if win_ref.borrow().is_none() {
                let win = MainWindow::build(application);
                win.window.present();
                // A test copy never asks the location service, only takes a made-up fix.
                if std::env::var_os("NYX_MAPS_APP_ID").is_none() || crate::test_hooks::fix().is_some() {
                    win.check_location_on_startup();
                }
                crate::test_hooks::typing(&win);
                crate::test_hooks::me_card(&win);
                *win_ref.borrow_mut() = Some(win);
            } else if let Some(ref win) = *win_ref.borrow() {
                win.window.present();
            }
        });

        // Command line handling
        let win_cmd = self.main_window.clone();
        self.app.connect_command_line(move |application, cmdline| {
            let arguments = cmdline.arguments();
            let args: Vec<String> = arguments.iter().map(|s| s.to_string_lossy().to_string()).collect();

            application.activate();

            if args.len() > 1 {
                let arg = &args[1];
                if let Some(ref win) = *win_cmd.borrow() {
                    if let Some(link) = parse_deep_link(arg) {
                        match link {
                            DeepLink::Location { lat, lon, label } => {
                                win.open_location(lat, lon, &label);
                            }
                            DeepLink::Search { query } => {
                                win.search_query(&query);
                            }
                            DeepLink::Route { start, end, mode } => {
                                win.open_route(start, end, &mode);
                            }
                        }
                    } else if !arg.starts_with('-') {
                        win.search_query(arg);
                    }
                }
            }

            glib::ExitCode::SUCCESS
        });

        // Open URI handling (e.g. geo: URIs from browser or terminal)
        let win_open = self.main_window.clone();
        self.app.connect_open(move |application, files, _hint| {
            application.activate();
            for file in files {
                let uri = file.uri().to_string();
                if let Some(ref win) = *win_open.borrow() {
                    if let Some(link) = parse_deep_link(&uri) {
                        match link {
                            DeepLink::Location { lat, lon, label } => {
                                win.open_location(lat, lon, &label);
                            }
                            DeepLink::Search { query } => {
                                win.search_query(&query);
                            }
                            DeepLink::Route { start, end, mode } => {
                                win.open_route(start, end, &mode);
                            }
                        }
                    }
                }
            }
        });

        self.app.run()
    }
}
