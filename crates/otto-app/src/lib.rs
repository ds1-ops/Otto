//! Otto desktop app (iced). The binary in `main.rs` is a thin wrapper so the UI can
//! also be rendered headlessly in tests.

pub mod app;
pub mod dialogs;
pub mod dropdown;
pub mod graph;
pub mod grid;
pub mod icons;
pub mod logo;
pub mod placement;
pub mod plotted;
pub mod settings;
pub mod sidebar;
pub mod style;
pub mod view;

use std::path::PathBuf;

/// Runs the GUI, optionally opening a project or data file.
pub fn run_gui(open: Option<PathBuf>) -> iced::Result {
    iced::application(move || app::App::new(open.clone()), app::App::update, view::view)
        .title(app::App::title)
        .subscription(app::App::subscription)
        .theme(|_: &app::App| style::theme())
        // It opens hidden, is fitted to the screen above the taskbar once open (see
        // `placement`), and is then shown. From there it resizes like any other window; the
        // edge panels fold away when it gets narrow.
        .window(iced::window::Settings {
            size: iced::Size::new(1440.0, 900.0),
            min_size: Some(iced::Size::new(640.0, 480.0)),
            resizable: true,
            visible: false,
            position: iced::window::Position::Centered,
            icon: logo::window_icon(),
            ..Default::default()
        })
        .exit_on_close_request(false)
        .antialiasing(true)
        .run()
}
