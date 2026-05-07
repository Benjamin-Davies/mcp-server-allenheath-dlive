use anyhow::Context;
use bytes::{Buf, BufMut, BytesMut};
use midi_stream::{
    MidiCodec, RunningStatus,
    wmidi::{Channel as MidiChannel, ControlFunction, MidiMessage, Note, U7},
};
use tokio_util::codec::{Decoder, Encoder};

use crate::{
    channels::Channel,
    messages::{Level, Message},
};

const SYSEX_HEADER: [u8; 7] = [0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00];

const CONTROL_CHANNEL: ControlFunction = ControlFunction::NON_REGISTERED_PARAMETER_NUMBER_MSB;
const CONTROL_PARAM: ControlFunction = ControlFunction::NON_REGISTERED_PARAMETER_NUMBER_LSB;
const CONTROL_VALUE: ControlFunction = ControlFunction::DATA_ENTRY_MSB;

const PARAM_FADER_LEVEL: U7 = U7::from_u8_lossy(0x17);
const PARAM_MAIN_MIX_ASSIGN: U7 = U7::from_u8_lossy(0x18);
const PARAM_DCA_MG_ASSIGN: U7 = U7::from_u8_lossy(0x40);

#[derive(Debug, Default)]
pub struct DLiveCodec {
    inner: MidiCodec<RunningStatus>,
    current_channel: Channel,
    current_param: U7,
}

impl DLiveCodec {
    fn encode_sysex(
        &mut self,
        f: impl FnOnce(&mut BytesMut) -> anyhow::Result<()>,
        dst: &mut BytesMut,
    ) -> anyhow::Result<()> {
        let mut buf = BytesMut::new();
        buf.put_slice(&SYSEX_HEADER);
        f(&mut buf)?;

        self.inner
            .encode(MidiMessage::SysEx(U7::try_from_bytes(&buf)?), dst)?;

        Ok(())
    }
}

impl Encoder<Message> for DLiveCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> anyhow::Result<()> {
        match item {
            Message::MuteStatus { channel, mute } => {
                let (midi_channel, note) = channel.to_midi()?;
                let midi_channel = MidiChannel::from_index(midi_channel)?;
                let note = Note::from_u8_lossy(note);
                let velocity = if mute {
                    U7::from_u8_lossy(0x7F)
                } else {
                    U7::from_u8_lossy(0x3F)
                };
                self.inner
                    .encode(MidiMessage::NoteOn(midi_channel, note, velocity), dst)?;
                self.inner
                    .encode(MidiMessage::NoteOn(midi_channel, note, U7::MIN), dst)?;
                Ok(())
            }
            Message::GetMuteStatus { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x05, 0x09, note]);
                    Ok(())
                },
                dst,
            ),
            Message::FaderLevel { channel, level } => {
                let (midi_channel, note) = channel.to_midi()?;
                let midi_channel = MidiChannel::from_index(midi_channel)?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_CHANNEL, note.try_into()?),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_PARAM, PARAM_FADER_LEVEL),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_VALUE, level.0.try_into()?),
                    dst,
                )?;
                Ok(())
            }
            Message::GetFaderLevel { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x05, 0x0B, 0x17, note]);
                    Ok(())
                },
                dst,
            ),
            Message::MainMixAssignment { channel, assign } => {
                let (midi_channel, note) = channel.to_midi()?;
                let midi_channel = MidiChannel::from_index(midi_channel)?;
                let velocity = if assign {
                    U7::from_u8_lossy(0x7F)
                } else {
                    U7::from_u8_lossy(0x3F)
                };
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_CHANNEL, note.try_into()?),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_PARAM, PARAM_MAIN_MIX_ASSIGN),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_VALUE, velocity),
                    dst,
                )?;
                Ok(())
            }
            Message::GetMainMixAssignment { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x05, 0x0B, 0x18, note]);
                    Ok(())
                },
                dst,
            ),
            Message::SendLevel {
                channel,
                send,
                level,
            } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    let (send_midi_channel, send_note) = send.to_midi()?;
                    buf.put_slice(&[
                        midi_channel,
                        0x0D,
                        note,
                        send_midi_channel,
                        send_note,
                        level.0,
                    ]);
                    Ok(())
                },
                dst,
            ),
            Message::GetSendLevel { channel, send } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    let (send_midi_channel, send_note) = send.to_midi()?;
                    buf.put_slice(&[
                        midi_channel,
                        0x05,
                        0x0F,
                        0x0D,
                        note,
                        send_midi_channel,
                        send_note,
                    ]);
                    Ok(())
                },
                dst,
            ),
            Message::SendAssign {
                channel,
                send,
                assign,
            } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    let (send_midi_channel, send_note) = send.to_midi()?;
                    buf.put_slice(&[
                        midi_channel,
                        0x0D,
                        note,
                        send_midi_channel,
                        send_note,
                        if assign { 0x7F } else { 0x4F },
                    ]);
                    Ok(())
                },
                dst,
            ),
            Message::GetSendAssign { channel, send } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    let (send_midi_channel, send_note) = send.to_midi()?;
                    buf.put_slice(&[
                        midi_channel,
                        0x05,
                        0x0F,
                        0x0E,
                        note,
                        send_midi_channel,
                        send_note,
                    ]);
                    Ok(())
                },
                dst,
            ),
            Message::DcaAssign {
                channel,
                dca,
                assign,
            } => {
                let (midi_channel, note) = channel.to_midi()?;
                let midi_channel = MidiChannel::from_index(midi_channel)?;
                anyhow::ensure!((1..=24).contains(&dca));
                let value = dca - 1 + if assign { 0x40 } else { 0x00 };
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_CHANNEL, note.try_into()?),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_PARAM, PARAM_DCA_MG_ASSIGN),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(
                        midi_channel,
                        CONTROL_VALUE,
                        U7::from_u8_lossy(value),
                    ),
                    dst,
                )?;
                Ok(())
            }
            Message::MuteGroupAssign {
                channel,
                mute_group,
                assign,
            } => {
                let (midi_channel, note) = channel.to_midi()?;
                let midi_channel = MidiChannel::from_index(midi_channel)?;
                anyhow::ensure!((1..=8).contains(&mute_group));
                let value = mute_group - 1 + if assign { 0x58 } else { 0x18 };
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_CHANNEL, note.try_into()?),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(midi_channel, CONTROL_PARAM, PARAM_DCA_MG_ASSIGN),
                    dst,
                )?;
                self.inner.encode(
                    MidiMessage::ControlChange(
                        midi_channel,
                        CONTROL_VALUE,
                        U7::from_u8_lossy(value),
                    ),
                    dst,
                )?;
                Ok(())
            }
            Message::SetChannelName { channel, name } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x03, note]);
                    buf.put_slice(&name.0);
                    Ok(())
                },
                dst,
            ),
            Message::GetChannelName { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x01, note]);
                    Ok(())
                },
                dst,
            ),
            Message::ChannelName { channel, name } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x02, note]);
                    buf.put_slice(&name.0);
                    Ok(())
                },
                dst,
            ),
            Message::SetChannelColour { channel, colour } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x06, note, colour as u8]);
                    Ok(())
                },
                dst,
            ),
            Message::GetChannelColour { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x04, note]);
                    Ok(())
                },
                dst,
            ),
            Message::ChannelColour { channel, colour } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x05, note, colour as u8]);
                    Ok(())
                },
                dst,
            ),
        }
    }
}

impl Decoder for DLiveCodec {
    type Item = Message;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> anyhow::Result<Option<Message>> {
        while let Some(midi_msg) = self.inner.decode(src)? {
            match &midi_msg {
                MidiMessage::NoteOn(midi_channel, note, velocity) => {
                    let channel = Channel::from_midi(midi_channel.index(), u8::from(*note))?;
                    let mute = match u8::from(*velocity) {
                        0x00 => continue,
                        0x01..=0x3F => false,
                        0x40..=0x7F => true,
                        0x80..=0xFF => unreachable!(),
                    };
                    return Ok(Some(Message::MuteStatus { channel, mute }));
                }
                MidiMessage::NoteOff(_, _, _) => continue,
                MidiMessage::OwnedSysEx(data) => {
                    let bytes = U7::data_to_bytes(data);
                    let msg = decode_sysex_message(bytes).context("Invalid SysEx message")?;
                    return Ok(msg);
                }
                MidiMessage::ControlChange(midi_channel, func, value) => match *func {
                    CONTROL_CHANNEL => {
                        let channel = Channel::from_midi(midi_channel.index(), u8::from(*value))?;
                        self.current_channel = channel;
                    }
                    CONTROL_PARAM => {
                        self.current_param = *value;
                    }
                    CONTROL_VALUE => match self.current_param {
                        PARAM_FADER_LEVEL => {
                            let channel = self.current_channel;
                            let level = Level(u8::from(*value));
                            return Ok(Some(Message::FaderLevel { channel, level }));
                        }
                        PARAM_MAIN_MIX_ASSIGN => {
                            let channel = self.current_channel;
                            let assign = u8::from(*value) >= 0x40;
                            return Ok(Some(Message::MainMixAssignment { channel, assign }));
                        }
                        PARAM_DCA_MG_ASSIGN => {
                            let channel = self.current_channel;
                            let raw = u8::from(*value);
                            match raw {
                                0x00..=0x17 => {
                                    return Ok(Some(Message::DcaAssign {
                                        channel,
                                        dca: raw - 0x00 + 1,
                                        assign: false,
                                    }));
                                }
                                0x18..=0x1F => {
                                    return Ok(Some(Message::MuteGroupAssign {
                                        channel,
                                        mute_group: raw - 0x18 + 1,
                                        assign: true,
                                    }));
                                }
                                0x20..=0x3F => continue,
                                0x40..=0x57 => {
                                    return Ok(Some(Message::DcaAssign {
                                        channel,
                                        dca: raw - 0x40 + 1,
                                        assign: true,
                                    }));
                                }
                                0x58..=0x5F => {
                                    return Ok(Some(Message::MuteGroupAssign {
                                        channel,
                                        mute_group: raw - 0x58 + 1,
                                        assign: true,
                                    }));
                                }
                                0x60..=0x7F => continue,
                                0x80..=0xFF => unreachable!(),
                            }
                        }
                        _ => tracing::warn!("Unknown control parameter {}", u8::from(func.0)),
                    },
                    _ => tracing::warn!("Unknown control function {}", u8::from(func.0)),
                },
                _ => tracing::warn!("Unknown MIDI message {midi_msg:?}"),
            }
        }
        Ok(None)
    }
}

fn decode_sysex_message(raw: &[u8]) -> anyhow::Result<Option<Message>> {
    let mut raw = raw
        .strip_prefix(&SYSEX_HEADER)
        .context("Unknown SysEx message")?;
    let midi_channel = raw.get_u8();
    let kind = raw.get_u8();

    let message = match kind {
        0x01 => {
            anyhow::ensure!(raw.len() == 1);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;
            Message::GetChannelName { channel }
        }
        0x02 => {
            anyhow::ensure!(raw.len() >= 1);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;

            let name = raw
                .try_into()
                .context("received a channel name longer than 8 bytes")?;
            raw = &[];

            Message::ChannelName { channel, name }
        }
        0x03 => {
            anyhow::ensure!(raw.len() >= 1);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;

            let name = raw
                .try_into()
                .context("received a channel name longer than 8 bytes")?;
            raw = &[];

            Message::SetChannelName { channel, name }
        }
        0x04 => {
            anyhow::ensure!(raw.len() == 1);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;
            Message::GetChannelColour { channel }
        }
        0x05 if raw.len() == 2 && raw[1] <= 0x07 => {
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;
            let colour = raw.get_u8().try_into()?;
            Message::ChannelColour { channel, colour }
        }
        0x06 => {
            anyhow::ensure!(raw.len() == 2);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;
            let colour = raw.get_u8().try_into()?;
            Message::SetChannelColour { channel, colour }
        }
        0x05 => {
            anyhow::ensure!(raw.len() >= 2);
            match raw.get_u8() {
                0x09 => {
                    let note = raw.get_u8();
                    let channel = Channel::from_midi(midi_channel, note)?;

                    Message::GetMuteStatus { channel }
                }
                0x0B => match raw.get_u8() {
                    0x17 => {
                        anyhow::ensure!(raw.len() == 1);
                        let note = raw.get_u8();
                        let channel = Channel::from_midi(midi_channel, note)?;

                        Message::GetFaderLevel { channel }
                    }
                    0x18 => {
                        anyhow::ensure!(raw.len() == 1);
                        let note = raw.get_u8();
                        let channel = Channel::from_midi(midi_channel, note)?;

                        Message::GetMainMixAssignment { channel }
                    }
                    b => {
                        tracing::warn!("Unknown SysEx message: 05, 0B, {b:02X}");
                        return Ok(None);
                    }
                },
                0x0F => match raw.get_u8() {
                    0x0D => {
                        anyhow::ensure!(raw.len() == 3);
                        let note = raw.get_u8();
                        let channel = Channel::from_midi(midi_channel, note)?;

                        let send_midi_channel = raw.get_u8();
                        let send_note = raw.get_u8();
                        let send = Channel::from_midi(send_midi_channel, send_note)?;

                        Message::GetSendLevel { channel, send }
                    }
                    0x0E => {
                        anyhow::ensure!(raw.len() == 3);
                        let note = raw.get_u8();
                        let channel = Channel::from_midi(midi_channel, note)?;

                        let send_midi_channel = raw.get_u8();
                        let send_note = raw.get_u8();
                        let send = Channel::from_midi(send_midi_channel, send_note)?;

                        Message::GetSendAssign { channel, send }
                    }
                    b => {
                        tracing::warn!("Unknown SysEx message: 05, 0F, {b:02X}");
                        return Ok(None);
                    }
                },
                b => {
                    tracing::warn!("Unknown SysEx message: 05, {b:02X}");
                    return Ok(None);
                }
            }
        }
        0x0D => {
            anyhow::ensure!(raw.len() == 4);

            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;

            let send_midi_channel = raw.get_u8();
            let send_note = raw.get_u8();
            let send = Channel::from_midi(send_midi_channel, send_note)?;

            let raw_level = raw.get_u8();
            let level = Level(raw_level);

            Message::SendLevel {
                channel,
                send,
                level,
            }
        }
        0x0E => {
            anyhow::ensure!(raw.len() == 4);

            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;

            let send_midi_channel = raw.get_u8();
            let send_note = raw.get_u8();
            let send = Channel::from_midi(send_midi_channel, send_note)?;

            let raw_assign = raw.get_u8();
            let assign = raw_assign >= 0x40;

            Message::SendAssign {
                channel,
                send,
                assign,
            }
        }
        _ => {
            tracing::warn!("Unknown SysEx message kind: 0x{kind:02X}");
            return Ok(None);
        }
    };

    anyhow::ensure!(raw.is_empty());
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    use crate::{
        channels::{Channel, ChannelType},
        codecs::DLiveCodec,
        messages::Message,
    };

    #[test]
    fn test_encode_get_channel_name() {
        let message = Message::GetChannelName {
            channel: Channel(ChannelType::Input, 42),
        };

        let mut dst = BytesMut::new();
        DLiveCodec::default().encode(message, &mut dst).unwrap();

        assert_eq!(hex::encode(dst.to_vec()), "f000001a501001000b0129f7");
    }

    #[test]
    fn test_decode_get_channel_name() {
        let src = hex::decode("f000001a501001000b0129f7").unwrap();

        let message = DLiveCodec::default()
            .decode(&mut src.as_slice().into())
            .unwrap()
            .unwrap();

        assert_eq!(
            message,
            Message::GetChannelName {
                channel: Channel(ChannelType::Input, 42),
            }
        );
    }

    #[test]
    fn test_encode_get_channel_name_response() {
        let message = Message::ChannelName {
            channel: Channel(ChannelType::Input, 42),
            name: "Chan01".parse().unwrap(),
        };

        let mut dst = BytesMut::new();
        DLiveCodec::default().encode(message, &mut dst).unwrap();

        assert_eq!(
            hex::encode(dst.to_vec()),
            "f000001a501001000b02294368616e30310000f7"
        );
    }

    #[test]
    fn test_decode_get_channel_name_response() {
        let src = hex::decode("f000001a501001000b02294368616e30310000f7").unwrap();

        let message = DLiveCodec::default()
            .decode(&mut src.as_slice().into())
            .unwrap()
            .unwrap();

        assert_eq!(
            message,
            Message::ChannelName {
                channel: Channel(ChannelType::Input, 42),
                name: "Chan01".parse().unwrap()
            }
        );
    }
}
