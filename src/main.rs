//! RustCast - Windows System Audio Streaming Server
//! 
//! Double-click to start streaming your PC's audio to any device!
//! 
//! Features:
//! - System tray icon with right-click menu
//! - MP3 streaming via HTTP
//! - Configurable port and bitrate
//! - Auto-start streaming on launch

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod encoder;
mod server;
mod tray;

use audio::AudioCapture;
use config::Config;
use encoder::Mp3Encoder;
use server::StreamServer;
use tray::{SystemTray, TrayAction};

use crossbeam_channel::{self, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    log::info!("🎵 RustCast starting...");

    // Load configuration
    let config = Config::load();
    log::info!("Configuration: port={}, bitrate={}kbps", config.port, config.bitrate);

    // Run the application
    if let Err(e) = run_app(config) {
        log::error!("Application error: {}", e);
        
        // Show error message box on Windows
        #[cfg(windows)]
        {
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            use std::iter::once;

            let message: Vec<u16> = OsStr::new(&format!("RustCast Error:\n{}", e))
                .encode_wide()
                .chain(once(0))
                .collect();
            let title: Vec<u16> = OsStr::new("RustCast")
                .encode_wide()
                .chain(once(0))
                .collect();
            
            unsafe {
                winapi::um::winuser::MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONERROR,
                );
            }
        }
        
        std::process::exit(1);
    }
}

fn run_app(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Create channels for audio data
    let (audio_tx, audio_rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = crossbeam_channel::bounded(64);
    let (mp3_tx, mp3_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = crossbeam_channel::bounded(64);

    // Initialize audio capture
    let (mut audio_capture, _) = AudioCapture::new()?;
    let sample_rate = audio_capture.sample_rate;
    let channels = audio_capture.channels;

    log::info!("Audio: {}Hz, {} channels", sample_rate, channels);

    // Create MP3 encoder
    let mut encoder = Mp3Encoder::new(sample_rate, channels, config.bitrate)?;

    // Start encoding thread
    let is_streaming = Arc::new(AtomicBool::new(false));
    let is_streaming_clone = is_streaming.clone();
    
    thread::spawn(move || {
        while let Ok(samples) = audio_rx.recv() {
            if let Ok(mp3_data) = encoder.encode(&samples) {
                if !mp3_data.is_empty() {
                    let _ = mp3_tx.try_send(mp3_data);
                }
            }
        }
    });

    // Create and start server
    let mut server = StreamServer::new(config.port);
    server.start(mp3_rx)?;

    // Start audio capture if auto_start is enabled
    if config.auto_start {
        audio_capture.start(audio_tx.clone())?;
        is_streaming.store(true, Ordering::SeqCst);
    }

    // Create system tray
    let tray = SystemTray::new(config.port)?;
    let tray_rx = tray.get_receiver();

    // Open browser automatically
    let url = format!("http://localhost:{}", config.port);
    if let Err(e) = open_browser(&url) {
        log::warn!("Could not open browser: {}", e);
    }

    log::info!("✅ RustCast ready! Open http://localhost:{}", config.port);

    // Main event loop
    loop {
        // Check for tray actions
        if let Ok(action) = tray_rx.recv_timeout(Duration::from_millis(100)) {
            match action {
                TrayAction::OpenBrowser => {
                    let _ = open_browser(&url);
                }
                TrayAction::ToggleStream => {
                    if is_streaming.load(Ordering::SeqCst) {
                        audio_capture.stop();
                        is_streaming.store(false, Ordering::SeqCst);
                        log::info!("Streaming paused");
                    } else {
                        if let Err(e) = audio_capture.start(audio_tx.clone()) {
                            log::error!("Failed to start capture: {}", e);
                        } else {
                            is_streaming.store(true, Ordering::SeqCst);
                            log::info!("Streaming resumed");
                        }
                    }
                }
                TrayAction::Settings => {
                    show_settings_dialog(&config)?;
                }
                TrayAction::Quit => {
                    log::info!("Quitting...");
                    audio_capture.stop();
                    server.stop();
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Open URL in default browser
fn open_browser(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

/// Show settings dialog
fn show_settings_dialog(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::iter::once;

        let message = format!(
            "Current Settings:\n\n\
            Port: {}\n\
            Bitrate: {} kbps\n\
            Auto-start: {}\n\n\
            To change settings, edit the config file at:\n\
            %APPDATA%\\rustcast\\RustCast\\config.json\n\n\
            Then restart RustCast.",
            config.port, config.bitrate, config.auto_start
        );
        
        let message: Vec<u16> = OsStr::new(&message)
            .encode_wide()
            .chain(once(0))
            .collect();
        let title: Vec<u16> = OsStr::new("RustCast Settings")
            .encode_wide()
            .chain(once(0))
            .collect();
        
        unsafe {
            winapi::um::winuser::MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONINFORMATION,
            );
        }
    }
    Ok(())
}
