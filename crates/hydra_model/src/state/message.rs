//! Speech / whisper line classification.
//!
//! `message::classify` (4472 B) uses `Run::object` and the recovered literals
//! `"'s"`, `"to you "` (the speech forms) plus a `' '` separator.

/// Who said what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub speaker: String,
    pub text: String,
    pub whisper: bool,
    /// The speaker addressed you directly (`"to you"`).
    pub to_you: bool,
}

/// @0x0000000101b87cb0 (4472 B)
pub fn classify(line: &str) -> Option<Message> {
    let line = line.trim();
    let (speaker, rest, whisper) = if let Some((speaker, rest)) = line.split_once(" whispers to you, \"") {
        (speaker, rest, true)
    } else if let Some((speaker, rest)) = line.split_once(" whispers, \"") {
        (speaker, rest, true)
    } else if let Some((speaker, rest)) = line.split_once(" says, \"") {
        (speaker, rest, false)
    } else if let Some((speaker, rest)) = line.split_once(" asks, \"") {
        (speaker, rest, false)
    } else {
        return None;
    };
    let text = rest.trim_end_matches('"').trim().to_string();
    let to_you = line.contains("to you");
    Some(Message {
        speaker: speaker.trim().to_string(),
        text,
        whisper,
        to_you,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_is_recognised() {
        let message = classify("Kithar says, \"Hello there.\"").unwrap();
        assert_eq!(message.speaker, "Kithar");
        assert_eq!(message.text, "Hello there.");
        assert!(!message.whisper);
        assert!(!message.to_you);
    }

    #[test]
    fn whispers_are_flagged() {
        let message = classify("Ael whispers, \"quietly\"").unwrap();
        assert!(message.whisper);
        assert_eq!(message.text, "quietly");
    }

    #[test]
    fn addressed_messages_are_flagged() {
        let message = classify("Ael whispers to you, \"psst\"").unwrap();
        assert!(message.to_you);
        assert!(message.whisper);
    }

    #[test]
    fn prose_is_ignored() {
        assert_eq!(classify("You stand still."), None);
    }
}
