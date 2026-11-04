//! Graph sheet (centre) and the graph panel (right sidebar): choose graph type,
//! pick X / Y / Group columns from the linked table, filter rows, format and export.

use iced::widget::{button, checkbox, column, container, pick_list, responsive, row, scrollable, space, svg, text, text_input, Column};
use iced::{Alignment, Fill, Length};

use otto_core::analysis::{CurveDisplay, GraphAnnotation};
use otto_core::{
    AnalysisKind, AnalysisSpec, AxisNumbers, AxisScale, Center, ColumnKind, DataLayout, ErrorBars, Filter, Format, GraphKind, GraphSpec, OneOrMany,
    Replicates, Whiskers,
};
use otto_engine::*;

use crate::app::{App, Draft, Message, GRAPH_SCROLL};
use crate::dialogs::{analysis_summary, can_recalculate, DialogMsg};
use crate::dropdown::drop_down;
use crate::icons::{icon, Icon};
use crate::style;
use crate::view::{caption, heading, tip, El};

const PX_PER_PT: f32 = 96.0 / 72.0;
const PT_PER_MM: f32 = 72.0 / 25.4;

pub fn view<'a>(app: &'a App, id: &str) -> El<'a> {
    let Some(spec) = app.engine.project.graph(id) else { return space().into() };
    let zoom_buttons = row(Zoom::ALL.iter().map(|&z| {
        button(text(z.label()).size(12))
            .padding([3, 9])
            .style(style::tile(app.engine.zoom == z))
            .on_press(Message::Cmd(Command::UiSetZoom(ZoomArgs { zoom: z })))
            .into()
    }))
    .spacing(4);
    let (wmm, hmm) = spec.format.axis_mm();
    let size_info = match app.preview.as_ref().filter(|p| p.graph_id == spec.id).map(|p| &p.result) {
        Some(Ok(s)) => {
            let stats = if s.stats_height > 0.0 { format!(" (incl. {:.1} mm statistics)", s.stats_height / PT_PER_MM) } else { String::new() };
            format!("axes {wmm:.0} × {hmm:.0} mm · figure {:.1} × {:.1} mm{stats}", s.width / PT_PER_MM, s.height / PT_PER_MM)
        }
        _ => format!("axes {wmm:.0} × {hmm:.0} mm"),
    };
    // Statistics still being calculated in R are drawn as soon as they arrive.
    let size_info = if app.calculating(&spec.id) { format!("{size_info} · calculating statistics in R…") } else { size_info };
    // Name with the physical size underneath. A long name is cut off at the edge of its
    // space, so the buttons never move.
    let no_wrap = iced::widget::text::Wrapping::None;
    let title = column![
        row![
            icon(if spec.is_final() { Icon::Lock } else { Icon::Kind(spec.kind) }, 18.0, style::ACCENT),
            container(text(spec.name.clone()).size(17).wrapping(no_wrap)).width(Fill).clip(true),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        container(caption(size_info).wrapping(no_wrap)).width(Fill).clip(true),
    ]
    .spacing(2);
    // Title left, a quick graph's way forward in the centre, zoom right: the two sides
    // share the width equally, so the centre buttons stay centred.
    let side = |content: El<'a>, right: bool| -> El<'a> {
        let c = container(content).width(Length::FillPortion(1)).clip(true);
        if right { c.align_right(Fill).into() } else { c.into() }
    };
    let mut header = row![side(title.into(), false)].spacing(12).align_y(Alignment::Center);
    if !spec.is_final() {
        header = header.push(finalize_buttons(spec));
    }
    let header = header.push(side(zoom_buttons.into(), true));

    let sheet: El = match app.preview.as_ref().filter(|p| p.graph_id == spec.id) {
        Some(p) => match &p.result {
            Ok(s) => {
                let (w_pt, h_pt, stats_pt) = (s.width, s.height, s.stats_height);
                let handle = s.handle.clone();
                let zoom = app.engine.zoom.factor();
                responsive(move |size| {
                    const MARGIN: f32 = 30.0;
                    let (scale, pad) = match zoom {
                        Some(f) => (f, 24.0 * f.sqrt()),
                        None => {
                            // Fit the graph as it is without statistics: brackets add height above it
                            // (scroll up to see them) instead of shrinking the graph.
                            let pad = 28.0;
                            let avail_w = (size.width - 2.0 * (MARGIN + pad)).max(60.0);
                            let avail_h = (size.height - 2.0 * (MARGIN + pad)).max(60.0);
                            let scale = (avail_w / (w_pt * PX_PER_PT)).min(avail_h / ((h_pt - stats_pt) * PX_PER_PT)).clamp(0.5, 8.0);
                            (scale, pad)
                        }
                    };
                    let (iw, ih) = (w_pt * PX_PER_PT * scale, h_pt * PX_PER_PT * scale);
                    let page = container(svg(handle.clone()).width(iw).height(ih)).padding(pad).style(style::sheet);
                    // Centred while it fits; scrollable both ways once it doesn't.
                    let (cw, ch) = ((iw + 2.0 * (pad + MARGIN)).max(size.width), (ih + 2.0 * (pad + MARGIN)).max(size.height));
                    scrollable(container(page).center_x(cw).center_y(ch))
                        .id(GRAPH_SCROLL)
                        .direction(scrollable::Direction::Both {
                            vertical: scrollable::Scrollbar::default(),
                            horizontal: scrollable::Scrollbar::default(),
                        })
                        .width(Fill)
                        .height(Fill)
                        .into()
                })
                .into()
            }
            Err(e) => container(
                column![
                    text("This graph can't be drawn yet").size(16),
                    text(e.clone()).size(13),
                    caption("Pick suitable X / Y / Group columns in the panel on the right."),
                ]
                .spacing(8)
                .max_width(520),
            )
            .padding(16)
            .style(style::error_box)
            .into(),
        },
        None => space().into(),
    };
    column![container(header).padding([10, 14]), container(sheet).center(Fill)].into()
}

/// How much of a label fits on one line of a panel field (310 px panel, 78 px label).
/// Pick lists wrap rather than clip, so anything longer is elided before it gets there.
const FIELD_CHARS: usize = 24;

/// A column choice in a pick list ("(none)" or a column name).
#[derive(Clone, Debug, PartialEq)]
enum ColChoice {
    None,
    Col(String),
}

impl std::fmt::Display for ColChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColChoice::None => f.write_str("(none)"),
            ColChoice::Col(c) => f.write_str(&crate::view::elide(c, FIELD_CHARS)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TableChoice {
    id: String,
    name: String,
}

impl std::fmt::Display for TableChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", crate::view::elide(&self.name, FIELD_CHARS.saturating_sub(self.id.chars().count() + 3).max(8)), self.id)
    }
}

fn field<'a>(label: &'a str, w: impl Into<El<'a>>) -> El<'a> {
    row![caption(label).width(Length::Fixed(78.0)), w.into()].spacing(6).align_y(Alignment::Center).into()
}

/// A section that folds, with a caret and its title as the button. The panel holds a great
/// many settings and most are set once, so only the graph type and the data stay open and
/// the rest keep out of the way until they are wanted.
fn foldable<'a>(app: &'a App, key: &'static str, title: &str, default_open: bool, content: impl FnOnce() -> El<'a>) -> El<'a> {
    let open = app.section_open(key, default_open);
    let head = button(
        // Uppercase and muted, the same heading a fixed section gets, with a caret.
        row![
            text(if open { "\u{25be}" } else { "\u{25b8}" }).size(10.5).color(style::MUTED),
            text(title.to_uppercase()).size(10.5).color(style::MUTED)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([2, 0])
    .style(style::link)
    .on_press(Message::TogglePanelSection(key, !open));
    let mut col = Column::new().spacing(10).push(head);
    if open {
        col = col.push(content());
    }
    col.into()
}

/// Several named blocks inside one folded section, each under a small heading.
fn blocks<'a>(parts: Vec<(&'static str, El<'a>)>) -> El<'a> {
    Column::with_children(parts.into_iter().map(|(title, body)| {
        if title.is_empty() {
            body
        } else {
            column![caption(title).size(11), body].spacing(5).into()
        }
    }))
    .spacing(14)
    .into()
}

fn section<'a>(title: &str, content: impl Into<El<'a>>) -> El<'a> {
    column![heading(title), content.into()].spacing(8).into()
}

fn mapping(gid: &str) -> GraphMappingArgs {
    GraphMappingArgs { graph: Some(gid.to_string()), ..Default::default() }
}

pub fn panel<'a>(app: &'a App, id: &str) -> El<'a> {
    let Some(spec) = app.engine.project.graph(id) else { return space().into() };
    let gid = spec.id.clone();
    let table = app.engine.project.table_by_id(&spec.table_id);

    // Graph type tiles.
    let tiles: Vec<El> = GraphKind::ALL
        .iter()
        .map(|&k| {
            let b = button(
                column![icon(Icon::Kind(k), 28.0, style::TEXT), text(short_kind(k)).size(10.5)].spacing(3).align_x(Alignment::Center),
            )
            .width(Fill)
            .padding([6, 2])
            .style(style::tile(spec.kind == k))
            .on_press(Message::Cmd(Command::GraphSetKind(GraphKindArgs { graph: Some(gid.clone()), kind: k })));
            tip(b, k.label())
        })
        .collect();
    // Three tiles per row; the last row is padded so the tiles keep their width.
    let mut kind_grid = Column::new().spacing(6);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut r = Vec::new();
        for _ in 0..3 {
            r.push(tiles.next().unwrap_or_else(|| space::horizontal().into()));
        }
        kind_grid = kind_grid.push(row(r).spacing(6));
    }

    let mut content = Column::new().spacing(18).padding(14);
    content = content.push(name_field(app, spec));
    let plotted = button(row![icon(Icon::Data, 14.0, style::TEXT), text("Show plotted table").size(12)].spacing(6).align_y(Alignment::Center))
        .padding([5, 10])
        .style(style::secondary)
        .on_press(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::PlottedTables, item: Some(gid.clone()) })));
        let from = spec.locked.as_ref().map(|l| l.from_graph.clone()).unwrap_or_default();
        let update: El = match app.engine.project.graph(&from) {
            Some(_) => button(row![icon(Icon::Refresh, 14.0, style::TEXT), text("Update from quick graph").size(12)].spacing(6).align_y(Alignment::Center))
                .padding([5, 10])
                .style(style::secondary)
                .on_press(Message::Cmd(Command::GraphUpdateFromQuick(GraphRef { graph: Some(gid.clone()) })))
                .into(),
            None => caption("The quick graph this came from has been deleted, so there is nothing to update from.").into(),
        };
        content = content.push(
            row![plotted, update].spacing(8).wrap(),
        );
    if spec.is_final() {
        // Type, data, error bars and axes were settled on the quick graph; a final graph
        // only changes format, appearance and statistics, so its statistics always match.
        // The card above says where its data came from and updates it from that graph.
        // Statistics are what a final graph is for, so they stay open; everything else is
        // the look of the figure and folds into one section, as on a quick graph.
        if spec.kind != GraphKind::Heatmap {
            content = content.push(section("Statistics", analyze_section(app, spec)));
        }
        content = content.push(foldable(app, "appearance", "Appearance", false, || {
            let mut parts = vec![
                ("Format", format_section(spec)),
                ("Axis titles", titles_section(app, spec)),
            ];
            if spec.kind == GraphKind::Heatmap {
                parts.push(("Heatmap", summary_section(spec)));
            }
            parts.push(("Colors, legend and symbols", appearance_section(app, spec)));
            blocks(parts)
        }));
    } else {
        // The graph type and the columns are what a quick graph is for, so they are open.
        // Filters, the look of the figure and exporting fold away: "Make final graph" is
        // in the toolbar above, and the navigator shows what came from this graph.
        // The type is settled early and the tile grid is tall, so it folds away like the
        // rest; the heading says which type the graph has while it is shut.
        let kind_title = format!("Graph type \u{00b7} {}", spec.kind.label());
        content = content.push(foldable(app, "kind", &kind_title, false, move || kind_grid.into()));
        match table {
            Some(t) => {
                content = content.push(section("Data on table", data_section(app, spec, t)));
                let filtered = spec.filters.iter().filter(|f| !f.keeps_everything()).count();
                let title = if filtered > 0 { format!("Filters ({filtered})") } else { "Filters".to_string() };
                content = content.push(foldable(app, "filters", &title, false, || filters_section(app, spec, t)));
            }
            None => content = content.push(text("The linked table no longer exists.").size(13).color(style::DANGER)),
        }
        content = content.push(foldable(app, "appearance", "Appearance", false, || {
            let mut parts: Vec<(&'static str, El)> = Vec::new();
            match spec.kind {
                GraphKind::Survival | GraphKind::Qq => {}
                k if k.is_pie() => parts.push(("Slices", pie_caption())),
                GraphKind::Contingency => parts.push(("Counts", contingency_caption())),
                GraphKind::Heatmap => parts.push(("Heatmap", summary_section(spec))),
                GraphKind::Volcano => parts.push(("Cutoffs & labels", volcano_section(app, spec))),
                GraphKind::Box => parts.push(("Box & whiskers", summary_section(spec))),
                GraphKind::Violin => parts.push(("Violin", summary_section(spec))),
                k if k.is_stacked() => parts.push(("Stacked segments", summary_section(spec))),
                _ => parts.push(("Bars, symbols & error bars", summary_section(spec))),
            }
            parts.push(("Format (fixed publication size)", format_section(spec)));
            if !spec.kind.is_pie() && spec.kind != GraphKind::Contingency {
                parts.push(("Axes", axes_section(app, spec)));
            }
            parts.push(("Colors, legend and symbols", appearance_section(app, spec)));
            blocks(parts)
        }));
    }
    content = content.push(foldable(app, "export", "Export", false, || export_section(app, &gid)));
    content = content.push(
        row![
            button(text("Duplicate").size(12))
                .padding([5, 10])
                .style(style::secondary)
                .on_press(Message::Cmd(Command::GraphDuplicate(DuplicateGraphArgs { graph: Some(gid.clone()), name: None }))),
            button(text("Delete graph").size(12))
                .padding([5, 10])
                .style(style::danger)
                .on_press(Message::Cmd(Command::GraphDelete(GraphRef { graph: Some(gid.clone()) }))),
        ]
        .spacing(8),
    );
    scrollable(content).height(Fill).into()
}

/// "Make final graph" (choose statistics first) and "without statistics", shown at the top
/// of a quick graph. XY graphs have no t test / ANOVA, so they only get "Make final graph".
fn finalize_buttons<'a>(spec: &GraphSpec) -> El<'a> {
    let finalize = Message::Cmd(Command::GraphFinalize(FinalizeArgs { graph: Some(spec.id.clone()), name: None }));
    let primary = |msg: Message| {
        button(row![icon(Icon::Lock, 14.0, iced::Color::WHITE), text("Make final graph").size(13)].spacing(6).align_y(Alignment::Center))
            .padding([5, 14])
            .style(style::primary)
            .on_press(msg)
    };
    // The same two buttons on every graph type: statistics are chosen in Analyze Data.
    if matches!(spec.kind.shape(), otto_core::DataShape::Grid | otto_core::DataShape::Slices) {
        // Heatmaps, pies and donuts have no statistics.
        let what = if spec.kind.is_pie() { "chart" } else { "heatmap" };
        return tip(primary(finalize), format!("Copy this {what} as a final graph on a locked copy of the data, ready to refine for publication"));
    }
    let hint = match spec.kind.shape() {
        otto_core::DataShape::Xy => {
            "Choose statistics in Analyze Data (curve fits, regression, correlation, area under the curve): makes a final graph on a locked copy of the data with them"
        }
        otto_core::DataShape::Survival => {
            "Choose statistics in Analyze Data (Kaplan-Meier: log-rank, Gehan-Breslow-Wilcoxon, medians, hazard ratios): makes a final graph on a locked copy of the data with them"
        }
        otto_core::DataShape::Counts => {
            "Choose statistics in Analyze Data (chi-square, Fisher's exact test, relative risk, odds ratio): makes a final graph on a locked copy of the data with them"
        }
        _ => "Choose statistics in Analyze Data (t tests, one-way and two-way ANOVA): makes a final graph on a locked copy of the data with them",
    };
    row![
        tip(primary(Message::Dialog(DialogMsg::OpenAnalyze)), hint),
        tip(
            button(text("without statistics").size(13)).padding([5, 12]).style(style::secondary).on_press(finalize),
            "Make the final graph now; statistics can be added to it later"
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// A quick graph's way to statistics, and the final graphs already made from it.
/// Axis titles: labels, so they stay editable on final graphs.
fn titles_section<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let gid = spec.id.clone();
    // The ranges and the scale were settled on the quick graph, but how the numbers are
    // written is a label like the titles, so it can still be changed here.
    let mut col = column![
        field("X title", draft_input(app, Draft::AxisTitle(gid.clone(), Axis::X), spec.x_axis.title.clone().unwrap_or_default(), "automatic (column name)")),
        field("Y title", draft_input(app, Draft::AxisTitle(gid.clone(), Axis::Y), spec.y_axis.title.clone().unwrap_or_default(), "automatic (column name)")),
        field("Y labels", numbers_pick(spec, Axis::Y)),
    ]
    .spacing(8);
    if spec.kind.is_xy() {
        col = col.push(field("X labels", numbers_pick(spec, Axis::X)));
    }
    col.into()
}


/// A final graph's statistics: add an analysis, choose which ones are drawn, and deal
/// with analyses that no longer match the graph.
fn analyze_section<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let analyses = app.engine.graph_analyses(&spec.id);
    // Every analysis starts in Analyze Data.
    let mut col = Column::new().spacing(8).push(
        button(row![icon(Icon::Stats, 14.0, style::TEXT), text("Analyze data…").size(12)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([6, 8])
            .style(style::secondary)
            .on_press(Message::Dialog(DialogMsg::OpenAnalyze)),
    );
    if analyses.is_empty() {
        return col
            .push(caption(if spec.kind.has_bars() {
                "Significant comparisons appear as brackets with stars."
            } else if spec.kind == GraphKind::Survival {
                "Medians, hazard ratios and the comparison of the curves appear in the results."
            } else {
                "Fitted curves and regression lines are drawn on the graph."
            }))
            .into();
    }
    col = col.push(caption(if analyses.len() > 1 {
        "Statistics of this graph. One of them is drawn at a time: ticking one takes the previous one off the graph."
    } else {
        "Statistics of this graph:"
    }));
    let stale = analyses.iter().filter(|a| app.stale.contains_key(&a.id)).count();
    for a in analyses {
        col = col.push(analysis_card(app, a));
    }
    if stale > 1 {
        col = col.push(
            button(text("Recalculate all out-of-date statistics").size(12))
                .width(Fill)
                .padding([5, 8])
                .style(style::secondary)
                .on_press(Message::Dialog(DialogMsg::Recalculate(None))),
        );
    }
    col.into()
}

/// Show / hide one analysis on the graph.
fn annotation_cmd(a: &AnalysisSpec, graph: GraphAnnotation) -> Message {
    let analysis = Some(a.id.clone());
    match &a.kind {
        AnalysisKind::TTest(_) => Message::Cmd(Command::AnalysisEditTTest(EditTTestArgs { analysis, graph: Some(graph), ..Default::default() })),
        AnalysisKind::OneWayAnova(_) => {
            Message::Cmd(Command::AnalysisEditOneWayAnova(EditAnovaArgs { analysis, graph: Some(graph), ..Default::default() }))
        }
        AnalysisKind::CurveFit(_) => Message::Cmd(Command::AnalysisEditCurveFit(EditFitArgs {
            analysis,
            graph: Some(CurveDisplay { show: graph.show }),
            ..Default::default()
        })),
        AnalysisKind::LinearRegression(_) => Message::Cmd(Command::AnalysisEditLinearRegression(EditLinRegArgs {
            analysis,
            graph: Some(CurveDisplay { show: graph.show }),
            ..Default::default()
        })),
        AnalysisKind::TwoWayAnova(_) => {
            Message::Cmd(Command::AnalysisEditTwoWayAnova(EditTwoWayArgs { analysis, graph: Some(graph), ..Default::default() }))
        }
        AnalysisKind::Descriptive { .. }
        | AnalysisKind::Correlation(_)
        | AnalysisKind::Auc(_)
        | AnalysisKind::Survival(_)
        | AnalysisKind::Normality(_)
        | AnalysisKind::Outliers(_)
        | AnalysisKind::Contingency(_) => Message::Noop,
    }
}

fn analysis_card<'a>(app: &'a App, a: &'a AnalysisSpec) -> El<'a> {
    // Fits and regression lines are drawn or not; correlation draws nothing.
    let drawn = |c: CurveDisplay| GraphAnnotation { show: c.show, show_ns: false };
    let (ann, kind_xy) = match &a.kind {
        AnalysisKind::TTest(t) => (t.graph, false),
        AnalysisKind::OneWayAnova(x) => (x.graph, false),
        AnalysisKind::CurveFit(f) => (drawn(f.graph), true),
        AnalysisKind::LinearRegression(l) => (drawn(l.graph), true),
        AnalysisKind::TwoWayAnova(t) => (t.graph, false),
        AnalysisKind::Correlation(_)
        | AnalysisKind::Auc(_)
        | AnalysisKind::Survival(_)
        | AnalysisKind::Normality(_)
        | AnalysisKind::Outliers(_)
        | AnalysisKind::Contingency(_) => (GraphAnnotation { show: false, show_ns: false }, true),
        AnalysisKind::Descriptive { .. } => return space().into(),
    };
    let draws = !matches!(
        a.kind,
        AnalysisKind::Correlation(_) | AnalysisKind::Auc(_) | AnalysisKind::Survival(_) | AnalysisKind::Normality(_) | AnalysisKind::Outliers(_)
    );
    let link = |label: &'static str, msg: Message| -> El<'a> { button(text(label).size(12)).padding([2, 4]).style(style::link).on_press(msg).into() };
    let results = Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Results, item: Some(a.id.clone()) }));
    let body: El = match app.stale.get(&a.id) {
        Some(reasons) => {
            let recalc = can_recalculate(app, a);
            let mut c = column![
                row![icon(Icon::Warning, 14.0, style::WARNING), text(a.name.clone()).size(12.5)].spacing(6).align_y(Alignment::Center),
                caption(format!("Out of date: {}. Hidden from the graph until it is recalculated or set up again.", reasons.join("; "))).size(11.5),
            ]
            .spacing(5);
            if let Err(e) = &recalc {
                c = c.push(caption(e.clone()).size(11.5));
            }
            c.push(
                row![
                    button(text("Recalculate").size(12))
                        .padding([4, 10])
                        .style(style::primary)
                        .on_press_maybe(recalc.is_ok().then(|| Message::Dialog(DialogMsg::Recalculate(Some(a.id.clone()))))),
                    button(text("Set up again…").size(12)).padding([4, 10]).style(style::secondary).on_press(Message::Dialog(DialogMsg::SetUpAgain(a.id.clone()))),
                ]
                .spacing(6),
            )
            .into()
        }
        None => {
            let mut details = Column::new().spacing(4).push(caption(analysis_summary(a)).size(11.5));
            match app.report(&a.id) {
                Some(Err(otto_engine::EngineError::Calculating)) => details = details.push(text("Calculating in R…").size(11.5).color(style::ACCENT)),
                Some(Err(e)) => details = details.push(text(e.to_string()).size(11.5).color(style::DANGER)),
                Some(Ok(_)) | None => {}
            }
            if !kind_xy {
                details = details.push(
                    checkbox(ann.show_ns)
                        .label("Also show non-significant (ns)")
                        .size(14)
                        .text_size(12)
                        .on_toggle_maybe(ann.show.then_some(move |v| annotation_cmd(a, GraphAnnotation { show: true, show_ns: v }))),
                );
            }
            details = details.push(row![link("Parameters…", Message::Dialog(DialogMsg::EditAnalysis(a.id.clone()))), link("Results", results)].spacing(8));
            let head: El = if draws {
                checkbox(ann.show)
                    .label(a.name.clone())
                    .size(15)
                    .text_size(12.5)
                    .on_toggle(move |v| annotation_cmd(a, GraphAnnotation { show: v, show_ns: ann.show_ns }))
                    .into()
            } else {
                text(a.name.clone()).size(12.5).into()
            };
            column![head, container(details).padding(iced::Padding { left: 23.0, ..Default::default() })].spacing(4).into()
        }
    };
    container(body).padding([8, 10]).width(Fill).style(style::card).into()
}

/// A pie has nothing to summarise: its slices are the sums of the values, and their
/// shares are written on them. This says so where the other kinds offer their controls.
fn contingency_caption<'a>() -> El<'a> {
    caption("Rows are the X categories, columns the data sets, and each cell the sum of its Y values. Counts already tabulated come in as one row per group with a Y column per outcome; a column of 1s tallies one row per subject.").into()
}

/// A pie has nothing to summarise either.
fn pie_caption<'a>() -> El<'a> {
    caption("One slice per X category, sized by the sum of its Y values (not the mean, as on a bar graph). Each slice's share is written on it where it fits.").into()
}

fn short_kind(k: GraphKind) -> &'static str {
    match k {
        GraphKind::XyPoints => "XY points",
        GraphKind::XyPointsLine => "XY + line",
        GraphKind::XyLine => "XY line",
        GraphKind::Heatmap => "Heatmap",
        GraphKind::ColumnBar => "Column bar",
        GraphKind::GroupedInterleaved => "Interleaved",
        GraphKind::GroupedSeparated => "Separated",
        GraphKind::StackedBar => "Stacked",
        GraphKind::StackedPercent => "Stacked 100%",
        GraphKind::Pie => "Pie",
        GraphKind::Donut => "Donut",
        GraphKind::Contingency => "Contingency",
        GraphKind::XyScatter => "Scatter",
        GraphKind::Survival => "Survival",
        GraphKind::Box => "Box plot",
        GraphKind::Violin => "Violin",
        GraphKind::Volcano => "Volcano",
        GraphKind::Qq => "QQ plot",
    }
}

fn name_field<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let key = Draft::GraphName(spec.id.clone());
    let value = app.drafts.get(&key).cloned().unwrap_or_else(|| spec.name.clone());
    text_input("Graph name", &value)
        .on_input(move |s| Message::Draft(key.clone(), s))
        .on_submit(Message::Commit(Draft::GraphName(spec.id.clone())))
        .size(15)
        .padding([6, 8])
        .into()
}

fn data_section<'a>(app: &'a App, spec: &'a GraphSpec, t: &'a otto_core::DataTable) -> El<'a> {
    let gid = spec.id.clone();
    // Quick graphs pick any data table; a final graph stays on its locked copy.
    let table_pick: El = if spec.is_final() {
        caption(format!("locked copy ({})", t.id)).size(12.5).into()
    } else {
        let tables: Vec<TableChoice> = app
            .engine
            .project
            .tables
            .iter()
            .filter(|t| t.locked_by.is_none())
            .map(|t| TableChoice { id: t.id.clone(), name: t.name.clone() })
            .collect();
        let current_table = tables.iter().find(|c| c.id == t.id).cloned();
        let g1 = gid.clone();
        pick_list(tables, current_table, move |c: TableChoice| {
            Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { table: Some(c.id), ..mapping(&g1) }))
        })
        .text_size(13)
        .width(Fill)
        .into()
    };

    let factor_names: Vec<String> = t.columns.iter().filter(|c| c.kind() != ColumnKind::Numeric).map(|c| c.name.clone()).collect();

    // X: one numeric column for XY graphs; one or more factor columns for bar graphs.
    let x_widget: El = if spec.kind.is_xy() {
        let mut x_opts = vec![ColChoice::None];
        x_opts.extend(App::numeric_columns(t).into_iter().map(ColChoice::Col));
        let x_sel = Some(spec.x.first().cloned().map_or(ColChoice::None, ColChoice::Col));
        let g2 = gid.clone();
        pick_list(x_opts, x_sel, move |c: ColChoice| {
            let x = match c {
                ColChoice::None => String::new(),
                ColChoice::Col(s) => s,
            };
            Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { x: Some(OneOrMany::One(x)), ..mapping(&g2) }))
        })
        .text_size(13)
        .width(Fill)
        .into()
    } else {
        let g2 = gid.clone();
        let none = if spec.kind == GraphKind::ColumnBar { "(none: one bar per Y column)" } else { "(none)" };
        multi_picker(app, "x", Picker { placeholder: none, ordered: true, combined: true, bulk: false }, factor_names.clone(), spec.x.clone(), move |x| {
            Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { x: Some(OneOrMany::Many(x)), ..mapping(&g2) }))
        })
    };

    let g3 = gid.clone();
    let y_widget = multi_picker(app, "y", Picker { placeholder: "(choose)", ordered: false, combined: false, bulk: true }, App::numeric_columns(t), spec.y.clone(), move |y| {
        Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { y: Some(y), ..mapping(&g3) }))
    });

    let mut col = column![
        field("Table", table_pick),
        field(
            match spec.kind {
                GraphKind::Survival => "Time (X)",
                GraphKind::Heatmap => "Columns (X)",
                k if k.is_xy() => "X (numeric)",
                _ => "X (bars)",
            },
            x_widget
        ),
        field(
            match spec.kind {
                GraphKind::Survival => "Event (Y: 1 / 0)",
                GraphKind::Heatmap => "Value (Y)",
                _ => "Y values",
            },
            y_widget
        ),
    ]
    .spacing(8);
    let g4 = gid.clone();
    let groups = multi_picker(app, "group", Picker { placeholder: "(none)", ordered: true, combined: true, bulk: false }, factor_names, spec.group.clone(), move |g| {
        Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { group: Some(OneOrMany::Many(g)), ..mapping(&g4) }))
    });
    col = col.push(field(if spec.kind == GraphKind::Heatmap { "Rows (Group by)" } else { "Group by" }, groups));
    // Which subject a value belongs to: an XY graph that draws one line per replicate
    // cannot be drawn without it, since the place of a value among the ones at its X says
    // nothing about whose it is (one missing timepoint and every later subject moves up).
    if matches!(spec.kind, GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine) {
        let needed = spec.kind.has_line() && spec.summary.replicates == Replicates::Individual;
        let none = if needed { "(choose a column)" } else { "(none: k-th value at each X)" };
        let mut options = vec![ColChoice::None];
        options.extend(
            t.columns.iter().filter(|c| !spec.x.contains(&c.name) && !spec.y.contains(&c.name)).map(|c| ColChoice::Col(c.name.clone())),
        );
        let selected = Some(spec.replicate_by.clone().map_or(ColChoice::None, ColChoice::Col));
        let g5 = gid.clone();
        let replicate = pick_list(options, selected, move |c: ColChoice| {
            let by = match c {
                ColChoice::None => String::new(),
                ColChoice::Col(s) => s,
            };
            Message::Cmd(Command::GraphSetMapping(GraphMappingArgs { replicate_by: Some(by), ..mapping(&g5) }))
        })
        .placeholder(none)
        .text_size(13)
        .width(Fill);
        col = col.push(field("Replicates", replicate));
        if needed && spec.replicate_by.is_none() {
            col = col.push(caption("One line per replicate needs the column that says whose a value is."));
        }
    }
    col.into()
}

/// How a multi-select picker summarises and edits its selection.
struct Picker<'a> {
    placeholder: &'a str,
    /// Selection order matters (it sets the "A | B" order) and is shown as (1), (2)…
    ordered: bool,
    /// What the summary puts between the picked values: the columns that combine into one
    /// label join with " | ", everything else with a comma.
    combined: bool,
    /// Offer "All" / "None" shortcuts.
    bulk: bool,
}

/// A dropdown with a checkbox list; the closed state summarises the selection.
fn multi_picker<'a>(
    app: &'a App,
    id: &str,
    p: Picker<'a>,
    options: Vec<String>,
    selected: Vec<String>,
    on_change: impl Fn(Vec<String>) -> Message + Clone + 'a,
) -> El<'a> {
    let open = app.open_dropdown.as_deref() == Some(id);
    // Anything selected is always listed (e.g. a value set through MCP that this
    // picker would not normally offer), so it can be unticked.
    let mut options = options;
    for s in &selected {
        if !options.contains(s) {
            options.push(s.clone());
        }
    }
    let summary = if selected.is_empty() {
        p.placeholder.to_string()
    } else if p.combined {
        selected.join(otto_core::COMBINE_SEP)
    } else {
        selected.join(", ")
    };
    // The anchor clips, but the text still has to be one line to keep the row one line.
    let summary = crate::view::elide(&summary, 60);
    let anchor = button(
        row![
            text(summary).size(13).wrapping(iced::widget::text::Wrapping::None),
            space::horizontal(),
            icon(if open { Icon::Up } else { Icon::ChevronDown }, 12.0, style::MUTED)
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([5, 8])
    .clip(true)
    .style(style::picker(open))
    .on_press(Message::ToggleDropdown(id.to_string()));

    let mut list = Column::new().spacing(5);
    for name in &options {
        let position = selected.iter().position(|s| s == name);
        let label = match position {
            Some(i) if p.ordered && selected.len() > 1 => format!("{name}  ({})", i + 1),
            _ => name.clone(),
        };
        let (current, name, f) = (selected.clone(), name.clone(), on_change.clone());
        list = list.push(checkbox(position.is_some()).label(label).size(15).text_size(13).on_toggle(move |on| {
            let mut v = current.clone();
            if on {
                if !v.contains(&name) {
                    v.push(name.clone());
                }
            } else {
                v.retain(|c| c != &name);
            }
            f(v)
        }));
    }
    if options.is_empty() {
        list = list.push(caption("No suitable columns"));
    }
    let mut menu = Column::new().spacing(6);
    if p.bulk && options.len() > 1 {
        let (all, none) = (on_change.clone(), on_change.clone());
        let opts = options.clone();
        menu = menu.push(
            row![
                button(text("All").size(12)).padding([2, 6]).style(style::link).on_press(all(opts)),
                button(text("None").size(12)).padding([2, 6]).style(style::link).on_press(none(Vec::new())),
            ]
            .spacing(4),
        );
    }
    menu = menu.push(scrollable(list).height(Length::Shrink));
    let menu = container(menu).padding(8).max_height(320.0).width(Fill).style(style::menu);
    drop_down(anchor, menu, open, Message::CloseDropdown).into()
}

/// Which rows a graph plots: the values to keep for a categorical column, a range in its
/// own units for a numeric one. Both read from and write the whole filter list, since
/// `graph_set_filters` replaces it.
/// How much of a column name fits in a panel row's 78 px label column at caption size.
const LABEL_CHARS: usize = 13;

/// A panel row's label for a column: one line, cut with an ellipsis so a long column name
/// cannot push the row onto two or three lines, with the whole name on the tooltip. The
/// dot marks a column that is being filtered on.
fn col_label<'a>(name: &str, marked: bool) -> El<'a> {
    let budget = if marked { LABEL_CHARS - 2 } else { LABEL_CHARS };
    let shown = crate::view::elide(name, budget);
    let cut = shown != name;
    let shown = if marked { format!("{shown} \u{25cf}") } else { shown };
    let lbl = container(caption(shown).wrapping(text::Wrapping::None)).width(Length::Fixed(78.0)).clip(true);
    if cut {
        tip(lbl, name.to_string())
    } else {
        lbl.into()
    }
}

fn filters_section<'a>(app: &'a App, spec: &'a GraphSpec, t: &'a otto_core::DataTable) -> El<'a> {
    let gid = spec.id.clone();
    let set = move |filters: Vec<Filter>| Message::Cmd(Command::GraphSetFilters(GraphFiltersArgs { graph: Some(gid.clone()), filters }));
    let mut col = Column::new().spacing(10);
    let mut any = false;

    for c in &t.columns {
        let filter = spec.filters.iter().find(|f| f.column == c.name);
        let label = |name: &str| col_label(name, filter.is_some());
        match c.kind() {
            ColumnKind::Numeric => {
                any = true;
                // Typed as text, applied on Enter: a half-typed number is not a range yet.
                let (lo, hi) = (filter.and_then(|f| f.min), filter.and_then(|f| f.max));
                let bound = |draft: Draft, value: Option<f64>, hint: &'static str| {
                    let shown = value.map(otto_core::table::format_number).unwrap_or_default();
                    draft_input(app, draft, shown, hint)
                };
                let (gi, ci) = (spec.id.clone(), c.name.clone());
                col = col.push(
                    row![
                        label(&c.name),
                        bound(Draft::FilterMin(gi.clone(), ci.clone()), lo, "min"),
                        bound(Draft::FilterMax(gi, ci), hi, "max")
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
            ColumnKind::Categorical if c.levels().len() <= 60 => {
                any = true;
                let levels = c.levels();
                let selected = filter.map_or_else(|| levels.clone(), |f| f.include.clone());
                let (all_filters, all_levels, column) = (spec.filters.clone(), levels.clone(), c.name.clone());
                let set = set.clone();
                let picker = multi_picker(
                    app,
                    &format!("filter:{}", c.name),
                    Picker { placeholder: "(no rows)", ordered: false, combined: false, bulk: true },
                    levels.clone(),
                    selected,
                    move |mut include: Vec<String>| {
                        // Keep at least one value: an empty selection would mean "no rows".
                        if include.is_empty() {
                            return Message::Noop;
                        }
                        let mut filters = all_filters.clone();
                        let kept = filters.iter().find(|f| f.column == column).cloned().unwrap_or_default();
                        filters.retain(|f| f.column != column);
                        if include.len() < all_levels.len() {
                            include.sort_by_key(|l| all_levels.iter().position(|x| x == l));
                            filters.push(Filter { column: column.clone(), include, ..kept });
                        }
                        set(filters)
                    },
                );
                col = col.push(row![label(&c.name), picker].spacing(6).align_y(Alignment::Center));
            }
            _ => {}
        }
    }
    if !any {
        return caption("This table has nothing to filter on").into();
    }
    col.into()
}

fn summary_section<'a>(spec: &'a GraphSpec) -> El<'a> {
    let gid = spec.id.clone();
    let g1 = gid.clone();
    let center = pick_list(Center::ALL, Some(spec.summary.center), move |c| {
        Message::Cmd(Command::GraphSetSummary(GraphSummaryArgs { graph: Some(g1.clone()), center: Some(c), ..Default::default() }))
    })
    .text_size(13)
    .width(Fill);
    let g2 = gid.clone();
    let error = pick_list(ErrorBars::ALL, Some(spec.summary.error), move |e| {
        Message::Cmd(Command::GraphSetSummary(GraphSummaryArgs { graph: Some(g2.clone()), error: Some(e), ..Default::default() }))
    })
    .text_size(13)
    .width(Fill);
    if spec.kind == GraphKind::Heatmap {
        return heatmap_section(spec, center);
    }
    let center_label = if spec.kind.has_bars() {
        "Bar height"
    } else if spec.kind == GraphKind::XyLine {
        "Line through"
    } else {
        "Symbol at"
    };
    let individual = spec.summary.replicates == Replicates::Individual;
    let mut col = Column::new().spacing(8);
    if matches!(spec.kind, GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine) {
        let g3 = gid.clone();
        let reps = pick_list(Replicates::ALL, Some(spec.summary.replicates), move |r| {
            Message::Cmd(Command::GraphSetSummary(GraphSummaryArgs { graph: Some(g3.clone()), replicates: Some(r), ..Default::default() }))
        })
        .text_size(13)
        .width(Fill);
        col = col.push(field("Replicates", reps));
    }
    if spec.kind.is_distribution() {
        let g5 = gid.clone();
        let whiskers = pick_list(Whiskers::ALL.to_vec(), Some(spec.whiskers), move |w| {
            Message::Cmd(Command::GraphSetSummary(GraphSummaryArgs { graph: Some(g5.clone()), whiskers: Some(w), ..Default::default() }))
        })
        .text_size(13)
        .width(Fill);
        if spec.kind == GraphKind::Box {
            col = col.push(field("Whiskers", whiskers));
        }
        let g6 = gid.clone();
        let reps = pick_list(Replicates::ALL, Some(spec.summary.replicates), move |r| {
            Message::Cmd(Command::GraphSetSummary(GraphSummaryArgs { graph: Some(g6.clone()), replicates: Some(r), ..Default::default() }))
        })
        .text_size(13)
        .width(Fill);
        col = col.push(field("Points", reps));
        return col
            .push(caption(if spec.kind == GraphKind::Box {
                "The box spans the quartiles with the median across it. Tukey whiskers draw the values beyond them as outliers."
            } else {
                "The violin is a kernel density of the values, with the quartile box and the median inside it."
            }))
            .into();
    }
    if spec.kind.is_stacked() {
        return col
            .push(field("Segment height", center))
            .push(caption("Each data set is one segment, stacked from zero in legend order. Stacked segments leave no room for error bars or individual points."))
            .into();
    }
    if !individual || spec.kind.has_bars() {
        col = col.push(field(center_label, center)).push(field("Error bars", error));
    }
    col.into()
}

/// A heatmap's cell value and colors (colors may change on final graphs too).
fn heatmap_section<'a>(spec: &'a GraphSpec, center: impl Into<El<'a>>) -> El<'a> {
    use otto_core::{HeatNormalize, HeatScale};
    let gid = spec.id.clone();
    let set = move |a: GraphHeatmapArgs| Message::Cmd(Command::GraphSetHeatmap(GraphHeatmapArgs { graph: Some(gid.clone()), ..a }));
    let (s1, s2, s3) = (set.clone(), set.clone(), set);
    let scale = pick_list(HeatScale::ALL, Some(spec.heatmap.scale), move |v| s1(GraphHeatmapArgs { scale: Some(v), ..Default::default() })).text_size(13).width(Fill);
    let norm = pick_list(HeatNormalize::ALL, Some(spec.heatmap.normalize), move |v| s2(GraphHeatmapArgs { normalize: Some(v), ..Default::default() }))
        .text_size(13)
        .width(Fill);
    let mut col = Column::new().spacing(8);
    if !spec.is_final() {
        col = col.push(field("Cell value", center));
    }
    let decimals: Vec<usize> = (0..=6).collect();
    let g7 = spec.id.clone();
    let digits = pick_list(decimals, Some(spec.heatmap.digits()), move |d| {
        Message::Cmd(Command::GraphSetHeatmap(GraphHeatmapArgs { graph: Some(g7.clone()), decimals: Some(d), ..Default::default() }))
    })
    .text_size(13)
    .width(Fill);
    col.push(field("Colors", scale))
        .push(field("Scale", norm))
        .push(field("Decimals", digits))
        .push(
            checkbox(spec.heatmap.show_values)
                .label("Show the values in the cells")
                .size(14)
                .text_size(12.5)
                .on_toggle(move |v| s3(GraphHeatmapArgs { show_values: Some(v), ..Default::default() })),
        )
        .push(caption("Columns: the X categories. Rows: the data sets (Group by, or several Y columns). Each cell is the mean (or median) of its values."))
        .into()
}

/// A volcano plot's cutoffs and which column labels its most significant points.
fn volcano_section<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let gid = spec.id.clone();
    let o = &spec.volcano;
    let set = move |a: GraphVolcanoArgs| Message::Cmd(Command::GraphSetVolcano(GraphVolcanoArgs { graph: Some(gid.clone()), ..a }));
    let (s1, s2, s3) = (set.clone(), set.clone(), set);
    let none = "(no labels)".to_string();
    let mut label_opts = vec![none.clone()];
    if let Some(t) = app.engine.project.table_by_id(&spec.table_id) {
        label_opts.extend(t.columns.iter().map(|c| c.name.clone()));
    }
    let label_sel = Some(o.label.clone().unwrap_or_else(|| none.clone()));
    let n = none.clone();
    let labels = pick_list(label_opts, label_sel, move |c: String| {
        s1(GraphVolcanoArgs { label: Some(if c == n { String::new() } else { c }), ..Default::default() })
    })
    .text_size(13)
    .width(Fill);
    let counts: Vec<usize> = vec![0, 5, 10, 15, 20, 30, 50];
    let top = pick_list(counts, Some(o.label_top), move |v| s2(GraphVolcanoArgs { label_top: Some(v), ..Default::default() })).text_size(13).width(Fill);
    column![
        field(
            "log2 FC cutoff",
            draft_input(app, Draft::VolcanoFc(spec.id.clone()), otto_core::table::format_number(o.fc_cutoff), "1")
        ),
        field("P cutoff", draft_input(app, Draft::VolcanoP(spec.id.clone()), otto_core::table::format_number(o.p_cutoff), "0.05")),
        checkbox(o.y_is_log)
            .label("Y already holds -log10(P)")
            .size(14)
            .text_size(12.5)
            .on_toggle(move |v| s3(GraphVolcanoArgs { y_is_log: Some(v), ..Default::default() })),
        field("Label column", labels),
        field("Label top", top),
        caption("X is the log2 fold change, Y the P value. Points past both cutoffs are coloured up or down; the rest stay grey."),
    ]
    .spacing(8)
    .into()
}

fn format_section<'a>(spec: &'a GraphSpec) -> El<'a> {
    let tile = |f: Format, title: &'static str, sub: &'static str| -> El<'a> {
        let (w, h) = match f {
            Format::Slim => (10.0, 20.0),
            Format::Square => (20.0, 20.0),
            Format::Wide => (30.0, 20.0),
            Format::UltraWide => (40.0, 20.0),
        };
        button(
            column![
                container(space().width(w).height(h)).style(|_| iced::widget::container::Style {
                    border: iced::Border { color: style::TEXT, width: 1.5, radius: 0.0.into() },
                    ..Default::default()
                }),
                text(title).size(12),
                caption(sub).size(10.5)
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        )
        .width(Fill)
        .padding([8, 4])
        .style(style::tile(spec.format == f))
        .on_press(Message::Cmd(Command::GraphSetFormat(GraphFormatArgs { graph: Some(spec.id.clone()), format: f })))
        .into()
    };
    column![
        row![
            tile(Format::Slim, "Slim 1:2", "15 × 30 mm"),
            tile(Format::Square, "Square 1:1", "30 × 30 mm"),
            tile(Format::Wide, "Wide 1.5:1", "45 × 30 mm"),
            tile(Format::UltraWide, "Ultra 2:1", "60 × 30 mm")
        ]
        .spacing(6),
        caption("1 pt lines · Arial Bold 7 pt · vector output"),
    ]
    .spacing(6)
    .into()
}

fn draft_input<'a>(app: &'a App, key: Draft, current: String, placeholder: &'a str) -> El<'a> {
    let value = app.drafts.get(&key).cloned().unwrap_or(current);
    let k2 = key.clone();
    text_input(placeholder, &value)
        .on_input(move |s| Message::Draft(key.clone(), s))
        .on_submit(Message::Commit(k2))
        .size(13)
        .padding([5, 7])
        .width(Fill)
        .into()
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map(otto_core::table::format_number).unwrap_or_default()
}

/// Linear or logarithmic scale for one axis.
fn scale_pick<'a>(spec: &'a GraphSpec, axis: Axis) -> El<'a> {
    let gid = spec.id.clone();
    let current = if axis == Axis::X { spec.x_axis.scale } else { spec.y_axis.scale };
    pick_list(AxisScale::ALL.to_vec(), Some(current), move |v| {
        Message::Cmd(Command::GraphSetAxis(GraphAxisArgs {
            graph: Some(gid.clone()),
            axis,
            title: None,
            min: None,
            max: None,
            auto_range: false,
            scale: Some(v),
            numbers: None,
        }))
    })
    .text_size(13)
    .width(Fill)
    .into()
}

/// How the tick labels of one axis are written (plain digits, scientific notation...).
fn numbers_pick<'a>(spec: &'a GraphSpec, axis: Axis) -> El<'a> {
    let gid = spec.id.clone();
    let current = if axis == Axis::X { spec.x_axis.numbers } else { spec.y_axis.numbers };
    pick_list(AxisNumbers::ALL.to_vec(), Some(current), move |v| {
        Message::Cmd(Command::GraphSetAxis(GraphAxisArgs {
            graph: Some(gid.clone()),
            axis,
            title: None,
            min: None,
            max: None,
            auto_range: false,
            scale: None,
            numbers: Some(v),
        }))
    })
    .text_size(13)
    .width(Fill)
    .into()
}

fn axes_section<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let gid = spec.id.clone();
    let mut col = column![
        field("X title", draft_input(app, Draft::AxisTitle(gid.clone(), Axis::X), spec.x_axis.title.clone().unwrap_or_default(), "automatic (column name)")),
        field("Y title", draft_input(app, Draft::AxisTitle(gid.clone(), Axis::Y), spec.y_axis.title.clone().unwrap_or_default(), "automatic (column name)")),
        field(
            "Y range",
            row![
                draft_input(app, Draft::AxisMin(gid.clone(), Axis::Y), fmt_opt(spec.y_axis.min), "auto"),
                caption("to"),
                draft_input(app, Draft::AxisMax(gid.clone(), Axis::Y), fmt_opt(spec.y_axis.max), "auto"),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        ),
        field("Y scale", scale_pick(spec, Axis::Y)),
        field("Y labels", numbers_pick(spec, Axis::Y)),
    ]
    .spacing(8);
    if spec.kind.is_xy() {
        col = col.push(field("X scale", scale_pick(spec, Axis::X)));
        col = col.push(field("X labels", numbers_pick(spec, Axis::X)));
        col = col.push(field(
            "X range",
            row![
                draft_input(app, Draft::AxisMin(gid.clone(), Axis::X), fmt_opt(spec.x_axis.min), "auto"),
                caption("to"),
                draft_input(app, Draft::AxisMax(gid.clone(), Axis::X), fmt_opt(spec.x_axis.max), "auto"),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ));
    }
    if spec.x_axis.scale.is_log() || spec.y_axis.scale.is_log() {
        col = col.push(caption("A log axis has no zero: values of zero or less are left out, and its limits are given in the data's own units."));
    }
    col.into()
}

fn appearance_section<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let gid = spec.id.clone();
    let legend = checkbox(spec.show_legend)
        .label("Show legend")
        .size(15)
        .text_size(12.5)
        .on_toggle(move |show| Message::Cmd(Command::GraphSetLegend(GraphLegendArgs { graph: Some(gid.clone()), show })));
    let mut col = column![field("Color theme", theme_note(spec))].spacing(8);
    if !matches!(spec.kind, GraphKind::Volcano | GraphKind::Heatmap) {
        let (g2, palette) = (spec.id.clone(), spec.palette);
        let highlight = checkbox(spec.highlight)
            .label("Highlight mode")
            .size(15)
            .text_size(12.5)
            .on_toggle(move |on| {
                Message::Cmd(Command::GraphSetPalette(GraphPaletteArgs { graph: Some(g2.clone()), palette, highlight: Some(on) }))
            });
        col = col.push(field("In focus", focus_pick(app, spec)));
        col = col.push(highlight);
    }
    if spec.kind.has_symbols() {
        col = col.push(field("Symbols", symbol_pick(spec)));
    }
    col.push(legend).into()
}

/// The theme the Colors menu in the toolbar is set to, with a swatch of it.
fn theme_note<'a>(spec: &'a GraphSpec) -> El<'a> {
    row![crate::view::swatch(spec.palette, 9.0, 12.0), text(spec.palette.to_string()).size(13)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

/// The data sets in focus, in the order they take the theme's colors.
fn focus_pick<'a>(app: &'a App, spec: &'a GraphSpec) -> El<'a> {
    let sets = app.preview.as_ref().filter(|p| p.graph_id == spec.id).map(|p| p.sets.clone()).unwrap_or_default();
    let gid = spec.id.clone();
    multi_picker(
        app,
        "focus",
        Picker { placeholder: "(none — every data set keeps its own color)", ordered: true, combined: false, bulk: false },
        sets,
        spec.focus.clone(),
        move |focus| Message::Cmd(Command::GraphSetFocus(GraphFocusArgs { graph: Some(gid.clone()), focus })),
    )
}

/// Circles throughout, or a different shape per data set. Picking one shape for every set
/// is left to `graph_set_symbols`: by hand there is nothing to gain from it.
fn symbol_pick<'a>(spec: &'a GraphSpec) -> El<'a> {
    const CIRCLES: &str = "Circles";
    const VARY: &str = "Different per data set";
    let gid = spec.id.clone();
    let varies = spec.symbols.len() > 1 && spec.symbols.iter().any(|s| *s != spec.symbols[0]);
    let current = if varies { VARY } else { CIRCLES };
    pick_list(vec![CIRCLES, VARY], Some(current), move |choice: &str| {
        let vary = choice == VARY;
        Message::Cmd(Command::GraphSetSymbols(GraphSymbolsArgs { graph: Some(gid.clone()), symbols: Vec::new(), vary }))
    })
    .text_size(13)
    .width(Fill)
    .into()
}

fn export_section<'a>(app: &'a App, gid: &str) -> El<'a> {
    let g = || Some(gid.to_string());
    let btn = |label: &'static str, i: Icon, msg: Message, primary: bool| -> El<'a> {
        button(row![icon(i, 14.0, if primary { iced::Color::WHITE } else { style::TEXT }), text(label).size(12)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([6, 8])
            .style(if primary { style::primary } else { style::secondary })
            .on_press(msg)
            .into()
    };
    let layout = pick_list(DataLayout::ALL, Some(app.data_layout), Message::SetDataLayout).text_size(12.5).width(Fill);
    column![
        caption("Graph"),
        row![
            btn("Copy graph", Icon::Copy, Message::Cmd(Command::GraphCopyVector(GraphRef { graph: g() })), true),
            btn("Export file…", Icon::Export, Message::ExportGraph, false)
        ]
        .spacing(6),
        caption("Plotted data"),
        layout,
        row![
            btn("Copy data", Icon::Copy, Message::Cmd(Command::GraphCopyData(CopyDataArgs { graph: g(), layout: app.data_layout })), false),
            btn("Export CSV…", Icon::Data, Message::ExportData, false)
        ]
        .spacing(6),
        caption("R code"),
        row![
            btn("Copy code", Icon::Copy, Message::Cmd(Command::GraphCopyRCode(GraphRef { graph: g() })), false),
            btn("Export .txt…", Icon::Code, Message::ExportCode, false)
        ]
        .spacing(6),
    ]
    .spacing(6)
    .into()
}
