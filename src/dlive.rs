use anyhow::{Context, Result, bail};
use std::net::IpAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use wmidi::{Channel, ControlFunction, MidiMessage, Note, U7};

const DLIVE_TCP_PORT: u16 = 51325;
const DLIVE_DEFAULT_BASE_CHANNEL: u8 = 11; // 0-indexed, = MIDI channel 12

// SysEx manufacturer header: F0 00 00 1A 50 10 01 00
const SYSEX_HEADER: &[u8] = &[0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00];

// NRPN CC numbers
const NRPN_MSB: ControlFunction = ControlFunction::NON_REGISTERED_PARAMETER_NUMBER_MSB; // CC 99
const NRPN_LSB: ControlFunction = ControlFunction::NON_REGISTERED_PARAMETER_NUMBER_LSB; // CC 98
const DATA_ENTRY: ControlFunction = ControlFunction::DATA_ENTRY_MSB; // CC 6

// NRPN parameter IDs
const PARAM_FADER_LEVEL: U7 = U7::from_u8_lossy(0x17);

pub struct DLive {
    sock: TcpStream,
    base_channel: u8,
    input_names: Option<Vec<String>>,
    mix_names: Option<Vec<String>>,
}

impl DLive {
    /// Connects to the DLive mix-rack without TLS.
    pub async fn new(addr: IpAddr) -> Result<Self> {
        let sock = TcpStream::connect((addr, DLIVE_TCP_PORT))
            .await
            .with_context(|| format!("Failed to connect to dLive MixRack at {addr}"))?;

        Ok(DLive {
            sock,
            base_channel: DLIVE_DEFAULT_BASE_CHANNEL,
            input_names: None,
            mix_names: None,
        })
    }

    fn channel(&self, offset: u8) -> Result<Channel> {
        Channel::from_index(self.base_channel + offset).with_context(|| {
            format!(
                "MIDI channel index {} out of range",
                self.base_channel + offset
            )
        })
    }

    async fn send_midi(&mut self, msg: MidiMessage<'_>) -> Result<()> {
        let mut buf = vec![0u8; msg.bytes_size()];
        msg.copy_to_slice(&mut buf)
            .context("Failed to serialise MIDI message")?;
        self.sock
            .write_all(&buf)
            .await
            .context("Failed to send MIDI message to dLive")
    }

    async fn send_sysex(&mut self, payload: &[u8]) -> Result<()> {
        // Assemble header + payload, then wrap as U7 (SysEx bytes are all 7-bit)
        let data: Vec<U7> = SYSEX_HEADER
            .iter()
            .chain(payload)
            .map(|&b| U7::from_u8_lossy(b))
            .collect();

        self.send_midi(MidiMessage::SysEx(&data)).await
    }

    async fn read_sysex_reply(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            self.sock
                .read_exact(&mut byte)
                .await
                .context("Failed to read SysEx reply from dLive")?;
            buf.push(byte[0]);
            if byte[0] == 0xF7 {
                break;
            }
        }
        Ok(buf)
    }

    async fn fetch_channel_name(&mut self, midi_n: u8, ch: u8) -> Result<String> {
        let on_byte = midi_n & 0x0F;
        self.send_sysex(&[on_byte, 0x01, ch]).await?;

        let reply = self.read_sysex_reply().await?;
        // Reply layout: SYSEX_HEADER + [0N, 0x02, CH, <name bytes...>, 0xF7]
        let name_start = SYSEX_HEADER.len() + 3;
        let name_bytes: Vec<u8> = reply[name_start..]
            .iter()
            .take_while(|&&b| b != 0xF7)
            .copied()
            .collect();
        Ok(String::from_utf8_lossy(&name_bytes).trim().to_string())
    }

    /// Gets the names of all 128 inputs, fetching from the desk if not cached.
    pub async fn list_inputs(&mut self) -> Result<&[String]> {
        if self.input_names.is_none() {
            let midi_n = self.base_channel;
            let mut names = Vec::with_capacity(128);
            for ch in 0x00u8..=0x7F {
                names.push(
                    self.fetch_channel_name(midi_n, ch)
                        .await
                        .with_context(|| format!("Failed to fetch name for input {}", ch + 1))?,
                );
            }
            self.input_names = Some(names);
        }
        Ok(self.input_names.as_deref().unwrap())
    }

    /// Gets the names of all 62 mono aux mixes, fetching from the desk if not cached.
    pub async fn list_outputs(&mut self) -> Result<&[String]> {
        if self.mix_names.is_none() {
            let midi_n = self.base_channel + 2;
            let mut names = Vec::with_capacity(62);
            for ch in 0x00u8..=0x3D {
                names.push(
                    self.fetch_channel_name(midi_n, ch)
                        .await
                        .with_context(|| format!("Failed to fetch name for aux mix {}", ch + 1))?,
                );
            }
            self.mix_names = Some(names);
        }
        Ok(self.mix_names.as_deref().unwrap())
    }

    /// Gets the current fader level for an input channel in dB.
    ///
    /// Per the spec, fader level is queried via SysEx:
    ///   SysEx Header, 0N, 05, 0B, 17, CH, F7
    /// The desk replies with the NRPN fader level message; the level byte LV
    /// is the last data byte before F7.
    pub async fn fader_level(&mut self, input: u32) -> Result<f32> {
        // FIX: was sending wrong NRPN CC messages; spec requires a SysEx query.
        // FIX: was ignoring `input` (ch_note was computed but never used).
        // FIX: was reading a fixed 9-byte reply; SysEx replies are variable-length.
        let on_byte = self.base_channel & 0x0F;
        let ch = input as u8;

        // Query: SysEx Header, 0N, 05, 0B, 17, CH, F7
        self.send_sysex(&[on_byte, 0x05, 0x0B, 0x17, ch])
            .await
            .with_context(|| format!("Failed to send fader level query for input {input}"))?;

        let reply = self
            .read_sysex_reply()
            .await
            .with_context(|| format!("Failed to read fader level reply for input {input}"))?;

        // Reply layout: SYSEX_HEADER + [0N, 05, 0B, 17, CH, LV, F7]
        // LV is at index SYSEX_HEADER.len() + 5
        let lv_index = SYSEX_HEADER.len() + 5;
        if reply.len() <= lv_index {
            bail!(
                "Fader level reply too short ({} bytes) for input {input}",
                reply.len()
            );
        }
        Ok(dlive_value_to_db(reply[lv_index]))
    }

    /// Sets the send level from `input` to `mix` in dB.
    pub async fn set_fader_level(&mut self, mix: u32, input: u32, db: f32) -> Result<()> {
        let lv = U7::from_u8_lossy(db_to_dlive_value(db));
        let input_ch = U7::from_u8_lossy(input as u8);

        // FIX: was using channel.index() (= self.base_channel, a full channel index)
        // instead of self.base_channel & 0x0F (the lower nibble), which is what 0N
        // means in the spec — the MIDI channel nibble for the input.
        let n = self.base_channel & 0x0F;
        let snd_n = (self.base_channel + 2) & 0x0F;
        let snd_ch = (mix as u8) & 0x3D;

        // AUX send level SysEx: SysEx Header, 0N, 0D, CH, SndN, SndCH, LV, F7
        self.send_sysex(&[n, 0x0D, input_ch.into(), snd_n, snd_ch, lv.into()])
            .await
            .with_context(|| format!("Failed to set fader for input {input}, mix {mix} to {db}dB"))
    }
}

/// Converts a dLive 7-bit fader value (0x00–0x7F) to dB.
/// 0x00 = -inf, 0x6B = 0dB, 0x7F = +10dB.
fn dlive_value_to_db(value: u8) -> f32 {
    if value == 0 {
        return f32::NEG_INFINITY;
    }
    (value as f32 - 107.0) / 2.0
}

/// Converts a dB value to a dLive 7-bit fader value.
fn db_to_dlive_value(gain: f32) -> u8 {
    if gain == f32::NEG_INFINITY || gain < -90.0 {
        return 0x00;
    }
    (2.0 * gain + 107.0).clamp(0.0, 127.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlive_value_to_db() {
        assert_eq!(dlive_value_to_db(0x7F), 10.0);
        assert_eq!(dlive_value_to_db(0x75), 5.0);
        assert_eq!(dlive_value_to_db(0x6B), 0.0);
        assert_eq!(dlive_value_to_db(0x61), -5.0);
        assert_eq!(dlive_value_to_db(0x57), -10.0);
        assert_eq!(dlive_value_to_db(0x4D), -15.0);
        assert_eq!(dlive_value_to_db(0x43), -20.0);
        assert_eq!(dlive_value_to_db(0x39), -25.0);
        assert_eq!(dlive_value_to_db(0x2F), -30.0);
        assert_eq!(dlive_value_to_db(0x25), -35.0);
        assert_eq!(dlive_value_to_db(0x1B), -40.0);
        assert_eq!(dlive_value_to_db(0x11), -45.0);
        assert_eq!(dlive_value_to_db(0x00), f32::NEG_INFINITY);
    }

    #[test]
    fn test_db_to_dlive_value() {
        assert_eq!(db_to_dlive_value(10.0), 0x7F);
        assert_eq!(db_to_dlive_value(5.0), 0x75);
        assert_eq!(db_to_dlive_value(0.0), 0x6B);
        assert_eq!(db_to_dlive_value(-5.0), 0x61);
        assert_eq!(db_to_dlive_value(-10.0), 0x57);
        assert_eq!(db_to_dlive_value(-15.0), 0x4D);
        assert_eq!(db_to_dlive_value(-20.0), 0x43);
        assert_eq!(db_to_dlive_value(-25.0), 0x39);
        assert_eq!(db_to_dlive_value(-30.0), 0x2F);
        assert_eq!(db_to_dlive_value(-35.0), 0x25);
        assert_eq!(db_to_dlive_value(-40.0), 0x1B);
        assert_eq!(db_to_dlive_value(-45.0), 0x11);
        assert_eq!(db_to_dlive_value(f32::NEG_INFINITY), 0x00);
    }
}
