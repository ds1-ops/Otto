//! Generates a self-contained R/ggplot2 script that reproduces a graph from its
//! plotted data, with the same fixed panel size, 1 pt lines and 7 pt Arial Bold.

use std::fmt::Write;

use otto_core::{AxisNumbers, AxisScale, Center, DataLayout, ErrorBars, GraphKind, GraphSpec, Replicates, Symbol, Whiskers, COMBINE_SEP};

use crate::data_export;
use crate::distill::PlotData;
use crate::scene::Scene;
use crate::theme::{bar_color, mark_color, volcano_color, Theme, PT_PER_MM};
use crate::ticks::{axis_ticks, target_intervals, AxisTicks};

/// A heatmap: the cell means (or medians) as tiles with the same color scale.
fn heatmap(spec: &GraphSpec, data: &PlotData, scene: &Scene) -> String {
    let opts = spec.heatmap;
    let (wmm, hmm) = (scene.plot.width / PT_PER_MM, scene.plot.height / PT_PER_MM);
    let m = crate::heatmap::cells(data, &opts);
    let mut rows = Vec::new();
    for (r, set) in data.sets.iter().enumerate() {
        for (c, x) in data.x_levels.iter().enumerate() {
            if let Some(v) = m[r][c] {
                rows.push(format!("{},{},{}", csv_field(x), csv_field(set), r_num(v)));
            }
        }
    }
    let (lo, hi) = crate::heatmap::range(&m, opts.scale);
    let colours: Vec<String> = crate::heatmap::stops(opts.scale).iter().map(|&c| crate::scene::Color::from_hex(c).hex()).collect();
    let center = match spec.summary.center {
        Center::Mean => "mean",
        Center::Median => "median",
    };
    let norm = match opts.normalize {
        otto_core::HeatNormalize::None => String::new(),
        otto_core::HeatNormalize::Rows => " Values are z-scores within each row.".into(),
        otto_core::HeatNormalize::Columns => " Values are z-scores within each column.".into(),
    };
    let mut r = String::new();
    let _ = writeln!(r, "# Otto graph \"{}\" (heatmap)", spec.name);
    let _ = writeln!(r, "# Cells: the {center} of the values of each X category and data set.{norm}");
    let _ = writeln!(r, "# Packages: install.packages(c(\"ggplot2\", \"ggh4x\", \"svglite\"))\n");
    let _ = writeln!(r, "library(ggplot2)\nlibrary(ggh4x)\n");
    let _ = writeln!(r, "cells <- read.csv(text = r\"-(\nx,row,value\n{})-\", check.names = FALSE)", rows.join("\n"));
    let _ = writeln!(r, "cells$x <- factor(cells$x, levels = {})", r_vec_str(&data.x_levels));
    // First data set at the top, as in Otto.
    let rev: Vec<String> = data.sets.iter().rev().cloned().collect();
    let _ = writeln!(r, "cells$row <- factor(cells$row, levels = {})", r_vec_str(&rev));
    let _ = writeln!(r, "lw <- 1 / (.pt * 72 / 96)\n");
    let mut chain = vec![
        "ggplot(cells, aes(x = x, y = row, fill = value))".to_string(),
        "geom_tile()".to_string(),
    ];
    if opts.show_values {
        chain.push(format!("geom_text(aes(label = sprintf(\"%.{}f\", value)), size = 7 / .pt, fontface = \"bold\")", opts.digits()));
    }
    chain.push(format!("scale_fill_gradientn(colours = {}, limits = c({}, {}), name = {})", r_vec_str(&colours), r_num(lo), r_num(hi), q(&crate::heatmap::value_title(data, &opts))));
    chain.push("scale_x_discrete(expand = expansion(0))".into());
    chain.push("scale_y_discrete(expand = expansion(0))".into());
    chain.push(format!("labs(x = {}, y = {})", q(&data.x_title), q(&crate::heatmap::row_title(data, spec))));
    chain.push("theme_classic(base_size = 7, base_family = \"Arial\")".into());
    chain.push(format!(
        "theme(\n    text = element_text(face = \"bold\", colour = \"black\"),\n    axis.text = element_text(face = \"bold\", colour = \"black\", size = 7),\n    axis.line = element_blank(),\n    axis.ticks = element_blank(),\n    legend.position = {},\n    plot.background = element_blank()\n  )",
        if spec.show_legend { "\"right\"" } else { "\"none\"" }
    ));
    chain.push(format!("force_panelsizes(rows = unit({}, \"mm\"), cols = unit({}, \"mm\"))", r_num(hmm), r_num(wmm)));
    let _ = writeln!(r, "p <- {}", chain.join(" +\n  "));
    let _ = writeln!(r, "\nprint(p)");
    let _ = writeln!(
        r,
        "ggsave(\"{}.svg\", p, device = svglite::svglite, width = {:.0}, height = {:.0}, units = \"mm\")",
        file_stem(&spec.name),
        (scene.width / PT_PER_MM + 6.0).ceil(),
        (scene.height / PT_PER_MM + 6.0).ceil()
    );
    r
}

/// A pie or donut chart: slices are the sums per X category, drawn as a single stacked
/// bar bent into a circle (ggplot2's own way of making a pie).
fn pie(spec: &GraphSpec, data: &PlotData) -> String {
    let donut = data.kind == GraphKind::Donut;
    let values = crate::pie::slices(data);
    let rows: Vec<String> = data
        .x_levels
        .iter()
        .zip(&values)
        .filter(|(_, &v)| v > 0.0)
        .map(|(x, &v)| format!("{},{}", csv_field(x), r_num(v)))
        .collect();
    let (wmm, _) = spec.format.axis_mm();
    let mut r = String::new();
    let _ = writeln!(r, "# Otto graph \"{}\" ({})", spec.name, data.kind.label());
    let _ = writeln!(r, "# Slices: the sum of {} per {}.", q(&data.y_name), q(&data.x_name));
    let _ = writeln!(r, "# Packages: install.packages(c(\"ggplot2\", \"svglite\"))
");
    let _ = writeln!(r, "library(ggplot2)
");
    let _ = writeln!(r, "slices <- read.csv(text = r\"-(
x,value
{})-\", check.names = FALSE)", rows.join("
"));
    let _ = writeln!(r, "slices$x <- factor(slices$x, levels = {})", r_vec_str(&data.x_levels));
    let _ = writeln!(r, "lw <- 1 / (.pt * 72 / 96)
");
    // x = 1 fills the circle; a donut keeps a hole by starting the bar away from the centre.
    let x_aes = if donut { "2" } else { "1" };
    let mut chain = vec![
        format!("ggplot(slices, aes(x = {x_aes}, y = value, fill = x))"),
        "geom_col(width = 1, colour = \"black\", linewidth = lw)".to_string(),
        // direction = 1 runs clockwise from twelve o'clock, as Otto draws it.
        "coord_polar(theta = \"y\", start = 0, direction = 1)".to_string(),
    ];
    if donut {
        chain.push("xlim(0.5, 2.5)".into());
    }
    chain.push(format!("scale_fill_manual(values = {}, name = NULL)", r_colors(&data.x_levels, |i| bar_color(spec.palette, data.x_color(i)).hex())));
    chain.push("theme_void(base_size = 7, base_family = \"Arial\")".into());
    chain.push(format!(
        "theme(
    text = element_text(face = \"bold\", colour = \"black\"),
    legend.position = {},
    plot.background = element_blank()
  )",
        if spec.show_legend { "\"right\"" } else { "\"none\"" }
    ));
    let _ = writeln!(r, "p <- {}", chain.join(" +
  "));
    let _ = writeln!(r, "
print(p)");
    let _ = writeln!(
        r,
        "ggsave(\"{}.svg\", p, device = svglite::svglite, width = {:.0}, height = {:.0}, units = \"mm\")",
        file_stem(&spec.name),
        wmm + 25.0,
        wmm + 6.0
    );
    r
}

/// A contingency table: builds the matrix of counts and runs the tests on it. There is no
/// plot to reproduce, so the code is the table and its statistics.
fn contingency(spec: &GraphSpec, data: &PlotData) -> String {
    let m = crate::contingency::counts(data);
    let (nr, nc) = (m.len(), data.sets.len().max(1));
    // Column-major, the order matrix() reads.
    let mut flat: Vec<String> = Vec::new();
    for c in 0..nc {
        for row in m.iter().take(nr) {
            flat.push(r_num(row[c].unwrap_or(0.0)));
        }
    }
    let mut r = String::new();
    let _ = writeln!(r, "# Otto graph \"{}\" (contingency table)", spec.name);
    let _ = writeln!(r, "# Cells: the sum of {} for each {} and {}.", q(&data.y_name), q(&data.x_name), q(data.set_name.as_deref().unwrap_or("column")));
    let _ = writeln!(r, "# Packages: none beyond base R.
");
    let _ = writeln!(r, "counts <- matrix(c({}), nrow = {nr}, ncol = {nc})", flat.join(", "));
    let _ = writeln!(r, "dimnames(counts) <- list({}, {})", r_vec_str(&data.x_levels), r_vec_str(&data.sets));
    let _ = writeln!(r, "addmargins(counts)
");
    let _ = writeln!(r, "chisq.test(counts, correct = FALSE)   # Pearson chi-square");
    if nr == 2 && nc == 2 {
        let _ = writeln!(r, "chisq.test(counts, correct = TRUE)    # with Yates' correction");
    }
    let _ = writeln!(r, "fisher.test(counts)");
    r
}

/// A CSV field, quoted when it needs to be.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Quotes a string as an R string literal.
fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Backtick-quotes a column name for use inside aes().
fn bt(s: &str) -> String {
    format!("`{}`", s.replace('`', "'"))
}

fn r_num(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn r_vec_str(items: &[String]) -> String {
    format!("c({})", items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", "))
}

fn r_vec_num(items: &[f64]) -> String {
    format!("c({})", items.iter().map(|&v| r_num(v)).collect::<Vec<_>>().join(", "))
}

/// Named color vector: c("WT" = "#E64B35", ...).
fn r_colors(names: &[String], color: impl Fn(usize) -> String) -> String {
    let pairs: Vec<String> = names.iter().enumerate().map(|(i, n)| format!("{} = {}", q(n), q(&color(i)))).collect();
    format!("c({})", pairs.join(", "))
}

pub fn generate(spec: &GraphSpec, data: &PlotData, scene: &Scene, brackets: &[crate::draw::Bracket]) -> String {
    if data.kind == GraphKind::Heatmap {
        return heatmap(spec, data, scene);
    }
    if data.kind.is_pie() {
        return pie(spec, data);
    }
    if data.kind == GraphKind::Contingency {
        return contingency(spec, data);
    }
    let theme = Theme::default();
    let kind = data.kind;
    let long = data_export::table(data, DataLayout::Long);
    // Categorical X and data sets may combine several columns ("A | B"); the combined
    // factor is rebuilt in R from the separate columns of the long table.
    let mut x_col = if kind.is_xy() { long.headers[1].clone() } else { data.x_name.clone() };
    let set_col = data.set_name.clone();
    let y_col = long.headers.last().cloned().unwrap_or_default();
    let (wmm, hmm) = (scene.plot.width / PT_PER_MM, scene.plot.height / PT_PER_MM);
    let multi = data.sets.len() > 1;
    let legend = scene.items.iter().any(|i| matches!(i, crate::scene::Item::Group { id, .. } if id == "legend"));

    let mut r = String::new();
    let _ = writeln!(r, "# Otto graph \"{}\" ({})", spec.name, kind.label());
    let _ = writeln!(r, "# Reproduces the graph from its plotted data: {wmm:.0} x {hmm:.0} mm axes, 1 pt lines, 7 pt Arial Bold.");
    let _ = writeln!(r, "# Packages: install.packages(c(\"ggplot2\", \"ggbeeswarm\", \"ggh4x\", \"svglite\"))\n");
    let _ = writeln!(r, "library(ggplot2)");
    if (kind.has_bars() && !kind.is_stacked()) || kind.is_distribution() {
        let _ = writeln!(r, "library(ggbeeswarm)");
    }
    let _ = writeln!(r, "library(ggh4x)\n");

    // Data.
    let _ = writeln!(r, "# Plotted data: the rows used in the graph (after filters and missing values).");
    let mut classes: Vec<String> = Vec::new();
    let factor_cols = if kind.is_xy() { &[][..] } else { &data.x_columns[..] };
    for c in factor_cols.iter().chain(&data.set_columns) {
        let entry = format!("{} = \"character\"", q(c));
        if !classes.contains(&entry) {
            classes.push(entry);
        }
    }
    let col_classes = if classes.is_empty() { String::new() } else { format!(",\n               colClasses = c({})", classes.join(", ")) };
    let _ = writeln!(r, "df <- read.csv(text = r\"-(\n{})-\", check.names = FALSE{col_classes})", long.to_csv().trim_end());
    if !kind.is_xy() {
        let _ = writeln!(r, "df[[{}]] <- {}", q(&x_col), factor_expr(&data.x_columns, &data.x_levels));
    }
    if let Some(s) = &set_col {
        let _ = writeln!(r, "df[[{}]] <- {}", q(s), factor_expr(&data.set_columns, &data.sets));
    }

    // Helpers.
    let center_fun = match data.summary.center {
        Center::Mean => "mean",
        Center::Median => "median",
    };
    let err = match data.summary.error {
        ErrorBars::Sd => "m - sd(y), ymax = m + sd(y)",
        ErrorBars::Sem => "m - sd(y) / sqrt(length(y)), ymax = m + sd(y) / sqrt(length(y))",
        ErrorBars::Ci95 => {
            "mean(y) - qt(0.975, length(y) - 1) * sd(y) / sqrt(length(y)),\n             ymax = mean(y) + qt(0.975, length(y) - 1) * sd(y) / sqrt(length(y))"
        }
        ErrorBars::Range => "min(y), ymax = max(y)",
        ErrorBars::None => "m, ymax = m",
    };
    let _ = writeln!(r, "\n# Bar height / symbol = {center_fun}; error bars = {}.", data.summary.error.label());
    let _ = writeln!(r, "otto_summary <- function(y) {{\n  m <- {center_fun}(y)\n  data.frame(y = m, ymin = {err})\n}}");
    let _ = writeln!(r, "lw <- 1 / (.pt * 72 / 96)  # 1 pt line width in ggplot2 units");
    let _ = writeln!(r, "pt_size <- {} / .pt       # {} pt symbol diameter\n", r_num(theme.marker_size), r_num(theme.marker_size));

    let has_err = data.summary.error != ErrorBars::None;
    let aes_set = set_col.as_deref().map(bt);
    let mut layers: Vec<String> = Vec::new();
    let mut scales: Vec<String> = Vec::new();
    let mut facet = None;
    let mut panel_widths = vec![wmm];
    let mut spacing = 0.0;

    // Y range as drawn (from the layout, so log axes carry over).
    let yt = crate::draw::y_axis_ticks(data, spec, &theme);
    let (ylo, yhi) = yt.data_range();
    let yticks = yt.ticks.clone();
    let _ = y_axis_from_scene(data, spec, scene);

    match kind {
        GraphKind::ColumnBar => {
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}))", bt(&x_col), bt(&y_col));
            layers.push(format!(
                "stat_summary(aes(fill = {}), fun = {center_fun}, geom = \"col\", width = {}, colour = \"black\", linewidth = lw)",
                bt(&x_col),
                r_num(theme.bar_frac)
            ));
            layers.push("geom_beeswarm(size = pt_size, shape = 16, cex = 2.5)".into());
            if has_err {
                layers.push(format!(
                    "stat_summary(fun.data = otto_summary, geom = \"errorbar\", width = {}, linewidth = lw)",
                    r_num(theme.bar_frac * theme.cap_frac)
                ));
            }
            scales.push(format!("scale_fill_manual(values = {})", r_colors(&data.x_levels, |i| bar_color(spec.palette, data.x_color(i)).hex())));
        }
        GraphKind::GroupedInterleaved => {
            let set = aes_set.clone().unwrap_or_else(|| bt(&x_col));
            let k = data.sets.len().max(1) as f64;
            let bw = theme.cluster_frac / k;
            let _ = writeln!(r, "dodge <- position_dodge(width = {})", r_num(theme.cluster_frac));
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}, fill = {set}, group = {set}))", bt(&x_col), bt(&y_col));
            layers.push(format!(
                "stat_summary(fun = {center_fun}, geom = \"col\", position = dodge, width = {}, colour = \"black\", linewidth = lw)",
                r_num(theme.cluster_frac)
            ));
            layers.push(format!("geom_beeswarm(dodge.width = {}, size = pt_size, shape = 16, cex = 2.5)", r_num(theme.cluster_frac)));
            if has_err {
                layers.push(format!(
                    "stat_summary(fun.data = otto_summary, geom = \"errorbar\", position = dodge, width = {}, linewidth = lw)",
                    r_num(bw * theme.cap_frac)
                ));
            }
            scales.push(format!("scale_fill_manual(values = {})", r_colors(&data.sets, |i| bar_color(spec.palette, data.set_color(i)).hex())));
        }
        GraphKind::StackedBar | GraphKind::StackedPercent => {
            let set = aes_set.clone().unwrap_or_else(|| bt(&x_col));
            // One segment per X and data set: aggregate first, because a stacking layer
            // cannot summarise its own values.
            let _ = writeln!(r, "# One segment per X category and data set: the {center_fun} of its values.");
            let _ = writeln!(
                r,
                "agg <- aggregate(df[[{}]], by = list({} = df[[{}]], {} = df[[{}]]), FUN = {center_fun})",
                q(&y_col),
                bt(&x_col),
                q(&x_col),
                set,
                q(set_col.as_deref().unwrap_or(&x_col))
            );
            let _ = writeln!(r, "names(agg)[3] <- {}", q(&y_col));
            if kind == GraphKind::StackedPercent {
                let _ = writeln!(
                    r,
                    "agg[[{}]] <- 100 * ave(agg[[{}]], agg[[{}]], FUN = function(v) v / sum(v))  # each bar fills 100%",
                    q(&y_col),
                    q(&y_col),
                    q(&x_col)
                );
            }
            let _ = writeln!(r, "p <- ggplot(agg, aes(x = {}, y = {}, fill = {set}))", bt(&x_col), bt(&y_col));
            // reverse = TRUE puts the first data set at the foot of the bar, as Otto draws it.
            layers.push(format!(
                "geom_col(position = position_stack(reverse = TRUE), width = {}, colour = \"black\", linewidth = lw)",
                r_num(theme.bar_frac)
            ));
            scales.push(format!("scale_fill_manual(values = {})", r_colors(&data.sets, |i| bar_color(spec.palette, data.set_color(i)).hex())));
        }
        GraphKind::GroupedSeparated => {
            let blocks = crate::draw::separated_blocks(data);
            let k = blocks.len().max(1) as f64;
            let total = blocks.iter().map(|b| b.len() as f64).sum::<f64>() + (k - 1.0) * 0.7;
            panel_widths = blocks.iter().map(|b| wmm * b.len() as f64 / total).collect();
            spacing = wmm * 0.7 / total;
            let fill = aes_set.clone().unwrap_or_else(|| bt(&x_col));
            if let Some(s) = &set_col {
                // One label per bar: "X | data set", in block order.
                let bars: Vec<String> = blocks
                    .iter()
                    .enumerate()
                    .flat_map(|(b, levels)| levels.iter().map(move |&i| format!("{}{COMBINE_SEP}{}", data.x_levels[i], data.sets[b])))
                    .collect();
                let _ = writeln!(
                    r,
                    "df[[\".bar\"]] <- factor(paste(df[[{}]], df[[{}]], sep = \"{COMBINE_SEP}\"), levels = {})",
                    q(&x_col),
                    q(s),
                    r_vec_str(&bars)
                );
                x_col = ".bar".to_string();
            }
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}, fill = {fill}))", bt(&x_col), bt(&y_col));
            layers.push(format!(
                "stat_summary(fun = {center_fun}, geom = \"col\", width = 0.8, colour = \"black\", linewidth = lw)"
            ));
            layers.push("geom_beeswarm(size = pt_size, shape = 16, cex = 2.5)".into());
            if has_err {
                layers.push("stat_summary(fun.data = otto_summary, geom = \"errorbar\", width = 0.4, linewidth = lw)".into());
            }
            scales.push(format!("scale_fill_manual(values = {})", r_colors(&data.sets, |i| bar_color(spec.palette, data.set_color(i)).hex())));
            if let Some(s) = &set_col {
                facet = Some(format!("facet_grid(cols = vars({}), scales = \"free_x\", space = \"free_x\")", bt(s)));
            }
        }
        GraphKind::Box | GraphKind::Violin => {
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}, fill = {}))", bt(&x_col), bt(&y_col), bt(&x_col));
            if kind == GraphKind::Box {
                // Otto's whiskers: Tukey (coef 1.5), min-to-max (coef Inf) or the 10-90 percentiles.
                let geom = match spec.whiskers {
                    Whiskers::Tukey => "geom_boxplot(coef = 1.5, width = {w}, linewidth = lw, outlier.size = pt_size)".to_string(),
                    Whiskers::MinMax => "geom_boxplot(coef = Inf, width = {w}, linewidth = lw, outlier.shape = NA)".to_string(),
                    Whiskers::Pct10To90 => {
                        "stat_summary(fun.data = function(y) { q <- quantile(y, c(.1, .25, .5, .75, .9));                          data.frame(ymin = q[1], lower = q[2], middle = q[3], upper = q[4], ymax = q[5]) },                          geom = \"boxplot\", width = {w}, linewidth = lw)"
                            .to_string()
                    }
                };
                layers.push(geom.replace("{w}", &r_num(theme.bar_frac)));
            } else {
                layers.push(format!("geom_violin(width = {}, linewidth = lw, trim = TRUE)", r_num(theme.bar_frac)));
                // A slim black quartile bar with a white median dot, as Otto draws it.
                layers.push(format!(
                    "geom_boxplot(width = {}, linewidth = 0, fill = \"black\", outlier.shape = NA, coef = 0)",
                    r_num(theme.bar_frac * 0.12)
                ));
                layers.push("stat_summary(fun = median, geom = \"point\", color = \"white\", size = pt_size * 0.8)".into());
            }
            if data.summary.replicates == Replicates::Individual {
                layers.push("geom_beeswarm(size = pt_size, shape = 16, cex = 2.5)".into());
            }
            scales.push(format!("scale_fill_manual(values = {})", r_colors(&data.x_levels, |i| bar_color(spec.palette, data.x_color(i)).hex())));
        }
        GraphKind::Volcano => {
            let o = &spec.volcano;
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}, colour = {}))", bt(&x_col), bt(&y_col), bt(set_col.as_deref().unwrap_or("Change")));
            layers.push(format!(
                "geom_hline(yintercept = {}, linetype = \"dashed\", linewidth = lw * 0.75, colour = \"grey50\")",
                r_num(-o.p_cutoff.log10())
            ));
            layers.push(format!(
                "geom_vline(xintercept = c({}, {}), linetype = \"dashed\", linewidth = lw * 0.75, colour = \"grey50\")",
                r_num(-o.fc_cutoff),
                r_num(o.fc_cutoff)
            ));
            layers.push("geom_point(size = pt_size, shape = 16)".into());
            scales.push(format!("scale_colour_manual(values = {})", r_colors(&data.sets, |i| volcano_color(spec.palette, data.set_color(i)).hex())));
            let (xlo, xhi) = x_range_from_data(data, spec, scene);
            let xt = axis_ticks(xlo, xhi, target_intervals(scene.plot.width), Some(xlo), Some(xhi));
            scales.push(continuous_scale("x", &xt.ticks, spec.x_axis.scale, &r_labels(&xt, spec.x_axis.numbers)));
        }
        GraphKind::Qq => {
            let colour = if multi { format!(", colour = {s}", s = aes_set.clone().unwrap_or_default()) } else { String::new() };
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}{colour}))", bt(&x_col), bt(&y_col));
            let fixed = if multi { "" } else { ", colour = \"black\"" };
            // The reference line is the robust quartile line Otto draws.
            let _ = writeln!(r, "# The line runs through the first and third quartiles of each data set.");
            layers.push(format!("stat_qq_line(aes(sample = {}), linewidth = lw * 0.75{fixed})", bt(&y_col)));
            layers.push(format!("geom_point(size = pt_size, shape = 16{fixed})"));
            if multi {
                scales.push(format!(
                    "scale_colour_manual(values = {})",
                    r_colors(&data.sets, |i| mark_color(spec.palette, data.set_color(i), data.sets.len()).hex())
                ));
            }
            let (xlo, xhi) = x_range_from_data(data, spec, scene);
            let xt = axis_ticks(xlo, xhi, target_intervals(scene.plot.width), Some(xlo), Some(xhi));
            scales.push(continuous_scale("x", &xt.ticks, AxisScale::Linear, &r_labels(&xt, spec.x_axis.numbers)));
        }
        // Heatmaps have their own script (see `heatmap` above).
        GraphKind::Heatmap => unreachable!("heatmaps are written by rcode::heatmap"),
        GraphKind::Pie | GraphKind::Donut => unreachable!("pies are written by rcode::pie"),
        GraphKind::Contingency => unreachable!("contingency tables are written by rcode::contingency"),
        GraphKind::Survival => {
            // Kaplan-Meier curves from R's survival package, drawn as steps with censor ticks.
            let strata = match &aes_set {
                Some(s) => format!("~ {s}"),
                None => "~ 1".to_string(),
            };
            let _ = writeln!(r, "\nlibrary(survival)");
            let _ = writeln!(r, "fit <- survfit(Surv({}, {}) {strata}, data = df)", bt(&x_col), bt(&y_col));
            let _ = writeln!(r, "# Curves as steps starting at 100% at time 0; censored times marked.");
            let _ = writeln!(
                r,
                "strata <- if (is.null(fit$strata)) rep({}, length(fit$time)) else sub(\"^[^=]*=\", \"\", rep(names(fit$strata), fit$strata))",
                q(data.sets.first().map(String::as_str).unwrap_or("all"))
            );
            let _ = writeln!(r, "km <- data.frame(time = fit$time, surv = 100 * fit$surv, group = strata, censored = fit$n.censor > 0)");
            let _ = writeln!(r, "km <- rbind(data.frame(time = 0, surv = 100, group = unique(strata), censored = FALSE), km)");
            let colour = if multi { ", colour = group" } else { "" };
            let _ = writeln!(r, "p <- ggplot(km, aes(x = time, y = surv, group = group{colour}))");
            let fixed = if multi { "" } else { ", colour = \"black\"" };
            layers.push(format!("geom_step(linewidth = lw{fixed})"));
            layers.push(format!("geom_point(data = subset(km, censored), shape = 124, size = pt_size * 1.5{fixed})"));
            if multi {
                scales.push(format!(
                    "scale_colour_manual(values = {})",
                    r_colors(&data.sets, |i| mark_color(spec.palette, data.set_color(i), data.sets.len()).hex())
                ));
            }
            let (xlo, xhi) = x_range_from_data(data, spec, scene);
            let xt = axis_ticks(xlo, xhi, target_intervals(scene.plot.width), Some(xlo), Some(xhi));
            scales.push(format!("scale_x_continuous(breaks = {}, expand = expansion(0))", r_vec_num(&xt.ticks)));
        }
        GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::XyScatter => {
            let colour = if multi { format!(", colour = {s}, group = {s}", s = aes_set.clone().unwrap_or_default()) } else { String::new() };
            let _ = writeln!(r, "p <- ggplot(df, aes(x = {}, y = {}{colour}))", bt(&x_col), bt(&y_col));
            let fixed = if multi { "" } else { ", colour = \"black\"" };
            let (xlo, xhi) = x_range_from_data(data, spec, scene);
            let cap_data = theme.xy_cap / scene.plot.width * (xhi - xlo);
            let individual = kind == GraphKind::XyScatter || data.summary.replicates == Replicates::Individual;
            if kind.has_line() {
                if individual {
                    // One line per replicate (the replicate column of the data).
                    let rep = bt(&data.replicate_name.clone().unwrap_or_else(|| "Replicate".into()));
                    let group = match &aes_set {
                        Some(s) => format!("interaction({s}, {rep})"),
                        None => rep,
                    };
                    layers.push(format!("geom_line(aes(group = {group}), linewidth = lw{fixed})"));
                } else {
                    layers.push(format!("stat_summary(fun = {center_fun}, geom = \"line\", linewidth = lw{fixed})"));
                }
            }
            let shape = r_shape(spec.symbol(0));
            if individual {
                if kind != GraphKind::XyLine {
                    layers.push(format!("geom_point(size = pt_size, shape = {shape}{fixed})"));
                }
            } else if kind == GraphKind::XyLine {
                if has_err {
                    layers.push(format!(
                        "stat_summary(fun.data = otto_summary, geom = \"errorbar\", width = {}, linewidth = lw{fixed})",
                        r_num(cap_data)
                    ));
                }
            } else {
                if has_err {
                    layers.push(format!(
                        "stat_summary(fun.data = otto_summary, geom = \"errorbar\", width = {}, linewidth = lw{fixed})",
                        r_num(cap_data)
                    ));
                }
                layers.push(format!("stat_summary(fun = {center_fun}, geom = \"point\", size = pt_size, shape = {shape}{fixed})"));
            }
            if multi {
                scales.push(format!(
                    "scale_colour_manual(values = {})",
                    r_colors(&data.sets, |i| mark_color(spec.palette, data.set_color(i), data.sets.len()).hex())
                ));
            }
            let xt = crate::draw::x_axis_ticks(data, spec, &theme).unwrap_or_else(|| axis_ticks(xlo, xhi, target_intervals(scene.plot.width), Some(xlo), Some(xhi)));
            scales.push(continuous_scale("x", &xt.ticks, spec.x_axis.scale, &r_labels(&xt, spec.x_axis.numbers)));
        }
    }

    if !kind.is_xy() {
        scales.push("scale_x_discrete(expand = expansion(add = 0.5))".into());
    }
    scales.push(continuous_scale("y", &yticks, spec.y_axis.scale, &r_labels(&yt, spec.y_axis.numbers)));
    let xlim = if kind.is_xy() {
        let (xlo, xhi) = x_range_from_data(data, spec, scene);
        format!("xlim = c({}, {}), ", r_num(xlo), r_num(xhi))
    } else {
        String::new()
    };

    let mut chain = vec!["p".to_string()];
    chain.extend(layers);
    chain.extend(scales);
    if let Some(f) = facet {
        chain.push(f);
    }
    // Clipping is off so symbols on the axes are drawn whole, as in Otto. With a range set
    // by hand, data past it must stop at the axes, so clipping is on.
    let manual = |a: &otto_core::AxisSpec| a.min.is_some() || a.max.is_some();
    let clip = if manual(&spec.y_axis) || (kind.is_xy() && manual(&spec.x_axis)) { "on" } else { "off" };
    chain.push(format!("coord_cartesian({xlim}ylim = c({}, {}), clip = \"{clip}\")", r_num(ylo), r_num(yhi)));
    if legend && data.sets.len() > crate::draw::LEGEND_ROWS {
        // Same as Otto: 12 entries per legend column, further columns to the right.
        let aes = if kind.has_bars() { "fill" } else { "colour" };
        chain.push(format!("guides({aes} = guide_legend(nrow = {}))", crate::draw::LEGEND_ROWS));
    }
    chain.push(format!("labs(x = {}, y = {})", q(&data.x_title), q(&data.y_title)));
    chain.push("theme_classic(base_size = 7, base_family = \"Arial\")".into());
    let rotate = scene_has_rotated_x(scene);
    let mut theme_args = vec![
        "text = element_text(face = \"bold\", colour = \"black\")".to_string(),
        "axis.text = element_text(face = \"bold\", colour = \"black\", size = 7)".to_string(),
        "axis.title = element_text(face = \"bold\", size = 7)".to_string(),
        "axis.line = element_line(linewidth = lw, colour = \"black\", lineend = \"square\")".to_string(),
        "axis.ticks = element_line(linewidth = lw, colour = \"black\")".to_string(),
        format!("axis.ticks.length = unit({}, \"pt\")", r_num(theme.tick_len)),
        format!("legend.position = {}", if legend { "\"right\"" } else { "\"none\"" }),
        "legend.title = element_blank()".to_string(),
        "legend.key.size = unit(6, \"pt\")".to_string(),
        "legend.text = element_text(face = \"bold\", size = 7)".to_string(),
        "plot.background = element_blank()".to_string(),
        "panel.background = element_blank()".to_string(),
    ];
    if rotate {
        theme_args.push("axis.text.x = element_text(angle = 45, hjust = 1, vjust = 1)".into());
    }
    if kind == GraphKind::GroupedSeparated {
        theme_args.push("strip.background = element_blank()".into());
        theme_args.push("strip.text = element_blank()".into());
        theme_args.push(format!("panel.spacing = unit({}, \"mm\")", r_num(spacing)));
    }
    chain.push(format!("theme(\n    {}\n  )", theme_args.join(",\n    ")));
    chain.push(format!(
        "force_panelsizes(rows = unit({}, \"mm\"), cols = unit({}, \"mm\"))",
        r_num(hmm),
        if panel_widths.len() == 1 { r_num(panel_widths[0]) } else { r_vec_num(&panel_widths) }
    ));
    let _ = writeln!(r, "p <- {}", chain.join(" +\n  "));
    if !brackets.is_empty() {
        // Otto draws these brackets itself; in R they can be added with e.g. ggsignif::geom_signif().
        let _ = writeln!(r, "\n# Significance shown on the Otto graph:");
        for b in brackets {
            let _ = writeln!(r, "#   {} vs. {}: {}", b.a, b.b, b.text);
        }
    }
    let _ = writeln!(r, "\nprint(p)");
    let fig_w = scene.width / PT_PER_MM + 6.0;
    let fig_h = scene.height / PT_PER_MM + 6.0;
    let _ = writeln!(
        r,
        "ggsave(\"{}.svg\", p, device = svglite::svglite, width = {:.0}, height = {:.0}, units = \"mm\")",
        file_stem(&spec.name),
        fig_w.ceil(),
        fig_h.ceil()
    );
    let _ = writeln!(
        r,
        "# ggsave(\"{}.pdf\", p, device = cairo_pdf, width = {:.0}, height = {:.0}, units = \"mm\")",
        file_stem(&spec.name),
        fig_w.ceil(),
        fig_h.ceil()
    );
    r
}

fn file_stem(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() {
        "graph".into()
    } else {
        s
    }
}

/// Recovers the drawn Y range and ticks from the scene's tick labels.
fn y_axis_from_scene(data: &PlotData, spec: &GraphSpec, scene: &Scene) -> (f64, f64, Vec<f64>) {
    let mut ticks: Vec<(f64, f64)> = Vec::new(); // (pixel y, value)
    for item in &scene.items {
        if let crate::scene::Item::Group { id, items } = item {
            if id == "y-axis" {
                let lines: Vec<f64> = items
                    .iter()
                    .filter_map(|i| if let crate::scene::Item::Line { y1, .. } = i { Some(*y1) } else { None })
                    .collect();
                let labels: Vec<f64> = items
                    .iter()
                    .filter_map(|i| match i {
                        crate::scene::Item::Text { text, rotate, .. } if *rotate == 0.0 => text.parse().ok(),
                        _ => None,
                    })
                    .collect();
                ticks = lines.into_iter().zip(labels).collect();
            }
        }
    }
    if ticks.len() >= 2 {
        let (p0, v0) = ticks[0];
        let (p1, v1) = ticks[ticks.len() - 1];
        let per_px = (v1 - v0) / (p1 - p0);
        let lo = v0 + (scene.plot.bottom() - p0) * per_px;
        let hi = v0 + (scene.plot.top - p0) * per_px;
        let round = |v: f64| (v * 1e9).round() / 1e9;
        return (round(lo), round(hi), ticks.iter().map(|t| t.1).collect());
    }
    let _ = (data, spec);
    (0.0, 1.0, vec![0.0, 1.0])
}

fn x_range_from_data(data: &PlotData, spec: &GraphSpec, scene: &Scene) -> (f64, f64) {
    let _ = scene;
    crate::draw::x_axis_range(data, spec, &Theme::default()).unwrap_or((0.0, 1.0))
}

/// `scale_*_continuous()` for an axis, honouring a log scale.
/// `labels` is empty unless the axis writes its numbers in some other way than plain
/// digits, in which case the labels Otto draws are spelled out for ggplot2.
fn continuous_scale(axis: &str, ticks: &[f64], scale: AxisScale, labels: &[String]) -> String {
    let trans = match scale {
        AxisScale::Linear => String::new(),
        AxisScale::Log10 => ", transform = \"log10\"".to_string(),
        AxisScale::Log2 => ", transform = \"log2\"".to_string(),
        AxisScale::Ln => ", transform = \"log\"".to_string(),
    };
    let labels = if labels.is_empty() { String::new() } else { format!(", labels = {}", r_vec_str(labels)) };
    format!("scale_{axis}_continuous(breaks = {}{labels}{trans}, expand = expansion(0))", r_vec_num(ticks))
}

/// The tick labels to spell out in the R script: none while they are plain numbers,
/// which ggplot2 writes the same way by itself.
fn r_labels(t: &AxisTicks, numbers: AxisNumbers) -> Vec<String> {
    if numbers == AxisNumbers::Plain {
        Vec::new()
    } else {
        t.clone().numbered(numbers).labels()
    }
}

/// ggplot2's `shape` code for an Otto symbol (all of them filled).
fn r_shape(sym: Symbol) -> u8 {
    match sym {
        Symbol::Circle => 16,
        Symbol::Square => 15,
        Symbol::TriangleUp => 17,
        Symbol::TriangleDown => 25,
        Symbol::Diamond => 18,
        Symbol::Hexagon => 19,
    }
}

fn scene_has_rotated_x(scene: &Scene) -> bool {
    scene.items.iter().any(|i| match i {
        crate::scene::Item::Group { id, items } if id == "x-axis" => {
            items.iter().any(|t| matches!(t, crate::scene::Item::Text { rotate, .. } if *rotate == -45.0))
        }
        _ => false,
    })
}

/// `factor(...)` for one column, or for several columns pasted together as "A | B".
fn factor_expr(columns: &[String], levels: &[String]) -> String {
    let source = match columns {
        [one] => format!("df[[{}]]", q(one)),
        many => format!(
            "paste({}, sep = \"{COMBINE_SEP}\")",
            many.iter().map(|c| format!("df[[{}]]", q(c))).collect::<Vec<_>>().join(", ")
        ),
    };
    format!("factor({source}, levels = {})", r_vec_str(levels))
}
