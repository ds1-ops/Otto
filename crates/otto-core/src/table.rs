//! Multivariable data tables: named, typed columns of equal length.

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// The type of a table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    /// Numbers (measurements, concentrations, time points).
    Numeric,
    /// A factor with a fixed, ordered set of levels (conditions, genotypes, treatments).
    Categorical,
    /// Free text such as sample or well IDs.
    Text,
}

impl ColumnKind {
    pub const ALL: [ColumnKind; 3] = [ColumnKind::Numeric, ColumnKind::Categorical, ColumnKind::Text];

    pub fn label(self) -> &'static str {
        match self {
            ColumnKind::Numeric => "Numeric",
            ColumnKind::Categorical => "Categorical",
            ColumnKind::Text => "Text",
        }
    }

    /// Short badge shown in column headers.
    pub fn badge(self) -> &'static str {
        match self {
            ColumnKind::Numeric => "#",
            ColumnKind::Categorical => "C",
            ColumnKind::Text => "T",
        }
    }
}

impl std::fmt::Display for ColumnKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// How the numbers of a numeric column are written in cells. The values themselves stay
/// plain numbers, so every graph and every statistic keeps working on them; only the way
/// they are read and shown changes. Dates are whole days since 1970-01-01, times are
/// seconds since midnight — the same convention spreadsheets use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NumberFormat {
    #[default]
    Plain,
    /// Written with a trailing `%`; `12%` is the number 12.
    Percent,
    /// Whole days since 1970-01-01, written `YYYY-MM-DD`.
    Date,
    /// Seconds since midnight, written `HH:MM` or `HH:MM:SS`.
    Time,
}

/// A column's type as it is offered in the editor: the kind, plus the number format that
/// percentages, dates and times are really made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Numeric,
    Percentage,
    Date,
    Time,
    Categorical,
    Text,
}

impl ColumnType {
    pub const ALL: [ColumnType; 6] = [
        ColumnType::Numeric,
        ColumnType::Percentage,
        ColumnType::Date,
        ColumnType::Time,
        ColumnType::Categorical,
        ColumnType::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColumnType::Numeric => "Numeric",
            ColumnType::Percentage => "Percentage",
            ColumnType::Date => "Date",
            ColumnType::Time => "Time",
            ColumnType::Categorical => "Categorical",
            ColumnType::Text => "Text",
        }
    }

    /// Short badge shown in column headers.
    pub fn badge(self) -> &'static str {
        match self {
            ColumnType::Numeric => "#",
            ColumnType::Percentage => "%",
            ColumnType::Date => "D",
            ColumnType::Time => "H",
            ColumnType::Categorical => "C",
            ColumnType::Text => "T",
        }
    }

    pub fn kind(self) -> ColumnKind {
        match self {
            ColumnType::Numeric | ColumnType::Percentage | ColumnType::Date | ColumnType::Time => ColumnKind::Numeric,
            ColumnType::Categorical => ColumnKind::Categorical,
            ColumnType::Text => ColumnKind::Text,
        }
    }

    pub fn format(self) -> NumberFormat {
        match self {
            ColumnType::Percentage => NumberFormat::Percent,
            ColumnType::Date => NumberFormat::Date,
            ColumnType::Time => NumberFormat::Time,
            _ => NumberFormat::Plain,
        }
    }

    pub fn of(kind: ColumnKind, format: NumberFormat) -> ColumnType {
        match kind {
            ColumnKind::Categorical => ColumnType::Categorical,
            ColumnKind::Text => ColumnType::Text,
            ColumnKind::Numeric => match format {
                NumberFormat::Plain => ColumnType::Numeric,
                NumberFormat::Percent => ColumnType::Percentage,
                NumberFormat::Date => ColumnType::Date,
                NumberFormat::Time => ColumnType::Time,
            },
        }
    }
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A column's cell values. Shared, so that cloning a table (every undo snapshot takes one)
/// copies pointers rather than rows; a column is copied only when it is actually edited.
/// Mutate through [`Arc::make_mut`], never by replacing the `Arc`.
pub type Cells<T> = std::sync::Arc<Vec<T>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColumnData {
    Numeric {
        values: Cells<Option<f64>>,
        /// How the numbers are written in cells; it never changes what they are.
        #[serde(default, skip_serializing_if = "crate::table::NumberFormat::is_plain")]
        format: NumberFormat,
    },
    /// `levels` stays unshared: there are few of them and they are read everywhere.
    Categorical { levels: Vec<String>, codes: Cells<Option<u32>> },
    Text { values: Cells<Option<String>> },
}

/// Equality that skips rows the two columns share: `Arc::ptr_eq` means the same
/// allocation, so the values are the same without looking at them. This is what makes
/// "did this command change anything?" cheap on a large table.
fn same<T: PartialEq>(a: &Cells<T>, b: &Cells<T>) -> bool {
    std::sync::Arc::ptr_eq(a, b) || a == b
}

/// The cells of a column, for writing: copies the rows away from any snapshot still
/// holding them, then hands out the only reference.
fn cells<T: Clone>(c: &mut Cells<T>) -> &mut Vec<T> {
    std::sync::Arc::make_mut(c)
}

impl PartialEq for ColumnData {
    fn eq(&self, other: &Self) -> bool {
        use ColumnData::*;
        match (self, other) {
            (Numeric { values: a, format: fa }, Numeric { values: b, format: fb }) => fa == fb && same(a, b),
            (Categorical { levels: la, codes: a }, Categorical { levels: lb, codes: b }) => la == lb && same(a, b),
            (Text { values: a }, Text { values: b }) => same(a, b),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(flatten)]
    pub data: ColumnData,
}

/// Tokens treated as "no value" when reading numbers.
const MISSING_TOKENS: [&str; 8] = ["na", "n/a", "nan", "#n/a", "nd", "n.d.", "-", "null"];

pub fn is_missing_token(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || MISSING_TOKENS.iter().any(|m| t.eq_ignore_ascii_case(m))
}

/// Parses a number, optionally accepting a decimal comma (`1,5`).
pub fn parse_number(s: &str, decimal_comma: bool) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let owned;
    let t = if decimal_comma && t.contains(',') && !t.contains('.') {
        owned = t.replace(',', ".");
        owned.as_str()
    } else {
        t
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Formats a number the way it is shown in table cells (shortest round-trip form).
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

impl NumberFormat {
    pub fn is_plain(&self) -> bool {
        *self == NumberFormat::Plain
    }
}

/// Days since 1970-01-01 as a calendar date (Howard Hinnant's civil_from_days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A calendar date as days since 1970-01-01 (the inverse of [`civil_from_days`]).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Formats a cell value under its column's number format.
pub fn format_value(v: f64, format: NumberFormat) -> String {
    match format {
        NumberFormat::Plain => format_number(v),
        NumberFormat::Percent => format!("{}%", format_number(v)),
        NumberFormat::Date => {
            if !v.is_finite() || v.abs() > 4e6 {
                return format_number(v);
            }
            let (y, m, d) = civil_from_days(v.floor() as i64);
            format!("{y:04}-{m:02}-{d:02}")
        }
        NumberFormat::Time => {
            if !v.is_finite() || v.abs() >= 1e9 {
                return format_number(v);
            }
            let neg = v < 0.0;
            let total = v.abs().round() as i64;
            let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
            let sign = if neg { "-" } else { "" };
            if s == 0 {
                format!("{sign}{h:02}:{m:02}")
            } else {
                format!("{sign}{h:02}:{m:02}:{s:02}")
            }
        }
    }
}

/// Reads a date written `YYYY-MM-DD`, `DD.MM.YYYY` or `DD/MM/YYYY` as days since
/// 1970-01-01. Slashes are read day-first, the way the rest of Otto reads dates.
fn parse_date(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(['-', '.', '/']).map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.len() != 3 {
        return None;
    }
    let n: Vec<i64> = parts.iter().map(|p| p.parse::<i64>().ok()).collect::<Option<Vec<_>>>()?;
    // Year first when the first field is clearly a year, otherwise day first.
    let (y, m, d) = if parts[0].len() == 4 || n[0] > 31 { (n[0], n[1], n[2]) } else { (n[2], n[1], n[0]) };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m as u32, d as u32) as f64)
}

/// Reads `HH:MM`, `HH:MM:SS` or a bare number of seconds.
fn parse_time(s: &str) -> Option<f64> {
    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest.trim()),
        None => (1.0, s),
    };
    let parts: Vec<&str> = body.split(':').map(str::trim).collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let sec: f64 = match parts.get(2) {
        Some(p) => p.parse().ok()?,
        None => 0.0,
    };
    if !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&sec) {
        return None;
    }
    Some(sign * (h * 3600.0 + m * 60.0 + sec))
}

/// Reads a typed cell under its column's number format, falling back to a plain number
/// so a date column still takes the raw day count and a percentage column takes `12`.
pub fn parse_value(s: &str, format: NumberFormat, decimal_comma: bool) -> Option<f64> {
    let t = s.trim();
    match format {
        NumberFormat::Plain => parse_number(t, decimal_comma),
        NumberFormat::Percent => parse_number(t.strip_suffix('%').unwrap_or(t).trim(), decimal_comma),
        NumberFormat::Date => parse_date(t).or_else(|| parse_number(t, decimal_comma)),
        NumberFormat::Time => parse_time(t).or_else(|| parse_number(t, decimal_comma)),
    }
}

impl Column {
    pub fn new(name: impl Into<String>, kind: ColumnKind, len: usize) -> Self {
        let data = match kind {
            ColumnKind::Numeric => ColumnData::Numeric { values: vec![None; len].into(), format: NumberFormat::Plain },
            ColumnKind::Categorical => ColumnData::Categorical { levels: Vec::new(), codes: vec![None; len].into() },
            ColumnKind::Text => ColumnData::Text { values: vec![None; len].into() },
        };
        Column { name: name.into(), data }
    }

    pub fn numeric(name: impl Into<String>, values: Vec<Option<f64>>) -> Self {
        Column { name: name.into(), data: ColumnData::Numeric { values: values.into(), format: NumberFormat::Plain } }
    }

    /// A numeric column whose cells are written as percentages, dates or times.
    pub fn numeric_formatted(name: impl Into<String>, values: Vec<Option<f64>>, format: NumberFormat) -> Self {
        Column { name: name.into(), data: ColumnData::Numeric { values: values.into(), format } }
    }

    pub fn text(name: impl Into<String>, values: Vec<Option<String>>) -> Self {
        Column { name: name.into(), data: ColumnData::Text { values: values.into() } }
    }

    /// Builds a categorical column; levels are taken in order of first appearance.
    pub fn categorical(name: impl Into<String>, values: &[Option<String>]) -> Self {
        // The level each value belongs to is looked up in a map rather than by scanning the
        // levels found so far, which made importing a column of many distinct values
        // quadratic in the number of rows.
        let mut levels: Vec<String> = Vec::new();
        let mut seen: HashMap<&str, u32> = HashMap::new();
        let codes: Vec<Option<u32>> = values
            .iter()
            .map(|v| {
                v.as_deref().map(|s| match seen.get(s) {
                    Some(&i) => i,
                    None => {
                        let i = levels.len() as u32;
                        levels.push(s.to_string());
                        seen.insert(s, i);
                        i
                    }
                })
            })
            .collect();
        Column { name: name.into(), data: ColumnData::Categorical { levels, codes: codes.into() } }
    }

    pub fn kind(&self) -> ColumnKind {
        match self.data {
            ColumnData::Numeric { .. } => ColumnKind::Numeric,
            ColumnData::Categorical { .. } => ColumnKind::Categorical,
            ColumnData::Text { .. } => ColumnKind::Text,
        }
    }

    /// How this column's numbers are written (always [`NumberFormat::Plain`] off a
    /// numeric column).
    pub fn format(&self) -> NumberFormat {
        match self.data {
            ColumnData::Numeric { format, .. } => format,
            _ => NumberFormat::Plain,
        }
    }

    /// The type as the editor offers it: the kind together with the number format.
    pub fn column_type(&self) -> ColumnType {
        ColumnType::of(self.kind(), self.format())
    }

    pub fn len(&self) -> usize {
        match &self.data {
            ColumnData::Numeric { values, .. } => values.len(),
            ColumnData::Categorical { codes, .. } => codes.len(),
            ColumnData::Text { values } => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_missing(&self, row: usize) -> bool {
        match &self.data {
            ColumnData::Numeric { values, .. } => values.get(row).copied().flatten().is_none(),
            ColumnData::Categorical { codes, .. } => codes.get(row).copied().flatten().is_none(),
            ColumnData::Text { values } => values.get(row).is_none_or(|v| v.is_none()),
        }
    }

    pub fn missing_count(&self) -> usize {
        (0..self.len()).filter(|&r| self.is_missing(r)).count()
    }

    /// Numeric value of a cell (numeric columns only).
    pub fn number(&self, row: usize) -> Option<f64> {
        match &self.data {
            ColumnData::Numeric { values, .. } => values.get(row).copied().flatten(),
            _ => None,
        }
    }

    /// Category label of a cell; text columns behave as categorical here.
    pub fn label(&self, row: usize) -> Option<&str> {
        match &self.data {
            ColumnData::Categorical { levels, codes } => {
                codes.get(row).copied().flatten().map(|c| levels[c as usize].as_str())
            }
            ColumnData::Text { values } => values.get(row).and_then(|v| v.as_deref()),
            ColumnData::Numeric { .. } => None,
        }
    }

    /// Levels in display order. For text columns: distinct values in order of appearance.
    pub fn levels(&self) -> Vec<String> {
        match &self.data {
            ColumnData::Categorical { levels, .. } => levels.clone(),
            ColumnData::Text { values } => {
                // Distinct values in order of appearance. Asking a set whether a value is
                // already there, rather than scanning the ones found so far: a text column
                // is a legal X or Group column, so this runs on whole tables.
                let mut seen: HashSet<&str> = HashSet::new();
                values.iter().flatten().filter(|v| seen.insert(v.as_str())).cloned().collect()
            }
            ColumnData::Numeric { values, format } => {
                let mut nums: Vec<f64> = values.iter().flatten().copied().collect();
                nums.sort_by(|a, b| a.total_cmp(b));
                nums.dedup();
                nums.into_iter().map(|v| format_value(v, *format)).collect()
            }
        }
    }

    /// Cell content as shown in the table editor (empty string for missing).
    pub fn display(&self, row: usize) -> String {
        match &self.data {
            ColumnData::Numeric { values, format } => {
                values.get(row).copied().flatten().map(|v| format_value(v, *format)).unwrap_or_default()
            }
            _ => self.label(row).unwrap_or_default().to_string(),
        }
    }

    /// Sets a cell from user-typed text. Empty text clears the cell.
    pub fn set_from_str(&mut self, row: usize, s: &str) -> Result<(), CoreError> {
        let trimmed = s.trim();
        match &mut self.data {
            ColumnData::Numeric { values, format } => {
                let fmt = *format;
                let v = if is_missing_token(trimmed) {
                    None
                } else {
                    Some(parse_value(trimmed, fmt, true).ok_or_else(|| CoreError::NotANumber {
                        column: self.name.clone(),
                        value: trimmed.to_string(),
                    })?)
                };
                cells(values)[row] = v;
            }
            ColumnData::Categorical { levels, codes } => {
                cells(codes)[row] = if trimmed.is_empty() {
                    None
                } else {
                    Some(match levels.iter().position(|l| l == trimmed) {
                        Some(i) => i as u32,
                        None => {
                            levels.push(trimmed.to_string());
                            (levels.len() - 1) as u32
                        }
                    })
                };
            }
            ColumnData::Text { values } => {
                cells(values)[row] = (!trimmed.is_empty()).then(|| trimmed.to_string());
            }
        }
        Ok(())
    }

    /// Sets a cell from a JSON value (number, string, bool or null).
    pub fn set_from_json(&mut self, row: usize, v: &serde_json::Value) -> Result<(), CoreError> {
        match v {
            serde_json::Value::Null => self.set_from_str(row, ""),
            serde_json::Value::Number(n) => match &mut self.data {
                ColumnData::Numeric { values, .. } => {
                    cells(values)[row] = n.as_f64();
                    Ok(())
                }
                _ => self.set_from_str(row, &n.to_string()),
            },
            serde_json::Value::String(s) => self.set_from_str(row, s),
            serde_json::Value::Bool(b) => self.set_from_str(row, if *b { "TRUE" } else { "FALSE" }),
            other => self.set_from_str(row, &other.to_string()),
        }
    }

    pub fn insert_rows(&mut self, at: usize, count: usize) {
        match &mut self.data {
            ColumnData::Numeric { values, .. } => cells(values).splice(at..at, std::iter::repeat_n(None, count)).for_each(drop),
            ColumnData::Categorical { codes, .. } => cells(codes).splice(at..at, std::iter::repeat_n(None, count)).for_each(drop),
            ColumnData::Text { values } => cells(values).splice(at..at, std::iter::repeat_n(None, count)).for_each(drop),
        }
    }

    /// Removes the given rows (indices must be sorted ascending and unique).
    pub fn delete_rows(&mut self, rows: &[usize]) {
        fn retain<T>(v: &mut Vec<T>, rows: &[usize]) {
            let mut i = 0;
            v.retain(|_| {
                let keep = rows.binary_search(&i).is_err();
                i += 1;
                keep
            });
        }
        match &mut self.data {
            ColumnData::Numeric { values, .. } => retain(cells(values), rows),
            ColumnData::Categorical { codes, .. } => retain(cells(codes), rows),
            ColumnData::Text { values } => retain(cells(values), rows),
        }
    }

    /// Converts the column to another type. Fails (without changing anything) if a
    /// non-empty value cannot be represented, e.g. text that is not a number.
    pub fn convert(&self, kind: ColumnKind) -> Result<Column, CoreError> {
        self.convert_to(ColumnType::of(kind, self.format()))
    }

    /// Converts the column to one of the types the editor offers. Changing only the
    /// number format of a numeric column keeps its numbers exactly as they are.
    pub fn convert_to(&self, ty: ColumnType) -> Result<Column, CoreError> {
        let (kind, format) = (ty.kind(), ty.format());
        if kind == self.kind() {
            if kind != ColumnKind::Numeric || format == self.format() {
                return Ok(self.clone());
            }
            let mut out = self.clone();
            if let ColumnData::Numeric { format: f, .. } = &mut out.data {
                *f = format;
            }
            return Ok(out);
        }
        let strings: Vec<Option<String>> = (0..self.len())
            .map(|r| (!self.is_missing(r)).then(|| self.display(r)))
            .collect();
        let name = self.name.clone();
        Ok(match kind {
            ColumnKind::Numeric => {
                let mut values = Vec::with_capacity(strings.len());
                for s in &strings {
                    values.push(match s {
                        Some(s) if !is_missing_token(s) => Some(parse_value(s, format, true).ok_or_else(|| {
                            CoreError::NotANumber { column: name.clone(), value: s.clone() }
                        })?),
                        _ => None,
                    });
                }
                Column::numeric_formatted(name, values, format)
            }
            ColumnKind::Categorical => {
                let mut col = Column::categorical(name, &strings);
                // Numbers become levels in ascending numeric order.
                if self.kind() == ColumnKind::Numeric {
                    col.set_level_order(&self.levels())?;
                }
                col
            }
            ColumnKind::Text => Column::text(name, strings),
        })
    }

    /// Reorders categorical levels. `order` must name every existing level exactly once.
    pub fn set_level_order(&mut self, order: &[String]) -> Result<(), CoreError> {
        let ColumnData::Categorical { levels, codes } = &mut self.data else {
            return Err(CoreError::NotCategorical(self.name.clone()));
        };
        let mut sorted_old = levels.clone();
        sorted_old.sort();
        let mut sorted_new = order.to_vec();
        sorted_new.sort();
        if sorted_old != sorted_new {
            return Err(CoreError::Invalid(format!(
                "level order for '{}' must list each existing level exactly once: {}",
                self.name,
                levels.join(", ")
            )));
        }
        let remap: Vec<u32> = levels
            .iter()
            .map(|l| order.iter().position(|o| o == l).unwrap() as u32)
            .collect();
        for c in cells(codes).iter_mut().flatten() {
            *c = remap[*c as usize];
        }
        *levels = order.to_vec();
        Ok(())
    }

    /// Renames a level; renaming onto an existing level merges the two.
    pub fn rename_level(&mut self, old: &str, new: &str) -> Result<(), CoreError> {
        let name = self.name.clone();
        let ColumnData::Categorical { levels, codes } = &mut self.data else {
            return Err(CoreError::NotCategorical(name));
        };
        let new = new.trim();
        if new.is_empty() {
            return Err(CoreError::Invalid("level name cannot be empty".into()));
        }
        let from = levels
            .iter()
            .position(|l| l == old)
            .ok_or_else(|| CoreError::NotFound(format!("level '{old}' in column '{name}'")))?;
        match levels.iter().position(|l| l == new) {
            Some(to) if to != from => {
                for c in cells(codes).iter_mut().flatten() {
                    if *c as usize == from {
                        *c = to as u32;
                    }
                    if *c as usize > from {
                        *c -= 1;
                    }
                }
                levels.remove(from);
            }
            _ => levels[from] = new.to_string(),
        }
        Ok(())
    }
}

/// A pasted / typed value as cell text (`None` for empty).
fn json_text(v: &serde_json::Value) -> Option<String> {
    let s = match v {
        serde_json::Value::Null => return None,
        serde_json::Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    };
    (!s.is_empty()).then_some(s)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataTable {
    pub id: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub nrows: usize,
    /// Set on the locked data copy of a final graph (the graph's id): its values can't be
    /// edited, and it isn't listed with the data tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_by: Option<String>,
}

impl DataTable {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        DataTable { id: id.into(), name: name.into(), columns: Vec::new(), nrows: 0, locked_by: None }
    }

    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    /// Finds a column by exact name, falling back to a case-insensitive match.
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.name == name)
            .or_else(|| self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name.trim())))
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.column_index(name).map(|i| &self.columns[i])
    }

    pub fn require_column(&self, name: &str) -> Result<&Column, CoreError> {
        self.column(name).ok_or_else(|| self.missing_column(name))
    }

    pub fn require_column_index(&self, name: &str) -> Result<usize, CoreError> {
        self.column_index(name).ok_or_else(|| self.missing_column(name))
    }

    fn missing_column(&self, name: &str) -> CoreError {
        CoreError::NotFound(format!(
            "column '{name}' in table '{}' (columns: {})",
            self.name,
            self.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
        ))
    }

    /// Returns `base` or `base (2)`, `base (3)`… so that it is unique among column names.
    pub fn unique_column_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() { "Column" } else { base.trim() };
        if self.column_index_exact(base).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|i| format!("{base} ({i})"))
            .find(|n| self.column_index_exact(n).is_none())
            .unwrap()
    }

    fn column_index_exact(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Adds a column, resizing it to the table length. Names must be unique.
    pub fn add_column(&mut self, mut col: Column, at: Option<usize>) -> Result<usize, CoreError> {
        if col.name.trim().is_empty() {
            return Err(CoreError::Invalid("column name cannot be empty".into()));
        }
        if self.column_index_exact(&col.name).is_some() {
            return Err(CoreError::Invalid(format!("a column named '{}' already exists", col.name)));
        }
        let len = col.len();
        if self.columns.is_empty() && self.nrows < len {
            self.nrows = len;
        }
        if len < self.nrows {
            col.insert_rows(len, self.nrows - len);
        } else if len > self.nrows {
            let extra = len - self.nrows;
            for c in &mut self.columns {
                c.insert_rows(c.len(), extra);
            }
            self.nrows = len;
        }
        let at = at.unwrap_or(self.columns.len()).min(self.columns.len());
        self.columns.insert(at, col);
        Ok(at)
    }

    pub fn rename_column(&mut self, old: &str, new: &str) -> Result<(), CoreError> {
        let idx = self.require_column_index(old)?;
        let new = new.trim();
        if new.is_empty() {
            return Err(CoreError::Invalid("column name cannot be empty".into()));
        }
        if let Some(other) = self.column_index_exact(new) {
            if other != idx {
                return Err(CoreError::Invalid(format!("a column named '{new}' already exists")));
            }
        }
        self.columns[idx].name = new.to_string();
        Ok(())
    }

    pub fn insert_rows(&mut self, at: usize, count: usize) {
        let at = at.min(self.nrows);
        for c in &mut self.columns {
            c.insert_rows(at, count);
        }
        self.nrows += count;
    }

    pub fn delete_rows(&mut self, rows: &[usize]) {
        let mut rows: Vec<usize> = rows.iter().copied().filter(|&r| r < self.nrows).collect();
        rows.sort_unstable();
        rows.dedup();
        for c in &mut self.columns {
            c.delete_rows(&rows);
        }
        self.nrows -= rows.len();
    }

    /// Makes sure the table has at least `rows` rows and `cols` columns
    /// (new columns are numeric and named "Column N").
    pub fn ensure_size(&mut self, rows: usize, cols: usize) {
        if rows > self.nrows {
            let n = self.nrows;
            self.insert_rows(n, rows - n);
        }
        while self.columns.len() < cols {
            let name = self.unique_column_name(&format!("Column {}", self.columns.len() + 1));
            let col = Column::new(name, ColumnKind::Numeric, self.nrows);
            self.columns.push(col);
        }
    }

    /// Writes a block of cells starting at (`row`, `col`), growing the table as needed.
    /// Columns that were empty take the type the pasted values suggest (numeric,
    /// categorical or text, as when importing). Text that is not a number in a numeric
    /// column that already has values is an error.
    pub fn set_cells(&mut self, row: usize, col: usize, block: &[Vec<serde_json::Value>]) -> Result<(), CoreError> {
        let rows = block.len();
        let cols = block.iter().map(|r| r.len()).max().unwrap_or(0);
        if rows == 0 || cols == 0 {
            return Ok(());
        }
        self.ensure_size(row + rows, col + cols);
        let was_empty: Vec<bool> = (col..col + cols).map(|c| self.columns[c].missing_count() == self.columns[c].len()).collect();
        for (dc, empty) in was_empty.iter().enumerate() {
            if !*empty {
                continue;
            }
            let mut cells: Vec<Option<String>> = vec![None; self.nrows];
            for (dr, values) in block.iter().enumerate() {
                cells[row + dr] = values.get(dc).and_then(json_text);
            }
            let name = self.columns[col + dc].name.clone();
            self.columns[col + dc] = crate::import::infer_from_text(name, &cells);
        }
        for (dr, values) in block.iter().enumerate() {
            for (dc, v) in values.iter().enumerate() {
                if was_empty[dc] {
                    continue;
                }
                let column = &mut self.columns[col + dc];
                if let Err(e) = column.set_from_json(row + dr, v) {
                    if column.kind() == ColumnKind::Numeric && column.missing_count() == column.len() {
                        *column = column.convert(ColumnKind::Text)?;
                        column.set_from_json(row + dr, v)?;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// Rows as JSON objects keyed by column name (for MCP `table_get`).
    pub fn rows_json(&self, offset: usize, limit: usize) -> Vec<serde_json::Value> {
        (offset..(offset + limit).min(self.nrows))
            .map(|r| {
                let mut obj = serde_json::Map::new();
                for c in &self.columns {
                    let v = match &c.data {
                        ColumnData::Numeric { values, .. } => {
                            values[r].map_or(serde_json::Value::Null, |v| serde_json::json!(v))
                        }
                        _ => c.label(r).map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
                    };
                    obj.insert(c.name.clone(), v);
                }
                serde_json::Value::Object(obj)
            })
            .collect()
    }

    /// The table as delimited text (header + rows).
    pub fn to_delimited(&self, delimiter: u8) -> String {
        self.rows_delimited(0..self.nrows, delimiter)
    }

    /// The header row and the rows in `range`, as delimited text. A window of a big table
    /// is written straight out rather than by copying the table and deleting the rest.
    pub fn rows_delimited(&self, range: std::ops::Range<usize>, delimiter: u8) -> String {
        let mut w = csv::WriterBuilder::new().delimiter(delimiter).from_writer(Vec::new());
        let _ = w.write_record(self.columns.iter().map(|c| c.name.as_str()));
        for r in range.start..range.end.min(self.nrows) {
            let _ = w.write_record(self.columns.iter().map(|c| c.display(r)));
        }
        String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cloning a table must not copy its rows, and editing a clone must not reach back
    /// into the original: that is what makes an undo snapshot cheap. If a mutator ever
    /// writes through the `Arc` instead of [`cells`], one half of this fails.
    #[test]
    fn clones_share_rows_until_one_is_edited() {
        let mut a = DataTable::new("T1", "t");
        a.add_column(Column::numeric("Y", vec![Some(1.0), Some(2.0)]), None).unwrap();
        let before = a.clone();
        assert_eq!(before, a);
        let ptr = |t: &DataTable| match &t.columns[0].data {
            ColumnData::Numeric { values, .. } => std::sync::Arc::as_ptr(values),
            _ => unreachable!(),
        };
        assert_eq!(ptr(&before), ptr(&a), "a clone shares the rows");

        a.columns[0].set_from_str(0, "9").unwrap();
        assert_ne!(ptr(&before), ptr(&a), "editing copies the rows away from the clone");
        assert_eq!(before.columns[0].number(0), Some(1.0), "the clone keeps the old value");
        assert_eq!(a.columns[0].number(0), Some(9.0));
        assert_ne!(before, a);
    }

    #[test]
    fn categorical_levels_follow_first_appearance() {
        let vals: Vec<Option<String>> = ["b", "a", "b", "c"].iter().map(|s| Some(s.to_string())).collect();
        let c = Column::categorical("f", &vals);
        assert_eq!(c.levels(), vec!["b", "a", "c"]);
        assert_eq!(c.label(2), Some("b"));
    }

    #[test]
    fn level_order_and_rename_merge() {
        let vals: Vec<Option<String>> = ["x", "y", "z", "y"].iter().map(|s| Some(s.to_string())).collect();
        let mut c = Column::categorical("f", &vals);
        c.set_level_order(&["z".into(), "y".into(), "x".into()]).unwrap();
        assert_eq!(c.label(0), Some("x"));
        assert_eq!(c.levels(), vec!["z", "y", "x"]);
        assert!(c.set_level_order(&["z".into()]).is_err());
        c.rename_level("z", "y").unwrap();
        assert_eq!(c.levels(), vec!["y", "x"]);
        assert_eq!(c.label(2), Some("y"));
        assert_eq!(c.label(0), Some("x"));
    }

    #[test]
    fn convert_numeric_text_roundtrip() {
        let c = Column::numeric("n", vec![Some(2.0), None, Some(1.5)]);
        let t = c.convert(ColumnKind::Categorical).unwrap();
        assert_eq!(t.levels(), vec!["1.5", "2"]);
        let back = t.convert(ColumnKind::Numeric).unwrap();
        assert_eq!(back, c);
        let bad = Column::text("t", vec![Some("abc".into())]);
        assert!(bad.convert(ColumnKind::Numeric).is_err());
    }

    #[test]
    fn set_cells_grows_table() {
        let mut t = DataTable::new("T1", "t");
        t.set_cells(0, 0, &[vec![serde_json::json!("a"), serde_json::json!(1)], vec![serde_json::json!("b"), serde_json::json!(2.5)]])
            .unwrap();
        assert_eq!(t.nrows, 2);
        assert_eq!(t.columns[0].kind(), ColumnKind::Text);
        assert_eq!(t.columns[1].number(1), Some(2.5));
        t.delete_rows(&[0]);
        assert_eq!(t.nrows, 1);
        assert_eq!(t.columns[0].display(0), "b");
    }

    #[test]
    fn number_parsing() {
        assert_eq!(parse_number("1,5", true), Some(1.5));
        assert_eq!(parse_number("1,5", false), None);
        assert_eq!(parse_number(" -2e-3 ", false), Some(-0.002));
        assert!(is_missing_token("N/A"));
        assert_eq!(format_number(10.0), "10");
        assert_eq!(format_number(0.25), "0.25");
    }

    /// Percentages, dates and times are numbers wearing a format: what is typed comes
    /// back out the same way, and the number underneath is what graphs and statistics see.
    #[test]
    fn formatted_numbers_round_trip() {
        let cases = [
            (NumberFormat::Percent, "12%", 12.0, "12%"),
            (NumberFormat::Percent, "7.5", 7.5, "7.5%"),
            (NumberFormat::Date, "2024-02-29", 19782.0, "2024-02-29"),
            (NumberFormat::Date, "1970-01-01", 0.0, "1970-01-01"),
            (NumberFormat::Date, "29.02.2024", 19782.0, "2024-02-29"),
            (NumberFormat::Time, "01:30", 5400.0, "01:30"),
            (NumberFormat::Time, "00:00:09", 9.0, "00:00:09"),
        ];
        for (format, typed, number, shown) in cases {
            let mut c = Column::numeric_formatted("V", vec![None], format);
            c.set_from_str(0, typed).unwrap_or_else(|e| panic!("{typed} as {format:?}: {e}"));
            assert_eq!(c.number(0), Some(number), "{typed} as {format:?}");
            assert_eq!(c.display(0), shown, "{typed} as {format:?}");
        }
    }

    /// Giving a numeric column a format must not touch its numbers, and taking the
    /// format off must not either.
    #[test]
    fn changing_the_format_keeps_the_numbers() {
        let c = Column::numeric("Day", vec![Some(19782.0), None, Some(0.0)]);
        let dated = c.convert_to(ColumnType::Date).unwrap();
        assert_eq!(dated.display(0), "2024-02-29");
        assert!(dated.is_missing(1));
        assert_eq!(dated.convert_to(ColumnType::Numeric).unwrap(), c);
    }

    /// Text that reads as dates becomes the day numbers behind them.
    #[test]
    fn text_converts_into_a_date_column() {
        let c = Column::text("When", vec![Some("2024-02-29".into()), Some("1970-01-02".into())]);
        let dated = c.convert_to(ColumnType::Date).unwrap();
        assert_eq!(dated.kind(), ColumnKind::Numeric);
        assert_eq!(dated.number(1), Some(1.0));
        assert!(Column::text("When", vec![Some("not a date".into())]).convert_to(ColumnType::Date).is_err());
    }
}
