//! Importing multivariable tables from CSV/TSV text and Excel workbooks,
//! with delimiter sniffing and column type inference.

use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::spec::simple_display;

use crate::table::{is_missing_token, parse_number, Column, DataTable};
use crate::CoreError;

/// Level used for blank cells in categorical columns, so that e.g. rows without
/// an additive form their own group. It can be renamed like any other level.
pub const BLANK_LEVEL: &str = "(none)";

/// A raw cell before type inference.
#[derive(Clone, Debug, PartialEq)]
pub enum RawCell {
    Empty,
    Num(f64),
    Str(String),
}

impl RawCell {
    fn as_text(&self) -> String {
        match self {
            RawCell::Empty => String::new(),
            RawCell::Num(v) => crate::table::format_number(*v),
            RawCell::Str(s) => s.trim().to_string(),
        }
    }

    fn is_blank(&self) -> bool {
        match self {
            RawCell::Empty => true,
            RawCell::Num(_) => false,
            RawCell::Str(s) => s.trim().is_empty(),
        }
    }
}

/// How the delimited text was interpreted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFormat {
    pub delimiter: u8,
    pub decimal_comma: bool,
}

/// What a leading row of the file is. Rows past the end of [`ImportOptions::rows`] are data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RowRole {
    /// Part of the column names. Several name rows join with " | ", so a file whose header
    /// is spread over two rows ("Treatment" above "Dose") becomes "Treatment | Dose".
    Name,
    #[default]
    Data,
    /// Left out: a title line, a units row, a note above the table.
    Skip,
}

simple_display!(RowRole { Name => "Name row", Data => "Data row", Skip => "Skip" });

/// The field separator of delimited text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Separator {
    /// Worked out from the first lines.
    #[default]
    Auto,
    Tab,
    Comma,
    Semicolon,
    Space,
    /// Any other single character, e.g. '|'.
    Other(char),
}

impl Separator {
    pub const ALL: [Separator; 5] = [Separator::Auto, Separator::Tab, Separator::Comma, Separator::Semicolon, Separator::Space];

    pub fn label(self) -> String {
        match self {
            Separator::Auto => "Detect".into(),
            Separator::Tab => "Tab".into(),
            Separator::Comma => "Comma".into(),
            Separator::Semicolon => "Semicolon".into(),
            Separator::Space => "Space".into(),
            Separator::Other(c) => format!("Other ({c})"),
        }
    }

    /// The byte to split on, or `None` to work it out from the text.
    pub fn byte(self) -> Option<u8> {
        match self {
            Separator::Auto => None,
            Separator::Tab => Some(b'\t'),
            Separator::Comma => Some(b','),
            Separator::Semicolon => Some(b';'),
            Separator::Space => Some(b' '),
            // The csv reader splits on one byte, so a multi-byte character cannot be one.
            Separator::Other(c) => (c.len_utf8() == 1).then_some(c as u8),
        }
    }
}

/// Which mark separates the decimals of a number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Decimal {
    /// Worked out from the text: a comma between digits where the separator is not a comma.
    #[default]
    Auto,
    /// 1.25
    Dot,
    /// 1,25 — the European convention.
    Comma,
}

simple_display!(Decimal { Auto => "Detect", Dot => "Point (1.25)", Comma => "Comma (1,25)" });

/// How the bytes of a text file are read. Workbooks carry their own encoding, so this
/// applies to .csv / .tsv / .txt only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// From the byte order mark if there is one, else UTF-8, falling back to Windows-1252
    /// for bytes that are not valid UTF-8 (which is what a Latin-1 export looks like).
    #[default]
    Auto,
    Utf8,
    /// Western European (Windows-1252), what Excel writes as "CSV" on a Western install.
    Windows1252,
    Utf16Le,
    Utf16Be,
}

simple_display!(Encoding {
    Auto => "Detect",
    Utf8 => "UTF-8",
    Windows1252 => "Western European (Windows-1252)",
    Utf16Le => "UTF-16 LE",
    Utf16Be => "UTF-16 BE",
});

impl Encoding {
    /// Decodes the bytes, dropping a leading byte order mark.
    pub fn decode(self, bytes: &[u8]) -> String {
        let text = match self {
            Encoding::Utf8 => encoding_rs::UTF_8.decode(bytes).0,
            Encoding::Windows1252 => encoding_rs::WINDOWS_1252.decode(bytes).0,
            Encoding::Utf16Le => encoding_rs::UTF_16LE.decode(bytes).0,
            Encoding::Utf16Be => encoding_rs::UTF_16BE.decode(bytes).0,
            Encoding::Auto => return Encoding::detect(bytes).decode(bytes),
        };
        text.trim_start_matches('\u{feff}').to_string()
    }

    /// The encoding the bytes look like: a byte order mark decides, otherwise valid UTF-8
    /// is UTF-8 and anything else is read as Windows-1252, which never fails.
    pub fn detect(bytes: &[u8]) -> Encoding {
        match bytes {
            [0xEF, 0xBB, 0xBF, ..] => Encoding::Utf8,
            [0xFF, 0xFE, ..] => Encoding::Utf16Le,
            [0xFE, 0xFF, ..] => Encoding::Utf16Be,
            _ if std::str::from_utf8(bytes).is_ok() => Encoding::Utf8,
            _ => Encoding::Windows1252,
        }
    }
}

/// What to do with one column of the file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColumnOption {
    /// False leaves the column out of the table.
    #[serde(default = "yes")]
    pub include: bool,
    /// Forces the column's type instead of inferring it. A value that will not convert
    /// (text in a numeric column) becomes empty rather than failing the import.
    /// `percentage`, `date` and `time` read as numbers written that way.
    #[serde(default)]
    pub kind: Option<crate::table::ColumnType>,
}

pub(crate) fn yes() -> bool {
    true
}

impl Default for ColumnOption {
    fn default() -> Self {
        ColumnOption { include: true, kind: None }
    }
}

/// Everything the import wizard decides. The default is what Otto did before there was a
/// wizard: detect the separator, encoding and decimal mark, treat the first row as the
/// column names and infer every column's type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportOptions {
    #[serde(default)]
    pub separator: Separator,
    #[serde(default)]
    pub encoding: Encoding,
    #[serde(default)]
    pub decimal: Decimal,
    /// What each leading row is, from the top. Rows past the end are data rows, so the
    /// default `[name]` means "first row holds the column names".
    #[serde(default = "ImportOptions::default_rows")]
    pub rows: Vec<RowRole>,
    /// Per column, left to right. Columns past the end are included and inferred.
    #[serde(default)]
    pub columns: Vec<ColumnOption>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            separator: Separator::default(),
            encoding: Encoding::default(),
            decimal: Decimal::default(),
            rows: Self::default_rows(),
            columns: Vec::new(),
        }
    }
}

impl ImportOptions {
    fn default_rows() -> Vec<RowRole> {
        vec![RowRole::Name]
    }

    /// The old `has_header` flag: the first row names the columns, or there is no header.
    pub fn with_header(has_header: bool) -> Self {
        ImportOptions { rows: if has_header { Self::default_rows() } else { Vec::new() }, ..Default::default() }
    }

    /// What row `i` of the file is.
    pub fn role(&self, i: usize) -> RowRole {
        self.rows.get(i).copied().unwrap_or(RowRole::Data)
    }

    pub fn column(&self, i: usize) -> ColumnOption {
        self.columns.get(i).copied().unwrap_or_default()
    }

    /// Grows `rows` so that row `i` can be set, then sets it.
    pub fn set_role(&mut self, i: usize, role: RowRole) {
        if self.rows.len() <= i {
            self.rows.resize(i + 1, RowRole::Data);
        }
        self.rows[i] = role;
        while self.rows.last() == Some(&RowRole::Data) {
            self.rows.pop();
        }
    }

    /// Grows `columns` so that column `i` can be set, then hands it to `f`.
    pub fn set_column(&mut self, i: usize, f: impl FnOnce(&mut ColumnOption)) {
        if self.columns.len() <= i {
            self.columns.resize(i + 1, ColumnOption::default());
        }
        f(&mut self.columns[i]);
    }
}

/// Guesses the delimiter (`,` `;` or tab) from the first lines, and whether numbers use a decimal comma.
pub fn sniff_format(text: &str) -> TextFormat {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(5).collect();
    let count = |line: &str, d: char| {
        let mut in_quotes = false;
        line.chars()
            .filter(|&c| {
                if c == '"' {
                    in_quotes = !in_quotes;
                }
                !in_quotes && c == d
            })
            .count()
    };
    let mut best = (b',', 0usize);
    for d in ['\t', ';', ','] {
        // Prefer a delimiter that appears the same number of times on every line.
        let counts: Vec<usize> = lines.iter().map(|l| count(l, d)).collect();
        let min = counts.iter().copied().min().unwrap_or(0);
        if min > best.1 {
            best = (d as u8, min);
        }
    }
    let delimiter = best.0;
    let decimal_comma = delimiter != b','
        && text
            .split([delimiter as char, '\n', '\r'])
            .any(|cell| {
                let c = cell.trim().trim_start_matches('-');
                c.contains(',') && c.replace(',', "").chars().all(|ch| ch.is_ascii_digit()) && c.len() > 2
            });
    TextFormat { delimiter, decimal_comma }
}

/// Parses delimited text (CSV/TSV, e.g. pasted from Excel) into a table.
pub fn table_from_text(id: &str, name: &str, text: &str, has_header: bool) -> Result<DataTable, CoreError> {
    table_from_text_with(id, name, text, &ImportOptions::with_header(has_header))
}

/// As [`table_from_text`], with the separator, decimal mark, row roles and column choices
/// the import wizard settled on.
pub fn table_from_text_with(id: &str, name: &str, text: &str, options: &ImportOptions) -> Result<DataTable, CoreError> {
    let fmt = text_format(text, options);
    let grid = text_grid(text, fmt.delimiter)?;
    table_from_grid(id, name, grid, options, fmt.decimal_comma)
}

/// The separator and decimal mark to read `text` with: what `options` says, and where it
/// says to work it out, what the text looks like.
pub fn text_format(text: &str, options: &ImportOptions) -> TextFormat {
    let sniffed = sniff_format(text);
    let delimiter = options.separator.byte().unwrap_or(sniffed.delimiter);
    let decimal_comma = match options.decimal {
        Decimal::Auto => {
            // Sniffing assumed its own separator; re-ask with the one actually in use.
            delimiter != b',' && sniffed.decimal_comma
        }
        Decimal::Dot => false,
        Decimal::Comma => true,
    };
    TextFormat { delimiter, decimal_comma }
}

pub(crate) fn text_grid(text: &str, delimiter: u8) -> Result<Vec<Vec<RawCell>>, CoreError> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let mut grid = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| CoreError::Import(format!("could not parse text: {e}")))?;
        grid.push(rec.iter().map(|s| if s.is_empty() { RawCell::Empty } else { RawCell::Str(s.to_string()) }).collect());
    }
    Ok(grid)
}

/// Imports a CSV/TSV/TXT or Excel (xlsx/xlsm/xls/ods) file.
/// For workbooks, `sheet` selects a sheet by name (default: the first).
pub fn table_from_file(id: &str, path: &Path, sheet: Option<&str>) -> Result<DataTable, CoreError> {
    table_from_file_with(id, path, sheet, &ImportOptions::default())
}

/// As [`table_from_file`], with the settings the import wizard decided on.
pub fn table_from_file_with(id: &str, path: &Path, sheet: Option<&str>, options: &ImportOptions) -> Result<DataTable, CoreError> {
    let (grid, name, fmt) = read_file(path, sheet, options)?;
    table_from_grid(id, &name, grid, options, fmt.decimal_comma)
}

/// Reads a file into a raw grid, with the table name the file suggests and how its text was
/// read. The wizard previews this before any of it becomes a table.
pub fn read_file(path: &Path, sheet: Option<&str>, options: &ImportOptions) -> Result<(Vec<Vec<RawCell>>, String, TextFormat), CoreError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Data").to_string();
    match ext.as_str() {
        "csv" | "tsv" | "txt" | "tab" => {
            let bytes = std::fs::read(path).map_err(|e| CoreError::Import(format!("{}: {e}", path.display())))?;
            let text = options.encoding.decode(&bytes);
            let fmt = text_format(&text, options);
            Ok((text_grid(&text, fmt.delimiter)?, stem, fmt))
        }
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => {
            let mut wb = open_workbook_auto(path).map_err(|e| CoreError::Import(format!("{}: {e}", path.display())))?;
            let names = wb.sheet_names().to_vec();
            let sheet_name = match sheet {
                Some(s) => names
                    .iter()
                    .find(|n| n.eq_ignore_ascii_case(s))
                    .cloned()
                    .ok_or_else(|| CoreError::NotFound(format!("sheet '{s}' (sheets: {})", names.join(", "))))?,
                None => names.first().cloned().ok_or_else(|| CoreError::Import("workbook has no sheets".into()))?,
            };
            let range = wb
                .worksheet_range(&sheet_name)
                .map_err(|e| CoreError::Import(format!("sheet '{sheet_name}': {e}")))?;
            let grid = range
                .rows()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            Data::Empty => RawCell::Empty,
                            Data::Int(i) => RawCell::Num(*i as f64),
                            Data::Float(f) => RawCell::Num(*f),
                            Data::String(s) => RawCell::Str(s.clone()),
                            Data::Error(_) => RawCell::Empty,
                            other => RawCell::Str(other.to_string()),
                        })
                        .collect()
                })
                .collect();
            let name = if names.len() > 1 { format!("{stem} – {sheet_name}") } else { stem };
            // A workbook carries typed cells, so there is no separator or encoding to pick;
            // only a decimal comma typed into a text cell is still worth honouring.
            let decimal_comma = options.decimal == Decimal::Comma;
            Ok((grid, name, TextFormat { delimiter: b',', decimal_comma }))
        }
        other => Err(CoreError::Import(format!(
            "unsupported file type '.{other}' (use .csv, .tsv, .txt, .xlsx, .xls or .ods)"
        ))),
    }
}

/// Sheet names of a workbook (empty for text files).
pub fn sheet_names(path: &Path) -> Result<Vec<String>, CoreError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "xlsx" | "xlsm" | "xlsb" | "xls" | "ods") {
        return Ok(Vec::new());
    }
    let wb = open_workbook_auto(path).map_err(|e| CoreError::Import(format!("{}: {e}", path.display())))?;
    Ok(wb.sheet_names().to_vec())
}

/// Builds a typed table from a raw grid, following the row roles and column choices in
/// `options` and inferring the type of every column that is not given one.
pub fn table_from_grid(id: &str, name: &str, grid: Vec<Vec<RawCell>>, options: &ImportOptions, decimal_comma: bool) -> Result<DataTable, CoreError> {
    if grid.iter().all(|r| r.iter().all(RawCell::is_blank)) {
        return Err(CoreError::Import("no data found".into()));
    }
    let (header, rows) = split_grid(&grid, options);
    if rows.is_empty() {
        return Err(CoreError::Import("no data rows: every row is a name row or skipped".into()));
    }
    let ncols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut table = DataTable::new(id, name);
    table.nrows = rows.len();
    for c in 0..ncols {
        let choice = options.column(c);
        if !choice.include {
            continue;
        }
        let cells: Vec<&RawCell> = rows.iter().map(|r| r.get(c).unwrap_or(&RawCell::Empty)).collect();
        let title = header.get(c).cloned().unwrap_or_default();
        if title.is_empty() && cells.iter().all(|c| c.is_blank()) {
            continue; // fully empty spacer column
        }
        let base = if title.is_empty() { format!("Column {}", c + 1) } else { title };
        let title = table.unique_column_name(&base);
        let col = match choice.kind {
            Some(kind) => column_as(title, &cells, kind, decimal_comma),
            None => infer_column(title, &cells, decimal_comma),
        };
        table.columns.push(col);
    }
    if table.columns.is_empty() {
        return Err(CoreError::Import("no columns found".into()));
    }
    Ok(table)
}

/// The column names and the data rows of a grid, by the roles in `options`. Several name
/// rows join with " | " (blank parts dropped), which is how a header spread over two rows
/// — a treatment above its doses — becomes one name per column.
pub(crate) fn split_grid<'a>(grid: &'a [Vec<RawCell>], options: &ImportOptions) -> (Vec<String>, Vec<&'a Vec<RawCell>>) {
    // Trailing blank rows are not data, however the roles read.
    let mut end = grid.len();
    while end > 0 && grid[end - 1].iter().all(RawCell::is_blank) {
        end -= 1;
    }
    let grid = &grid[..end];
    let ncols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut parts: Vec<Vec<String>> = vec![Vec::new(); ncols];
    let mut rows: Vec<&Vec<RawCell>> = Vec::new();
    for (i, row) in grid.iter().enumerate() {
        match options.role(i) {
            RowRole::Name => {
                for (c, part) in parts.iter_mut().enumerate() {
                    let text = row.get(c).map(RawCell::as_text).unwrap_or_default();
                    if !text.is_empty() {
                        part.push(text);
                    }
                }
            }
            RowRole::Data => rows.push(row),
            RowRole::Skip => {}
        }
    }
    (parts.into_iter().map(|p| p.join(crate::spec::COMBINE_SEP)).collect(), rows)
}

/// A column of the type the user asked for. A value that does not fit — text where a number
/// was asked for — becomes empty, so choosing a type never fails the whole import.
fn column_as(name: String, cells: &[&RawCell], ty: crate::table::ColumnType, decimal_comma: bool) -> Column {
    use crate::table::ColumnKind;
    match ty.kind() {
        ColumnKind::Numeric => {
            let format = ty.format();
            let values = cells.iter().map(|c| as_formatted_number(c, format, decimal_comma)).collect();
            Column::numeric_formatted(name, values, format)
        }
        ColumnKind::Text => Column::text(name, cells.iter().map(|c| (!c.is_blank()).then(|| c.as_text())).collect()),
        ColumnKind::Categorical => {
            let texts: Vec<Option<String>> = cells.iter().map(|c| Some(if c.is_blank() { BLANK_LEVEL.to_string() } else { c.as_text() })).collect();
            Column::categorical(name, &texts)
        }
    }
}

/// One cell as a number written the way the chosen format writes them (a date, a time or
/// a percentage), falling back to a plain number.
fn as_formatted_number(c: &RawCell, format: crate::table::NumberFormat, decimal_comma: bool) -> Option<f64> {
    match c {
        RawCell::Num(v) => Some(*v),
        RawCell::Str(s) => crate::table::parse_value(s, format, decimal_comma),
        RawCell::Empty => None,
    }
}

/// How many rows of the file the wizard shows.
pub const PREVIEW_ROWS: usize = 200;

/// What the import wizard shows: the name and type each column would get, and the first
/// rows exactly as the file reads them, each with its role. Built from the raw grid, so
/// changing a setting re-previews without reading the file again.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportPreview {
    /// Per column: the name it would get (several name rows joined with " | ").
    pub names: Vec<String>,
    /// Per column: the type it would get, chosen or inferred.
    pub kinds: Vec<crate::table::ColumnType>,
    /// Per column: whether it would be imported at all.
    pub include: Vec<bool>,
    /// The rows shown, each with its role and its cells as text.
    pub rows: Vec<(RowRole, Vec<String>)>,
    /// Rows in the file, which may be more than are shown.
    pub total_rows: usize,
    /// Data rows the current roles leave, which is what the table would have.
    pub data_rows: usize,
}

/// Builds the wizard's preview of a grid under `options`.
pub fn preview(grid: &[Vec<RawCell>], options: &ImportOptions, decimal_comma: bool) -> ImportPreview {
    let (names, rows) = split_grid(grid, options);
    let ncols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    let kinds = (0..ncols)
        .map(|c| match options.column(c).kind {
            Some(ty) => ty,
            None => {
                let cells: Vec<&RawCell> = rows.iter().map(|r| r.get(c).unwrap_or(&RawCell::Empty)).collect();
                infer_column(String::new(), &cells, decimal_comma).column_type()
            }
        })
        .collect();
    let shown = grid
        .iter()
        .take(PREVIEW_ROWS)
        .enumerate()
        .map(|(i, row)| {
            let cells = (0..ncols).map(|c| row.get(c).map(RawCell::as_text).unwrap_or_default()).collect();
            (options.role(i), cells)
        })
        .collect();
    ImportPreview {
        names: (0..ncols).map(|c| names.get(c).cloned().filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Column {}", c + 1))).collect(),
        kinds,
        include: (0..ncols).map(|c| options.column(c).include).collect(),
        rows: shown,
        total_rows: grid.len(),
        data_rows: rows.len(),
    }
}

/// A column from typed or pasted text, with the type an import would give it.
pub fn infer_from_text(name: String, cells: &[Option<String>]) -> Column {
    let raw: Vec<RawCell> = cells.iter().map(|c| c.clone().map_or(RawCell::Empty, RawCell::Str)).collect();
    let refs: Vec<&RawCell> = raw.iter().collect();
    infer_column(name, &refs, true)
}

fn infer_column(name: String, cells: &[&RawCell], decimal_comma: bool) -> Column {
    let filled: Vec<&RawCell> = cells.iter().copied().filter(|c| !c.is_blank()).collect();
    let as_number = |c: &RawCell| match c {
        RawCell::Num(v) => Some(*v),
        RawCell::Str(s) => parse_number(s, decimal_comma),
        RawCell::Empty => None,
    };
    let numeric = filled.iter().all(|c| as_number(c).is_some() || matches!(c, RawCell::Str(s) if is_missing_token(s)));
    if numeric {
        let values = cells.iter().map(|c| as_number(c)).collect();
        return Column::numeric(name, values);
    }
    let texts: Vec<Option<String>> = cells.iter().map(|c| (!c.is_blank()).then(|| c.as_text())).collect();
    let mut distinct: Vec<&String> = texts.iter().flatten().collect();
    distinct.sort();
    distinct.dedup();
    let repeats = distinct.len() < filled.len();
    if repeats && distinct.len() <= 100 {
        let has_blanks = texts.iter().any(|t| t.is_none());
        let texts: Vec<Option<String>> = if has_blanks {
            texts.into_iter().map(|t| Some(t.unwrap_or_else(|| BLANK_LEVEL.to_string()))).collect()
        } else {
            texts
        };
        Column::categorical(name, &texts)
    } else {
        Column::text(name, texts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{ColumnKind, ColumnType};

    #[test]
    fn sniffs_delimiters() {
        assert_eq!(sniff_format("a,b\n1,2\n").delimiter, b',');
        assert_eq!(sniff_format("a;b\n1,5;2\n"), TextFormat { delimiter: b';', decimal_comma: true });
        assert_eq!(sniff_format("a\tb\n1\t2\n").delimiter, b'\t');
        assert_eq!(sniff_format("\"x,y\";b\n1;2\n").delimiter, b';');
    }

    #[test]
    fn infers_types_from_text() {
        let t = table_from_text("T1", "t", "id,group,value\nA1,WT,1.5\nA2,WT,2\nA3,KO,NA\n", true).unwrap();
        let kinds: Vec<_> = t.columns.iter().map(|c| c.kind()).collect();
        assert_eq!(kinds, vec![ColumnKind::Text, ColumnKind::Categorical, ColumnKind::Numeric]);
        assert_eq!(t.columns[2].number(2), None);
        assert_eq!(t.nrows, 3);
    }

    #[test]
    fn quoted_fields_and_decimal_comma() {
        let t = table_from_text("T1", "t", "name;val\n\"a; b\";1,25\n\"a; b\";2\n", true).unwrap();
        assert_eq!(t.columns[0].label(0), Some("a; b"));
        assert_eq!(t.columns[1].number(0), Some(1.25));
    }

    /// A header spread over two rows: the parts join with " | ", which is the same
    /// separator a graph uses when several columns make one category.
    #[test]
    fn name_rows_join_with_a_pipe() {
        let text = "Treatment,Treatment,Control\nLow,High,\n1,2,3\n4,5,6\n";
        let mut o = ImportOptions::default();
        o.set_role(0, RowRole::Name);
        o.set_role(1, RowRole::Name);
        let t = table_from_text_with("T1", "t", text, &o).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Treatment | Low", "Treatment | High", "Control"]);
        assert_eq!(t.nrows, 2);
        assert_eq!(t.columns[0].number(1), Some(4.0));
    }

    /// A title line above the table, which breaks type inference if it is read as data.
    #[test]
    fn a_preamble_row_can_be_skipped() {
        let text = "Plate 3 — run of 12 May,,\nWell,Dose,Response\nA1,1,5\nA2,2,7\n";
        let mut o = ImportOptions::default();
        o.set_role(0, RowRole::Skip);
        o.set_role(1, RowRole::Name);
        let t = table_from_text_with("T1", "t", text, &o).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Well", "Dose", "Response"]);
        assert_eq!(t.nrows, 2);
        assert_eq!(t.columns[2].kind(), ColumnKind::Numeric, "the title row no longer makes it text");
    }

    #[test]
    fn a_column_can_be_left_out_or_given_a_type() {
        let text = "id,group,value\n1,WT,1\n2,KO,2\n";
        let mut o = ImportOptions::default();
        o.set_column(1, |c| c.include = false);
        o.set_column(0, |c| c.kind = Some(ColumnType::Text));
        o.set_column(2, |c| c.kind = Some(ColumnType::Categorical));
        let t = table_from_text_with("T1", "t", text, &o).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "value"]);
        assert_eq!(t.columns[0].kind(), ColumnKind::Text);
        assert_eq!(t.columns[1].kind(), ColumnKind::Categorical);
    }

    /// Asking for a numeric column where the text is not numbers empties those cells
    /// rather than failing the import.
    #[test]
    fn a_forced_type_that_does_not_fit_empties_the_cell() {
        let text = "v\n1\nmissing\n3\n";
        let mut o = ImportOptions::default();
        o.set_column(0, |c| c.kind = Some(ColumnType::Numeric));
        let t = table_from_text_with("T1", "t", text, &o).unwrap();
        assert_eq!(t.columns[0].number(0), Some(1.0));
        assert_eq!(t.columns[0].number(1), None);
        assert_eq!(t.columns[0].number(2), Some(3.0));
    }

    #[test]
    fn separator_and_decimal_mark_can_be_chosen() {
        // Sniffing would call this comma-separated with no decimal comma.
        let text = "a,b\n1,5,2,5\n";
        let o = ImportOptions { separator: Separator::Other('|'), ..Default::default() };
        assert_eq!(text_format(text, &o).delimiter, b'|');
        let o = ImportOptions { decimal: Decimal::Comma, separator: Separator::Semicolon, ..Default::default() };
        let t = table_from_text_with("T1", "t", "a;b\n1,5;2\n", &o).unwrap();
        assert_eq!(t.columns[0].number(0), Some(1.5));
    }

    #[test]
    fn encodings_are_detected_and_can_be_chosen() {
        assert_eq!(Encoding::detect(b"plain ascii"), Encoding::Utf8);
        assert_eq!(Encoding::detect(&[0xEF, 0xBB, 0xBF, b'a']), Encoding::Utf8);
        assert_eq!(Encoding::detect(&[0xFF, 0xFE, b'a', 0]), Encoding::Utf16Le);
        // 0xB5 is "µ" in Windows-1252 and not valid UTF-8 on its own.
        let latin1 = [b'c', b'=', 0xB5, b'M'];
        assert_eq!(Encoding::detect(&latin1), Encoding::Windows1252);
        assert_eq!(Encoding::Auto.decode(&latin1), "c=µM");
        assert_eq!(Encoding::Utf16Le.decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
    }

    #[test]
    fn preview_shows_names_types_and_roles() {
        let text = "Well,Dose,Response\nA1,1,5\nA2,2,7\n";
        let o = ImportOptions::default();
        let grid = text_grid(text, b',').unwrap();
        let p = preview(&grid, &o, false);
        assert_eq!(p.names, vec!["Well", "Dose", "Response"]);
        assert_eq!(p.kinds, vec![ColumnType::Text, ColumnType::Numeric, ColumnType::Numeric]);
        assert_eq!(p.rows[0], (RowRole::Name, vec!["Well".into(), "Dose".into(), "Response".into()]));
        assert_eq!(p.rows[1].0, RowRole::Data);
        assert_eq!((p.total_rows, p.data_rows), (3, 2));
    }

    #[test]
    fn imports_sample_workbook() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/data.xlsx");
        let t = table_from_file("T1", &path, None).unwrap();
        assert_eq!(t.nrows, 24);
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Well", "Condition", "Additive", "IL2", "IL7", "IL15", "IL18", "IL21"]);
        let kinds: Vec<_> = t.columns.iter().map(|c| c.kind()).collect();
        use ColumnKind::*;
        assert_eq!(kinds, vec![Text, Categorical, Categorical, Numeric, Numeric, Numeric, Numeric, Numeric]);
        assert_eq!(t.columns[2].levels()[0], BLANK_LEVEL);
        // Formula cells (=D2*10) come through as their cached values.
        assert_eq!(t.columns[3].number(12), Some(10.0));
    }
}
