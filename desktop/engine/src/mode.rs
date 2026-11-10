//! How deep a run goes: Quick (the default) or Deep Think. Same rules as `lib/core.sh`.
//!
//! Quick: one blind round with short seat answers, the clarify check alongside it, and a
//! short verdict. Deep Think: the coach first, up to four rounds with the argue-against-the-
//! leader step, long seat answers, and a long verdict.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Quick,
    Deep,
}

impl Mode {
    /// `quick` or `deep` (also `deep-think`). Empty means Quick.
    pub fn parse(s: &str) -> Result<Mode, String> {
        match s.trim().to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "" | "quick" | "fast" => Ok(Mode::Quick),
            "deep" | "deep-think" | "deepthink" | "think" => Ok(Mode::Deep),
            other => Err(format!("Unknown mode '{other}'. Use quick or deep.")),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Mode::Quick => "quick",
            Mode::Deep => "deep",
        }
    }
    /// The words people see.
    pub fn title(self) -> &'static str {
        match self {
            Mode::Quick => "Quick",
            Mode::Deep => "Deep Think",
        }
    }
    /// Rounds when `--rounds` is not given: one blind round, or up to four.
    pub fn default_rounds(self) -> u32 {
        match self {
            Mode::Quick => 1,
            Mode::Deep => 4,
        }
    }
    /// About how many words a seat writes.
    pub fn seat_words(self) -> u32 {
        match self {
            Mode::Quick => 250,
            Mode::Deep => 800,
        }
    }
    /// The coach always runs in Deep Think.
    pub fn coach(self) -> bool {
        self == Mode::Deep
    }
    /// The last section of every seat's prompt: how long, and what to lead with.
    pub fn seat_rule(self) -> String {
        match self {
            Mode::Quick => format!(
                "## LENGTH\nKeep your answer to about {} words. Lead with your pick and how sure you are. Then give the main trade-off, the risks that could hurt this person, and the facts you do not have that would change the pick.",
                self.seat_words()
            ),
            Mode::Deep => format!(
                "## LENGTH\nTake up to about {} words. Lead with your pick and how sure you are. Then reason it through: what each serious option costs and buys, the risks that could hurt this person, the facts you do not have and how to find them out, and the steps in order.",
                self.seat_words()
            ),
        }
    }
}

/// A round's prompt with the mode's length rule at the end.
pub fn with_rule(prompt: &str, mode: Mode) -> String {
    format!("{}\n\n{}\n", prompt.trim_end_matches('\n'), mode.seat_rule())
}

/// Seconds the bench measured per part of a run (2026-11-10d, three seats on signed-in apps).
const COACH_SECS: u32 = 45;
const CHAIR_SECS: u32 = 30;

/// About how long a run takes, in whole minutes (at least 1). The app shows it before asking:
/// "Deep Think: about N minutes". Same numbers as `estimateMinutes` in the app (ui/src/state.ts).
pub fn estimate_minutes(mode: Mode, rounds: u32) -> u32 {
    let rounds = rounds.clamp(1, crate::council::MAX_ROUNDS);
    let (seat, verdict, coach) = match mode {
        Mode::Quick => (75, 30, 0),
        Mode::Deep => (140, 75, COACH_SECS),
    };
    // A chair ruling follows every round of a deliberation; one round has none.
    let chair = if rounds > 1 { rounds * CHAIR_SECS } else { 0 };
    let secs = coach + rounds * seat + chair + verdict;
    secs.div_ceil(60).max(1)
}

/// The line shown before a Deep Think run asks: "Deep Think: about 14 minutes."
pub fn estimate_line(mode: Mode, rounds: u32) -> String {
    let n = estimate_minutes(mode, rounds);
    format!("{}: about {n} minute{}.", mode.title(), if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_names() {
        assert_eq!(Mode::parse("").unwrap(), Mode::Quick);
        assert_eq!(Mode::parse("Deep Think").unwrap(), Mode::Deep);
        assert_eq!(Mode::parse("deep_think").unwrap(), Mode::Deep);
        assert!(Mode::parse("slow").is_err());
        assert_eq!(Mode::Deep.title(), "Deep Think");
        assert_eq!(Mode::Quick.default_rounds(), 1);
        assert_eq!(Mode::Deep.default_rounds(), 4);
        assert!(Mode::Deep.coach() && !Mode::Quick.coach());
    }

    #[test]
    fn seat_rule_caps_the_words() {
        assert!(Mode::Quick.seat_rule().contains("about 250 words"));
        assert!(Mode::Deep.seat_rule().contains("up to about 800 words"));
        assert_eq!(with_rule("Q\n\n", Mode::Quick), format!("Q\n\n{}\n", Mode::Quick.seat_rule()));
    }

    #[test]
    fn estimates() {
        assert_eq!(estimate_minutes(Mode::Quick, 1), 2);
        assert_eq!(estimate_minutes(Mode::Deep, 4), 14, "the app shows the same (ui/src/state.ts)");
        assert_eq!(estimate_minutes(Mode::Deep, 2), 8);
        assert!(estimate_minutes(Mode::Deep, 4) > estimate_minutes(Mode::Deep, 2));
        assert_eq!(estimate_line(Mode::Deep, 4), format!("Deep Think: about {} minutes.", estimate_minutes(Mode::Deep, 4)));
    }
}
