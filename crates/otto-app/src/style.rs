//! Dark theme and widget styles.

use iced::theme::Palette;
use iced::widget::{button, container, text_input, toggler};
use iced::{color, Background, Border, Color, Shadow, Theme, Vector};

pub const BG: Color = color!(0x1E1F22);
pub const PANEL: Color = color!(0x2B2D30);
pub const TOOLBAR: Color = color!(0x26282B);
pub const BORDER: Color = color!(0x3C3F41);
pub const TEXT: Color = color!(0xDFE1E5);
pub const MUTED: Color = color!(0x8C9099);
pub const ACCENT: Color = color!(0x4F8EF7);
pub const ACCENT_SOFT: Color = Color { r: 0.31, g: 0.557, b: 0.969, a: 0.22 };
pub const HOVER: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 0.06 };
pub const CELL: Color = color!(0x232427);
pub const CELL_ALT: Color = color!(0x27282C);
pub const HEADER: Color = color!(0x323439);
pub const DANGER: Color = color!(0xF85149);
pub const SUCCESS: Color = color!(0x3FB950);
pub const CANVAS: Color = color!(0x18191B);

pub fn theme() -> Theme {
    Theme::custom(
        "Otto Dark",
        Palette { background: BG, text: TEXT, primary: ACCENT, success: SUCCESS, warning: color!(0xD29922), danger: DANGER },
    )
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border { color, width, radius: radius.into() }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style { background: Some(PANEL.into()), text_color: Some(TEXT), ..Default::default() }
}

pub fn toolbar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(TOOLBAR.into()),
        border: border(BORDER, 0.0, 0.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn canvas(_: &Theme) -> container::Style {
    container::Style { background: Some(CANVAS.into()), ..Default::default() }
}

pub fn statusbar(_: &Theme) -> container::Style {
    container::Style { background: Some(TOOLBAR.into()), text_color: Some(MUTED), ..Default::default() }
}

/// The white "sheet" a graph sits on, like a Prism graph page.
pub fn sheet(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        border: border(color!(0x000000, 0.2), 1.0, 2.0),
        shadow: Shadow { color: color!(0x000000, 0.45), offset: Vector::new(0.0, 4.0), blur_radius: 18.0 },
        ..Default::default()
    }
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(PANEL.into()),
        border: border(BORDER, 1.0, 6.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn error_box(_: &Theme) -> container::Style {
    container::Style {
        background: Some(color!(0xF85149, 0.12).into()),
        border: border(color!(0xF85149, 0.5), 1.0, 4.0),
        text_color: Some(color!(0xFFA198)),
        ..Default::default()
    }
}

pub const WARNING: Color = color!(0xE3B341);

/// Amber notice, e.g. statistics that no longer match their graph.
pub fn warning_box(_: &Theme) -> container::Style {
    container::Style {
        background: Some(color!(0xD29922, 0.13).into()),
        border: border(color!(0xD29922, 0.55), 1.0, 4.0),
        text_color: Some(color!(0xF0D89A)),
        ..Default::default()
    }
}

pub fn divider(_: &Theme) -> container::Style {
    container::Style { background: Some(BORDER.into()), ..Default::default() }
}

/// A round × like a Chrome tab's close button: just the × until pointed at, then a grey
/// circle behind it (the × on hovered navigator entries, and dialog close buttons).
pub fn delete_x(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Some(Background::Color(color!(0x4A4D52))),
        button::Status::Pressed => Some(Background::Color(color!(0x5A5D63))),
        _ => None,
    };
    button::Style { background: bg, text_color: TEXT, border: border(Color::TRANSPARENT, 0.0, 999.0), ..Default::default() }
}

/// Flat icon button used in the toolbar.
pub fn tool(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Some(Background::Color(HOVER)),
        button::Status::Pressed => Some(Background::Color(ACCENT_SOFT)),
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: if status == button::Status::Disabled { color!(0x5A5D63) } else { TEXT },
        border: border(Color::TRANSPARENT, 0.0, 4.0),
        ..Default::default()
    }
}

/// Navigator entry.
/// Outline of navigator entries related to the selected one (its table, graphs…).
pub const RELATED: Color = Color { r: 0.56, g: 0.58, b: 0.62, a: 0.75 };

/// A navigator entry related to the selected one: a grey outline, otherwise as usual.
pub fn nav_related(_: &Theme, status: button::Status) -> button::Style {
    let mut s = nav_item(false)(&Theme::Dark, status);
    s.border = border(RELATED, 1.0, 4.0);
    s
}

pub fn nav_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if selected {
            Some(Background::Color(ACCENT_SOFT))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(HOVER))
        } else {
            None
        };
        button::Style {
            background: bg,
            text_color: if selected { Color::WHITE } else { TEXT },
            border: border(if selected { ACCENT } else { Color::TRANSPARENT }, if selected { 1.0 } else { 0.0 }, 4.0),
            ..Default::default()
        }
    }
}

/// Toggle-able tile (graph types, formats).
pub fn tile(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if selected {
                ACCENT_SOFT
            } else if hovered {
                color!(0x3A3D42)
            } else {
                color!(0x323438)
            })),
            text_color: if selected { Color::WHITE } else { TEXT },
            border: border(if selected { ACCENT } else { BORDER }, 1.0, 5.0),
            ..Default::default()
        }
    }
}

/// Regular text button in panels.
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => color!(0x3E4147),
        button::Status::Pressed => color!(0x45484F),
        button::Status::Disabled => color!(0x2E3034),
        button::Status::Active => color!(0x36383D),
    };
    button::Style {
        background: Some(bg.into()),
        text_color: if status == button::Status::Disabled { MUTED } else { TEXT },
        border: border(BORDER, 1.0, 4.0),
        ..Default::default()
    }
}

pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let (bg, text) = match status {
        button::Status::Hovered => (color!(0x6A9FF8), Color::WHITE),
        button::Status::Pressed => (color!(0x3D7BE6), Color::WHITE),
        // Not available right now: clearly dimmed, not an invitation to click.
        button::Status::Disabled => (color!(0x4F8EF7, 0.25), color!(0xFFFFFF, 0.45)),
        button::Status::Active => (ACCENT, Color::WHITE),
    };
    button::Style { background: Some(bg.into()), text_color: text, border: border(bg, 1.0, 4.0), ..Default::default() }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { color!(0xF85149, 0.25) } else { color!(0xF85149, 0.1) })),
        text_color: color!(0xFFA198),
        border: border(color!(0xF85149, 0.45), 1.0, 4.0),
        ..Default::default()
    }
}

/// Grid cells.
pub fn cell(selected: bool, alt: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if selected {
            color!(0x2D4573)
        } else if alt {
            CELL_ALT
        } else {
            CELL
        })),
        border: border(if selected { ACCENT } else { color!(0x34363A) }, if selected { 1.5 } else { 0.5 }, 0.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// A cell in a highlighted column.
pub fn highlighted_cell(alt: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if alt { color!(0x263247) } else { color!(0x222D40) })),
        border: border(color!(0x34363A), 0.5, 0.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn header_cell(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if selected { color!(0x3A4A68) } else { HEADER })),
        border: border(color!(0x404247), 0.5, 0.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// A row-number cell, lit up while its row is selected.
pub fn row_header_cell(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if selected { color!(0x3A4A68) } else { HEADER })),
        border: border(color!(0x404247), 0.5, 0.0),
        text_color: Some(if selected { Color::WHITE } else { MUTED }),
        ..Default::default()
    }
}

pub fn row_header(_: &Theme) -> container::Style {
    container::Style {
        background: Some(HEADER.into()),
        border: border(color!(0x404247), 0.5, 0.0),
        text_color: Some(MUTED),
        ..Default::default()
    }
}

pub fn cell_editor(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut s = text_input::default(theme, status);
    s.background = Background::Color(color!(0x1B2A44));
    s.border = border(ACCENT, 1.5, 0.0);
    s
}

pub fn pill(connected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if connected { color!(0x3FB950, 0.14) } else { color!(0x8C9099, 0.12) })),
        border: border(if connected { color!(0x3FB950, 0.5) } else { BORDER }, 1.0, 10.0),
        text_color: Some(if connected { color!(0x7EE787) } else { MUTED }),
        ..Default::default()
    }
}

/// The closed state of a dropdown picker (looks like an input field).
pub fn picker(open: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(color!(0x1E1F22))),
            text_color: TEXT,
            border: border(if open || hovered { ACCENT } else { BORDER }, 1.0, 4.0),
            ..Default::default()
        }
    }
}

/// The floating menu of a dropdown picker.
pub fn menu(_: &Theme) -> container::Style {
    container::Style {
        background: Some(color!(0x2F3136).into()),
        border: border(ACCENT, 1.0, 5.0),
        shadow: Shadow { color: color!(0x000000, 0.5), offset: Vector::new(0.0, 4.0), blur_radius: 12.0 },
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// Small text-like button inside menus ("All", "None").
pub fn link(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(Background::Color(HOVER)),
        text_color: ACCENT,
        border: border(Color::TRANSPARENT, 0.0, 3.0),
        ..Default::default()
    }
}

/// A clickable status pill (the Claude MCP switch).
pub fn pill_button(connected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let base = pill(connected)(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = match (connected, hovered) {
            (true, false) => color!(0x3FB950, 0.14),
            (true, true) => color!(0x3FB950, 0.24),
            (false, false) => color!(0x8C9099, 0.12),
            (false, true) => color!(0x8C9099, 0.22),
        };
        button::Style {
            background: Some(bg.into()),
            text_color: base.text_color.unwrap_or(TEXT),
            border: base.border,
            ..Default::default()
        }
    }
}

/// The Claude MCP on/off switch: green when on, grey when off.
pub fn switch(_: &Theme, status: toggler::Status) -> toggler::Style {
    let (on, hovered) = match status {
        toggler::Status::Active { is_toggled } => (is_toggled, false),
        toggler::Status::Hovered { is_toggled } => (is_toggled, true),
        toggler::Status::Disabled { is_toggled } => (is_toggled, false),
    };
    let track = match (on, hovered) {
        (true, false) => color!(0x2EA043),
        (true, true) => color!(0x3FB950),
        (false, false) => color!(0x4A4D52),
        (false, true) => color!(0x5A5D63),
    };
    toggler::Style {
        background: track.into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: Color::WHITE.into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: None,
        padding_ratio: 0.1,
    }
}
