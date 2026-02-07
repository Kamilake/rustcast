//! Native Windows GUI module for RustCast Settings Panel
//! Provides a settings window with streaming controls, status indicator, and port configuration

#![cfg(windows)]

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::config::Config;

/// Actions from the GUI
#[derive(Debug, Clone)]
pub enum GuiAction {
    ToggleStream,
    SaveConfig(Config),
    OpenBrowser,
    Quit,
}

/// Shared application state for the GUI
pub struct AppState {
    pub is_streaming: Arc<AtomicBool>,
    pub client_count: Arc<AtomicUsize>,
    pub config: RefCell<Config>,
}

/// Settings Panel Window
pub struct SettingsPanel {
    pub window: nwg::Window,
    pub icon: nwg::Icon,
    pub tray: nwg::TrayNotification,
    pub tray_menu: nwg::Menu,
    pub tray_item_open: nwg::MenuItem,
    pub tray_item_settings: nwg::MenuItem,
    pub tray_item_sep: nwg::MenuSeparator,
    pub tray_item_quit: nwg::MenuItem,
    
    // Status group
    pub status_frame: nwg::Frame,
    pub status_label: nwg::Label,
    pub status_indicator: nwg::Label,
    pub clients_label: nwg::Label,
    
    // Controls
    pub stream_button: nwg::Button,
    pub open_browser_button: nwg::Button,
    
    // Settings group
    pub settings_frame: nwg::Frame,
    pub port_label: nwg::Label,
    pub port_input: nwg::TextInput,
    pub bitrate_label: nwg::Label,
    pub bitrate_combo: nwg::ComboBox<String>,
    pub autostart_check: nwg::CheckBox,
    
    // Bottom buttons
    pub save_button: nwg::Button,
    
    // Timer for status updates
    pub status_timer: nwg::AnimationTimer,
    
    // Communication
    pub action_tx: RefCell<Option<Sender<GuiAction>>>,
    pub state: RefCell<Option<Arc<AppState>>>,
}

impl SettingsPanel {
    /// Build the settings panel UI
    pub fn build(tx: Sender<GuiAction>, state: Arc<AppState>) -> Result<Self, nwg::NwgError> {
        // Initialize native-windows-gui
        nwg::init()?;
        
        // Try to load icon
        let icon = Self::load_icon()?;
        
        // Build window
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((380, 380))
            .position((300, 200))
            .title("RustCast 설정")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::MINIMIZE_BOX)
            .icon(Some(&icon))
            .build(&mut window)?;
        
        // Tray notification
        let mut tray = nwg::TrayNotification::default();
        nwg::TrayNotification::builder()
            .parent(&window)
            .icon(Some(&icon))
            .tip(Some("RustCast - 시스템 오디오 스트리밍"))
            .build(&mut tray)?;
        
        // Tray context menu
        let mut tray_menu = nwg::Menu::default();
        nwg::Menu::builder()
            .popup(true)
            .parent(&window)
            .build(&mut tray_menu)?;
        
        let mut tray_item_open = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .parent(&tray_menu)
            .text("브라우저에서 열기")
            .build(&mut tray_item_open)?;
        
        let mut tray_item_settings = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .parent(&tray_menu)
            .text("설정 열기")
            .build(&mut tray_item_settings)?;
        
        let mut tray_item_sep = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&tray_menu)
            .build(&mut tray_item_sep)?;
        
        let mut tray_item_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .parent(&tray_menu)
            .text("종료")
            .build(&mut tray_item_quit)?;
        
        // Status frame
        let mut status_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((15, 15))
            .size((340, 100))
            .build(&mut status_frame)?;
        
        let mut status_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&status_frame)
            .text("서버 상태:")
            .position((15, 15))
            .size((80, 25))
            .build(&mut status_label)?;
        
        let mut status_indicator = nwg::Label::default();
        nwg::Label::builder()
            .parent(&status_frame)
            .text("● 정지됨")
            .position((100, 15))
            .size((220, 25))
            .build(&mut status_indicator)?;
        
        let mut clients_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&status_frame)
            .text("연결된 클라이언트: 0")
            .position((15, 45))
            .size((200, 25))
            .build(&mut clients_label)?;
        
        // Stream toggle button
        let mut stream_button = nwg::Button::default();
        nwg::Button::builder()
            .parent(&status_frame)
            .text("▶ 스트리밍 시작")
            .position((15, 70))
            .size((150, 25))
            .build(&mut stream_button)?;
        
        // Open browser button
        let mut open_browser_button = nwg::Button::default();
        nwg::Button::builder()
            .parent(&status_frame)
            .text("🌐 브라우저에서 열기")
            .position((175, 70))
            .size((150, 25))
            .build(&mut open_browser_button)?;
        
        // Settings frame
        let mut settings_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((15, 125))
            .size((340, 150))
            .build(&mut settings_frame)?;
        
        let mut port_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&settings_frame)
            .text("포트:")
            .position((15, 15))
            .size((80, 25))
            .build(&mut port_label)?;
        
        let config = state.config.borrow();
        
        let mut port_input = nwg::TextInput::default();
        nwg::TextInput::builder()
            .parent(&settings_frame)
            .text(&config.port.to_string())
            .position((100, 12))
            .size((100, 22))
            .build(&mut port_input)?;
        
        let mut bitrate_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&settings_frame)
            .text("비트레이트:")
            .position((15, 50))
            .size((80, 25))
            .build(&mut bitrate_label)?;
        
        let mut bitrate_combo = nwg::ComboBox::default();
        nwg::ComboBox::builder()
            .parent(&settings_frame)
            .position((100, 47))
            .size((100, 25))
            .collection(vec![
                "64 kbps".to_string(),
                "96 kbps".to_string(),
                "128 kbps".to_string(),
                "160 kbps".to_string(),
                "192 kbps".to_string(),
                "256 kbps".to_string(),
                "320 kbps".to_string(),
            ])
            .build(&mut bitrate_combo)?;
        
        // Set current bitrate selection
        let bitrate_index = match config.bitrate {
            64 => 0,
            96 => 1,
            128 => 2,
            160 => 3,
            192 => 4,
            256 => 5,
            320 => 6,
            _ => 4, // default to 192
        };
        bitrate_combo.set_selection(Some(bitrate_index));
        
        let mut autostart_check = nwg::CheckBox::default();
        nwg::CheckBox::builder()
            .parent(&settings_frame)
            .text("시작 시 자동으로 스트리밍 시작")
            .position((15, 85))
            .size((250, 25))
            .check_state(if config.auto_start { nwg::CheckBoxState::Checked } else { nwg::CheckBoxState::Unchecked })
            .build(&mut autostart_check)?;
        
        drop(config);
        
        // Info label
        let mut info_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&settings_frame)
            .text("※ 포트/비트레이트 변경은 재시작 후 적용됩니다")
            .position((15, 115))
            .size((300, 20))
            .build(&mut info_label)?;
        
        // Save button
        let mut save_button = nwg::Button::default();
        nwg::Button::builder()
            .parent(&window)
            .text("💾 설정 저장")
            .position((15, 285))
            .size((340, 35))
            .build(&mut save_button)?;
        
        // Status update timer (500ms interval)
        let mut status_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_millis(500))
            .build(&mut status_timer)?;
        
        let panel = Self {
            window,
            icon,
            tray,
            tray_menu,
            tray_item_open,
            tray_item_settings,
            tray_item_sep,
            tray_item_quit,
            status_frame,
            status_label,
            status_indicator,
            clients_label,
            stream_button,
            open_browser_button,
            settings_frame,
            port_label,
            port_input,
            bitrate_label,
            bitrate_combo,
            autostart_check,
            save_button,
            status_timer,
            action_tx: RefCell::new(Some(tx)),
            state: RefCell::new(Some(state)),
        };
        
        Ok(panel)
    }
    
    fn load_icon() -> Result<nwg::Icon, nwg::NwgError> {
        // Try to load from file first
        let icon_paths = [
            "resources/rustcast_envelope.ico",
            "../resources/rustcast_envelope.ico",
            "rustcast_envelope.ico",
        ];
        
        for path in &icon_paths {
            if std::path::Path::new(path).exists() {
                let mut icon = nwg::Icon::default();
                if nwg::Icon::builder()
                    .source_file(Some(path))
                    .size(Some((32, 32)))
                    .build(&mut icon)
                    .is_ok()
                {
                    log::info!("Loaded icon from: {}", path);
                    return Ok(icon);
                }
            }
        }
        
        // Fallback to embedded resource
        let mut icon = nwg::Icon::default();
        nwg::Icon::builder()
            .source_embed(Some(&nwg::EmbedResource::load(None)?))
            .source_embed_id(1) // Main icon resource ID
            .size(Some((32, 32)))
            .build(&mut icon)?;
        
        Ok(icon)
    }
    
    /// Update the UI based on current state
    pub fn update_status(&self) {
        if let Some(state) = self.state.borrow().as_ref() {
            let is_streaming = state.is_streaming.load(Ordering::SeqCst);
            let client_count = state.client_count.load(Ordering::SeqCst);
            
            if is_streaming {
                self.status_indicator.set_text("● 스트리밍 중");
                self.stream_button.set_text("⏹ 스트리밍 정지");
            } else {
                self.status_indicator.set_text("● 정지됨");
                self.stream_button.set_text("▶ 스트리밍 시작");
            }
            
            self.clients_label.set_text(&format!("연결된 클라이언트: {}", client_count));
        }
    }
    
    /// Get the current config from UI inputs
    pub fn get_config_from_ui(&self) -> Config {
        let port: u16 = self.port_input.text().parse().unwrap_or(3000);
        
        let bitrate: u32 = match self.bitrate_combo.selection() {
            Some(0) => 64,
            Some(1) => 96,
            Some(2) => 128,
            Some(3) => 160,
            Some(4) => 192,
            Some(5) => 256,
            Some(6) => 320,
            _ => 192,
        };
        
        let auto_start = self.autostart_check.check_state() == nwg::CheckBoxState::Checked;
        
        Config {
            port,
            bitrate,
            auto_start,
        }
    }
    
    /// Send an action
    fn send_action(&self, action: GuiAction) {
        if let Some(tx) = self.action_tx.borrow().as_ref() {
            let _ = tx.send(action);
        }
    }
    
    /// Show the window
    pub fn show(&self) {
        self.window.set_visible(true);
        nwg::Window::set_focus(&self.window);
    }
    
    /// Hide to tray
    pub fn hide_to_tray(&self) {
        self.window.set_visible(false);
    }
}

// Event handler module
mod settings_panel_events {
    use super::*;
    
    pub struct SettingsPanelEvents {
        inner: std::rc::Rc<SettingsPanel>,
        default_handler: RefCell<Option<nwg::EventHandler>>,
    }
    
    impl nwg::NativeUi<SettingsPanelEvents> for SettingsPanel {
        fn build_ui(data: SettingsPanel) -> Result<SettingsPanelEvents, nwg::NwgError> {
            // Start the timer
            data.status_timer.start();
            
            let ui = SettingsPanelEvents {
                inner: std::rc::Rc::new(data),
                default_handler: RefCell::new(None),
            };
            
            let evt_ui = std::rc::Rc::downgrade(&ui.inner);
            let handle_events = move |evt, _evt_data, handle| {
                if let Some(ui) = evt_ui.upgrade() {
                    match evt {
                        // Window events
                        nwg::Event::OnWindowClose => {
                            if &handle == &ui.window {
                                // Hide to tray instead of closing
                                ui.hide_to_tray();
                            }
                        }
                        nwg::Event::OnWindowMinimize => {
                            if &handle == &ui.window {
                                ui.hide_to_tray();
                            }
                        }
                        
                        // Tray events
                        nwg::Event::OnContextMenu => {
                            if &handle == &ui.tray {
                                let (x, y) = nwg::GlobalCursor::position();
                                ui.tray_menu.popup(x, y);
                            }
                        }
                        nwg::Event::OnMousePress(nwg::MousePressEvent::MousePressLeftUp) => {
                            // Left click on tray icon opens settings window
                            if &handle == &ui.tray {
                                ui.show();
                            }
                        }
                        
                        // Menu events
                        nwg::Event::OnMenuItemSelected => {
                            if &handle == &ui.tray_item_open {
                                ui.send_action(GuiAction::OpenBrowser);
                            } else if &handle == &ui.tray_item_settings {
                                ui.show();
                            } else if &handle == &ui.tray_item_quit {
                                ui.send_action(GuiAction::Quit);
                                nwg::stop_thread_dispatch();
                            }
                        }
                        
                        // Button events
                        nwg::Event::OnButtonClick => {
                            if &handle == &ui.stream_button {
                                ui.send_action(GuiAction::ToggleStream);
                            } else if &handle == &ui.open_browser_button {
                                ui.send_action(GuiAction::OpenBrowser);
                            } else if &handle == &ui.save_button {
                                let config = ui.get_config_from_ui();
                                ui.send_action(GuiAction::SaveConfig(config));
                                nwg::modal_info_message(&ui.window, "저장 완료", "설정이 저장되었습니다.\n포트/비트레이트 변경은 재시작 후 적용됩니다.");
                            }
                        }
                        
                        // Timer events
                        nwg::Event::OnTimerTick => {
                            if &handle == &ui.status_timer {
                                ui.update_status();
                            }
                        }
                        
                        _ => {}
                    }
                }
            };
            
            *ui.default_handler.borrow_mut() = Some(nwg::full_bind_event_handler(
                &ui.inner.window.handle,
                handle_events,
            ));
            
            Ok(ui)
        }
    }
    
    impl Drop for SettingsPanelEvents {
        fn drop(&mut self) {
            if let Some(handler) = self.default_handler.borrow_mut().take() {
                nwg::unbind_event_handler(&handler);
            }
        }
    }
    
    impl std::ops::Deref for SettingsPanelEvents {
        type Target = SettingsPanel;
        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }
}

// SettingsPanelEvents is used internally

/// Run the GUI application
pub fn run_gui(
    tx: Sender<GuiAction>,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    use native_windows_gui::NativeUi;
    
    let panel = SettingsPanel::build(tx, state)?;
    let _ui = SettingsPanel::build_ui(panel)?;
    
    nwg::dispatch_thread_events();
    
    Ok(())
}
