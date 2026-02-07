// build.rs - Windows resource embedding for tray icon and app icon

fn main() {
    // Only compile resources on Windows
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources/");
        
        let icon_path = "resources/rustcast_envelope.ico";
        
        if std::path::Path::new(icon_path).exists() {
            let mut res = winres::WindowsResource::new();
            
            // Set application icon (shows in taskbar, file explorer, etc.)
            res.set_icon(icon_path);
            
            // Compile the resources
            if let Err(e) = res.compile() {
                println!("cargo:warning=Failed to compile Windows resources: {}", e);
            } else {
                println!("cargo:warning=Successfully embedded icon: {}", icon_path);
            }
        } else {
            println!("cargo:warning=Icon file not found: {}", icon_path);
        }
    }
}
