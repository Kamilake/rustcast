//! HTTP streaming server
//! Serves MP3 audio stream to connected clients

use crossbeam_channel::Receiver;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tiny_http::{Response, Server, StatusCode};

/// HTTP streaming server
pub struct StreamServer {
    port: u16,
    is_running: Arc<AtomicBool>,
    client_count: Arc<AtomicUsize>,
}

impl StreamServer {
    /// Create a new stream server
    pub fn new(port: u16) -> Self {
        Self {
            port,
            is_running: Arc::new(AtomicBool::new(false)),
            client_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get current client count
    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::SeqCst)
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Start the server
    pub fn start(
        &mut self,
        audio_rx: Receiver<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let addr = format!("0.0.0.0:{}", self.port);
        let server = Server::http(&addr).map_err(|e| format!("Failed to start server: {}", e))?;
        
        log::info!("Server started on http://{}", addr);
        
        self.is_running.store(true, Ordering::SeqCst);
        let is_running = self.is_running.clone();
        let client_count = self.client_count.clone();
        let port = self.port;

        thread::spawn(move || {
            // Use a broadcast mechanism for multiple clients
            let clients: Arc<std::sync::Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            
            let clients_clone = clients.clone();
            let is_running_clone = is_running.clone();

            // Audio broadcast thread
            thread::spawn(move || {
                while is_running_clone.load(Ordering::SeqCst) {
                    if let Ok(data) = audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        let mut clients_guard = clients_clone.lock().unwrap();
                        clients_guard.retain(|client| client.send(data.clone()).is_ok());
                    }
                }
            });

            // Accept connections
            for request in server.incoming_requests() {
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                let url = request.url().to_string();
                
                match url.as_str() {
                    "/" => {
                        // Serve main page
                        let html = Self::get_index_html(port);
                        let response = Response::from_string(html)
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                            );
                        let _ = request.respond(response);
                    }
                    "/stream" | "/stream.mp3" => {
                        // Create channel for this client
                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        
                        {
                            let mut clients_guard = clients.lock().unwrap();
                            clients_guard.push(tx);
                        }
                        
                        client_count.fetch_add(1, Ordering::SeqCst);
                        log::info!("Client connected. Total: {}", client_count.load(Ordering::SeqCst));

                        // Send streaming response
                        let response = Response::empty(StatusCode(200))
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"audio/mpeg"[..]).unwrap()
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap()
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Connection"[..], &b"keep-alive"[..]).unwrap()
                            )
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
                            );

                        let client_count_clone = client_count.clone();
                        
                        // Stream in a separate thread
                        thread::spawn(move || {
                            let mut stream = request.into_writer();
                            while let Ok(data) = rx.recv() {
                                if stream.write_all(&data).is_err() {
                                    break;
                                }
                                if stream.flush().is_err() {
                                    break;
                                }
                            }
                            client_count_clone.fetch_sub(1, Ordering::SeqCst);
                            log::info!("Client disconnected. Total: {}", client_count_clone.load(Ordering::SeqCst));
                        });
                    }
                    "/status" => {
                        let status = format!(r#"{{"clients": {}, "running": true}}"#, 
                            client_count.load(Ordering::SeqCst));
                        let response = Response::from_string(status)
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
                            );
                        let _ = request.respond(response);
                    }
                    _ => {
                        let response = Response::from_string("Not Found")
                            .with_status_code(StatusCode(404));
                        let _ = request.respond(response);
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the server
    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        log::info!("Server stopped");
    }

    /// Get index HTML page
    fn get_index_html(port: u16) -> String {
        format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>🎵 RustCast - Audio Stream</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            min-height: 100vh;
            display: flex;
            justify-content: center;
            align-items: center;
            color: #fff;
        }}
        .container {{
            text-align: center;
            padding: 2rem;
            background: rgba(255,255,255,0.1);
            border-radius: 20px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px rgba(0,0,0,0.3);
        }}
        h1 {{
            font-size: 2.5rem;
            margin-bottom: 1rem;
            background: linear-gradient(45deg, #f39c12, #e74c3c);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }}
        .player {{
            margin: 2rem 0;
        }}
        audio {{
            width: 300px;
            filter: sepia(20%) saturate(70%) grayscale(1) contrast(99%) invert(12%);
        }}
        .status {{
            margin-top: 1rem;
            padding: 0.5rem 1rem;
            background: rgba(46, 204, 113, 0.2);
            border-radius: 10px;
            font-size: 0.9rem;
        }}
        .info {{
            margin-top: 1.5rem;
            font-size: 0.8rem;
            color: #aaa;
        }}
        a {{
            color: #3498db;
            text-decoration: none;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>🎵 RustCast</h1>
        <p>Windows System Audio Streaming</p>
        
        <div class="player">
            <audio controls autoplay>
                <source src="/stream.mp3" type="audio/mpeg">
                Your browser does not support audio.
            </audio>
        </div>
        
        <div class="status" id="status">
            🟢 Streaming Live
        </div>
        
        <div class="info">
            <p>Direct stream: <a href="/stream.mp3">/stream.mp3</a></p>
            <p>Port: {}</p>
        </div>
    </div>
    
    <script>
        // Auto-reconnect on error
        const audio = document.querySelector('audio');
        audio.addEventListener('error', () => {{
            setTimeout(() => {{
                audio.src = '/stream.mp3?' + Date.now();
                audio.load();
                audio.play();
            }}, 1000);
        }});
    </script>
</body>
</html>"#, port)
    }
}
