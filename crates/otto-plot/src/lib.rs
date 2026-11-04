//! Otto plot engine.
//!
//! `GraphSpec` + `DataTable` → [`distill`] (plotted subset) → [`draw`] (Prism-style scene)
//! → SVG / PDF / PNG / EMF, plus plotted-data tables and an equivalent R/ggplot2 script.

pub mod contingency;
pub mod data_export;
pub mod describe;
pub mod distill;
pub mod draw;
#[cfg(windows)]
pub mod emf;
pub mod font;
pub mod heatmap;
pub mod pie;
pub mod raster;
pub mod rcode;
pub mod scene;
pub mod survival;
pub mod svg;
pub mod theme;
pub mod ticks;

use otto_core::{DataLayout, DataTable, GraphSpec};

pub use data_export::TextTable;
pub use distill::{PlotData, XValue};
pub use draw::{Bracket, Curve};
pub use scene::Scene;
pub use svg::SvgOptions;
pub use theme::Theme;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlotError {
    /// The X/Y/Group choice does not fit the graph type.
    #[error("{0}")]
    Mapping(String),
    #[error("{0}")]
    NoData(String),
    #[error("render failed: {0}")]
    Render(String),
}

/// A rendered graph: the vector scene and the exact data it shows.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub scene: Scene,
    pub data: PlotData,
    /// Significance brackets drawn on the graph.
    pub brackets: Vec<Bracket>,
    /// Fitted curves and regression lines drawn on the graph.
    pub curves: Vec<Curve>,
}

impl Rendered {
    pub fn svg(&self) -> String {
        svg::to_svg(&self.scene, SvgOptions::default())
    }

    pub fn svg_with_background(&self) -> String {
        svg::to_svg(&self.scene, SvgOptions { background: true })
    }

    /// Size of the whole figure in millimetres.
    pub fn size_mm(&self) -> (f64, f64) {
        (self.scene.width / theme::PT_PER_MM, self.scene.height / theme::PT_PER_MM)
    }

    pub fn png(&self, dpi: f32) -> Result<Vec<u8>, PlotError> {
        raster::svg_to_png(&self.svg(), dpi)
    }

    pub fn pdf(&self) -> Result<Vec<u8>, PlotError> {
        raster::svg_to_pdf(&self.svg())
    }

    pub fn svg_outlined(&self) -> Result<String, PlotError> {
        let (w, h) = self.size_mm();
        raster::outline_text(&self.svg(), w, h)
    }

    #[cfg(windows)]
    pub fn emf(&self) -> Result<Vec<u8>, PlotError> {
        emf::to_emf(&self.scene)
    }

    #[cfg(not(windows))]
    pub fn emf(&self) -> Result<Vec<u8>, PlotError> {
        Err(PlotError::Render("EMF export is only available on Windows".into()))
    }

    pub fn data_table(&self, layout: DataLayout) -> TextTable {
        data_export::table(&self.data, layout)
    }

    pub fn r_code(&self, spec: &GraphSpec) -> String {
        rcode::generate(spec, &self.data, &self.scene, &self.brackets)
    }
}

/// Distills and draws a graph with the default Prism theme.
pub fn render(spec: &GraphSpec, table: &DataTable) -> Result<Rendered, PlotError> {
    render_with(spec, table, &[])
}

/// Like [`render`], with significance brackets between bars (matched by bar label).
pub fn render_with(spec: &GraphSpec, table: &DataTable, brackets: &[Bracket]) -> Result<Rendered, PlotError> {
    render_full(spec, table, brackets, &[])
}

/// Like [`render_with`], with fitted curves on XY graphs (matched by data set label).
pub fn render_full(spec: &GraphSpec, table: &DataTable, brackets: &[Bracket], curves: &[Curve]) -> Result<Rendered, PlotError> {
    let data = distill::distill(spec, table)?;
    let scene = draw::draw_full(&data, spec, &Theme::default(), brackets, curves);
    Ok(Rendered { scene, data, brackets: brackets.to_vec(), curves: curves.to_vec() })
}
