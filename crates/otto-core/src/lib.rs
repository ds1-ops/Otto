//! Otto core data model: multivariable tables, graph and analysis specs, import and project files.

pub mod analysis;
pub mod import;
pub mod project;
pub mod spec;
pub mod table;

pub use project::Project;
pub use spec::*;
pub use table::{Column, ColumnData, ColumnKind, ColumnType, DataTable, NumberFormat};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("'{value}' is not a number (column '{column}')")]
    NotANumber { column: String, value: String },
    #[error("column '{0}' is not categorical")]
    NotCategorical(String),
    #[error("import failed: {0}")]
    Import(String),
    #[error("{0}")]
    Invalid(String),
    #[error("file error: {0}")]
    Io(String),
}
