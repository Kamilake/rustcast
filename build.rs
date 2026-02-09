// build.rs - Windows resource embedding for tray icon, app icon, and visual styles manifest

fn main() {
    // Only compile resources on Windows
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources/");
        
        let icon_path = "resources/rustcast_envelope.ico";
        let manifest_path = "resources/app.manifest";
        
        let mut res = winres::WindowsResource::new();
        
        // Set application icon (shows in taskbar, file explorer, etc.)
        if std::path::Path::new(icon_path).exists() {
            res.set_icon(icon_path);
            println!("cargo:warning=Embedding icon: {}", icon_path);
        } else {
            println!("cargo:warning=Icon file not found: {}", icon_path);
        }
        
        // Set manifest for visual styles (ComCtl32 v6) and DPI awareness
        if std::path::Path::new(manifest_path).exists() {
            res.set_manifest_file(manifest_path);
            println!("cargo:warning=Embedding manifest: {}", manifest_path);
        } else {
            println!("cargo:warning=Manifest file not found: {}", manifest_path);
        }
        
        // Compile the resources
        if let Err(e) = res.compile() {
            println!("cargo:warning=Failed to compile Windows resources: {}", e);
        } else {
            println!("cargo:warning=Successfully compiled Windows resources");
        }
    }
}
