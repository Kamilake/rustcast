//! RustCast - Windows System Audio Streaming Server
//!
//! Double-click to start streaming your PC's audio to any device!
//!
//! Features:
//! - Native settings panel with streaming controls
//! - System tray icon with right-click menu
//! - MP3 streaming via HTTP
//! - Configurable port and bitrate
//! - Auto-start streaming on launch

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod encoder;
#[cfg(windows)]
mod gui;
mod server;

use audio::AudioCapture;
use config::Config;
use encoder::Mp3Encoder;
#[cfg(windows)]
use gui::{AppState, GuiAction};
use server::StreamServer;

use crossbeam_channel::{self, Receiver, Sender};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

fn main() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    log::info!("🎵 RustCast starting...");

    // Load configuration
    let config = Config::load();
    log::info!(
        "Configuration: port={}, bitrate={}kbps",
        config.port,
        config.bitrate
    );

    // Run the application
    #[cfg(windows)]
    {
        if let Err(e) = run_app_with_gui(config) {
            log::error!("Application error: {}", e);
            show_error_message(&format!("RustCast Error:\n{}", e));
            std::process::exit(1);
        }
    }

    #[cfg(not(windows))]
    {
        log::error!("RustCast only supports Windows");
        std::process::exit(1);
    }
}

/// Show error message box on Windows
#[cfg(windows)]
fn show_error_message(message: &str) {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    let message: Vec<u16> = OsStr::new(message).encode_wide().chain(once(0)).collect();
    let title: Vec<u16> = OsStr::new("RustCast").encode_wide().chain(once(0)).collect();

    unsafe {
        winapi::um::winuser::MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONERROR,
        );
    }
}

/// Run application with native Windows GUI
#[cfg(windows)]
fn run_app_with_gui(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Create channels for audio data
    let (audio_tx, audio_rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) =
        crossbeam_channel::bounded(64);
    let (mp3_tx, mp3_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = crossbeam_channel::bounded(64);

    // Initialize audio capture (get sample rate/channels info only)
    let (audio_capture_info, _) = AudioCapture::new()?;
    let sample_rate = audio_capture_info.sample_rate;
    let channels = audio_capture_info.channels;
    drop(audio_capture_info); // Drop to release resources, we'll create new one in audio thread

    log::info!("Audio: {}Hz, {} channels", sample_rate, channels);

    // Create MP3 encoder
    let mut encoder = Mp3Encoder::new(sample_rate, channels, config.bitrate)?;

    // Streaming state flags
    let is_streaming = Arc::new(AtomicBool::new(false));
    let client_count = Arc::new(AtomicUsize::new(0));
    let should_stream = Arc::new(AtomicBool::new(config.auto_start));
    let app_quit = Arc::new(AtomicBool::new(false));

    // Start encoding thread
    thread::spawn(move || {
        while let Ok(samples) = audio_rx.recv() {
            if let Ok(mp3_data) = encoder.encode(&samples) {
                if !mp3_data.is_empty() {
                    let _ = mp3_tx.try_send(mp3_data);
                }
            }
        }
    });

    // Create and start server with shared client_count
    let mut server = StreamServer::with_client_count(config.port, client_count.clone());
    server.start(mp3_rx)?;

    // Audio control thread - handles audio capture in its own thread
    let audio_tx_clone = audio_tx.clone();
    let is_streaming_clone = is_streaming.clone();
    let should_stream_clone = should_stream.clone();
    let app_quit_clone = app_quit.clone();

    thread::spawn(move || {
        let mut audio_capture: Option<AudioCapture> = None;

        loop {
            if app_quit_clone.load(Ordering::SeqCst) {
                break;
            }

            let want_stream = should_stream_clone.load(Ordering::SeqCst);
            let currently_streaming = audio_capture.is_some();

            if want_stream && !currently_streaming {
                // Start streaming
                match AudioCapture::new() {
                    Ok((mut capture, _)) => {
                        if let Err(e) = capture.start(audio_tx_clone.clone()) {
                            log::error!("Failed to start audio capture: {}", e);
                        } else {
                            audio_capture = Some(capture);
                            is_streaming_clone.store(true, Ordering::SeqCst);
                            log::info!("Audio streaming started");
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create audio capture: {}", e);
                    }
                }
            } else if !want_stream && currently_streaming {
                // Stop streaming
                if let Some(mut capture) = audio_capture.take() {
                    capture.stop();
                }
                is_streaming_clone.store(false, Ordering::SeqCst);
                log::info!("Audio streaming stopped");
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Cleanup
        if let Some(mut capture) = audio_capture {
            capture.stop();
        }
    });

    // Create shared state for GUI
    let app_state = Arc::new(AppState {
        is_streaming: is_streaming.clone(),
        client_count: client_count.clone(),
        config: RefCell::new(config.clone()),
    });

    // Create channel for GUI actions
    let (action_tx, action_rx) = mpsc::channel::<GuiAction>();

    // Spawn thread to handle GUI actions
    let should_stream_for_actions = should_stream.clone();
    let app_quit_for_actions = app_quit.clone();
    let port = config.port;

    thread::spawn(move || {
        while let Ok(action) = action_rx.recv() {
            match action {
                GuiAction::ToggleStream => {
                    let current = should_stream_for_actions.load(Ordering::SeqCst);
                    should_stream_for_actions.store(!current, Ordering::SeqCst);
                    log::info!("Toggle streaming: {} -> {}", current, !current);
                }
                GuiAction::SaveConfig(new_config) => {
                    if let Err(e) = new_config.save() {
                        log::error!("Failed to save config: {}", e);
                    } else {
                        log::info!("Config saved");
                    }
                }
                GuiAction::OpenBrowser => {
                    let url = format!("http://localhost:{}", port);
                    if let Err(e) = open_browser(&url) {
                        log::warn!("Could not open browser: {}", e);
                    }
                }
                GuiAction::Quit => {
                    log::info!("Quitting...");
                    app_quit_for_actions.store(true, Ordering::SeqCst);
                    std::process::exit(0);
                }
            }
        }
    });

    log::info!("✅ RustCast ready! Open http://localhost:{}", config.port);

    // Run the GUI (this blocks until quit)
    gui::run_gui(action_tx, app_state)?;

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
