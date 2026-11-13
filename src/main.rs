mod app;
mod control;
mod integration;
mod location;
mod providers;
mod runtime;
mod settings;
mod settings_dialog;
mod storage;
mod test_hooks;
mod theme;
mod ui;

use app::MapsApp;
use integration::handle_cli_api;

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App {
        slug: "nyx-maps",
        name: "Maps",
    });

    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Headless API query check for Nyx Assistant / scripts
    if !args.is_empty() && args[0] == "--api" {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to initialize async runtime");

        let handled = rt.block_on(async { handle_cli_api(&args).await });
        if handled {
            return glib::ExitCode::SUCCESS;
        }
    }

    let maps_app = MapsApp::new();
    maps_app.run()
}
