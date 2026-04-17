use anyhow::Context;
use bytes::{Buf, BufMut, BytesMut};
use midi_stream::{
    MidiCodec, RunningStatus,
    wmidi::{Channel as MidiChannel, ControlFunction, MidiMessage, U7},
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
            Message::GetFaderLevel { channel } => self.encode_sysex(
                |buf| {
                    let (midi_channel, note) = channel.to_midi()?;
                    buf.put_slice(&[midi_channel, 0x05, 0x0B, 0x17, note]);
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
        }
    }
}

impl Decoder for DLiveCodec {
    type Item = Message;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> anyhow::Result<Option<Message>> {
        while let Some(midi_msg) = self.inner.decode(src)? {
            match &midi_msg {
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
                        _ => anyhow::bail!("Unknown control parameter {}", u8::from(func.0)),
                    },
                    _ => anyhow::bail!("Unknown control function {}", u8::from(func.0)),
                },
                _ => anyhow::bail!("Unknown MIDI message {midi_msg:?}"),
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
        0x05 => {
            anyhow::ensure!(raw.len() >= 2);
            match raw.get_u16() {
                0x0B_17 => {
                    anyhow::ensure!(raw.len() == 1);
                    let note = raw.get_u8();
                    let channel = Channel::from_midi(midi_channel, note)?;

                    Message::GetFaderLevel { channel }
                }
                0x0F_0D => {
                    anyhow::ensure!(raw.len() == 3);
                    let note = raw.get_u8();
                    let channel = Channel::from_midi(midi_channel, note)?;

                    let send_midi_channel = raw.get_u8();
                    let send_note = raw.get_u8();
                    let send = Channel::from_midi(send_midi_channel, send_note)?;

                    Message::GetSendLevel { channel, send }
                }
                _ => todo!(),
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
        _ => anyhow::bail!("Unknown SysEx message kind: 0x{kind:02X}"),
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
