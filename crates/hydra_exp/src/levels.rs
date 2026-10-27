//! Level thresholds and the `level N (x% through)` label.
//!
//! Recovered from `hydra_exp::levels::label` at `0x1012ec07c` (272 B): the
//! function scans a `[u64; 101]` table held at `0x10240d7c0` backwards for the
//! highest entry `<= exp`, then prints the position between that entry and the
//! next one as a percentage. Everything in this module is **ESTABLISHED**:
//! the table is copied verbatim from `__const` and the arithmetic is the exact
//! instruction sequence (`ucvtf` of both differences, `fmul` by 100.0, `fdiv`).

/// Minimum total experience for each level.
///
/// Verbatim from the 101-entry `u64` table at `0x10240d7c0`. Index `i` is the
/// total experience at which level `i` *starts*: `LEVELS[0] == 0`,
/// `LEVELS[1] == 2500` (level 1 needs 2,500 absorbed), … `LEVELS[100] ==
/// 7,572,500`. The table has no entry 101, which is why level 100 is the cap.
pub const LEVELS: [u64; 101] = [
    0, 2500, 5000, 10000, 17500, 27500, 40000, 55000, 72500, 92500, //
    115_000, 140_000, 167_000, 197_500, 230_000, 265_000, 302_000, 341_000, 382_000, 425_000, //
    470_000, 517_000, 566_000, 617_000, 670_000, 725_000, 781_500, 839_500, 899_000, 960_000, //
    1_022_500, 1_086_500, 1_152_000, 1_219_000, 1_287_500, 1_357_500, 1_429_000, 1_502_000,
    1_576_500, 1_652_500, //
    1_730_000, 1_808_500, 1_888_000, 1_968_500, 2_050_000, 2_132_500, 2_216_000, 2_300_500,
    2_386_000, 2_472_500, //
    2_560_000, 2_648_000, 2_736_500, 2_825_500, 2_915_000, 3_005_000, 3_095_500, 3_186_500,
    3_278_000, 3_370_000, //
    3_462_500, 3_555_500, 3_649_000, 3_743_000, 3_837_500, 3_932_500, 4_028_000, 4_124_000,
    4_220_500, 4_317_500, //
    4_415_000, 4_513_000, 4_611_500, 4_710_500, 4_810_000, 4_910_000, 5_010_500, 5_111_500,
    5_213_000, 5_315_000, //
    5_417_500, 5_520_500, 5_624_000, 5_728_000, 5_832_500, 5_937_500, 6_043_000, 6_149_000,
    6_255_500, 6_362_500, //
    6_470_000, 6_578_000, 6_686_500, 6_795_500, 6_905_000, 7_015_000, 7_125_500, 7_236_500,
    7_348_000, 7_460_000, //
    7_572_500,
];

/// The level *number* for `exp`: the highest `i` with `LEVELS[i] <= exp`.
///
/// This is the scan `levels::label` performs (`0x1012ec07c`); it is split out
/// because `report::range` prints the bare numbers (`Levels Gained: {} ({} ->
/// {})` formats them with `<u64 as Display>::fmt`, not with the label).
pub fn number(exp: u64) -> usize {
    let mut level = 0usize;
    for i in (0..LEVELS.len()).rev() {
        if exp >= LEVELS[i] {
            level = i;
            break;
        }
    }
    level
}

/// The label printed once the table runs out at the top level.
///
/// Literal at `0x10240db06`, 15 bytes: `"level 100 (cap)"`.
pub const CAP: &str = "level 100 (cap)";

/// `levels::label(exp) -> String` — `@0x1012ec07c` (**full**).
///
/// The body walks the table down from index 100 while `exp < LEVELS[i]`, so the
/// result is the highest level whose threshold the character has reached:
/// `level {i} ({pct:.1}% through)` with
/// `pct = (exp - LEVELS[i]) * 100.0 / (LEVELS[i + 1] - LEVELS[i])`.
///
/// The percentage is computed in `f64` (`ucvtf` of the two `u64` differences)
/// and formatted with the placeholder spec the binary passes for the second
/// argument (`0x10240dae8`: pieces `"level "`, `" ("`, `"% through)"`, and a
/// flags word `0x70000020` with precision 1); the level itself is `u64`
/// `<u64 as Display>::fmt` with no spec.
///
/// At or above `LEVELS[100]` the binary prints [`CAP`] instead: there is no
/// `LEVELS[101]` to divide by. `exp < LEVELS[0]` cannot happen (the table starts
/// at 0 and the argument is unsigned); the binary's "fell off the bottom" path
/// returns level 0 for it, which is the same answer.
pub fn label(exp: u64) -> String {
    let level = number(exp);
    if level + 1 >= LEVELS.len() {
        return CAP.to_string();
    }
    let low = LEVELS[level];
    let high = LEVELS[level + 1];
    let percent = (exp - low) as f64 * 100.0 / (high - low) as f64;
    format!("level {} ({:.1}% through)", level, percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_endpoints_match_the_binary() {
        assert_eq!(LEVELS.len(), 101);
        assert_eq!(LEVELS[0], 0);
        assert_eq!(LEVELS[1], 2_500);
        assert_eq!(LEVELS[100], 7_572_500);
    }

    #[test]
    fn level_zero_for_no_experience() {
        assert_eq!(label(0), "level 0 (0.0% through)");
        assert_eq!(label(2_499), "level 0 (100.0% through)");
    }

    #[test]
    fn exact_threshold_starts_the_level() {
        assert_eq!(label(2_500), "level 1 (0.0% through)");
        assert_eq!(label(10_000), "level 3 (0.0% through)");
    }

    #[test]
    fn percentage_is_between_the_two_thresholds() {
        // level 1 spans 2,500 ..= 5,000 -> 3,750 is halfway.
        assert_eq!(label(3_750), "level 1 (50.0% through)");
        // level 2 spans 5,000 ..= 10,000 -> 7,500 is halfway.
        assert_eq!(label(7_500), "level 2 (50.0% through)");
        // level 99 spans 7,460,000 ..= 7,572,500.
        assert_eq!(label(7_462_500), "level 99 (2.2% through)");
    }

    #[test]
    fn number_is_the_scan_behind_the_label() {
        assert_eq!(number(0), 0);
        assert_eq!(number(2_499), 0);
        assert_eq!(number(2_500), 1);
        assert_eq!(number(7_572_500), 100);
        assert_eq!(number(u64::MAX), 100);
    }

    #[test]
    fn level_100_is_the_cap() {
        assert_eq!(label(7_572_500), CAP);
        assert_eq!(label(u64::MAX), CAP);
        // One below the cap is still level 99 and rounds up to 100.0 %.
        assert_eq!(label(7_572_499), "level 99 (100.0% through)");
    }
}
