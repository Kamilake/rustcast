//! System tray module
//! Handles tray icon and menu for the application

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::io::{self, BufRead};

/// Tray menu actions
#[derive(Debug, Clone)]
pub enum TrayAction {
    OpenBrowser,
    ToggleStream,
    Settings,
    Quit,
}

/// System tray handler (simplified console-based version)
pub struct SystemTray {
    rx: Receiver<TrayAction>,
}

/// Load icon from .ico file on Windows
#[cfg(windows)]
fn load_icon_from_file(path: &str) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::iter::once;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    
    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(once(0))
        .collect();
    
    unsafe {
        let hicon = LoadImageW(
            0, // NULL for loading from file
            wide_path.as_ptr(),
            IMAGE_ICON,
            32, // width
            32, // height
            LR_LOADFROMFILE,
        );
        
        if hicon != 0 {
            Some(hicon as HICON)
        } else {
            None
        }
    }
}

impl SystemTray {
    /// Create a new system tray (or console-based fallback)
    pub fn new(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx): (Sender<TrayAction>, Receiver<TrayAction>) = mpsc::channel();

        // Try to create actual tray, fallback to console
        match Self::try_create_tray(port, tx.clone()) {
            Ok(()) => {
                log::info!("System tray created successfully");
            }
            Err(e) => {
                log::warn!("Could not create system tray ({}), using console mode", e);
                Self::start_console_listener(tx);
            }
        }

        Ok(Self { rx })
    }

    fn try_create_tray(port: u16, tx: Sender<TrayAction>) -> Result<(), Box<dyn std::error::Error>> {
        use tray_item::TrayItem;
        
        // Try to find the icon file - check multiple locations
        let icon_paths = [
            "resources/rustcast_envelope.ico",
            "../resources/rustcast_envelope.ico",
            "rustcast_envelope.ico",
        ];
        
        let icon_path = icon_paths.iter()
            .find(|p| std::path::Path::new(p).exists());

        #[cfg(windows)]
        let mut tray = if let Some(path) = icon_path {
            if let Some(hicon) = load_icon_from_file(path) {
                log::info!("Loading tray icon from file: {}", path);
                TrayItem::new("RustCast", tray_item::IconSource::RawIcon(hicon))?
            } else {
                log::warn!("Failed to load icon from file, using resource");
                TrayItem::new("RustCast", tray_item::IconSource::Resource("main-icon"))?
            }
        } else {
            // Fallback to embedded resource (set by build.rs)
            TrayItem::new("RustCast", tray_item::IconSource::Resource("main-icon"))?
        };

        #[cfg(not(windows))]
        let mut tray = TrayItem::new("RustCast", tray_item::IconSource::Resource(""))?;

        // Add menu items
        tray.add_label(&format!("RustCast - Port {}", port))?;
        tray.inner_mut().add_separator()?;

        let tx_clone = tx.clone();
        tray.add_menu_item("Open in Browser", move || {
            let _ = tx_clone.send(TrayAction::OpenBrowser);
        })?;

        let tx_clone = tx.clone();
        tray.add_menu_item("Toggle Streaming", move || {
            let _ = tx_clone.send(TrayAction::ToggleStream);
        })?;

        tray.inner_mut().add_separator()?;

        let tx_clone = tx.clone();
        tray.add_menu_item("Quit", move || {
            let _ = tx_clone.send(TrayAction::Quit);
        })?;

        // Keep tray alive by boxing it
        std::mem::forget(tray);
        
        Ok(())
    }

    fn start_console_listener(tx: Sender<TrayAction>) {
        thread::spawn(move || {
            println!("\n=== RustCast Console Controls ===");
            println!("Commands:");
            println!("  o - Open in browser");
            println!("  t - Toggle streaming");
            println!("  s - Show settings");
            println!("  q - Quit");
            println!("================================\n");

            let stdin = io::stdin();
            for line in stdin.lock().lines().map_while(Result::ok) {
                match line.trim().to_lowercase().as_str() {
                    "o" | "open" => { let _ = tx.send(TrayAction::OpenBrowser); }
                    "t" | "toggle" => { let _ = tx.send(TrayAction::ToggleStream); }
                    "s" | "settings" => { let _ = tx.send(TrayAction::Settings); }
                    "q" | "quit" | "exit" => { 
                        let _ = tx.send(TrayAction::Quit);
                        break;
                    }
                    _ => println!("Unknown command. Use: o/t/s/q"),
                }
            }
        });
    }

    /// Get the action receiver
    pub fn get_receiver(&self) -> &Receiver<TrayAction> {
        &self.rx
    }
}
