//! Hotbar icons.
//!
//! Recovered literals (0x103994665 .. 0x103994e00):
//!
//! ```text
//! "an icon names a sheet and its cell, or an image"
//! "a sheet's cells are counted from 1"
//! struct Sheet struct IconDe struct Sheets
//! path sheet cell default none image sheet_cell
//! grayscale border border_width border_end border_dir
//! horizontal vertical diagonal_down diagonal_up radial square
//! IconMode GradientDir
//! ```
//!
//! so an icon is either an image (`path`, plus a border and a gradient) or a
//! cell of a sheet (`sheet`, `cell`, counted from 1).  `Sheet::cell` @0x1018d54cc
//! is 28 bytes: `if columns != 0 && cell != 0 { cell } else { 64 }`.

/// One sprite sheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// The sheet's file name (without the extension).
    pub sheet: String,
    /// How many cells across.
    pub columns: u32,
    /// The cell size in pixels; `0` means the default.
    pub cell: u32,
    /// The background colour name, if the sheet has one.
    pub background: Option<String>,
}

/// The default cell size when a sheet does not state one.
pub const DEFAULT_CELL: u32 = 64;

impl Sheet {
    /// `@0x1018d54cc` — the cell size, defaulting to [`DEFAULT_CELL`].
    pub fn cell(&self) -> u32 {
        if self.columns != 0 && self.cell != 0 {
            self.cell
        } else {
            DEFAULT_CELL
        }
    }
}

/// The sheet collection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheets {
    sheets: Vec<Sheet>,
}

impl Sheets {
    /// `@0x1018d54e8` — parse `sheet:cell` pairs.
    ///
    /// The recovered error text is `"an icon names a sheet and its cell, or an
    /// image"` and `"a sheet's cells are counted from 1"`, so a cell index of
    /// `0` is rejected.
    pub fn parse(src: &str) -> Result<Sheets, String> {
        let mut sheets: Vec<Sheet> = Vec::new();
        for part in src.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (name, cell) = match part.rsplit_once(':') {
                Some((name, cell)) => (name.trim(), cell.trim().parse::<u32>().map_err(|_| {
                    "a sheet's cells are counted from 1".to_string()
                })?),
                None => (part, 0),
            };
            if cell == 0 && part.contains(':') {
                return Err("a sheet's cells are counted from 1".to_string());
            }
            match sheets.iter_mut().find(|s| s.sheet == name) {
                Some(have) => have.columns = have.columns.max(cell),
                None => sheets.push(Sheet {
                    sheet: name.to_string(),
                    columns: cell,
                    cell: DEFAULT_CELL,
                    background: None,
                }),
            }
        }
        if sheets.is_empty() {
            return Err("an icon names a sheet and its cell, or an image".to_string());
        }
        Ok(Sheets { sheets })
    }

    /// The sheets.
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// Look a sheet up by name.
    pub fn get(&self, name: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.sheet == name)
    }
}

/// `@0x1018d5794` — the cell index a descriptor names, `1`-based.
///
/// The recovered function subtracts one and saturates at zero.
pub fn cell_in(sheet: &Sheet, cell: u32) -> u32 {
    if sheet.columns == 0 {
        0
    } else {
        cell.saturating_sub(1).min(sheet.columns.saturating_sub(1))
    }
}

/// `@0x1018d5610` — the path a sheet's image lives at.
pub fn source(sheet: &Sheet) -> String {
    if sheet.sheet.is_empty() {
        String::new()
    } else {
        format!("{}.png", sheet.sheet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_default_to_64() {
        let s = Sheet::default();
        assert_eq!(s.cell(), DEFAULT_CELL);
        // A sheet with columns but no cell size still uses the default.
        let s = Sheet {
            sheet: "x".into(),
            columns: 8,
            cell: 0,
            background: None,
        };
        assert_eq!(s.cell(), DEFAULT_CELL);
        let s = Sheet {
            sheet: "x".into(),
            columns: 8,
            cell: 32,
            background: None,
        };
        assert_eq!(s.cell(), 32);
    }

    #[test]
    fn parsing_sheets() {
        let s = Sheets::parse("weapons:8, armor:4").unwrap();
        assert_eq!(s.sheets().len(), 2);
        assert_eq!(s.get("weapons").unwrap().columns, 8);
        assert_eq!(s.get("nope"), None);
    }

    #[test]
    fn parsing_rejects_bad_input() {
        assert_eq!(
            Sheets::parse("").unwrap_err(),
            "an icon names a sheet and its cell, or an image"
        );
        assert_eq!(
            Sheets::parse("weapons:0").unwrap_err(),
            "a sheet's cells are counted from 1"
        );
        assert_eq!(
            Sheets::parse("weapons:x").unwrap_err(),
            "a sheet's cells are counted from 1"
        );
    }

    #[test]
    fn cell_indices_are_one_based() {
        let s = Sheet {
            sheet: "x".into(),
            columns: 4,
            cell: 32,
            background: None,
        };
        assert_eq!(cell_in(&s, 1), 0);
        assert_eq!(cell_in(&s, 2), 1);
        assert_eq!(cell_in(&s, 99), 3);
        assert_eq!(cell_in(&s, 0), 0);
        assert_eq!(cell_in(&Sheet::default(), 3), 0);
    }

    #[test]
    fn sources_are_png() {
        let s = Sheet {
            sheet: "weapons".into(),
            ..Sheet::default()
        };
        assert_eq!(source(&s), "weapons.png");
        assert_eq!(source(&Sheet::default()), "");
    }
}
