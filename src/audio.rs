//! Audio capture module using WASAPI (Windows Audio Session API)
//! Captures system audio output (loopback)

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Audio sample data
pub type AudioSample = Vec<f32>;

/// Audio capture handle
pub struct AudioCapture {
    stream: Option<Stream>,
    pub sample_rate: u32,
    pub channels: u16,
    is_capturing: Arc<AtomicBool>,
}

impl AudioCapture {
    /// Create a new audio capture instance
    pub fn new() -> Result<(Self, Receiver<AudioSample>), Box<dyn std::error::Error>> {
        // Use WASAPI host on Windows
        let host = cpal::host_from_id(cpal::HostId::Wasapi)?;
        
        // Get the default output device for loopback capture
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        
        log::info!("Using audio device: {}", device.name().unwrap_or_default());

        // Get supported config
        let config = device.default_output_config()?;
        log::info!("Audio config: {:?}", config);

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let (_tx, rx): (Sender<AudioSample>, Receiver<AudioSample>) = crossbeam_channel::bounded(4);
        let is_capturing = Arc::new(AtomicBool::new(false));

        let capture = Self {
            stream: None,
            sample_rate,
            channels,
            is_capturing,
        };

        // We'll store device and config info for later stream creation
        Ok((capture, rx))
    }

    /// Start capturing audio
    pub fn start(&mut self, tx: Sender<AudioSample>) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        let host = cpal::host_from_id(cpal::HostId::Wasapi)?;
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        
        let config = device.default_output_config()?;
        let stream_config: StreamConfig = config.clone().into();

        let _is_capturing = self.is_capturing.clone();
        
        // Build input stream for loopback capture
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => self.build_stream::<f32>(&device, &stream_config, tx)?,
            cpal::SampleFormat::I16 => self.build_stream_i16(&device, &stream_config, tx)?,
            cpal::SampleFormat::U16 => self.build_stream_u16(&device, &stream_config, tx)?,
            _ => return Err("Unsupported sample format".into()),
        };

        stream.play()?;
        self.stream = Some(stream);
        self.is_capturing.store(true, Ordering::SeqCst);
        
        log::info!("Audio capture started");
        Ok(())
    }

    fn build_stream<T>(
        &self,
        device: &Device,
        config: &StreamConfig,
        tx: Sender<AudioSample>,
    ) -> Result<Stream, Box<dyn std::error::Error>>
    where
        T: cpal::Sample + cpal::SizedSample + Into<f32>,
    {
        let err_fn = |err| log::error!("Audio stream error: {}", err);
        
        let stream = device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.to_vec();
                match tx.try_send(samples) {
                    Ok(_) => {},
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        log::warn!("[AUDIO] 채널 버퍼 풀! 오디오 샘플 {} 개 드롭됨", data.len());
                    },
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        log::error!("[AUDIO] 채널 연결 끊김!");
                    }
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }

    fn build_stream_i16(
        &self,
        device: &Device,
        config: &StreamConfig,
        tx: Sender<AudioSample>,
    ) -> Result<Stream, Box<dyn std::error::Error>> {
        let err_fn = |err| log::error!("Audio stream error: {}", err);
        
        let stream = device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                match tx.try_send(samples) {
                    Ok(_) => {},
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        log::warn!("[AUDIO] 채널 버퍼 풀! i16 오디오 샘플 {} 개 드롭됨", data.len());
                    },
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        log::error!("[AUDIO] 채널 연결 끊김!");
                    }
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }

    fn build_stream_u16(
        &self,
        device: &Device,
        config: &StreamConfig,
        tx: Sender<AudioSample>,
    ) -> Result<Stream, Box<dyn std::error::Error>> {
        let err_fn = |err| log::error!("Audio stream error: {}", err);
        
        let stream = device.build_input_stream(
            config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).collect();
                match tx.try_send(samples) {
                    Ok(_) => {},
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        log::warn!("[AUDIO] 채널 버퍼 풀! u16 오디오 샘플 {} 개 드롭됨", data.len());
                    },
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        log::error!("[AUDIO] 채널 연결 끊김!");
                    }
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }

    /// Stop capturing audio
    pub fn stop(&mut self) {
        self.stream = None;
        self.is_capturing.store(false, Ordering::SeqCst);
        log::info!("Audio capture stopped");
    }

    /// Check if currently capturing
    pub fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
