//! MP3 encoding module
//! Encodes raw PCM audio to MP3 for streaming

use mp3lame_encoder::{Builder, Encoder, FlushNoGap, InterleavedPcm};
use std::mem::MaybeUninit;

/// MP3 encoder wrapper
pub struct Mp3Encoder {
    encoder: Encoder,
    channels: u16,
}

impl Mp3Encoder {
    /// Create a new MP3 encoder
    pub fn new(sample_rate: u32, channels: u16, bitrate: u32) -> Result<Self, String> {
        let mut builder = Builder::new().ok_or("Failed to create MP3 encoder builder")?;
        
        builder.set_sample_rate(sample_rate).map_err(|e| format!("set_sample_rate: {:?}", e))?;
        builder.set_num_channels(channels as u8).map_err(|e| format!("set_num_channels: {:?}", e))?;
        builder.set_brate(match bitrate {
            64 => mp3lame_encoder::Bitrate::Kbps64,
            96 => mp3lame_encoder::Bitrate::Kbps96,
            128 => mp3lame_encoder::Bitrate::Kbps128,
            160 => mp3lame_encoder::Bitrate::Kbps160,
            192 => mp3lame_encoder::Bitrate::Kbps192,
            256 => mp3lame_encoder::Bitrate::Kbps256,
            320 => mp3lame_encoder::Bitrate::Kbps320,
            _ => mp3lame_encoder::Bitrate::Kbps192,
        }).map_err(|e| format!("set_brate: {:?}", e))?;
        // Use SecondWorst quality (8) for low latency while maintaining acceptable audio quality
        // Worst (9) has too many artifacts, Best (0) has too much latency
        builder.set_quality(mp3lame_encoder::Quality::SecondWorst).map_err(|e| format!("set_quality: {:?}", e))?;

        let encoder = builder.build().map_err(|e| format!("build: {:?}", e))?;
        
        Ok(Self { encoder, channels })
    }

    /// Encode PCM samples to MP3
    pub fn encode(&mut self, samples: &[f32]) -> Result<Vec<u8>, String> {
        // Convert f32 samples to i16
        let pcm_i16: Vec<i16> = samples
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        // Prepare output buffer (worst case: input size + some extra)
        let mut mp3_buffer: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); pcm_i16.len() * 2 + 7200];

        let input = InterleavedPcm(&pcm_i16);
        let encoded_size = self.encoder.encode(input, &mut mp3_buffer).map_err(|e| format!("encode: {:?}", e))?;
        
        // Convert MaybeUninit to initialized bytes
        let result: Vec<u8> = mp3_buffer[..encoded_size]
            .iter()
            .map(|m| unsafe { m.assume_init() })
            .collect();
        
        Ok(result)
    }

    /// Flush the encoder
    pub fn flush(&mut self) -> Result<Vec<u8>, String> {
        let mut mp3_buffer: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); 7200];
        let encoded_size = self.encoder.flush::<FlushNoGap>(&mut mp3_buffer).map_err(|e| format!("flush: {:?}", e))?;
        
        let result: Vec<u8> = mp3_buffer[..encoded_size]
            .iter()
            .map(|m| unsafe { m.assume_init() })
            .collect();
        
        Ok(result)
    }
}
