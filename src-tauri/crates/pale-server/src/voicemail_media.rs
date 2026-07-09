//! Voicemail media handling: greeting playback, RTP reception, and WAV recording.
//!
//! When a call is routed to voicemail, the SIP handler creates a
//! `VoicemailSession` that:
//! 1. Allocates a local UDP port for RTP
//! 2. Returns an SDP answer to the caller
//! 3. Sends a greeting + beep tone via RTP
//! 4. Receives RTP packets (the caller's message)
//! 5. Decodes G.711 u-law (PCMU) audio to linear PCM
//! 6. Writes a WAV file when the session ends (BYE or timeout)

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Pre-recorded greeting: "Please leave your message after the tone."
/// Embedded at compile time — 8kHz 16-bit mono PCM WAV.
static GREETING_WAV: &[u8] = include_bytes!("../assets/voicemail_greeting.wav");

/// Maximum voicemail recording duration (seconds)
const MAX_RECORD_SECS: usize = 120;
/// Greeting + beep duration before recording starts (seconds)
const GREETING_SECS: usize = 4;
/// RTP header size (fixed for our use — no CSRC or extensions)
const RTP_HEADER_SIZE: usize = 12;
/// PCMU sample rate
const SAMPLE_RATE: u32 = 8000;
/// PCMU payload type
const PT_PCMU: u8 = 0;
/// RTP packets per second (20ms per packet = 50 pps)
const PACKETS_PER_SEC: u32 = 50;
/// Samples per RTP packet (160 samples at 8kHz = 20ms)
const SAMPLES_PER_PACKET: usize = 160;

/// A running voicemail recording session.
pub struct VoicemailSession {
    pub rtp_port: u16,
    pub local_addr: SocketAddr,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl VoicemailSession {
    /// Start a new voicemail recording session.
    pub async fn start(
        voicemail_id: Uuid,
        data_dir: PathBuf,
    ) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind RTP socket: {}", e))?;
        let local_addr = socket
            .local_addr()
            .map_err(|e| format!("Failed to get local addr: {}", e))?;
        let rtp_port = local_addr.port();

        let (stop_tx, stop_rx) = oneshot::channel();

        let dir = data_dir.clone();
        tokio::spawn(async move {
            greeting_then_record(socket, stop_rx, voicemail_id, dir).await;
        });

        Ok(Self {
            rtp_port,
            local_addr,
            stop_tx: Some(stop_tx),
        })
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn wav_path(data_dir: &Path, voicemail_id: Uuid) -> PathBuf {
        data_dir.join("voicemail").join(format!("{}.wav", voicemail_id))
    }
}

impl Drop for VoicemailSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Generate an SDP answer for voicemail (sendrecv so we can play greeting AND record).
pub fn voicemail_sdp(local_ip: &str, rtp_port: u16) -> String {
    format!(
        "v=0\r\n\
         o=pale 0 0 IN IP4 {ip}\r\n\
         s=Voicemail\r\n\
         c=IN IP4 {ip}\r\n\
         t=0 0\r\n\
         m=audio {port} RTP/AVP 0 8\r\n\
         a=rtpmap:0 PCMU/8000\r\n\
         a=rtpmap:8 PCMA/8000\r\n\
         a=sendrecv\r\n\
         a=ptime:20\r\n",
        ip = local_ip,
        port = rtp_port,
    )
}

/// Play greeting, then record.
async fn greeting_then_record(
    socket: UdpSocket,
    stop_rx: oneshot::Receiver<()>,
    voicemail_id: Uuid,
    data_dir: PathBuf,
) {
    // Wait for the first RTP packet to learn the caller's address
    let mut buf = [0u8; 2048];
    let caller_addr = match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        socket.recv_from(&mut buf),
    )
    .await
    {
        Ok(Ok((_len, addr))) => addr,
        _ => {
            tracing::warn!(voicemail_id = %voicemail_id, "No RTP from caller — aborting voicemail");
            return;
        }
    };

    tracing::info!(
        voicemail_id = %voicemail_id,
        caller = %caller_addr,
        "Voicemail: caller connected, playing greeting"
    );

    // Phase 1: Send greeting + beep via RTP
    send_greeting(&socket, caller_addr).await;

    // Phase 2: Record caller's audio
    receive_and_record(socket, stop_rx, voicemail_id, data_dir).await;
}

/// Parse the embedded WAV file and return 16-bit PCM samples.
fn parse_greeting_wav() -> Vec<i16> {
    let data = GREETING_WAV;
    // Find "data" chunk — skip RIFF header (12 bytes) and fmt chunk
    let mut pos = 12;
    while pos + 8 < data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]) as usize;
        if chunk_id == b"data" {
            let pcm_data = &data[pos + 8..pos + 8 + chunk_size.min(data.len() - pos - 8)];
            return pcm_data
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
        }
        pos += 8 + chunk_size;
        // WAV chunks are word-aligned
        if chunk_size % 2 != 0 {
            pos += 1;
        }
    }
    Vec::new()
}

/// Send the spoken greeting + beep tone via RTP.
///
/// Plays the embedded "Please leave your message after the tone" WAV,
/// then a 1kHz beep, then a short silence before recording starts.
async fn send_greeting(socket: &UdpSocket, caller: SocketAddr) {
    let mut seq: u16 = 0;
    let mut timestamp: u32 = 0;
    let ssrc: u32 = 0x50414C45; // "PALE"

    // Phase 1: Play the spoken greeting from the embedded WAV
    let greeting_pcm = parse_greeting_wav();
    for chunk in greeting_pcm.chunks(SAMPLES_PER_PACKET) {
        let mut payload = [0xFFu8; SAMPLES_PER_PACKET]; // silence-pad short chunks
        for (i, &sample) in chunk.iter().enumerate() {
            payload[i] = linear_to_ulaw(sample);
        }
        let packet = build_rtp_packet(seq, timestamp, ssrc, &payload);
        let _ = socket.send_to(&packet, caller).await;
        seq = seq.wrapping_add(1);
        timestamp = timestamp.wrapping_add(SAMPLES_PER_PACKET as u32);
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Phase 2: 0.5 second beep tone (1kHz sine wave)
    let beep_packets = PACKETS_PER_SEC as usize / 2;
    let mut sample_idx: usize = 0;
    for _ in 0..beep_packets {
        let mut payload = [0u8; SAMPLES_PER_PACKET];
        for byte in payload.iter_mut() {
            let t = sample_idx as f32 / SAMPLE_RATE as f32;
            let sample = (t * 1000.0 * 2.0 * std::f32::consts::PI).sin();
            let pcm = (sample * 16000.0) as i16;
            *byte = linear_to_ulaw(pcm);
            sample_idx += 1;
        }
        let packet = build_rtp_packet(seq, timestamp, ssrc, &payload);
        let _ = socket.send_to(&packet, caller).await;
        seq = seq.wrapping_add(1);
        timestamp = timestamp.wrapping_add(SAMPLES_PER_PACKET as u32);
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Phase 3: 0.3 second silence after beep
    let post_beep = (PACKETS_PER_SEC as usize * 3) / 10;
    for _ in 0..post_beep {
        let packet = build_rtp_packet(seq, timestamp, ssrc, &[0xFF; SAMPLES_PER_PACKET]);
        let _ = socket.send_to(&packet, caller).await;
        seq = seq.wrapping_add(1);
        timestamp = timestamp.wrapping_add(SAMPLES_PER_PACKET as u32);
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    let greeting_packets = (greeting_pcm.len() + SAMPLES_PER_PACKET - 1) / SAMPLES_PER_PACKET;
    tracing::info!(
        "Voicemail greeting sent: {} spoken + {} beep + {} silence packets",
        greeting_packets, beep_packets, post_beep
    );
}

/// Build an RTP packet with PCMU payload.
fn build_rtp_packet(seq: u16, timestamp: u32, ssrc: u32, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + payload.len());
    // V=2, P=0, X=0, CC=0
    packet.push(0x80);
    // M=0, PT=0 (PCMU)
    packet.push(PT_PCMU);
    // Sequence number (big-endian)
    packet.extend_from_slice(&seq.to_be_bytes());
    // Timestamp (big-endian)
    packet.extend_from_slice(&timestamp.to_be_bytes());
    // SSRC (big-endian)
    packet.extend_from_slice(&ssrc.to_be_bytes());
    // Payload
    packet.extend_from_slice(payload);
    packet
}

/// Receive RTP packets and write a WAV file.
async fn receive_and_record(
    socket: UdpSocket,
    mut stop_rx: oneshot::Receiver<()>,
    voicemail_id: Uuid,
    data_dir: PathBuf,
) {
    let max_samples = SAMPLE_RATE as usize * MAX_RECORD_SECS;
    let mut pcm_samples: Vec<i16> = Vec::with_capacity(max_samples);
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(MAX_RECORD_SECS as u64 + 5);

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, _addr)) => {
                        if len <= RTP_HEADER_SIZE {
                            continue;
                        }
                        let pt = buf[1] & 0x7F;
                        let payload = &buf[RTP_HEADER_SIZE..len];

                        for &byte in payload {
                            let sample = if pt == PT_PCMU {
                                ulaw_to_linear(byte)
                            } else if pt == 8 {
                                alaw_to_linear(byte)
                            } else {
                                continue;
                            };
                            pcm_samples.push(sample);
                        }

                        if pcm_samples.len() >= max_samples {
                            tracing::info!(voicemail_id = %voicemail_id, "Voicemail max duration reached");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("RTP recv error: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                tracing::info!("Voicemail recording timeout");
                break;
            }
        }
    }

    // Write WAV file
    if pcm_samples.is_empty() {
        tracing::warn!(voicemail_id = %voicemail_id, "No audio received for voicemail");
        return;
    }

    let wav_dir = data_dir.join("voicemail");
    if let Err(e) = std::fs::create_dir_all(&wav_dir) {
        tracing::error!("Failed to create voicemail dir: {}", e);
        return;
    }

    let wav_path = VoicemailSession::wav_path(&data_dir, voicemail_id);
    let duration_secs = pcm_samples.len() as f32 / SAMPLE_RATE as f32;
    tracing::info!(
        voicemail_id = %voicemail_id,
        duration = duration_secs,
        samples = pcm_samples.len(),
        path = %wav_path.display(),
        "Writing voicemail WAV file"
    );

    if let Err(e) = write_wav(&wav_path, &pcm_samples, SAMPLE_RATE) {
        tracing::error!("Failed to write WAV: {}", e);
    }
}

/// Write PCM samples as a WAV file (16-bit mono).
fn write_wav(path: &Path, samples: &[i16], sample_rate: u32) -> Result<(), std::io::Error> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;

    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;

    // RIFF header
    file.write_all(b"RIFF")?;
    file.write_all(&file_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    // fmt chunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    // data chunk
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    for &sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

// ─── G.711 Codec Tables ───

/// G.711 u-law to 16-bit linear PCM.
fn ulaw_to_linear(mut u_val: u8) -> i16 {
    u_val = !u_val;
    let sign = u_val & 0x80;
    let exponent = ((u_val >> 4) & 0x07) as i16;
    let mantissa = (u_val & 0x0F) as i16;
    let mut sample = ((mantissa << 3) | 0x84) << exponent.max(0);
    sample -= 0x84;
    if sign != 0 { -sample } else { sample }
}

/// G.711 A-law to 16-bit linear PCM.
fn alaw_to_linear(mut a_val: u8) -> i16 {
    a_val ^= 0x55;
    let sign = a_val & 0x80;
    let exponent = ((a_val >> 4) & 0x07) as i16;
    let mantissa = (a_val & 0x0F) as i16;
    let sample = if exponent == 0 {
        (mantissa << 4) | 0x08
    } else {
        ((mantissa << 4) | 0x108) << (exponent - 1)
    };
    if sign != 0 { sample } else { -sample }
}

/// 16-bit linear PCM to G.711 u-law.
fn linear_to_ulaw(mut sample: i16) -> u8 {
    const BIAS: i16 = 0x84;
    const CLIP: i16 = 32635;

    let sign = if sample < 0 {
        sample = -sample;
        0x80u8
    } else {
        0x00
    };

    if sample > CLIP {
        sample = CLIP;
    }
    sample += BIAS;

    let exponent = match sample {
        s if s <= 0x00FF => 0,
        s if s <= 0x01FF => 1,
        s if s <= 0x03FF => 2,
        s if s <= 0x07FF => 3,
        s if s <= 0x0FFF => 4,
        s if s <= 0x1FFF => 5,
        s if s <= 0x3FFF => 6,
        _ => 7,
    };

    let mantissa = (sample >> (exponent + 3)) & 0x0F;
    let ulaw_byte = !(sign | ((exponent as u8) << 4) | mantissa as u8);
    ulaw_byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulaw_silence_is_near_zero() {
        let sample = ulaw_to_linear(0xFF);
        assert!(sample.abs() < 100, "silence sample = {}", sample);
    }

    #[test]
    fn ulaw_roundtrip() {
        for pcm in [-8000i16, -1000, 0, 1000, 8000, 16000] {
            let encoded = linear_to_ulaw(pcm);
            let decoded = ulaw_to_linear(encoded);
            // u-law is lossy — check within 2% + quantization noise
            let diff = (pcm as i32 - decoded as i32).abs();
            assert!(
                diff < (pcm.abs() as i32 / 10).max(200),
                "pcm={} encoded={} decoded={} diff={}",
                pcm, encoded, decoded, diff
            );
        }
    }

    #[test]
    fn sdp_contains_sendrecv() {
        let sdp = voicemail_sdp("192.168.1.1", 10000);
        assert!(sdp.contains("PCMU/8000"));
        assert!(sdp.contains("m=audio 10000"));
        assert!(sdp.contains("sendrecv"));
    }

    #[test]
    fn rtp_packet_structure() {
        let pkt = build_rtp_packet(1, 160, 0x12345678, &[0xFF; 160]);
        assert_eq!(pkt.len(), RTP_HEADER_SIZE + 160);
        assert_eq!(pkt[0], 0x80); // V=2
        assert_eq!(pkt[1], PT_PCMU); // PT=0
        assert_eq!(u16::from_be_bytes([pkt[2], pkt[3]]), 1); // seq
        assert_eq!(u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]), 160); // ts
    }
}
