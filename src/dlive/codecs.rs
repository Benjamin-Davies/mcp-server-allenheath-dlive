use anyhow::Context;
use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::dlive::messages::{Channel, Message};

const SYSEX_HEADER: [u8; 8] = [0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00];

#[derive(Debug)]
pub struct DLiveCodec;

impl Encoder<Message> for DLiveCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> anyhow::Result<()> {
        match item {
            Message::GetChannelName { channel } => {
                let (midi_channel, note) = channel.to_midi()?;
                dst.put_slice(&SYSEX_HEADER);
                dst.put_u8(midi_channel);
                dst.put_u8(0x01);
                dst.put_u8(note);
                dst.put_u8(0xF7);
            }
            Message::GetChannelNameResponse { channel, name } => {
                let (midi_channel, note) = channel.to_midi()?;
                dst.put_slice(&SYSEX_HEADER);
                dst.put_u8(midi_channel);
                dst.put_u8(0x02);
                dst.put_u8(note);
                dst.put_slice(name.as_bytes());
                dst.put_bytes(0x00, usize::saturating_sub(8, name.len()));
                dst.put_u8(0xF7);
            }
        }
        Ok(())
    }
}

impl Decoder for DLiveCodec {
    type Item = Message;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> anyhow::Result<Option<Message>> {
        match src.first() {
            Some(0xF0) => {
                let Some(body_len) = src.iter().skip(1).position(|&b| b & 0x80 != 0) else {
                    return Ok(None);
                };
                let raw = src.split_to(body_len + 2);
                decode_sysex_message(raw).context("Invalid SysEx message")
            }
            Some(status) => anyhow::bail!("Unknown MIDI status: 0x{status:02X}"),
            None => Ok(None),
        }
    }
}

fn decode_sysex_message(mut raw: BytesMut) -> anyhow::Result<Option<Message>> {
    anyhow::ensure!(raw.len() >= SYSEX_HEADER.len() + 3);
    anyhow::ensure!(raw.starts_with(&SYSEX_HEADER));
    anyhow::ensure!(raw.ends_with(&[0xF7]));

    raw.advance(SYSEX_HEADER.len());
    let midi_channel = raw.get_u8();
    let kind = raw.get_u8();

    let message = match kind {
        0x01 => {
            anyhow::ensure!(raw.len() >= 2);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;
            Message::GetChannelName { channel }
        }
        0x02 => {
            anyhow::ensure!(raw.len() >= 2);
            let note = raw.get_u8();
            let channel = Channel::from_midi(midi_channel, note)?;

            let name_bytes = raw.split_to(raw.len() - 1);
            let mut name = String::from_utf8(name_bytes.to_vec())?;
            if let Some(len) = name.find('\0') {
                name.truncate(len);
            }

            Message::GetChannelNameResponse { channel, name }
        }
        _ => anyhow::bail!("Unknown SysEx message kind: 0x{kind:02X}"),
    };

    anyhow::ensure!(raw.get_u8() == 0xF7);
    anyhow::ensure!(raw.is_empty());
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    use crate::dlive::{Channel, codecs::DLiveCodec, messages::Message};

    #[test]
    fn test_encode_get_channel_name() {
        let message = Message::GetChannelName {
            channel: Channel::Input(42),
        };

        let mut dst = BytesMut::new();
        DLiveCodec.encode(message, &mut dst).unwrap();

        assert_eq!(hex::encode(dst.to_vec()), "f000001a501001000b0129f7");
    }

    #[test]
    fn test_decode_get_channel_name() {
        let src = hex::decode("f000001a501001000b0129f7").unwrap();

        let message = DLiveCodec
            .decode(&mut src.as_slice().into())
            .unwrap()
            .unwrap();

        assert_eq!(
            message,
            Message::GetChannelName {
                channel: Channel::Input(42),
            }
        );
    }

    #[test]
    fn test_encode_get_channel_name_response() {
        let message = Message::GetChannelNameResponse {
            channel: Channel::Input(42),
            name: "Chan01".to_owned(),
        };

        let mut dst = BytesMut::new();
        DLiveCodec.encode(message, &mut dst).unwrap();

        assert_eq!(
            hex::encode(dst.to_vec()),
            "f000001a501001000b02294368616e30310000f7"
        );
    }

    #[test]
    fn test_decode_get_channel_name_response() {
        let src = hex::decode("f000001a501001000b02294368616e30310000f7").unwrap();

        let message = DLiveCodec
            .decode(&mut src.as_slice().into())
            .unwrap()
            .unwrap();

        assert_eq!(
            message,
            Message::GetChannelNameResponse {
                channel: Channel::Input(42),
                name: "Chan01".to_owned()
            }
        );
    }
}
