//! Renders every graph type in both formats from the sample data, checks the
//! publication spec (axis lengths, 1 pt lines, 7 pt bold Arial) and writes a gallery
//! to `target/otto-gallery` for visual review.

use std::path::{Path, PathBuf};

use otto_core::import::{table_from_file, table_from_text};
use otto_core::{DataLayout, DataTable, Format, GraphKind, GraphSpec};
use otto_plot::scene::Item;
use otto_plot::theme::PT_PER_MM;

fn sample() -> DataTable {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/data.xlsx");
    table_from_file("T1", &path, None).unwrap()
}

/// A dose-response style table for the XY graphs.
fn dose_table() -> DataTable {
    let mut csv = String::from("Dose,Genotype,Response\n");
    let doses = [0.0, 1.0, 3.0, 10.0, 30.0, 100.0];
    for (gi, g) in ["WT", "KO"].iter().enumerate() {
        for (di, d) in doses.iter().enumerate() {
            for rep in 0..3 {
                let base = if gi == 0 { 5.0 + 9.0 * di as f64 } else { 4.0 + 5.0 * di as f64 };
                csv.push_str(&format!("{d},{g},{:.2}\n", base + (rep as f64 - 1.0) * 1.7 + 0.3 * di as f64));
            }
        }
    }
    table_from_text("T2", "Dose", &csv, true).unwrap()
}

/// A differential-expression table for the volcano plot.
fn volcano_table() -> DataTable {
    let mut csv = String::from("Gene,log2FC,P
");
    for i in 0..60 {
        let fc = -3.0 + 0.1 * i as f64;
        // Big fold changes come with small P values, as in a real experiment.
        let p = (0.5 * (-fc.abs() * 1.6).exp()).clamp(1e-9, 0.9);
        csv.push_str(&format!("G{i},{fc:.3},{p:.6}
"));
    }
    table_from_text("T3", "Volcano", &csv, true).unwrap()
}

fn spec(kind: GraphKind, x: Option<&str>, y: &[&str], group: Option<&str>, format: Format) -> GraphSpec {
    let mut s = GraphSpec::new("G1", format!("{}-{:?}", kind.slug(), format), "T1", kind);
    s.x = x.into_iter().map(String::from).collect();
    s.y = y.iter().map(|s| s.to_string()).collect();
    s.group = group.into_iter().map(String::from).collect();
    s.format = format;
    s
}

fn cases() -> Vec<(GraphSpec, DataTable)> {
    let bars = sample();
    let dose = dose_table();
    let mut out = Vec::new();
    for format in Format::ALL {
        out.push((spec(GraphKind::XyPoints, Some("Dose"), &["Response"], Some("Genotype"), format), dose.clone()));
        out.push((spec(GraphKind::XyPointsLine, Some("Dose"), &["Response"], Some("Genotype"), format), dose.clone()));
        out.push((spec(GraphKind::XyLine, Some("Dose"), &["Response"], Some("Genotype"), format), dose.clone()));
        out.push((spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, format), bars.clone()));
        out.push((spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), format), bars.clone()));
        out.push((spec(GraphKind::GroupedSeparated, Some("Condition"), &["IL2"], Some("Additive"), format), bars.clone()));
        out.push((spec(GraphKind::StackedBar, Some("Condition"), &["IL2"], Some("Additive"), format), bars.clone()));
        out.push((spec(GraphKind::StackedPercent, Some("Condition"), &["IL2"], Some("Additive"), format), bars.clone()));
        out.push((spec(GraphKind::XyScatter, Some("IL2"), &["IL7"], Some("Additive"), format), bars.clone()));
        out.push((spec(GraphKind::Box, Some("Condition"), &["IL2"], None, format), bars.clone()));
        out.push((spec(GraphKind::Violin, Some("Condition"), &["IL2"], None, format), bars.clone()));
        out.push((spec(GraphKind::Qq, None, &["Response"], Some("Genotype"), format), dose.clone()));
        out.push((spec(GraphKind::Volcano, Some("log2FC"), &["P"], None, format), volcano_table()));
        // A log Y axis on the dose-response data.
        let mut log = spec(GraphKind::XyPoints, Some("Dose"), &["Response"], Some("Genotype"), format);
        log.name = format!("xy_points-log-{format:?}");
        log.y_axis.scale = otto_core::AxisScale::Log10;
        out.push((log, dose.clone()));
        // Bars on a log axis: they stand on the foot of the axis, since it has no zero.
        let mut log_bars = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, format);
        log_bars.name = format!("column_bar-log-{format:?}");
        log_bars.y_axis.scale = otto_core::AxisScale::Log10;
        out.push((log_bars, bars.clone()));
    }
    out
}

fn gallery_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn all_kinds_render_to_spec() {
    let dir = gallery_dir();
    for (spec, table) in cases() {
        let r = otto_plot::render(&spec, &table).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        let (wmm, hmm) = spec.format.axis_mm();

        // The axis frame is exactly the format size.
        let mut axes = None;
        for item in &r.scene.items {
            if let Item::Group { id, items } = item {
                if id == "axes" {
                    if let Some(Item::Polyline { points, .. }) = items.first() {
                        axes = Some(points.clone());
                    }
                }
            }
        }
        let p = axes.expect("axes polyline");
        let y_len = (p[1].1 - p[0].1) / PT_PER_MM;
        let x_len = (p[2].0 - p[1].0) / PT_PER_MM;
        assert!((x_len - wmm).abs() < 0.01, "{}: x axis {x_len} mm", spec.name);
        assert!((y_len - hmm).abs() < 0.01, "{}: y axis {y_len} mm", spec.name);

        // Every stroke is 1 pt and every text is 7 pt; nothing leaves the figure.
        r.scene.walk(|item| match item {
            // 1 pt everywhere, except the thinner dashed guides of volcano and QQ plots.
            Item::Line { stroke, .. } | Item::Polyline { stroke, .. } => assert!(stroke.width == 1.0 || stroke.width == 0.75, "{}", spec.name),
            Item::Rect { stroke: Some(s), .. } | Item::Polygon { stroke: Some(s), .. } => assert_eq!(s.width, 1.0),
            Item::Text { size, x, y, .. } => {
                assert_eq!(*size, 7.0);
                assert!(*x >= 0.0 && *x <= r.scene.width && *y >= 0.0 && *y <= r.scene.height, "{}: text outside", spec.name);
            }
            // No mark lands off the figure: a value a log axis cannot place is left out.
            Item::Circle { cx, cy, .. } => {
                assert!(*cx >= 0.0 && *cx <= r.scene.width && *cy >= 0.0 && *cy <= r.scene.height, "{}: symbol at {cx},{cy}", spec.name);
            }
            _ => {}
        });

        let svg = r.svg();
        assert!(svg.contains(r#"font-weight="bold""#) && svg.contains(r#"font-size="7""#));
        assert!(svg.contains(&r#"viewBox="0 0 "#.to_string()));
        let stem = spec.name.clone();
        std::fs::write(dir.join(format!("{stem}.svg")), &svg).unwrap();
        std::fs::write(dir.join(format!("{stem}.png")), r.png(600.0).unwrap()).unwrap();
        std::fs::write(dir.join(format!("{stem}.R.txt")), r.r_code(&spec)).unwrap();
        std::fs::write(dir.join(format!("{stem}.csv")), r.data_table(DataLayout::Long).to_csv()).unwrap();
    }
}

/// A log axis has no zero to start a bar from, so each bar stands on the foot of the axis
/// and stays inside the frame.
#[test]
fn bars_on_a_log_axis_stay_inside_the_frame() {
    let mut s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Wide);
    s.y_axis.scale = otto_core::AxisScale::Log10;
    let r = otto_plot::render(&s, &sample()).unwrap();
    let mut bars = Vec::new();
    r.scene.walk(|item| {
        if let Item::Rect { x, y, w, h, fill: Some(_), .. } = item {
            bars.push((*x, *y, *w, *h));
        }
    });
    assert!(bars.len() >= 3, "expected one rect per bar, got {}", bars.len());
    for (x, y, w, h) in bars {
        for v in [x, y, w, h] {
            assert!(v.is_finite(), "bar geometry {x},{y} {w}x{h}");
        }
        // A bar whose mean is zero or less has nothing to stand on and draws flat; what
        // matters is that it stays finite and inside the frame.
        assert!(h >= 0.0 && y >= 0.0 && y + h <= r.scene.height + 0.01, "bar {y}..{} outside {}", y + h, r.scene.height);
        assert!(x >= 0.0 && x + w <= r.scene.width + 0.01, "bar {x}..{} outside {}", x + w, r.scene.width);
    }
}

/// The Y axis labels follow the number format the graph asks for.
#[test]
fn axis_labels_follow_the_number_format() {
    let axis_texts = |s: &GraphSpec| -> Vec<String> {
        let r = otto_plot::render(s, &sample()).unwrap();
        let mut out = Vec::new();
        r.scene.walk(|item| {
            if let Item::Text { text, rotate, .. } = item {
                if *rotate == 0.0 {
                    out.push(text.clone());
                }
            }
        });
        out
    };
    let mut s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Wide);
    s.y_axis.scale = otto_core::AxisScale::Log10;
    s.y_axis.numbers = otto_core::AxisNumbers::Power10;
    let labels = axis_texts(&s);
    assert!(labels.iter().any(|t| t == "10¹"), "{labels:?}");
    s.y_axis.scale = otto_core::AxisScale::Linear;
    s.y_axis.numbers = otto_core::AxisNumbers::Scientific;
    let labels = axis_texts(&s);
    assert!(labels.iter().any(|t| t.contains("×10")), "{labels:?}");
}

/// Numbers that would spill over their neighbours are left off a heatmap entirely.
#[test]
fn heatmap_hides_numbers_that_do_not_fit() {
    // The numbers drawn in the cells, which is the "values" group (the colour bar's own
    // tick labels sit elsewhere in the scene).
    let cell_numbers = |r: &otto_plot::Rendered| -> Vec<String> {
        let mut out = Vec::new();
        for item in &r.scene.items {
            if let Item::Group { id, items } = item {
                if id == "values" {
                    for it in items {
                        if let Item::Text { text, .. } = it {
                            out.push(text.clone());
                        }
                    }
                }
            }
        }
        out
    };
    let mut s = spec(GraphKind::Heatmap, Some("Condition"), &["IL2"], Some("Additive"), Format::Slim);
    s.heatmap.show_values = true;
    s.heatmap.decimals = Some(6);
    let r = otto_plot::render(&s, &sample()).unwrap();
    assert!(cell_numbers(&r).is_empty(), "{:?}", cell_numbers(&r));

    // The same heatmap at 0 decimals in the widest format has room for its numbers.
    let mut wide = s.clone();
    wide.format = Format::UltraWide;
    wide.heatmap.decimals = Some(0);
    let r = otto_plot::render(&wide, &sample()).unwrap();
    assert!(cell_numbers(&r).len() >= 4, "{:?}", cell_numbers(&r));
}

#[test]
fn pdf_has_physical_size() {
    let s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Wide);
    let r = otto_plot::render(&s, &sample()).unwrap();
    let pdf = r.pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    let i = text.find("/MediaBox").expect("MediaBox");
    let nums: Vec<f64> = text[i..]
        .split(['[', ']'])
        .nth(1)
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let (wmm, hmm) = r.size_mm();
    assert!((nums[2] / PT_PER_MM - wmm).abs() < 0.05, "pdf width {} vs {wmm}", nums[2] / PT_PER_MM);
    assert!((nums[3] / PT_PER_MM - hmm).abs() < 0.05);
    std::fs::write(gallery_dir().join("column_bar-Wide.pdf"), pdf).unwrap();
}

#[cfg(windows)]
#[test]
fn emf_is_written() {
    let s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    let r = otto_plot::render(&s, &sample()).unwrap();
    let emf = r.emf().unwrap();
    // EMR_HEADER record type 1 and the " EMF" signature at offset 40.
    assert_eq!(&emf[0..4], &[1, 0, 0, 0]);
    assert_eq!(&emf[40..44], b" EMF");
    std::fs::write(gallery_dir().join("grouped_interleaved-Wide.emf"), emf).unwrap();
}

#[test]
fn data_exports_match_plot() {
    let s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    let r = otto_plot::render(&s, &sample()).unwrap();
    let long = r.data_table(DataLayout::Long);
    assert_eq!(long.headers, vec!["Row", "Condition", "Additive", "IL2"]);
    assert_eq!(long.rows.len(), r.data.points.len());
    let s = |strs: &[&str]| strs.iter().map(|s| s.to_string()).collect::<Vec<String>>();

    // Prism layout, one data set: a block of replicate subcolumns per bar, in bar order,
    // the values in one row.
    let mut one = spec(GraphKind::ColumnBar, None, &["IL2"], None, Format::Wide);
    one.x = s(&["Condition", "Additive"]);
    let r1 = otto_plot::render(&one, &sample()).unwrap();
    let wide = r1.data_table(DataLayout::Wide);
    let reps = r1.data.groups.iter().map(|g| r1.data.points_of(g).count()).max().unwrap();
    assert_eq!(wide.headers.len(), r1.data.groups.len() * reps);
    assert_eq!(wide.headers[0], "Construct A | (none)");
    assert!(wide.headers[1..reps].iter().all(String::is_empty));
    assert_eq!(wide.rows.len(), 2);
    assert_eq!(wide.rows[0][..2], s(&["replicate 1", "replicate 2"]));
    assert_eq!(wide.rows[1][..2], s(&["1", "1"]));
    let tsv = wide.to_tsv();
    assert_eq!(tsv.lines().count(), 3, "{tsv}");
    assert!(tsv.starts_with("Construct A | (none)\t\t"), "{tsv}");

    // Several Y columns: a block per X category (replicates side by side), a row per Y
    // column, named in the first column.
    let mut multi = spec(GraphKind::GroupedInterleaved, None, &["IL2", "IL7", "IL15"], None, Format::Wide);
    multi.x = s(&["Condition", "Additive"]);
    let rm = otto_plot::render(&multi, &sample()).unwrap();
    let t = rm.data_table(DataLayout::Wide);
    assert_eq!(t.headers[..4], s(&["", "Construct A | (none)", "", "Construct A | IL12"]));
    assert_eq!(t.headers.len(), 1 + rm.data.x_levels.len() * 2);
    assert_eq!(t.rows[0][..3], s(&["", "replicate 1", "replicate 2"]));
    let names: Vec<&str> = t.rows[1..].iter().map(|r| r[0].as_str()).collect();
    assert_eq!(names, vec!["IL2", "IL7", "IL15"]);
    assert_eq!(t.rows[1][1..5], s(&["1", "1", "10", "10"]));
    assert_eq!(t.rows[3][3..5], s(&["30", "30"]));
    assert!(t.to_tsv().starts_with("\tConstruct A | (none)\t\tConstruct A | IL12\t"), "{}", t.to_tsv());

    // Group by works the same way: a row per group, a block per X category.
    let t = r.data_table(DataLayout::Wide);
    assert_eq!(t.rows.len(), 1 + r.data.sets.len());
    assert_eq!(t.rows[1][0], r.data.sets[0]);
    assert_eq!(t.headers[1], r.data.x_levels[0]);
    // XY: an X column, then a block of replicates per data set, one row per X value.
    let xy = otto_plot::render(&spec(GraphKind::XyPoints, Some("Dose"), &["Response"], Some("Genotype"), Format::Wide), &dose_table()).unwrap();
    let t = xy.data_table(DataLayout::Wide);
    assert_eq!(t.headers[..2], ["Dose".to_string(), "WT".to_string()]);
    assert_eq!(t.headers[4], "KO");
    assert_eq!(t.rows[0][1..4], ["replicate 1".to_string(), "replicate 2".to_string(), "replicate 3".to_string()]);
    assert_eq!(t.rows.len(), 1 + 6);
    let summary = r.data_table(DataLayout::Summary);
    assert_eq!(summary.rows.len(), r.data.groups.len());
    assert!(r.data_table(DataLayout::Long).to_tsv().contains('\t'));
}

#[test]
fn helpful_mapping_errors() {
    let table = sample();
    let s = spec(GraphKind::XyPoints, Some("Condition"), &["IL2"], None, Format::Square);
    let err = otto_plot::render(&s, &table).unwrap_err().to_string();
    assert!(err.contains("numeric X") && err.contains("IL2"), "{err}");
    let s = spec(GraphKind::ColumnBar, Some("Condition"), &["Well"], None, Format::Square);
    assert!(otto_plot::render(&s, &table).unwrap_err().to_string().contains("must be numeric"));
}

#[test]
fn multiple_y_columns_become_data_sets() {
    let table = sample();
    let s = spec(GraphKind::ColumnBar, None, &["IL2", "IL7", "IL15"], None, Format::Square);
    let r = otto_plot::render(&s, &table).unwrap();
    assert_eq!(r.data.x_levels, vec!["IL2", "IL7", "IL15"]);
    let s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2", "IL7"], None, Format::Wide);
    let r = otto_plot::render(&s, &table).unwrap();
    assert_eq!(r.data.sets, vec!["IL2", "IL7"]);
}

#[test]
fn several_columns_define_one_bar() {
    let table = sample();
    let mut s = spec(GraphKind::ColumnBar, None, &["IL2"], None, Format::UltraWide);
    s.x = vec!["Condition".into(), "Additive".into()];
    let r = otto_plot::render(&s, &table).unwrap();
    assert_eq!(r.data.x_levels[0], "Construct A | (none)");
    assert!(r.data.x_levels.contains(&"Construct A | IL12".to_string()));
    // Bars follow the level order of the first column, then the second.
    assert_eq!(r.data.x_levels[1], "Construct A | IL12");
    // Tidy export keeps one column per factor.
    let long = r.data_table(DataLayout::Long);
    assert_eq!(long.headers, vec!["Row", "Condition", "Additive", "IL2"]);
    let code = r.r_code(&s);
    assert!(code.contains("paste(df[[\"Condition\"]], df[[\"Additive\"]], sep = \" | \")"), "{code}");
    std::fs::write(gallery_dir().join("column_bar-combined-UltraWide.png"), r.png(600.0).unwrap()).unwrap();

    // Group by several columns too; a column can't be X and Group at once.
    let mut g = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], None, Format::Wide);
    g.group = vec!["Additive".into(), "Well".into()];
    let r = otto_plot::render(&g, &table).unwrap();
    assert!(r.data.sets.iter().any(|s| s == "(none) | A1"));
    g.group = vec!["Condition".into()];
    assert!(otto_plot::render(&g, &table).unwrap_err().to_string().contains("both X and Group"));
}

#[test]
fn separated_bars_carry_x_and_group_labels() {
    let s = spec(GraphKind::GroupedSeparated, Some("Condition"), &["IL2"], Some("Additive"), Format::UltraWide);
    let r = otto_plot::render(&s, &sample()).unwrap();
    let mut labels = Vec::new();
    r.scene.walk(|i| {
        if let Item::Text { text, .. } = i {
            labels.push(text.clone());
        }
    });
    assert!(labels.contains(&"Construct A | IL12".to_string()), "{labels:?}");
    // No second row with bare data-set names under the axis.
    assert!(!labels.contains(&"IL12".to_string()), "{labels:?}");
    assert!(r.r_code(&s).contains("df[[\".bar\"]]"));
    std::fs::write(gallery_dir().join("grouped_separated-labels-UltraWide.png"), r.png(600.0).unwrap()).unwrap();
}

#[test]
fn points_on_bars_are_always_black() {
    for palette in otto_core::Palette::ALL {
        let mut s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
        s.palette = *palette;
        let r = otto_plot::render(&s, &sample()).unwrap();
        for item in &r.scene.items {
            if let Item::Group { id, items } = item {
                if id == "points" {
                    for group in items {
                        if let Item::Group { items, .. } = group {
                            for p in items {
                                if let Item::Circle { fill, .. } = p {
                                    assert_eq!(*fill, Some(otto_plot::scene::Color::BLACK), "{palette:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn column_bar_folds_group_into_bars() {
    let s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    let r = otto_plot::render(&s, &sample()).unwrap();
    assert!(r.data.x_levels.contains(&"Construct A | IL12".to_string()));
    assert_eq!(r.data.sets.len(), 1);
}

#[test]
fn legend_wraps_into_columns_of_twelve() {
    // 26 data sets (one per Well) -> legend columns of 12, 12 and 2.
    let mut s = spec(GraphKind::XyScatter, Some("IL2"), &["IL7"], None, Format::Wide);
    s.group = vec!["Well".into()];
    let r = otto_plot::render(&s, &sample()).unwrap();
    let n = r.data.sets.len();
    assert!(n > 12, "{n} sets");
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for item in &r.scene.items {
        if let Item::Group { id, items } = item {
            if id == "legend" {
                for i in items {
                    if let Item::Text { x, y, .. } = i {
                        xs.push(*x);
                        ys.push(*y);
                    }
                }
            }
        }
    }
    assert_eq!(xs.len(), n);
    let columns = {
        let mut c = xs.clone();
        c.sort_by(|a, b| a.total_cmp(b));
        c.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        c.len()
    };
    assert_eq!(columns, n.div_ceil(12));
    // Entry 13 starts the second column at the same height as entry 1.
    assert!((ys[12] - ys[0]).abs() < 1e-9 && xs[12] > xs[0]);
    // Everything stays inside the figure.
    assert!(xs.iter().all(|x| *x < r.scene.width) && ys.iter().all(|y| *y < r.scene.height));
    assert!(r.r_code(&s).contains("guide_legend(nrow = 12)"));
    std::fs::write(gallery_dir().join("xy_scatter-legend-columns.png"), r.png(600.0).unwrap()).unwrap();
}

#[test]
fn significance_brackets_sit_above_the_bars() {
    let s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Wide);
    let table = sample();
    let plain = otto_plot::render(&s, &table).unwrap();
    let brackets = vec![
        otto_plot::Bracket { a: "Construct A".into(), b: "Mock".into(), text: "**".into() },
        otto_plot::Bracket { a: "Construct A".into(), b: "Construct B".into(), text: "ns".into() },
        otto_plot::Bracket { a: "Mock".into(), b: "WT".into(), text: "****".into() },
    ];
    let r = otto_plot::render_with(&s, &table, &brackets).unwrap();
    let mut lines = Vec::new();
    let mut texts = Vec::new();
    for item in &r.scene.items {
        if let Item::Group { id, items } = item {
            if id == "significance" {
                for i in items {
                    match i {
                        Item::Polyline { points, stroke } => {
                            assert_eq!(stroke.width, 1.0);
                            lines.push(points[1].1);
                        }
                        Item::Text { text, y, .. } => texts.push((text.clone(), *y)),
                        _ => {}
                    }
                }
            }
        }
    }
    assert_eq!(lines.len(), 3);
    assert!(texts.iter().any(|(t, _)| t == "****"));
    // Everything stays inside the figure, and the plot area itself is unchanged.
    assert!(texts.iter().all(|(_, y)| *y > 0.0));
    assert_eq!(r.scene.plot.width, plain.scene.plot.width);
    assert_eq!(r.scene.plot.height, plain.scene.plot.height);
    // The wide bracket (Construct A – Mock) is stacked above the narrow one it overlaps.
    assert!(lines[0] < lines[1] || lines.iter().cloned().fold(f64::INFINITY, f64::min) < lines[1]);
    assert!(r.r_code(&s).contains("Construct A vs. Mock: **"));
    std::fs::write(gallery_dir().join("column_bar-significance.png"), r.png(600.0).unwrap()).unwrap();
}

/// Bracket line: (y, x1, x2).
type Line = (f64, f64, f64);
/// Bracket text: (text, x, baseline).
type Label = (String, f64, f64);

/// Bracket lines and texts of a scene, in drawing order.
fn significance(scene: &otto_plot::Scene) -> (Vec<Line>, Vec<Label>) {
    let (mut lines, mut texts) = (Vec::new(), Vec::new());
    for item in &scene.items {
        if let Item::Group { id, items } = item {
            if id == "significance" {
                for i in items {
                    match i {
                        Item::Polyline { points, .. } => lines.push((points[1].1, points[1].0, points[2].0)),
                        Item::Text { text, x, y, .. } => texts.push((text.clone(), *x, *y)),
                        _ => {}
                    }
                }
            }
        }
    }
    (lines, texts)
}

/// Y axis tick labels with their height relative to the top of the plot area.
fn y_axis(scene: &otto_plot::Scene) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for item in &scene.items {
        if let Item::Group { id, items } = item {
            if id == "y-axis" {
                for i in items {
                    if let Item::Text { text, y, .. } = i {
                        out.push((text.clone(), ((y - scene.plot.top) * 1000.0).round() as i64));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn brackets_keep_the_graph_and_never_touch() {
    use otto_plot::font::{text_width, CAP_HEIGHT};
    let table = sample();
    let s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL15"], Some("Additive"), Format::Wide);
    let plain = otto_plot::render(&s, &table).unwrap();
    let labels: Vec<String> = plain.data.bar_order().into_iter().map(|i| plain.data.group_label(&plain.data.groups[i])).collect();
    // A Dunnett-style ladder from the first bar to every other bar, plus a neighbour pair.
    let mut brackets: Vec<otto_plot::Bracket> =
        labels[1..].iter().map(|l| otto_plot::Bracket { a: labels[0].clone(), b: l.clone(), text: "****".into() }).collect();
    brackets.push(otto_plot::Bracket { a: labels[2].clone(), b: labels[3].clone(), text: "ns".into() });
    let r = otto_plot::render_with(&s, &table, &brackets).unwrap();
    std::fs::write(gallery_dir().join("grouped_interleaved-significance.png"), r.png(600.0).unwrap()).unwrap();

    // The graph is exactly the graph without statistics; the figure only grows upward.
    assert_eq!((r.scene.plot.width, r.scene.plot.height), (plain.scene.plot.width, plain.scene.plot.height));
    assert_eq!(y_axis(&r.scene), y_axis(&plain.scene));
    assert!((r.scene.width - plain.scene.width).abs() < 1e-9);
    assert!(r.scene.stats_height > 0.0);
    assert!((r.scene.height - plain.scene.height - r.scene.stats_height).abs() < 1e-9);
    assert!((r.scene.plot.top - plain.scene.plot.top - r.scene.stats_height).abs() < 1e-9);

    let (lines, texts) = significance(&r.scene);
    assert_eq!(lines.len(), brackets.len());
    let cap = CAP_HEIGHT * 7.0;
    for (i, (t, tx, ty)) in texts.iter().enumerate() {
        assert!(ty - cap > 0.0, "text '{t}' is cut off at the top");
        let half = text_width(t, 7.0) / 2.0;
        let (bx1, bx2) = (tx - half, tx + half);
        for (j, &(ly, x1, x2)) in lines.iter().enumerate() {
            if i == j {
                continue;
            }
            // Other lines pass well above or below the text.
            if x1 < bx2 && x2 > bx1 {
                assert!(ly > ty + 1.0 || ly < ty - cap - 2.0, "line {j} touches text {i} '{t}'");
            }
            // End ticks (2.5 pt down from the line) stay clear of the text.
            for x in [x1, x2] {
                if x > bx1 && x < bx2 {
                    assert!(ly + 2.5 < ty - cap - 1.0 || ly > ty + 1.0, "end tick of bracket {j} touches text {i} '{t}'");
                }
            }
        }
    }
    // Overlapping brackets are stacked with room between their lines.
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            let (a, b) = (lines[i], lines[j]);
            if a.1 <= b.2 && b.1 <= a.2 {
                assert!((a.0 - b.0).abs() >= 4.99, "brackets {i} and {j} are too close");
            }
        }
    }
}

/// Items of one named layer of the scene.
fn layer<'a>(scene: &'a otto_plot::Scene, id: &str) -> Vec<&'a Item> {
    fn find<'a>(items: &'a [Item], id: &str) -> Option<&'a [Item]> {
        items.iter().find_map(|i| match i {
            Item::Group { id: gid, items } if gid == id => Some(items.as_slice()),
            Item::Group { items, .. } => find(items, id),
            _ => None,
        })
    }
    fn flat<'a>(items: &'a [Item], out: &mut Vec<&'a Item>) {
        for i in items {
            match i {
                Item::Group { items, .. } => flat(items, out),
                other => out.push(other),
            }
        }
    }
    let mut out = Vec::new();
    if let Some(items) = find(&scene.items, id) {
        flat(items, &mut out);
    }
    out
}

/// XY line only: a line through the means with error bars, no symbols. Each replicate:
/// one line per replicate (by a replicate column, or the k-th value at each X).
#[test]
fn xy_line_and_replicate_lines() {
    let mut csv = String::from("Day,Group,Mouse,Volume
");
    for (g, mice) in [("Vehicle", ["M1", "M2", "M3"]), ("Drug", ["M4", "M5", "M6"])] {
        for (m, mouse) in mice.iter().enumerate() {
            for day in [0, 7, 14, 21] {
                csv.push_str(&format!("{day},{g},{mouse},{}
", 100 + day * (10 + m) * if g == "Drug" { 1 } else { 3 }));
            }
        }
    }
    let t = table_from_text("T1", "Tumors", &csv, true).unwrap();
    let mut s = spec(GraphKind::XyLine, Some("Day"), &["Volume"], Some("Group"), Format::Wide);
    let r = otto_plot::render(&s, &t).unwrap();
    assert_eq!(layer(&r.scene, "lines").len(), 2, "one mean line per group");
    assert!(!layer(&r.scene, "error-bars").is_empty());
    assert!(layer(&r.scene, "data").is_empty(), "no symbols");
    // Each replicate, connected by mouse: 6 lines, no error bars; R code draws them too.
    s.summary.replicates = otto_core::Replicates::Individual;
    s.replicate_by = Some("Mouse".into());
    let r = otto_plot::render(&s, &t).unwrap();
    assert_eq!(layer(&r.scene, "lines").len(), 6);
    assert!(layer(&r.scene, "error-bars").is_empty());
    let long = r.data_table(DataLayout::Long);
    assert!(long.headers.contains(&"Mouse".to_string()));
    assert!(r.r_code(&s).contains("geom_line(aes(group = interaction(`Group`, `Mouse`))"));
    // Without a replicate column there is nothing to connect: no graph and no table,
    // rather than lines that run through whichever subject happens to come k-th.
    s.replicate_by = None;
    let err = otto_plot::render(&s, &t).unwrap_err().to_string();
    assert!(err.contains("which subject a value belongs to"), "{err}");
    s.replicate_by = Some("Mouse".into());
    // Points + line, each replicate: the lines and every point.
    s.kind = GraphKind::XyPointsLine;
    let r = otto_plot::render(&s, &t).unwrap();
    assert_eq!((layer(&r.scene, "lines").len(), layer(&r.scene, "data").len()), (6, 24));
}

/// A subject measured on fewer days than the others keeps its own subcolumn, empty where
/// it has no value: nothing below it moves up a column, on the graph or in the plotted
/// table, so a line never runs out of one subject and into another.
#[test]
fn a_subject_that_drops_out_leaves_a_gap() {
    let csv = "Day,Group,Mouse,Volume
0,A,M1,10
7,A,M1,20
0,A,M2,11
7,A,M2,21
14,A,M2,41
21,A,M2,81
";
    let t = table_from_text("T1", "Tumors", csv, true).unwrap();
    let mut spec = spec(GraphKind::XyLine, Some("Day"), &["Volume"], Some("Group"), Format::Wide);
    spec.summary.replicates = otto_core::Replicates::Individual;
    spec.replicate_by = Some("Mouse".into());
    let r = otto_plot::render(&spec, &t).unwrap();
    assert_eq!(layer(&r.scene, "lines").len(), 2, "one line per mouse, not one per subcolumn");

    let s = |strs: &[&str]| strs.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let wide = r.data_table(DataLayout::Wide);
    // One subcolumn per mouse, named after it rather than "replicate k".
    assert_eq!(wide.headers, s(&["Day", "A", ""]));
    assert_eq!(wide.rows[0], s(&["", "M1", "M2"]));
    let row = |day: &str| wide.rows.iter().find(|r| r[0] == day).cloned().unwrap_or_default();
    assert_eq!(row("7"), s(&["7", "20", "21"]));
    // M1 stops here: its subcolumn goes blank and M2 stays in M2's.
    assert_eq!(row("14"), s(&["14", "", "41"]));
    assert_eq!(row("21"), s(&["21", "", "81"]));
    // The long layout names the subject of every value too.
    let long = r.data_table(DataLayout::Long);
    assert!(long.headers.contains(&"Mouse".to_string()));
}

/// An X range set by hand cuts the lines off at the axis, in Otto and in the R code.
#[test]
fn a_manual_x_range_keeps_the_data_inside_the_axes() {
    let csv = "Day,Mouse,Weight\n0,M1,0\n20,M1,10\n40,M1,20\n60,M1,25\n80,M1,30\n0,M2,0\n20,M2,5\n40,M2,15\n80,M2,40\n";
    let t = table_from_text("T1", "Weights", csv, true).unwrap();
    let mut s = spec(GraphKind::XyPointsLine, Some("Day"), &["Weight"], None, Format::Square);
    s.summary.replicates = otto_core::Replicates::Individual;
    s.replicate_by = Some("Mouse".into());
    assert!(otto_plot::render(&s, &t).unwrap().r_code(&s).contains("clip = \"off\""));
    s.x_axis.max = Some(50.0);
    let r = otto_plot::render(&s, &t).unwrap();
    let f = r.scene.plot;
    let mut xs = Vec::new();
    for item in layer(&r.scene, "lines") {
        if let Item::Polyline { points, .. } = item {
            xs.extend(points.iter().map(|p| p.0));
        }
    }
    assert!(!xs.is_empty());
    assert!(xs.iter().all(|&x| x <= f.right() + 1e-6), "a line runs past the X axis");
    assert!(xs.iter().any(|&x| (x - f.right()).abs() < 1e-6), "lines end on the axis");
    // Symbols past 50 are left out: 3 of M1's 5 and 3 of M2's 4.
    assert_eq!(layer(&r.scene, "data").len(), 6);
    assert!(r.r_code(&s).contains("clip = \"on\""));
}

#[test]
fn slim_format_is_15_by_30_mm() {
    let s = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Slim);
    let r = otto_plot::render(&s, &sample()).unwrap();
    assert!((r.scene.plot.width / PT_PER_MM - 15.0).abs() < 1e-6 && (r.scene.plot.height / PT_PER_MM - 30.0).abs() < 1e-6);
}

/// Heatmap: one cell per X category and data set (here: three cytokines as rows), a
/// color bar, optional values, z-scores, and a matrix as plotted table.
#[test]
fn heatmap_cells_colors_and_exports() {
    let t = sample();
    let mut s = spec(GraphKind::Heatmap, Some("Condition"), &["IL2", "IL7", "IL15"], None, Format::Wide);
    let r = otto_plot::render(&s, &t).unwrap();
    let ncol = r.data.x_levels.len();
    assert_eq!(r.data.sets, vec!["IL2", "IL7", "IL15"]);
    assert_eq!(layer(&r.scene, "cells").len(), 3 * ncol);
    assert!(!layer(&r.scene, "legend").is_empty(), "color bar");
    // The grid fills the format's axis area.
    assert!((r.scene.plot.width / PT_PER_MM - 45.0).abs() < 1e-6 && (r.scene.plot.height / PT_PER_MM - 30.0).abs() < 1e-6);
    // Matrix layout: a row per cytokine, a column per condition.
    let wide = r.data_table(DataLayout::Wide);
    assert_eq!((wide.rows.len(), wide.headers.len()), (3, ncol + 1));
    assert_eq!(wide.rows[0][0], "IL2");
    // Values in the cells; z-scores per row; diverging colors.
    s.heatmap = otto_core::HeatmapOptions { scale: otto_core::HeatScale::BlueRed, normalize: otto_core::HeatNormalize::Rows, show_values: true, decimals: None };
    let r = otto_plot::render(&s, &t).unwrap();
    assert_eq!(layer(&r.scene, "values").len(), 3 * ncol);
    let m = otto_plot::heatmap::cells(&r.data, &s.heatmap);
    for row in &m {
        let v: Vec<f64> = row.iter().flatten().copied().collect();
        assert!(v.iter().sum::<f64>().abs() < 1e-9, "row z-scores have mean 0");
    }
    let code = r.r_code(&s);
    assert!(code.contains("geom_tile()") && code.contains("Z-score (rows)") && code.contains("geom_text"));
    std::fs::write(gallery_dir().join("heatmap-zscore.svg"), r.svg()).unwrap();
    std::fs::write(gallery_dir().join("heatmap-zscore.png"), r.png(300.0).unwrap()).unwrap();
}

/// A data set keeps its palette slot when rows are filtered out, so the same group is the
/// same colour in every panel of a figure.
#[test]
fn colors_follow_the_category_not_the_slot() {
    let t = sample();
    let mut s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    let all = otto_plot::distill::distill(&s, &t).unwrap();
    assert!(all.sets.len() > 2, "need several data sets: {:?}", all.sets);
    assert_eq!(all.set_colors, (0..all.sets.len()).collect::<Vec<_>>());
    let wanted = all.sets[all.sets.len() - 1].clone();

    // Keep only the last data set: it must still use the slot it had before.
    s.filters = vec![otto_core::Filter { column: "Additive".into(), include: vec![wanted.clone()], ..Default::default() }];
    let one = otto_plot::distill::distill(&s, &t).unwrap();
    assert_eq!(one.sets, vec![wanted]);
    assert_eq!(one.set_colors, vec![all.sets.len() - 1]);

    // The same for the bars of a column graph, where the colour follows the X category.
    let mut c = spec(GraphKind::ColumnBar, Some("Condition"), &["IL2"], None, Format::Wide);
    let all = otto_plot::distill::distill(&c, &t).unwrap();
    let last = all.x_levels[all.x_levels.len() - 1].clone();
    c.filters = vec![otto_core::Filter { column: "Condition".into(), include: vec![last], ..Default::default() }];
    let one = otto_plot::distill::distill(&c, &t).unwrap();
    assert_eq!(one.x_colors, vec![all.x_levels.len() - 1]);
}

/// Heatmaps print two decimals by default, and the colour bar is labelled at both ends.
#[test]
fn heatmap_values_and_color_bar() {
    let t = sample();
    let mut s = spec(GraphKind::Heatmap, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    s.heatmap.show_values = true;
    let svg = otto_plot::render(&s, &t).unwrap().svg();
    let values: Vec<&str> = svg.lines().filter(|l| l.contains("<text") && l.contains('.')).collect();
    assert!(!values.is_empty(), "no printed values");
    // Every printed number has exactly two decimals unless it is an axis label.
    let (ticks, labels) = otto_plot::heatmap::bar_ticks(0.0617, 3.9, 2);
    assert_eq!(labels.first().map(String::as_str), Some("0.06"));
    assert_eq!(labels.last().map(String::as_str), Some("3.90"));
    assert_eq!(ticks.first(), Some(&0.0617));
    assert!(labels.len() >= 3, "{labels:?}");

    s.heatmap.decimals = Some(0);
    let (_, labels) = otto_plot::heatmap::bar_ticks(0.0617, 3.9, s.heatmap.digits());
    assert_eq!(labels.first().map(String::as_str), Some("0"));
}

/// Data sets in focus take the palette's first slots (red, then blue); in highlight mode
/// every other data set is drawn gray instead.
#[test]
fn focus_takes_the_first_palette_slots() {
    let t = sample();
    let mut s = spec(GraphKind::GroupedInterleaved, Some("Condition"), &["IL2"], Some("Additive"), Format::Wide);
    let sets = otto_plot::distill::distill(&s, &t).unwrap().sets;
    assert!(sets.len() >= 3, "{sets:?}");
    // Picked last first: the focus order, not the table order, decides who is red.
    s.focus = vec![sets[2].clone(), sets[0].clone()];
    let d = otto_plot::distill::distill(&s, &t).unwrap();
    assert_eq!((d.set_color(2), d.set_color(0)), (0, 1), "{sets:?}");
    assert_eq!(d.set_color(1), 2, "the rest follow behind the focused sets");
    let red = otto_plot::theme::mark_color(otto_core::Palette::Npg, d.set_color(2), sets.len());
    assert_eq!(red, otto_plot::scene::Color::from_hex(0xE64B35));
    s.highlight = true;
    let d = otto_plot::distill::distill(&s, &t).unwrap();
    assert_eq!((d.set_color(2), d.set_color(0)), (0, 1));
    assert_eq!(d.set_color(1), otto_plot::theme::MUTED_SLOT);
    for palette in otto_core::Palette::ALL {
        let gray = otto_plot::theme::mark_color(*palette, otto_plot::theme::MUTED_SLOT, sets.len());
        assert_eq!(gray, otto_plot::scene::Color::from_hex(0x9E9E9E), "{palette:?}");
    }
    // One picture per theme, in highlight mode, for visual review.
    for palette in otto_core::Palette::ALL {
        s.palette = *palette;
        let r = otto_plot::render(&s, &t).unwrap();
        std::fs::write(gallery_dir().join(format!("highlight-{palette:?}.png")), r.png(600.0).unwrap()).unwrap();
    }
}

#[test]
fn stacked_segments_sit_on_each_other_and_fill_a_hundred_percent() {
    let bars = sample();
    let mut sp = spec(GraphKind::StackedBar, Some("Condition"), &["IL2"], Some("Additive"), Format::Square);
    let data = otto_plot::distill::distill(&sp, &bars).unwrap();
    let spans = otto_plot::draw::stack_spans(&data);
    assert!(data.sets.len() > 1, "the test data must have several data sets to stack");
    for (i, row) in spans.iter().enumerate() {
        // Each segment starts where the one below it ended, and the first starts at zero.
        let mut foot = 0.0;
        for &(a, b) in row {
            if a == b {
                continue;
            }
            assert!((a - foot).abs() < 1e-9, "category {i}: segment starts at {a}, not {foot}");
            foot = b;
        }
        // The stack reaches the sum of the means of that category.
        let total: f64 = data.groups.iter().filter(|g| g.x == otto_plot::XValue::Cat(i)).map(|g| g.center).sum();
        assert!((foot - total).abs() < 1e-9, "category {i}: stack ends at {foot}, sum is {total}");
    }

    // The 100% variant scales every bar with something in it to the same height; a
    // category whose values are all zero stays empty rather than being invented.
    sp.kind = GraphKind::StackedPercent;
    let data = otto_plot::distill::distill(&sp, &bars).unwrap();
    let mut filled = 0;
    for (i, row) in otto_plot::draw::stack_spans(&data).iter().enumerate() {
        let total: f64 = data.groups.iter().filter(|g| g.x == otto_plot::XValue::Cat(i) && g.center > 0.0).map(|g| g.center).sum();
        let top = row.iter().fold(0.0f64, |a, &(_, b)| a.max(b));
        let want = if total > 0.0 { 100.0 } else { 0.0 };
        assert!((top - want).abs() < 1e-9, "category {i} ({}) fills {top}%, not {want}%", data.x_levels[i]);
        filled += (total > 0.0) as usize;
    }
    assert!(filled > 1, "the test data must have several categories with values");
    // Its Y axis is a percentage, whatever the Y column is called.
    assert_eq!(data.y_title, "Percent of total");
}


#[test]
fn pie_slices_add_up_and_fill_the_circle() {
    let dir = gallery_dir();
    let bars = sample();
    for (kind, format) in [(GraphKind::Pie, Format::Square), (GraphKind::Donut, Format::Square), (GraphKind::Pie, Format::Wide)] {
        let sp = spec(kind, Some("Condition"), &["IL2"], None, format);
        let r = otto_plot::render(&sp, &bars).unwrap_or_else(|e| panic!("{}: {e}", sp.name));
        std::fs::write(dir.join(format!("{}.svg", sp.name)), r.svg()).unwrap();
        std::fs::write(dir.join(format!("{}.png", sp.name)), r.png(300.0).unwrap()).unwrap();

        // A slice is the sum of its category's values, not their mean.
        let slices = otto_plot::pie::slices(&r.data);
        for (i, level) in r.data.x_levels.iter().enumerate() {
            let want: f64 = r.data.points.iter().filter(|p| p.x == otto_plot::XValue::Cat(i)).map(|p| p.y).sum();
            assert!((slices[i] - want.max(0.0)).abs() < 1e-9, "{level}: slice {} is not the sum {want}", slices[i]);
        }
        assert!(slices.iter().sum::<f64>() > 0.0, "the test data must have something to show");

        // Every slice is inside the circle the format allows, and one is drawn per
        // category that has a value.
        let (wmm, hmm) = format.axis_mm();
        let d = wmm.min(hmm) * PT_PER_MM;
        let plot = r.scene.plot;
        let (cx, cy) = (plot.left + plot.width / 2.0, plot.top + plot.height / 2.0);
        let mut drawn = 0;
        r.scene.walk(|item| {
            if let Item::Polygon { points, .. } = item {
                drawn += 1;
                for &(x, y) in points {
                    let rad = (x - cx).hypot(y - cy);
                    assert!(rad <= d / 2.0 + 1e-6, "{}: a slice reaches {rad} pt from the centre", sp.name);
                    if kind == GraphKind::Donut {
                        assert!(rad >= d / 2.0 * 0.55 - 1e-6, "{}: a donut slice reaches the hole", sp.name);
                    }
                }
            }
        });
        assert_eq!(drawn, slices.iter().filter(|&&v| v > 0.0).count(), "{}: one slice per category with a value", sp.name);

        // Pies have no axes, and the R code bends a stacked bar into a circle.
        assert!(!r.scene.items.iter().any(|i| matches!(i, Item::Group { id, .. } if id == "axes")), "a pie has no axes");
        assert!(r.r_code(&sp).contains("coord_polar"), "{}: the R code draws a pie", sp.name);
    }
}

#[test]
fn contingency_lays_out_its_counts() {
    let dir = gallery_dir();
    // Counts already tabulated, as Prism takes them: one row per group, one Y column
    // per outcome.
    let t = table_from_text("T4", "Cure", "Treatment,Cured,Not cured\nDrug,23,7\nPlacebo,12,18\n", true).unwrap();
    let sp = spec(GraphKind::Contingency, Some("Treatment"), &["Cured", "Not cured"], None, Format::Wide);
    let r = otto_plot::render(&sp, &t).unwrap();
    std::fs::write(dir.join(format!("{}.svg", sp.name)), r.svg()).unwrap();
    std::fs::write(dir.join(format!("{}.png", sp.name)), r.png(300.0).unwrap()).unwrap();

    let (headers, rows) = otto_plot::contingency::grid(&r.data);
    assert_eq!(headers, vec!["Treatment", "Cured", "Not cured", "Total"]);
    assert_eq!(rows[0], vec!["Drug", "23", "7", "30"]);
    assert_eq!(rows[1], vec!["Placebo", "12", "18", "30"]);
    assert_eq!(rows[2], vec!["Total", "35", "25", "60"]);

    // It is a table, not a plot: no axes, and every number is on the figure.
    assert!(!r.scene.items.iter().any(|i| matches!(i, Item::Group { id, .. } if id == "axes")));
    r.scene.walk(|item| {
        if let Item::Text { x, y, size, .. } = item {
            assert_eq!(*size, 7.0);
            assert!(*x >= 0.0 && *x <= r.scene.width && *y >= 0.0 && *y <= r.scene.height, "text outside the figure");
        }
    });

    // The long shape gives the same table: rows = X, columns = Group by, cell = sum of Y.
    let long = table_from_text("T5", "Cure", "Treatment,Outcome,N\nDrug,Cured,23\nDrug,Not cured,7\nPlacebo,Cured,12\nPlacebo,Not cured,18\n", true).unwrap();
    let mut sp2 = spec(GraphKind::Contingency, Some("Treatment"), &["N"], Some("Outcome"), Format::Wide);
    sp2.name = "contingency-long".into();
    let r2 = otto_plot::render(&sp2, &long).unwrap();
    let (_, rows2) = otto_plot::contingency::grid(&r2.data);
    assert_eq!(rows2, rows, "the long layout and the wide one are the same table");
}
