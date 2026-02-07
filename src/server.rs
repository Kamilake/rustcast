//! HTTP streaming server
//! Serves Opus/Ogg audio stream to connected clients

use crossbeam_channel::Receiver;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tiny_http::{Response, Server, StatusCode};

use crate::opus_encoder::OpusEncoder;

/// Opus stream info for each client to create proper Ogg stream
#[derive(Clone)]
struct OpusStreamInfo {
    channels: u16,
    sample_rate: u32,
    frame_size: usize,
}

/// HTTP streaming server
pub struct StreamServer {
    port: u16,
    is_running: Arc<AtomicBool>,
    client_count: Arc<AtomicUsize>,
    opus_info: Option<OpusStreamInfo>,
}

impl StreamServer {
    /// Create a new stream server
    pub fn new(port: u16) -> Self {
        Self {
            port,
            is_running: Arc::new(AtomicBool::new(false)),
            client_count: Arc::new(AtomicUsize::new(0)),
            opus_info: None,
        }
    }

    /// Create a new stream server with shared client count
    pub fn with_client_count(port: u16, client_count: Arc<AtomicUsize>) -> Self {
        Self {
            port,
            is_running: Arc::new(AtomicBool::new(false)),
            client_count,
            opus_info: None,
        }
    }
    
    /// Set Opus stream info (must be called before start)
    pub fn set_opus_info(&mut self, channels: u16, sample_rate: u32, frame_size: usize) {
        self.opus_info = Some(OpusStreamInfo { channels, sample_rate, frame_size });
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
        let opus_info = Arc::new(self.opus_info.clone().unwrap_or(OpusStreamInfo {
            channels: 2,
            sample_rate: 48000,
            frame_size: 480,
        }));

        thread::spawn(move || {
            // Use a broadcast mechanism for multiple clients
            let clients: Arc<std::sync::Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            
            let clients_clone = clients.clone();
            let is_running_clone = is_running.clone();

            // Audio broadcast thread
            thread::spawn(move || {
                let mut total_received = 0u64;
                let mut total_broadcast = 0u64;
                let mut last_log = std::time::Instant::now();
                
                while is_running_clone.load(Ordering::SeqCst) {
                    if let Ok(data) = audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        total_received += 1;
                        let mut clients_guard = clients_clone.lock().unwrap();
                        let client_count = clients_guard.len();
                        clients_guard.retain(|client| client.send(data.clone()).is_ok());
                        if client_count > 0 {
                            total_broadcast += 1;
                        }
                        
                        // 5초마다 통계 출력
                        if last_log.elapsed().as_secs() >= 5 {
                            log::info!("[SERVER] 통계: 수신됨={}, 브로드캐스트={}, 연결된 클라이언트={}", 
                                total_received, total_broadcast, client_count);
                            last_log = std::time::Instant::now();
                        }
                    }
                }
            });

            // Accept connections
            for request in server.incoming_requests() {
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                let url = request.url().to_string();
                // Strip query string for matching (e.g., "/stream.opus?123456" -> "/stream.opus")
                let path = url.split('?').next().unwrap_or(&url);
                
                match path {
                    "/" => {
                        // Serve main page
                        let html = Self::get_index_html(port);
                        let response = Response::from_string(html)
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                            );
                        let _ = request.respond(response);
                    }
                    "/stream" | "/stream.opus" | "/stream.ogg" => {
                        // Create channel for this client
                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        
                        {
                            let mut clients_guard = clients.lock().unwrap();
                            clients_guard.push(tx);
                        }
                        
                        client_count.fetch_add(1, Ordering::SeqCst);
                        log::info!("Client connected (Opus). Total: {}", client_count.load(Ordering::SeqCst));

                        let client_count_clone = client_count.clone();
                        let info = opus_info.clone();
                        
                        // Stream in a separate thread
                        thread::spawn(move || {
                            // Get raw TCP stream from the request
                            let mut stream = request.into_writer();
                            
                            // Manually write HTTP response headers for Ogg/Opus
                            let http_headers = b"HTTP/1.1 200 OK\r\n\
                                Content-Type: audio/ogg\r\n\
                                Cache-Control: no-cache, no-store\r\n\
                                Connection: keep-alive\r\n\
                                Access-Control-Allow-Origin: *\r\n\
                                \r\n";
                            
                            if stream.write_all(http_headers).is_err() {
                                client_count_clone.fetch_sub(1, Ordering::SeqCst);
                                log::info!("Client disconnected (header write failed). Total: {}", client_count_clone.load(Ordering::SeqCst));
                                return;
                            }
                            
                            // Generate unique serial for this client's Ogg stream
                            let serial = generate_serial();
                            
                            // Send Ogg/Opus headers (unique per client)
                            let headers = OpusEncoder::get_headers_with_serial(info.channels, info.sample_rate, serial);
                            if stream.write_all(&headers).is_err() {
                                client_count_clone.fetch_sub(1, Ordering::SeqCst);
                                log::info!("Client disconnected (Opus header write failed). Total: {}", client_count_clone.load(Ordering::SeqCst));
                                return;
                            }
                            
                            if stream.flush().is_err() {
                                client_count_clone.fetch_sub(1, Ordering::SeqCst);
                                log::info!("Client disconnected (header flush failed). Total: {}", client_count_clone.load(Ordering::SeqCst));
                                return;
                            }
                            
                            // Track granule position and page sequence for this client
                            let mut granule_position: u64 = 0;
                            let mut page_sequence: u32 = 2; // 0 and 1 used by headers
                            let frame_size = info.frame_size as u64;
                            
                            // Stream audio data - wrap each raw Opus packet in Ogg
                            while let Ok(opus_packet) = rx.recv() {
                                granule_position += frame_size;
                                
                                // Use our manual Ogg page creation (proper flags)
                                let ogg_page = OpusEncoder::wrap_opus_packet(
                                    &opus_packet, 
                                    serial, 
                                    granule_position, 
                                    page_sequence
                                );
                                page_sequence += 1;
                                
                                if stream.write_all(&ogg_page).is_err() {
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
    <title>🎵 RustCast - Low Latency Audio</title>
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
            min-width: 350px;
        }}
        h1 {{
            font-size: 2.5rem;
            margin-bottom: 0.5rem;
            background: linear-gradient(45deg, #9b59b6, #3498db);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }}
        .subtitle {{
            color: #888;
            margin-bottom: 1rem;
        }}
        .codec-badge {{
            display: inline-block;
            padding: 4px 12px;
            background: linear-gradient(45deg, #9b59b6, #8e44ad);
            border-radius: 20px;
            font-size: 0.75rem;
            margin-bottom: 1rem;
        }}
        .player {{
            margin: 1.5rem 0;
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
        .status.buffering {{
            background: rgba(241, 196, 15, 0.2);
        }}
        .latency-info {{
            margin-top: 0.5rem;
            font-size: 0.8rem;
            color: #27ae60;
            font-weight: bold;
        }}
        .controls {{
            margin-top: 1rem;
            display: flex;
            gap: 10px;
            justify-content: center;
            flex-wrap: wrap;
        }}
        button {{
            padding: 10px 20px;
            border: none;
            border-radius: 8px;
            cursor: pointer;
            font-size: 0.9rem;
            transition: transform 0.1s;
        }}
        button:hover {{
            transform: scale(1.05);
        }}
        button:active {{
            transform: scale(0.95);
        }}
        .play-btn {{
            background: linear-gradient(45deg, #27ae60, #2ecc71);
            color: white;
            font-size: 1.1rem;
            padding: 12px 24px;
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
        <p class="subtitle">Windows System Audio Streaming</p>
        <span class="codec-badge">🚀 Opus Low-Latency</span>
        
        <div class="player">
            <audio id="audio" controls playsinline webkit-playsinline>
                <source src="/stream.opus" type="audio/ogg">
                <source src="/stream.ogg" type="audio/ogg">
                Your browser does not support Opus audio.
            </audio>
        </div>
        
        <div class="controls">
            <button class="play-btn" id="playBtn" onclick="togglePlay()">▶ Play</button>
        </div>
        
        <div class="status" id="status">
            ⏸ Ready to stream
        </div>
        <div class="latency-info" id="latencyInfo">Expected latency: ~50-100ms</div>
        
        <div class="info">
            <p>Direct stream: <a href="/stream.opus">/stream.opus</a></p>
            <p>Port: {} | Codec: Opus</p>
        </div>
    </div>
    
    <script>
        const audio = document.getElementById('audio');
        const status = document.getElementById('status');
        const latencyInfo = document.getElementById('latencyInfo');
        const playBtn = document.getElementById('playBtn');
        
        let isPlaying = false;
        let bufferCheckInterval = null;
        
        function togglePlay() {{
            if (isPlaying) {{
                audio.pause();
                audio.src = '';
                isPlaying = false;
                playBtn.textContent = '▶ Play';
                status.textContent = '⏸ Paused';
                status.className = 'status';
                latencyInfo.textContent = 'Expected latency: ~50-100ms';
                if (bufferCheckInterval) {{
                    clearInterval(bufferCheckInterval);
                    bufferCheckInterval = null;
                }}
            }} else {{
                // Reload stream for fresh start with Opus
                audio.src = '/stream.opus?' + Date.now();
                audio.load();
                audio.play().then(() => {{
                    isPlaying = true;
                    playBtn.textContent = '⏹ Stop';
                    status.textContent = '🟢 Streaming Live (Opus)';
                    status.className = 'status';
                    startBufferMonitor();
                }}).catch(e => {{
                    console.error('Play failed:', e);
                    status.textContent = '❌ Error: ' + e.message;
                }});
            }}
        }}
        
        function startBufferMonitor() {{
            // Monitor buffer and skip ahead if too large
            bufferCheckInterval = setInterval(() => {{
                if (!isPlaying) return;
                
                const buffered = audio.buffered;
                if (buffered.length > 0) {{
                    const bufferedEnd = buffered.end(buffered.length - 1);
                    const currentTime = audio.currentTime;
                    const bufferSize = bufferedEnd - currentTime;
                    
                    // Opus has low latency, skip if buffer > 150ms
                    if (bufferSize > 0.15) {{
                        audio.currentTime = bufferedEnd - 0.05;
                        latencyInfo.textContent = `⚡ Buffer: ${{(bufferSize * 1000).toFixed(0)}}ms → Synced!`;
                    }} else {{
                        latencyInfo.textContent = `⚡ Buffer: ${{(bufferSize * 1000).toFixed(0)}}ms`;
                    }}
                }}
            }}, 100);
        }}
        
        // Auto-reconnect on error
        audio.addEventListener('error', (e) => {{
            console.error('Audio error:', e);
            if (isPlaying) {{
                status.textContent = '🔄 Reconnecting...';
                status.className = 'status buffering';
                setTimeout(() => {{
                    audio.src = '/stream.opus?' + Date.now();
                    audio.load();
                    audio.play().catch(console.error);
                }}, 1000);
            }}
        }});
        
        audio.addEventListener('waiting', () => {{
            status.textContent = '⏳ Buffering...';
            status.className = 'status buffering';
        }});
        
        audio.addEventListener('playing', () => {{
            status.textContent = '🟢 Streaming Live (Opus)';
            status.className = 'status';
        }});
    </script>
</body>
</html>"#, port)
    }
}

/// Generate a random serial number for Ogg stream
fn generate_serial() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::atomic::{AtomicU32, Ordering};
    
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    
    let time_part = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(0);
    
    let counter_part = COUNTER.fetch_add(1, Ordering::SeqCst);
    
    time_part.wrapping_add(counter_part)
}
