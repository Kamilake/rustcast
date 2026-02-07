//! Opus encoding module for low-latency audio streaming
//! Opus is optimized for real-time audio with latency as low as 5ms

use audiopus::{coder::Encoder, Application, Channels, SampleRate};

/// Opus encoder wrapper
pub struct OpusEncoder {
    encoder: Encoder,
    sample_rate: u32,
    channels: u16,
    frame_size: usize,
}

impl OpusEncoder {
    /// Create a new Opus encoder
    /// 
    /// # Arguments
    /// * `sample_rate` - Input sample rate (will be resampled to 48kHz for Opus)
    /// * `channels` - Number of channels (1 or 2)
    /// * `bitrate` - Target bitrate in kbps (e.g., 64, 96, 128)
    pub fn new(sample_rate: u32, channels: u16, bitrate: u32) -> Result<Self, String> {
        // Opus works best at 48kHz
        let opus_sample_rate = match sample_rate {
            8000 => SampleRate::Hz8000,
            12000 => SampleRate::Hz12000,
            16000 => SampleRate::Hz16000,
            24000 => SampleRate::Hz24000,
            48000 => SampleRate::Hz48000,
            _ => SampleRate::Hz48000, // Default to 48kHz
        };
        
        let opus_channels = match channels {
            1 => Channels::Mono,
            _ => Channels::Stereo,
        };
        
        // Use LowDelay application for minimal latency
        let mut encoder = Encoder::new(opus_sample_rate, opus_channels, Application::LowDelay)
            .map_err(|e| format!("Failed to create Opus encoder: {:?}", e))?;
        
        // Set bitrate (in bits per second)
        encoder.set_bitrate(audiopus::Bitrate::BitsPerSecond((bitrate * 1000) as i32))
            .map_err(|e| format!("Failed to set bitrate: {:?}", e))?;
        
        // Enable DTX (Discontinuous Transmission) for efficiency
        encoder.set_dtx(false)
            .map_err(|e| format!("Failed to set DTX: {:?}", e))?;
        
        // Set complexity (0-10, lower = faster encoding)
        encoder.set_complexity(5)
            .map_err(|e| format!("Failed to set complexity: {:?}", e))?;
        
        // Frame size in samples at 48kHz
        // Opus supports: 2.5, 5, 10, 20, 40, 60, 80, 100, 120ms
        // 10ms = 480 samples at 48kHz (good balance of latency and efficiency)
        let frame_size = 480; // 10ms at 48kHz
        
        log::info!(
            "Opus encoder created: {}Hz -> 48kHz, {} channels, {}kbps, {}ms frame",
            sample_rate,
            channels,
            bitrate,
            frame_size * 1000 / 48000
        );
        
        Ok(Self {
            encoder,
            sample_rate,
            channels,
            frame_size,
        })
    }
    
    /// Create a raw Ogg page with proper flags (no BOS for audio data pages)
    /// This is needed because PacketWriter always sets BOS on first packet
    pub fn create_ogg_page(data: &[u8], serial: u32, granule: u64, page_sequence: u32, is_bos: bool) -> Vec<u8> {
        // Ogg page structure (RFC 3533)
        let mut page = Vec::with_capacity(27 + 255 + data.len());
        
        // Capture pattern
        page.extend_from_slice(b"OggS");
        
        // Version (always 0)
        page.push(0);
        
        // Header type flags: 0x02 = BOS, 0x04 = EOS, 0x01 = continued
        let header_type = if is_bos { 0x02u8 } else { 0x00u8 };
        page.push(header_type);
        
        // Granule position (8 bytes, little-endian)
        page.extend_from_slice(&granule.to_le_bytes());
        
        // Serial number (4 bytes, little-endian)
        page.extend_from_slice(&serial.to_le_bytes());
        
        // Page sequence number (4 bytes, little-endian)
        page.extend_from_slice(&page_sequence.to_le_bytes());
        
        // CRC checksum placeholder (will be filled later)
        let crc_position = page.len();
        page.extend_from_slice(&[0u8; 4]);
        
        // Calculate segment table
        let mut segments = Vec::new();
        let mut remaining = data.len();
        while remaining > 0 {
            if remaining >= 255 {
                segments.push(255u8);
                remaining -= 255;
            } else {
                segments.push(remaining as u8);
                remaining = 0;
            }
        }
        // Ensure we have at least one segment for empty packets
        if segments.is_empty() {
            segments.push(0);
        }
        
        // Number of segments
        page.push(segments.len() as u8);
        
        // Segment table
        page.extend_from_slice(&segments);
        
        // Page data
        page.extend_from_slice(data);
        
        // Calculate and insert CRC-32
        let crc = ogg_crc32(&page);
        page[crc_position..crc_position + 4].copy_from_slice(&crc.to_le_bytes());
        
        page
    }
    
    /// Get Ogg Opus headers with a specific serial (for new client streams)
    pub fn get_headers_with_serial(channels: u16, sample_rate: u32, serial: u32) -> Vec<u8> {
        // OpusHead header (RFC 7845)
        let mut opus_head = Vec::with_capacity(19);
        opus_head.extend_from_slice(b"OpusHead");           // Magic signature
        opus_head.push(1);                                   // Version
        opus_head.push(channels as u8);                      // Channel count
        opus_head.extend_from_slice(&(312u16).to_le_bytes());  // Pre-skip (samples) - standard value
        opus_head.extend_from_slice(&sample_rate.to_le_bytes()); // Original input sample rate
        opus_head.extend_from_slice(&(0i16).to_le_bytes());  // Output gain
        opus_head.push(0);                                   // Channel mapping family
        
        // OpusTags header
        let vendor = b"RustCast";
        let mut opus_tags = Vec::new();
        opus_tags.extend_from_slice(b"OpusTags");
        opus_tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        opus_tags.extend_from_slice(vendor);
        opus_tags.extend_from_slice(&0u32.to_le_bytes()); // No user comments
        
        let mut result = Vec::new();
        
        // Page 0: OpusHead (BOS)
        result.extend(Self::create_ogg_page(&opus_head, serial, 0, 0, true));
        
        // Page 1: OpusTags (not BOS)
        result.extend(Self::create_ogg_page(&opus_tags, serial, 0, 1, false));
        
        result
    }
    
    /// Wrap a raw Opus packet in an Ogg page (for audio data)
    pub fn wrap_opus_packet(packet: &[u8], serial: u32, granule: u64, page_sequence: u32) -> Vec<u8> {
        Self::create_ogg_page(packet, serial, granule, page_sequence, false)
    }
    
    /// Get frame size in samples
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Encode PCM samples to raw Opus packets (without Ogg container)
    /// Returns a list of encoded Opus packets
    pub fn encode_raw(&mut self, samples: &[f32]) -> Result<Vec<Vec<u8>>, String> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        
        // Resample if necessary (simple linear interpolation for speed)
        let resampled: Vec<f32> = if self.sample_rate != 48000 {
            let ratio = 48000.0 / self.sample_rate as f64;
            let input_frames = samples.len() / self.channels as usize;
            let output_frames = (input_frames as f64 * ratio) as usize;
            let output_len = output_frames * self.channels as usize;
            
            let mut output = Vec::with_capacity(output_len);
            
            for i in 0..output_frames {
                let src_pos = (i as f64 / ratio).min((input_frames - 1) as f64);
                let src_idx = src_pos as usize;
                let frac = src_pos - src_idx as f64;
                
                for ch in 0..self.channels as usize {
                    let idx0 = src_idx * self.channels as usize + ch;
                    let idx1 = ((src_idx + 1).min(input_frames - 1)) * self.channels as usize + ch;
                    
                    // Linear interpolation
                    let sample = samples[idx0] as f64 * (1.0 - frac) + samples[idx1] as f64 * frac;
                    output.push(sample as f32);
                }
            }
            output
        } else {
            samples.to_vec()
        };
        
        // Convert f32 to i16 for Opus
        let samples_i16: Vec<i16> = resampled
            .iter()
            .map(|&s: &f32| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        
        // Process in frame-sized chunks
        let samples_per_frame = self.frame_size * self.channels as usize;
        let mut packets = Vec::new();
        
        for chunk in samples_i16.chunks(samples_per_frame) {
            if chunk.len() < samples_per_frame {
                // Pad incomplete frame with silence
                let mut padded = chunk.to_vec();
                padded.resize(samples_per_frame, 0);
                packets.push(self.encode_frame_raw(&padded)?);
            } else {
                packets.push(self.encode_frame_raw(chunk)?);
            }
        }
        
        Ok(packets)
    }
    
    fn encode_frame_raw(&mut self, samples: &[i16]) -> Result<Vec<u8>, String> {
        // Opus output buffer (max packet size)
        let mut opus_data = vec![0u8; 4000];
        
        let encoded_len = self.encoder
            .encode(samples, &mut opus_data)
            .map_err(|e| format!("Opus encode error: {:?}", e))?;
        
        opus_data.truncate(encoded_len.into());
        
        Ok(opus_data)
    }
}

/// CRC-32 lookup table for Ogg (polynomial 0x04C11DB7)
const CRC_LOOKUP: [u32; 256] = generate_crc_table();

/// Generate CRC lookup table at compile time
const fn generate_crc_table() -> [u32; 256] {
    const POLYNOMIAL: u32 = 0x04C11DB7;
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = (i as u32) << 24;
        let mut j = 0;
        while j < 8 {
            if crc & 0x80000000 != 0 {
                crc = (crc << 1) ^ POLYNOMIAL;
            } else {
                crc <<= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// CRC-32 for Ogg pages
fn ogg_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        let index = ((crc >> 24) ^ (byte as u32)) as u8;
        crc = (crc << 8) ^ CRC_LOOKUP[index as usize];
    }
    crc
}

/// Generate a random serial number for Ogg stream
fn rand_serial() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(12345)
}
