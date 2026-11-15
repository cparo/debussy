//! MIDI 1.0 channel messages: note off and on, polyphonic pressure, control change, program
//! change, channel pressure and pitch bend. System messages are not channel messages and are not
//! represented.

/// A MIDI 1.0 channel message, always valid: a status byte from `0x80` to `0xEF` followed by the
/// one or two data bytes its kind takes, each at most `0x7F`. Three bytes; an unused second data
/// byte is held as 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Message {
    /// The status byte, then the data bytes.
    bytes: [u8; 3],
}

/// A [`Message`] decoded, for matching. Notes, velocities, controllers, values, programs and
/// pressures are 0–127; a pitch bend is 0–16383, centred on 8192.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A note released (`0x8n`).
    NoteOff {
        /// The note number.
        note: u8,
        /// The release velocity.
        velocity: u8,
    },
    /// A note struck (`0x9n`). A velocity of 0 is, by MIDI convention, a note off.
    NoteOn {
        /// The note number.
        note: u8,
        /// The strike velocity.
        velocity: u8,
    },
    /// Pressure on one held note (`0xAn`).
    PolyPressure {
        /// The note number.
        note: u8,
        /// The pressure.
        pressure: u8,
    },
    /// A controller moved (`0xBn`).
    ControlChange {
        /// The controller number.
        controller: u8,
        /// Its new value.
        value: u8,
    },
    /// A program chosen (`0xCn`).
    ProgramChange {
        /// The program number.
        program: u8,
    },
    /// Pressure on the whole channel (`0xDn`).
    ChannelPressure {
        /// The pressure.
        pressure: u8,
    },
    /// The pitch wheel moved (`0xEn`).
    PitchBend {
        /// The 14-bit position, 8192 at rest.
        value: u16,
    },
}

/// The largest data byte.
const DATA_MAX: u8 = 0x7F;

/// The largest pitch-bend value.
const BEND_MAX: u16 = 0x3FFF;

/// The number of data bytes a channel message of status `status` takes.
fn data_len(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1,
        _ => 2,
    }
}

impl Message {
    /// A message of status kind `high` (`0x80`–`0xE0`) on `channel` with data `first` and
    /// `second`, or `None` if the channel is above 15 or a data byte above 127.
    fn channel_message(high: u8, channel: u8, first: u8, second: u8) -> Option<Message> {
        (channel <= 0x0F && first <= DATA_MAX && second <= DATA_MAX).then_some(Message {
            bytes: [high | channel, first, second],
        })
    }

    /// A note off on `channel` (0–15), or `None` if any argument is out of range.
    pub fn note_off(channel: u8, note: u8, velocity: u8) -> Option<Message> {
        Message::channel_message(0x80, channel, note, velocity)
    }

    /// A note on on `channel` (0–15), or `None` if any argument is out of range.
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Option<Message> {
        Message::channel_message(0x90, channel, note, velocity)
    }

    /// Polyphonic pressure on `channel` (0–15), or `None` if any argument is out of range.
    pub fn poly_pressure(channel: u8, note: u8, pressure: u8) -> Option<Message> {
        Message::channel_message(0xA0, channel, note, pressure)
    }

    /// A control change on `channel` (0–15), or `None` if any argument is out of range.
    pub fn control_change(channel: u8, controller: u8, value: u8) -> Option<Message> {
        Message::channel_message(0xB0, channel, controller, value)
    }

    /// A program change on `channel` (0–15), or `None` if any argument is out of range.
    pub fn program_change(channel: u8, program: u8) -> Option<Message> {
        Message::channel_message(0xC0, channel, program, 0)
    }

    /// Channel pressure on `channel` (0–15), or `None` if any argument is out of range.
    pub fn channel_pressure(channel: u8, pressure: u8) -> Option<Message> {
        Message::channel_message(0xD0, channel, pressure, 0)
    }

    /// A pitch bend on `channel` (0–15) to `value` (0–16383, 8192 at rest), or `None` if either
    /// is out of range.
    pub fn pitch_bend(channel: u8, value: u16) -> Option<Message> {
        if value > BEND_MAX {
            return None;
        }
        // Seven bits each, so the casts are exact.
        let (lsb, msb) = ((value & 0x7F) as u8, (value >> 7) as u8);
        Message::channel_message(0xE0, channel, lsb, msb)
    }

    /// The message whose bytes are exactly `bytes`, or `None` unless they are one complete
    /// channel message: a status byte from `0x80` to `0xEF` and the number of data bytes it takes,
    /// each at most `0x7F`. System messages, running status (a missing status byte) and short or
    /// long input are refused.
    pub fn from_bytes(bytes: &[u8]) -> Option<Message> {
        let (&status, data) = bytes.split_first()?;
        if !(0x80..=0xEF).contains(&status) || data.len() != data_len(status) {
            return None;
        }
        let first = data[0];
        let second = data.get(1).copied().unwrap_or(0);
        Message::channel_message(status & 0xF0, status & 0x0F, first, second)
    }

    /// The message's bytes as sent on the wire: two or three.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..1 + data_len(self.bytes[0])]
    }

    /// The status byte.
    pub fn status(&self) -> u8 {
        self.bytes[0]
    }

    /// The channel, 0–15.
    pub fn channel(&self) -> u8 {
        self.bytes[0] & 0x0F
    }

    /// The message decoded.
    pub fn kind(&self) -> Kind {
        let [status, first, second] = self.bytes;
        match status & 0xF0 {
            0x80 => Kind::NoteOff {
                note: first,
                velocity: second,
            },
            0x90 => Kind::NoteOn {
                note: first,
                velocity: second,
            },
            0xA0 => Kind::PolyPressure {
                note: first,
                pressure: second,
            },
            0xB0 => Kind::ControlChange {
                controller: first,
                value: second,
            },
            0xC0 => Kind::ProgramChange { program: first },
            0xD0 => Kind::ChannelPressure { pressure: first },
            _ => Kind::PitchBend {
                value: u16::from(first) | (u16::from(second) << 7),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), &'static str>;

    #[test]
    fn test_from_bytes_accepts_every_channel_kind() -> TestResult {
        for status in 0x80..=0xEF_u8 {
            let bytes = [status, 0x40, 0x7F];
            let len = if matches!(status & 0xF0, 0xC0 | 0xD0) {
                2
            } else {
                3
            };
            let message = Message::from_bytes(&bytes[..len]).ok_or("refused")?;
            assert_eq!(message.status(), status);
            assert_eq!(message.channel(), status & 0x0F);
            assert_eq!(message.bytes(), &bytes[..len]);
        }
        Ok(())
    }

    #[test]
    fn test_from_bytes_refuses_non_channel_status() {
        assert_eq!(Message::from_bytes(&[0x7F, 0x40, 0x40]), None);
        assert_eq!(Message::from_bytes(&[0xF0, 0x40, 0x40]), None);
        assert_eq!(Message::from_bytes(&[0xF8]), None);
        assert_eq!(Message::from_bytes(&[0xFF, 0x00, 0x00]), None);
    }

    #[test]
    fn test_from_bytes_refuses_running_status_and_wrong_lengths() {
        // Running status: data bytes without a status byte.
        assert_eq!(Message::from_bytes(&[0x40, 0x40]), None);
        assert_eq!(Message::from_bytes(&[]), None);
        assert_eq!(Message::from_bytes(&[0x90, 0x40]), None);
        assert_eq!(Message::from_bytes(&[0x90, 0x40, 0x40, 0x40]), None);
        assert_eq!(Message::from_bytes(&[0xC0, 0x01, 0x02]), None);
        assert_eq!(Message::from_bytes(&[0xD0]), None);
    }

    #[test]
    fn test_from_bytes_refuses_data_with_high_bit() {
        assert_eq!(Message::from_bytes(&[0x90, 0x80, 0x40]), None);
        assert_eq!(Message::from_bytes(&[0x90, 0x40, 0x80]), None);
        assert_eq!(Message::from_bytes(&[0xC0, 0x80]), None);
    }

    #[test]
    fn test_constructors_refuse_out_of_range() {
        assert_eq!(Message::note_on(16, 60, 100), None);
        assert_eq!(Message::note_on(0, 128, 100), None);
        assert_eq!(Message::note_off(0, 60, 128), None);
        assert_eq!(Message::control_change(0, 128, 0), None);
        assert_eq!(Message::program_change(0, 128), None);
        assert_eq!(Message::channel_pressure(0, 128), None);
        assert_eq!(Message::poly_pressure(0, 60, 128), None);
        assert_eq!(Message::pitch_bend(0, 16_384), None);
    }

    #[test]
    fn test_kind_decodes_each_constructor() -> TestResult {
        let cases = [
            (
                Message::note_off(1, 60, 10),
                Kind::NoteOff {
                    note: 60,
                    velocity: 10,
                },
            ),
            (
                Message::note_on(2, 61, 100),
                Kind::NoteOn {
                    note: 61,
                    velocity: 100,
                },
            ),
            (
                Message::poly_pressure(3, 62, 5),
                Kind::PolyPressure {
                    note: 62,
                    pressure: 5,
                },
            ),
            (
                Message::control_change(4, 7, 127),
                Kind::ControlChange {
                    controller: 7,
                    value: 127,
                },
            ),
            (
                Message::program_change(5, 9),
                Kind::ProgramChange { program: 9 },
            ),
            (
                Message::channel_pressure(6, 33),
                Kind::ChannelPressure { pressure: 33 },
            ),
            (
                Message::pitch_bend(15, 16_383),
                Kind::PitchBend { value: 16_383 },
            ),
        ];
        for (message, kind) in cases {
            assert_eq!(message.ok_or("refused")?.kind(), kind);
        }
        Ok(())
    }

    #[test]
    fn test_pitch_bend_splits_into_seven_bit_bytes() -> TestResult {
        let message = Message::pitch_bend(0, 8192).ok_or("refused")?;
        assert_eq!(message.bytes(), [0xE0, 0x00, 0x40]);
        let message = Message::pitch_bend(3, 0x1234).ok_or("refused")?;
        assert_eq!(message.bytes(), [0xE3, 0x34, 0x24]);
        assert_eq!(message.kind(), Kind::PitchBend { value: 0x1234 });
        Ok(())
    }

    #[test]
    fn test_message_is_three_bytes() {
        assert_eq!(std::mem::size_of::<Message>(), 3);
    }
}
