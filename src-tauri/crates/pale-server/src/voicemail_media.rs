//! Voicemail media handling: RTP reception and WAV recording.
//!
//! When a call is routed to voicemail, the SIP handler creates a
//! `VoicemailSession` that:
//! 1. Allocates a local UDP port for RTP
//! 2. Returns an SDP answer to the caller
//! 3. Receives RTP packets in the background
//! 4. Decodes G.711 μ-law (PCMU) audio to linear PCM
//! 5. Writes a WAV file when the session ends (BYE or timeout)
//! 6. Stores the file and updates the voicemail record

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Maximum voicemail recording duration (seconds)
const MAX_RECORD_SECS: usize = 120;
/// RTP header size
const RTP_HEADER_SIZE: usize = 12;
/// PCMU sample rate
const SAMPLE_RATE: u32 = 8000;
/// PCMU payload type
const PT_PCMU: u8 = 0;
/// PCMA payload type
const PT_PCMA: u8 = 8;

/// A running voicemail recording session.
pub struct VoicemailSession {
    pub rtp_port: u16,
    pub local_addr: SocketAddr,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl VoicemailSession {
    /// Start a new voicemail recording session.
    ///
    /// Binds a UDP socket, spawns a background task to receive RTP, and
    /// returns the session. Call `stop()` or let the session drop to
    /// finish recording.
    pub async fn start(
        voicemail_id: Uuid,
        data_dir: std::path::PathBuf,
    ) -> Result<Self, String> {
        // Bind to any available port
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind RTP socket: {}", e))?;
        let local_addr = socket
            .local_addr()
            .map_err(|e| format!("Failed to get local addr: {}", e))?;
        let rtp_port = local_addr.port();

        let (stop_tx, stop_rx) = oneshot::channel();

        // Spawn background RTP receiver
        let vm_id = voicemail_id;
        let dir = data_dir.clone();
        tokio::spawn(async move {
            receive_and_record(socket, stop_rx, vm_id, dir).await;
        });

        Ok(Self {
            rtp_port,
            local_addr,
            stop_tx: Some(stop_tx),
        })
    }

    /// Stop the recording and finalize the WAV file.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Get the file path where the WAV will be written.
    pub fn wav_path(data_dir: &std::path::Path, voicemail_id: Uuid) -> std::path::PathBuf {
        data_dir.join("voicemail").join(format!("{}.wav", voicemail_id))
    }
}

impl Drop for VoicemailSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Generate an SDP answer for voicemail recording.
///
/// Offers PCMU (G.711 μ-law) at 8000 Hz on the given RTP port.
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
         a=recvonly\r\n",
        ip = local_ip,
        port = rtp_port,
    )
}

/// Receive RTP packets and write a WAV file.
async fn receive_and_record(
    socket: UdpSocket,
    mut stop_rx: oneshot::Receiver<()>,
    voicemail_id: Uuid,
    data_dir: std::path::PathBuf,
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
                            } else if pt == PT_PCMA {
                                alaw_to_linear(byte)
                            } else {
                                continue;
                            };
                            pcm_samples.push(sample);
                        }

                        if pcm_samples.len() >= max_samples {
                            tracing::info!(
                                voicemail_id = %voicemail_id,
                                "Voicemail max duration reached"
                            );
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
fn write_wav(
    path: &std::path::Path,
    samples: &[i16],
    sample_rate: u32,
) -> Result<(), std::io::Error> {
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
    file.write_all(&16u32.to_le_bytes())?; // chunk size
    file.write_all(&1u16.to_le_bytes())?; // PCM format
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

/// G.711 μ-law to 16-bit linear PCM conversion.
fn ulaw_to_linear(mut u_val: u8) -> i16 {
    u_val = !u_val;
    let sign = u_val & 0x80;
    let exponent = ((u_val >> 4) & 0x07) as i16;
    let mantissa = (u_val & 0x0F) as i16;
    let mut sample = ((mantissa << 3) | 0x84) << (exponent.max(0));
    sample -= 0x84;
    if sign != 0 {
        sample = -sample;
    }
    sample
}

/// G.711 A-law to 16-bit linear PCM conversion.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulaw_silence_is_near_zero() {
        // μ-law 0xFF encodes near-silence
        let sample = ulaw_to_linear(0xFF);
        assert!(sample.abs() < 100, "silence sample = {}", sample);
    }

    #[test]
    fn alaw_roundtrip_sign() {
        let pos = alaw_to_linear(0x55); // A-law with XOR
        let neg = alaw_to_linear(0xD5);
        assert!(pos > 0 || neg > 0, "one should be positive");
    }

    #[test]
    fn sdp_contains_pcmu() {
        let sdp = voicemail_sdp("192.168.1.1", 10000);
        assert!(sdp.contains("PCMU/8000"));
        assert!(sdp.contains("m=audio 10000"));
        assert!(sdp.contains("recvonly"));
    }
}
