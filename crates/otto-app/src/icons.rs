//! Monochrome SVG icons (24×24), tinted by the widget style.

use iced::widget::svg::{self, Svg};
use iced::{Color, Length};

use otto_core::GraphKind;

macro_rules! icon_svg {
    ($body:expr) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">"##,
            $body,
            "</svg>"
        )
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Import,
    Paste,
    Table,
    Stats,
    Copy,
    Export,
    Code,
    Plus,
    ChevronDown,
    ChevronRight,
    Graph,
    Result,
    Up,
    Down,
    Spark,
    Data,
    Warning,
    Lock,
    Close,
    /// A tick, for the chosen entry of a menu.
    Check,
    /// A fitted curve through points.
    Curve,
    /// The statistics engine (an "R").
    Engine,
    /// Bring something back in step with its source.
    Refresh,
    /// The navigator, at the left edge.
    PanelLeft,
    /// The settings panel, at the right edge.
    PanelRight,
    Kind(GraphKind),
}

impl Icon {
    fn source(self) -> &'static str {
        match self {
            Icon::New => icon_svg!(r#"<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/><path d="M12 11v6M9 14h6"/>"#),
            Icon::Open => icon_svg!(r#"<path d="M3 6h6l2 2h10v11H3z"/>"#),
            Icon::Save => icon_svg!(r#"<path d="M5 4h11l3 3v13H5z"/><path d="M8 4v5h7V4"/><path d="M8 20v-6h8v6"/>"#),
            Icon::SaveAs => icon_svg!(r#"<path d="M5 4h11l3 3v6"/><path d="M5 4v16h7"/><path d="M8 4v5h7V4"/><path d="m14 20 1-3 5-5 2 2-5 5z"/>"#),
            Icon::Undo => icon_svg!(r#"<path d="M8 5 3 10l5 5"/><path d="M3 10h11a6 6 0 0 1 0 12h-3"/>"#),
            Icon::Redo => icon_svg!(r#"<path d="m16 5 5 5-5 5"/><path d="M21 10H10a6 6 0 0 0 0 12h3"/>"#),
            Icon::Import => icon_svg!(r#"<path d="M12 3v11"/><path d="m7 9 5 5 5-5"/><path d="M4 16v4h16v-4"/>"#),
            Icon::Paste => icon_svg!(r#"<path d="M9 3h6v3H9z"/><path d="M8 5H5v16h14V5h-3"/><path d="M8 11h8M8 15h6"/>"#),
            Icon::Curve => icon_svg!(r#"<path d="M3 19C9 19 9 5 15 5h6"/><circle cx="5" cy="15.5" r="1.3"/><circle cx="11.5" cy="10.5" r="1.3"/><circle cx="18" cy="7.5" r="1.3"/>"#),
            Icon::Engine => icon_svg!(r#"<path d="M7 20V4h6a4 4 0 0 1 0 8H7"/><path d="m12 12 5 8"/>"#),
            Icon::Refresh => icon_svg!(r#"<path d="M20 11a8 8 0 1 0-1.6 5.6"/><path d="M20 4v7h-7"/>"#),
            Icon::PanelLeft => icon_svg!(r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>"#),
            Icon::PanelRight => icon_svg!(r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>"#),
            Icon::Table => icon_svg!(r#"<rect x="3" y="4" width="18" height="16" rx="1"/><path d="M3 9h18M3 14h18M9 4v16M15 4v16"/>"#),
            Icon::Stats => icon_svg!(r#"<path d="M17 5H7l6 7-6 7h10"/>"#),
            Icon::Copy => icon_svg!(r#"<rect x="8" y="8" width="12" height="12" rx="1"/><path d="M4 16V5a1 1 0 0 1 1-1h11"/>"#),
            Icon::Export => icon_svg!(r#"<path d="M12 15V3"/><path d="m7 8 5-5 5 5"/><path d="M4 13v8h16v-8"/>"#),
            Icon::Code => icon_svg!(r#"<path d="m8 7-5 5 5 5"/><path d="m16 7 5 5-5 5"/><path d="m14 4-4 16"/>"#),
            Icon::Plus => icon_svg!(r#"<path d="M12 5v14M5 12h14"/>"#),
            Icon::ChevronDown => icon_svg!(r#"<path d="m6 9 6 6 6-6"/>"#),
            Icon::Check => icon_svg!(r#"<path d="m5 13 5 5L20 7"/>"#),
            Icon::ChevronRight => icon_svg!(r#"<path d="m9 6 6 6-6 6"/>"#),
            Icon::Graph => icon_svg!(r#"<path d="M4 4v16h16"/><path d="M8 16v-5M12 16V8M16 16v-7"/>"#),
            Icon::Result => icon_svg!(r#"<rect x="4" y="3" width="16" height="18" rx="1"/><path d="M8 8h8M8 12h8M8 16h5"/>"#),
            Icon::Up => icon_svg!(r#"<path d="M12 19V5"/><path d="m6 11 6-6 6 6"/>"#),
            Icon::Down => icon_svg!(r#"<path d="M12 5v14"/><path d="m6 13 6 6 6-6"/>"#),
            Icon::Spark => icon_svg!(
                r#"<path d="M12 3v5M12 16v5M3 12h5M16 12h5M6 6l3 3M15 15l3 3M6 18l3-3M15 9l3-3"/>"#
            ),
            Icon::Data => icon_svg!(r#"<path d="M4 6h16M4 12h16M4 18h16"/><path d="M9 3v18"/>"#),
            Icon::Warning => icon_svg!(r#"<path d="M12 3 2 20h20z"/><path d="M12 10v4.5M12 17.2v.3"/>"#),
            Icon::Lock => icon_svg!(r#"<rect x="5" y="11" width="14" height="10" rx="1"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>"#),
            Icon::Close => icon_svg!(r#"<path d="M6 6l12 12M18 6 6 18"/>"#),
            Icon::Kind(kind) => kind_source(kind),
        }
    }

    pub fn handle(self) -> svg::Handle {
        svg::Handle::from_memory(self.source().as_bytes())
    }
}

macro_rules! kind_svg {
    ($body:expr) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3.5 3v17.5H21" fill="none" stroke="#000" stroke-width="1.6"/>"##,
            $body,
            "</svg>"
        )
    };
}

macro_rules! plain_svg {
    ($body:expr) => {
        concat!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">"##, $body, "</svg>")
    };
}

fn kind_source(kind: GraphKind) -> &'static str {
    match kind {
        GraphKind::XyPoints => kind_svg!(
            r##"<g fill="#000"><circle cx="8" cy="16" r="1.7"/><circle cx="11.5" cy="12" r="1.7"/><circle cx="15" cy="9.5" r="1.7"/><circle cx="19" cy="6" r="1.7"/></g>"##
        ),
        GraphKind::Heatmap => kind_svg!(
            r##"<g stroke="none"><rect x="6" y="5" width="4.5" height="4.5" fill="#000" fill-opacity="0.85"/><rect x="10.5" y="5" width="4.5" height="4.5" fill="#000" fill-opacity="0.35"/><rect x="15" y="5" width="4.5" height="4.5" fill="#000" fill-opacity="0.6"/><rect x="6" y="9.5" width="4.5" height="4.5" fill="#000" fill-opacity="0.25"/><rect x="10.5" y="9.5" width="4.5" height="4.5" fill="#000" fill-opacity="0.75"/><rect x="15" y="9.5" width="4.5" height="4.5" fill="#000" fill-opacity="0.45"/><rect x="6" y="14" width="4.5" height="4.5" fill="#000" fill-opacity="0.55"/><rect x="10.5" y="14" width="4.5" height="4.5" fill="#000" fill-opacity="0.15"/><rect x="15" y="14" width="4.5" height="4.5" fill="#000" fill-opacity="0.9"/></g>"##
        ),
        GraphKind::XyLine => kind_svg!(
            r##"<path d="M7 17 11 12.5 14.5 10 19.5 6" fill="none" stroke="#000" stroke-width="1.6"/>"##
        ),
        GraphKind::XyPointsLine => kind_svg!(
            r##"<path d="M8 16 11.5 12 15 9.5 19 6" fill="none" stroke="#000" stroke-width="1.4"/><g fill="#000"><circle cx="8" cy="16" r="1.7"/><circle cx="11.5" cy="12" r="1.7"/><circle cx="15" cy="9.5" r="1.7"/><circle cx="19" cy="6" r="1.7"/></g>"##
        ),
        GraphKind::ColumnBar => kind_svg!(
            r##"<g fill="#000" fill-opacity="0.45" stroke="#000" stroke-width="1.1"><rect x="6.5" y="9" width="4.5" height="11"/><rect x="14" y="13" width="4.5" height="7"/></g><g fill="#000"><circle cx="7.9" cy="8" r="1"/><circle cx="9.7" cy="10.5" r="1"/><circle cx="15.4" cy="12" r="1"/><circle cx="17.2" cy="14.5" r="1"/></g>"##
        ),
        GraphKind::GroupedInterleaved => kind_svg!(
            r##"<g stroke="#000" stroke-width="1"><rect x="5.5" y="10" width="3" height="10" fill="#000" fill-opacity="0.75"/><rect x="8.5" y="6" width="3" height="14" fill="#000" fill-opacity="0.25"/><rect x="14" y="12" width="3" height="8" fill="#000" fill-opacity="0.75"/><rect x="17" y="8" width="3" height="12" fill="#000" fill-opacity="0.25"/></g>"##
        ),
        GraphKind::GroupedSeparated => kind_svg!(
            r##"<g stroke="#000" stroke-width="1"><rect x="5" y="9" width="2.8" height="11" fill="#000" fill-opacity="0.75"/><rect x="7.8" y="12" width="2.8" height="8" fill="#000" fill-opacity="0.75"/><rect x="14.2" y="7" width="2.8" height="13" fill="#000" fill-opacity="0.25"/><rect x="17" y="11" width="2.8" height="9" fill="#000" fill-opacity="0.25"/></g>"##
        ),
        GraphKind::StackedBar => kind_svg!(
            r##"<g stroke="#000" stroke-width="1"><rect x="6" y="13" width="5" height="7" fill="#000" fill-opacity="0.75"/><rect x="6" y="8" width="5" height="5" fill="#000" fill-opacity="0.25"/><rect x="14" y="11" width="5" height="9" fill="#000" fill-opacity="0.75"/><rect x="14" y="5" width="5" height="6" fill="#000" fill-opacity="0.25"/></g>"##
        ),
        GraphKind::StackedPercent => kind_svg!(
            r##"<g stroke="#000" stroke-width="1"><rect x="6" y="12" width="5" height="8" fill="#000" fill-opacity="0.75"/><rect x="6" y="4" width="5" height="8" fill="#000" fill-opacity="0.25"/><rect x="14" y="9" width="5" height="11" fill="#000" fill-opacity="0.75"/><rect x="14" y="4" width="5" height="5" fill="#000" fill-opacity="0.25"/></g>"##
        ),
        GraphKind::Pie => plain_svg!(
            r##"<g stroke="#000" stroke-width="1.3"><path d="M12 12V3a9 9 0 0 1 7.8 13.5z" fill="#000" fill-opacity="0.25"/><path d="M12 12l7.8 4.5A9 9 0 1 1 12 3z" fill="#000" fill-opacity="0.6"/></g>"##
        ),
        GraphKind::Donut => plain_svg!(
            r##"<g stroke="#000" stroke-width="1.3"><path d="M12 12V3a9 9 0 0 1 7.8 13.5z" fill="#000" fill-opacity="0.25"/><path d="M12 12l7.8 4.5A9 9 0 1 1 12 3z" fill="#000" fill-opacity="0.6"/><circle cx="12" cy="12" r="4" fill="#fff"/></g>"##
        ),
        GraphKind::Contingency => plain_svg!(
            r##"<g stroke="#000" stroke-width="1.3" fill="none"><path d="M3 6.5h18M3 11.5h18M3 17.5h18M9 3v18M15.5 3v18"/></g><g fill="#000" fill-opacity="0.25" stroke="none"><rect x="3" y="3" width="6" height="3.5"/><rect x="3" y="6.5" width="6" height="5"/><rect x="3" y="11.5" width="6" height="6"/></g>"##
        ),
        GraphKind::Survival => kind_svg!(
            r##"<path d="M4.5 5H9V9H12.5V13.5H16V17H20.5" fill="none" stroke="#000" stroke-width="1.5"/><path d="M10.7 7.8V10.2M18 15.8V18.2" stroke="#000" stroke-width="1.3"/>"##
        ),
        GraphKind::Box => kind_svg!(
            r##"<g fill="none" stroke="#000" stroke-width="1.2"><path d="M8.5 5v3M8.5 18v3M7 5h3M7 21h3"/><rect x="5.8" y="8" width="5.4" height="10" fill="#000" fill-opacity="0.35"/><path d="M5.8 13h5.4"/><path d="M17 7v3M17 17v3M15.5 7h3M15.5 20h3"/><rect x="14.3" y="10" width="5.4" height="7" fill="#000" fill-opacity="0.35"/><path d="M14.3 13.5h5.4"/></g>"##
        ),
        GraphKind::Violin => kind_svg!(
            r##"<g fill="#000" fill-opacity="0.35" stroke="#000" stroke-width="1.2"><path d="M8.5 4c2.6 0 3 4 3 6s-3 2.5-3 5 3 2 3 4-1.4 2-3 2-3-.6-3-2 3-1.5 3-4-3-3-3-5 .4-6 3-6z"/><path d="M17 7c2.3 0 2.7 3 2.7 4.8s-2.7 2.2-2.7 4.3 2.7 1.7 2.7 3.4S18.5 21 17 21s-2.7-.8-2.7-1.5 2.7-1.3 2.7-3.4-2.7-2.5-2.7-4.3S14.7 7 17 7z"/></g>"##
        ),
        GraphKind::Volcano => kind_svg!(
            r##"<g fill="#000"><circle cx="6.5" cy="8" r="1.3"/><circle cx="7.5" cy="12" r="1.3"/><circle cx="19" cy="7" r="1.3"/><circle cx="18" cy="11" r="1.3"/></g><g fill="#000" fill-opacity="0.3"><circle cx="11" cy="17" r="1.3"/><circle cx="13" cy="15" r="1.3"/><circle cx="12" cy="19" r="1.3"/><circle cx="14.5" cy="18" r="1.3"/><circle cx="9.5" cy="15" r="1.3"/></g>"##
        ),
        GraphKind::Qq => kind_svg!(
            r##"<path d="M5 19 20 5" fill="none" stroke="#000" stroke-width="1.2" stroke-dasharray="2 2"/><g fill="#000"><circle cx="6.5" cy="18.5" r="1.3"/><circle cx="9.5" cy="14.5" r="1.3"/><circle cx="12.5" cy="12" r="1.3"/><circle cx="15.5" cy="8.5" r="1.3"/><circle cx="18.5" cy="6.5" r="1.3"/></g>"##
        ),
        GraphKind::XyScatter => kind_svg!(
            r##"<g fill="#000"><circle cx="7" cy="15" r="1.4"/><circle cx="9" cy="17" r="1.4"/><circle cx="10.5" cy="12.5" r="1.4"/><circle cx="13" cy="14" r="1.4"/><circle cx="14" cy="9" r="1.4"/><circle cx="16.5" cy="10.5" r="1.4"/><circle cx="18.5" cy="6.5" r="1.4"/></g>"##
        ),
    }
}

/// An icon widget of `size` logical pixels in `color`.
pub fn icon<'a>(i: Icon, size: f32, color: Color) -> Svg<'a> {
    iced::widget::svg(i.handle())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
