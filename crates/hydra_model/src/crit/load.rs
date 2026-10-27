//! Crit-table TSV parsing and the crit-entry record.
//!
//! The table is embedded in the binary: `CritTables::load` calls
//! `parse_tsv(0x103a3ff08, 0x7dcc)` — a 32204-byte copy of the generated
//! `crit.tsv` data file. It has been extracted verbatim into
//! `src/crit/data/crit.tsv`; [`EMBEDDED_TSV`] is that slice.
//!
//! Header, read out of the literal at `0x103a3ff08`:
//!
//! ```text
//! type  location  rank  damage  position  fatal  stunned  amputated  crippled
//! sleeping  dazed  limb_favored  roundtime  silenced  slowed  wound_rank
//! secondary_location  secondary_wound_rank  pattern
//! ```
//!
//! Recovered error texts (literals in `parse_tsv` / `BuildError::fmt`):
//! `file is empty; expected a header row`, `header is {}`,
//! `unknown secondary wound location {}`,
//! `secondary wound has a location or a rank but not both`, `empty pattern`.

use std::fmt;

use super::types::{Position, WoundLocation};

/// The embedded crit table, byte-for-byte from the binary (`0x103a3ff08`,
/// length `0x37dcc` = 228812).
pub const EMBEDDED_TSV: &str = include_str!("data/crit.tsv");

/// The column order of the embedded table.
pub const HEADER: [&str; 19] = [
    "type",
    "location",
    "rank",
    "damage",
    "position",
    "fatal",
    "stunned",
    "amputated",
    "crippled",
    "sleeping",
    "dazed",
    "limb_favored",
    "roundtime",
    "silenced",
    "slowed",
    "wound_rank",
    "secondary_location",
    "secondary_wound_rank",
    "pattern",
];

/// One row of the crit table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CritEntry {
    /// Damage type: `acid`, `vacuum`, `cold`, `disintegrate`, …
    pub damage_type: String,
    pub location: WoundLocation,
    pub rank: u32,
    pub damage: u32,
    pub position: Position,
    pub fatal: bool,
    /// Stun rounds.
    pub stunned: u32,
    pub amputated: bool,
    pub crippled: bool,
    pub sleeping: bool,
    pub dazed: u32,
    pub limb_favored: bool,
    pub roundtime: u32,
    pub silenced: u32,
    pub slowed: u32,
    /// 0..=3 in the embedded table.
    pub wound_rank: u32,
    pub secondary_location: Option<WoundLocation>,
    pub secondary_wound_rank: Option<u32>,
    /// The message pattern (regex source).
    pub pattern: String,
}

/// Errors `parse_tsv` can produce.
///
/// `// UNKNOWN:` the variant names; the `Display` texts are recovered from the
/// binary's literals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// `file is empty; expected a header row`
    Empty,
    /// `header is {}`
    Header { found: String },
    /// `unknown secondary wound location {}`
    UnknownSecondaryLocation { found: String },
    /// `secondary wound has a location or a rank but not both`
    SecondaryWound { line: usize },
    /// `empty pattern`
    EmptyPattern { line: usize },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Empty => write!(f, "file is empty; expected a header row"),
            BuildError::Header { found } => write!(f, "header is {found}"),
            BuildError::UnknownSecondaryLocation { found } => {
                write!(f, "unknown secondary wound location {found}")
            }
            BuildError::SecondaryWound { line } => write!(
                f,
                "line {}: secondary wound has a location or a rank but not both",
                line
            ),
            BuildError::EmptyPattern { line } => write!(f, "line {}: empty pattern", line),
        }
    }
}

impl std::error::Error for BuildError {}

fn number(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(0)
}

fn flag(text: &str) -> bool {
    matches!(text.trim(), "1" | "true" | "yes")
}

/// Parse a crit-table TSV document.
///
/// Returns the rows in file order. The header row must match [`HEADER`]
/// (the recovered check compares the whole header line against the literal
/// embedded in `CritTables::load`).
pub fn parse_tsv(text: &str) -> Result<Vec<CritEntry>, BuildError> {
    let mut lines = text.split('\n');
    let header = lines.next().unwrap_or("");
    if header.is_empty() {
        return Err(BuildError::Empty);
    }
    let expected = HEADER.join("\t");
    if header.trim_end_matches('\r') != expected {
        return Err(BuildError::Header {
            found: header.to_string(),
        });
    }
    let mut out = Vec::new();
    for (index, line) in lines.enumerate() {
        let line_no = index + 2;
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() < HEADER.len() {
            // `// UNKNOWN:` the binary pads short rows implicitly; treat a short
            // row as malformed and skip it rather than inventing columns.
            continue;
        }
        let secondary_location_text = cells[16];
        let secondary_rank_text = cells[17];
        let secondary_location = if secondary_location_text.is_empty() {
            None
        } else {
            let parsed = WoundLocation::parse(secondary_location_text);
            if parsed == WoundLocation::Unknown {
                return Err(BuildError::UnknownSecondaryLocation {
                    found: secondary_location_text.to_string(),
                });
            }
            Some(parsed)
        };
        let secondary_wound_rank = if secondary_rank_text.is_empty() {
            None
        } else {
            Some(number(secondary_rank_text))
        };
        if secondary_location.is_some() != secondary_wound_rank.is_some() {
            return Err(BuildError::SecondaryWound { line: line_no });
        }
        let pattern = cells[18].trim_end_matches('\r');
        if pattern.is_empty() {
            return Err(BuildError::EmptyPattern { line: line_no });
        }
        out.push(CritEntry {
            damage_type: cells[0].to_string(),
            location: WoundLocation::parse(cells[1]),
            rank: number(cells[2]),
            damage: number(cells[3]),
            position: Position::parse(cells[4]),
            fatal: flag(cells[5]),
            stunned: number(cells[6]),
            amputated: flag(cells[7]),
            crippled: flag(cells[8]),
            sleeping: flag(cells[9]),
            dazed: number(cells[10]),
            limb_favored: flag(cells[11]),
            roundtime: number(cells[12]),
            silenced: number(cells[13]),
            slowed: number(cells[14]),
            wound_rank: number(cells[15]),
            secondary_location,
            secondary_wound_rank,
            pattern: pattern.to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_table_parses() {
        let entries = parse_tsv(EMBEDDED_TSV).expect("embedded crit table must parse");
        assert_eq!(entries.len(), 2394);
        assert_eq!(EMBEDDED_TSV.len(), 228812);
    }

    #[test]
    fn first_row_values_are_recovered_verbatim() {
        let entries = parse_tsv(EMBEDDED_TSV).unwrap();
        let first = &entries[0];
        assert_eq!(first.damage_type, "acid");
        assert_eq!(first.location, WoundLocation::Head);
        assert_eq!(first.rank, 0);
        assert_eq!(first.damage, 0);
        assert_eq!(first.position, Position::Standing);
        assert!(!first.fatal);
        assert_eq!(first.stunned, 0);
        assert_eq!(first.wound_rank, 0);
        assert_eq!(first.pattern, "^Too bad .*? ducked.");
        let second = &entries[1];
        assert_eq!(second.rank, 1);
        assert_eq!(second.damage, 5);
        assert_eq!(second.wound_rank, 1);
        assert_eq!(
            second.pattern,
            "^Acid splatters on .*? cheek leaving a bright red burn."
        );
    }

    #[test]
    fn amputating_row_parses_flags() {
        let entries = parse_tsv(EMBEDDED_TSV).unwrap();
        let row = entries
            .iter()
            .find(|e| e.damage_type == "vacuum" && e.rank == 8 && e.location == WoundLocation::RightLeg)
            .expect("vacuum/right_leg/8 row");
        assert_eq!(row.damage, 40);
        assert_eq!(row.position, Position::Prone);
        assert_eq!(row.stunned, 10);
        assert!(row.amputated);
        assert!(!row.fatal);
        assert_eq!(row.wound_rank, 3);
        assert_eq!(row.secondary_location, None);
        assert_eq!(row.pattern, "^Right leg explodes at the hip.");
    }

    #[test]
    fn empty_input_is_reported() {
        assert_eq!(parse_tsv(""), Err(BuildError::Empty));
        assert_eq!(
            BuildError::Empty.to_string(),
            "file is empty; expected a header row"
        );
    }

    #[test]
    fn wrong_header_is_reported_with_the_found_text() {
        let err = parse_tsv("type\tlocation\nacid\thead\n").unwrap_err();
        assert_eq!(
            err,
            BuildError::Header {
                found: "type\tlocation".to_string()
            }
        );
        assert_eq!(err.to_string(), "header is type\tlocation");
    }

    #[test]
    fn empty_pattern_is_rejected() {
        let header = HEADER.join("\t");
        let row = "acid\thead\t0\t0\t\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t\t\t";
        let text = format!("{header}\n{row}\n");
        assert_eq!(parse_tsv(&text), Err(BuildError::EmptyPattern { line: 2 }));
    }

    #[test]
    fn secondary_wound_must_be_paired() {
        let header = HEADER.join("\t");
        // secondary location but no rank
        let row = "acid\thead\t0\t0\t\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tneck\t\t^x";
        let text = format!("{header}\n{row}\n");
        assert_eq!(parse_tsv(&text), Err(BuildError::SecondaryWound { line: 2 }));
        // both present is fine
        let row = "acid\thead\t0\t0\t\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tneck\t2\t^x";
        let text = format!("{header}\n{row}\n");
        let entries = parse_tsv(&text).unwrap();
        assert_eq!(entries[0].secondary_location, Some(WoundLocation::Neck));
        assert_eq!(entries[0].secondary_wound_rank, Some(2));
    }

    #[test]
    fn unknown_secondary_location_is_reported() {
        let header = HEADER.join("\t");
        let row = "acid\thead\t0\t0\t\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tleft_toe\t2\t^x";
        let text = format!("{header}\n{row}\n");
        assert_eq!(
            parse_tsv(&text),
            Err(BuildError::UnknownSecondaryLocation {
                found: "left_toe".to_string()
            })
        );
    }
}
