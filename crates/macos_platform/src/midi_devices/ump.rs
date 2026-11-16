//! MIDI 1.0 channel voice messages to and from Universal MIDI Packet words, the untrusted-input
//! boundary for live MIDI (`docs/architecture/platform.md` "MIDI devices").
//!
//! Only message type `0x2`, a MIDI 1.0 channel voice message in one 32-bit word, is accepted:
//! `0x2` in the top four bits, then the group, the status byte and two data bytes. Every other
//! type (utility, system, SysEx, MIDI 2.0) and every invalid channel message is refused.

use midi::Message;

/// The UMP message type of a MIDI 1.0 channel voice message.
const CHANNEL_VOICE_1: u32 = 0x2;

/// The number of 32-bit words a UMP message of type `message_type` takes.
fn message_words(message_type: u32) -> usize {
    match message_type {
        0x0..=0x2 | 0x6 | 0x7 => 1,
        0x3 | 0x4 | 0x8..=0xA => 2,
        0xB | 0xC => 3,
        _ => 4,
    }
}

/// Calls `each` once per UMP message in `words`: with the message if it is a valid MIDI 1.0
/// channel voice message, otherwise with `None`. A message cut short by the end of `words` is one
/// `None`. Never allocates or panics.
pub(crate) fn parse(words: &[u32], mut each: impl FnMut(Option<Message>)) {
    let mut rest = words;
    while let Some(&first) = rest.first() {
        let message_type = first >> 28;
        let len = message_words(message_type);
        if rest.len() < len {
            each(None);
            return;
        }
        rest = &rest[len..];
        if message_type != CHANNEL_VOICE_1 {
            each(None);
            continue;
        }
        let [_, status, first_data, second_data] = first.to_be_bytes();
        // Program change and channel pressure take one data byte.
        let bytes = [status, first_data, second_data];
        let len = if matches!(status & 0xF0, 0xC0 | 0xD0) {
            2
        } else {
            3
        };
        each(Message::from_bytes(&bytes[..len]));
    }
}

/// `message` as one UMP word in group 0.
pub(crate) fn word(message: &Message) -> u32 {
    let bytes = message.bytes();
    let byte = |i: usize| u32::from(bytes.get(i).copied().unwrap_or(0));
    (CHANNEL_VOICE_1 << 28) | (byte(0) << 16) | (byte(1) << 8) | byte(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn parsed(words: &[u32]) -> Vec<Option<Message>> {
        let mut out = Vec::new();
        parse(words, |m| out.push(m));
        out
    }

    #[test]
    fn test_parse_valid_note_on() -> TestResult {
        let note = Message::note_on(3, 60, 100).ok_or("bad note")?;
        assert_eq!(parsed(&[0x2393_3C64]), [Some(note)]);
        Ok(())
    }

    #[test]
    fn test_parse_one_data_byte_messages() -> TestResult {
        let program = Message::program_change(1, 5).ok_or("bad program")?;
        // The unused byte is ignored.
        assert_eq!(parsed(&[0x20C1_05FF]), [Some(program)]);
        Ok(())
    }

    #[test]
    fn test_parse_invalid_status_and_data() {
        // A data byte as status, a system status, and a data byte above 127.
        assert_eq!(
            parsed(&[0x2040_4040, 0x20F8_0000, 0x2090_8040]),
            [None, None, None]
        );
    }

    #[test]
    fn test_parse_sysex_is_refused_whole() -> TestResult {
        let note = Message::note_off(0, 1, 2).ok_or("bad note")?;
        // A two-word SysEx packet, then a note off.
        assert_eq!(
            parsed(&[0x3006_7E7F, 0x0601_0000, 0x2080_0102]),
            [None, Some(note)]
        );
        Ok(())
    }

    #[test]
    fn test_parse_utility_is_refused() {
        assert_eq!(parsed(&[0x0000_0000, 0x0010_1234]), [None, None]);
    }

    #[test]
    fn test_parse_truncated_words() {
        assert_eq!(parsed(&[0x4090_3C00]), [None]);
        assert_eq!(parsed(&[0x5000_0000, 0, 0]), [None]);
        assert!(parsed(&[]).is_empty());
    }

    #[test]
    fn test_parse_random_words_never_panic() {
        // A fixed xorshift generator, so failures reproduce.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u32
        };
        for _ in 0..10_000 {
            let len = (next() % 9) as usize;
            let words: Vec<u32> = (0..len).map(|_| next()).collect();
            let mut count = 0;
            parse(&words, |message| {
                count += 1;
                if let Some(m) = message {
                    assert!((0x80..=0xEF).contains(&m.status()));
                }
            });
            assert!(count <= words.len());
        }
    }

    #[test]
    fn test_word_round_trips() -> TestResult {
        for message in [
            Message::note_on(15, 127, 1).ok_or("bad")?,
            Message::channel_pressure(2, 64).ok_or("bad")?,
            Message::pitch_bend(0, 0x3FFF).ok_or("bad")?,
        ] {
            assert_eq!(parsed(&[word(&message)]), [Some(message)]);
        }
        Ok(())
    }
}
