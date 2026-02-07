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
                        // Serve main page (low-latency WebSocket player)
                        let html = Self::get_low_latency_html(port);
                        let response = Response::from_string(html)
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                            );
                        let _ = request.respond(response);
                    }
                    "/legacy" => {
                        // Serve legacy player (for compatibility)
                        let html = Self::get_index_html(port);
                        let response = Response::from_string(html)
                            .with_header(
                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                            );
                        let _ = request.respond(response);
                    }
                    "/ws" | "/ws/" => {
                        // WebSocket upgrade for ultra-low latency streaming
                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        
                        {
                            let mut clients_guard = clients.lock().unwrap();
                            clients_guard.push(tx);
                        }
                        
                        client_count.fetch_add(1, Ordering::SeqCst);
                        log::info!("WebSocket client connecting. Total: {}", client_count.load(Ordering::SeqCst));
                        
                        let client_count_clone = client_count.clone();
                        
                        // Handle WebSocket in separate thread
                        thread::spawn(move || {
                            if let Err(e) = handle_websocket(request, rx) {
                                log::debug!("WebSocket error: {}", e);
                            }
                            client_count_clone.fetch_sub(1, Ordering::SeqCst);
                            log::info!("WebSocket client disconnected. Total: {}", client_count_clone.load(Ordering::SeqCst));
                        });
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

    /// Get ultra-low latency HTML page with WebSocket + Web Audio API
    fn get_low_latency_html(port: u16) -> String {
        format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>🎵 RustCast - Ultra Low Latency</title>
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
            min-width: 380px;
            max-width: 450px;
        }}
        h1 {{
            font-size: 2.5rem;
            margin-bottom: 0.5rem;
            background: linear-gradient(45deg, #e74c3c, #f39c12);
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
            background: linear-gradient(45deg, #e74c3c, #c0392b);
            border-radius: 20px;
            font-size: 0.75rem;
            margin-bottom: 1rem;
        }}
        .status {{
            margin-top: 1rem;
            padding: 0.75rem 1rem;
            background: rgba(46, 204, 113, 0.2);
            border-radius: 10px;
            font-size: 0.9rem;
        }}
        .status.buffering {{
            background: rgba(241, 196, 15, 0.2);
        }}
        .status.error {{
            background: rgba(231, 76, 60, 0.2);
        }}
        .stats {{
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: 10px;
            margin-top: 1rem;
        }}
        .stat-box {{
            background: rgba(0,0,0,0.2);
            padding: 10px;
            border-radius: 10px;
        }}
        .stat-value {{
            font-size: 1.5rem;
            font-weight: bold;
            color: #2ecc71;
        }}
        .stat-value.warn {{ color: #f39c12; }}
        .stat-value.bad {{ color: #e74c3c; }}
        .stat-label {{
            font-size: 0.7rem;
            color: #888;
            margin-top: 2px;
        }}
        .controls {{
            margin-top: 1.5rem;
            display: flex;
            gap: 10px;
            justify-content: center;
            flex-wrap: wrap;
        }}
        button {{
            padding: 12px 24px;
            border: none;
            border-radius: 10px;
            cursor: pointer;
            font-size: 1rem;
            transition: all 0.2s;
            font-weight: 600;
        }}
        button:hover {{
            transform: scale(1.05);
        }}
        button:active {{
            transform: scale(0.95);
        }}
        button:disabled {{
            opacity: 0.5;
            cursor: not-allowed;
            transform: none;
        }}
        .play-btn {{
            background: linear-gradient(45deg, #27ae60, #2ecc71);
            color: white;
            min-width: 140px;
        }}
        .stop-btn {{
            background: linear-gradient(45deg, #e74c3c, #c0392b);
            color: white;
            min-width: 140px;
        }}
        .buffer-control {{
            margin-top: 1rem;
            padding: 1rem;
            background: rgba(0,0,0,0.2);
            border-radius: 10px;
        }}
        .buffer-control label {{
            display: block;
            font-size: 0.8rem;
            color: #888;
            margin-bottom: 0.5rem;
        }}
        .buffer-control input {{
            width: 100%;
        }}
        .info {{
            margin-top: 1.5rem;
            font-size: 0.75rem;
            color: #666;
        }}
        .info a {{
            color: #3498db;
            text-decoration: none;
        }}
        .visualizer {{
            height: 60px;
            background: rgba(0,0,0,0.3);
            border-radius: 10px;
            margin-top: 1rem;
            display: flex;
            align-items: flex-end;
            justify-content: center;
            gap: 2px;
            padding: 5px;
            overflow: hidden;
        }}
        .bar {{
            width: 4px;
            background: linear-gradient(to top, #27ae60, #2ecc71);
            border-radius: 2px;
            transition: height 0.05s ease;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>🚀 RustCast</h1>
        <p class="subtitle">Ultra Low Latency Audio</p>
        <span class="codec-badge">⚡ WebSocket + Web Audio API</span>
        
        <div class="visualizer" id="visualizer"></div>
        
        <div class="status" id="status">
            ⏸ Click Play to start streaming
        </div>
        
        <div class="stats">
            <div class="stat-box">
                <div class="stat-value" id="latency">--</div>
                <div class="stat-label">Latency (ms)</div>
            </div>
            <div class="stat-box">
                <div class="stat-value" id="buffer">--</div>
                <div class="stat-label">Buffer (ms)</div>
            </div>
            <div class="stat-box">
                <div class="stat-value" id="packets">0</div>
                <div class="stat-label">Packets/s</div>
            </div>
        </div>
        
        <div class="controls">
            <button class="play-btn" id="playBtn">▶ Play</button>
        </div>
        
        <div class="buffer-control">
            <label>🎯 Target Buffer: <span id="targetBufferValue">60</span>ms (lower = less latency, more glitches)</label>
            <input type="range" id="targetBuffer" min="20" max="200" value="60" step="10">
        </div>
        
        <div class="info">
            <p>WebSocket: ws://localhost:{}/ws | <a href="/legacy">Legacy Player</a></p>
            <p>Opus 48kHz Stereo | 10ms frames</p>
        </div>
    </div>

    <script type="module">
        // Import opus-decoder as ES module
        import {{ OpusDecoder }} from 'https://cdn.jsdelivr.net/npm/opus-decoder@0.7.11/+esm';
        
        // UI Elements
        const statusEl = document.getElementById('status');
        const latencyEl = document.getElementById('latency');
        const bufferEl = document.getElementById('buffer');
        const packetsEl = document.getElementById('packets');
        const playBtn = document.getElementById('playBtn');
        const targetBufferSlider = document.getElementById('targetBuffer');
        const targetBufferValue = document.getElementById('targetBufferValue');
        const visualizer = document.getElementById('visualizer');
        
        // Audio state
        let isPlaying = false;
        let audioContext = null;
        let opusDecoder = null;
        let ws = null;
        let nextPlayTime = 0;
        let packetsReceived = 0;
        let packetsPerSecond = 0;
        let lastPacketCount = 0;
        let statsInterval = null;
        let targetBufferMs = 60;
        let audioQueue = [];
        let isProcessing = false;
        let startTime = 0;
        let totalSamplesPlayed = 0;
        
        // Visualizer bars
        const NUM_BARS = 32;
        for (let i = 0; i < NUM_BARS; i++) {{
            const bar = document.createElement('div');
            bar.className = 'bar';
            bar.style.height = '2px';
            visualizer.appendChild(bar);
        }}
        const bars = visualizer.querySelectorAll('.bar');
        
        // Load saved preference
        const savedBuffer = localStorage.getItem('rustcast_target_buffer');
        if (savedBuffer) {{
            targetBufferMs = parseInt(savedBuffer);
            targetBufferSlider.value = targetBufferMs;
            targetBufferValue.textContent = targetBufferMs;
        }}
        
        targetBufferSlider.addEventListener('input', (e) => {{
            targetBufferMs = parseInt(e.target.value);
            targetBufferValue.textContent = targetBufferMs;
            localStorage.setItem('rustcast_target_buffer', targetBufferMs);
        }});
        
        playBtn.addEventListener('click', togglePlay);
        
        async function togglePlay() {{
            if (isPlaying) {{
                stop();
            }} else {{
                await start();
            }}
        }}
        
        async function start() {{
            try {{
                statusEl.textContent = '⏳ Initializing...';
                statusEl.className = 'status buffering';
                playBtn.disabled = true;
                
                // Initialize Audio Context
                audioContext = new (window.AudioContext || window.webkitAudioContext)({{
                    sampleRate: 48000,
                    latencyHint: 'interactive'
                }});
                
                // Resume if suspended (browser autoplay policy)
                if (audioContext.state === 'suspended') {{
                    await audioContext.resume();
                }}
                
                // Initialize Opus decoder
                statusEl.textContent = '⏳ Loading Opus decoder...';
                opusDecoder = new OpusDecoder({{
                    channels: 2,
                    sampleRate: 48000
                }});
                await opusDecoder.ready;
                
                // Connect WebSocket
                statusEl.textContent = '⏳ Connecting...';
                const wsUrl = `ws://${{location.host}}/ws`;
                ws = new WebSocket(wsUrl);
                ws.binaryType = 'arraybuffer';
                
                ws.onopen = () => {{
                    statusEl.textContent = '🟢 Streaming (Ultra Low Latency)';
                    statusEl.className = 'status';
                    isPlaying = true;
                    playBtn.disabled = false;
                    playBtn.textContent = '⏹ Stop';
                    playBtn.className = 'stop-btn';
                    startTime = audioContext.currentTime;
                    nextPlayTime = audioContext.currentTime + (targetBufferMs / 1000);
                    totalSamplesPlayed = 0;
                    startStats();
                }};
                
                ws.onmessage = async (event) => {{
                    packetsReceived++;
                    const opusData = new Uint8Array(event.data);
                    
                    // Decode Opus to PCM
                    try {{
                        const decoded = await opusDecoder.decodeFrame(opusData);
                        if (decoded && decoded.channelData && decoded.channelData.length > 0) {{
                            scheduleAudio(decoded.channelData, decoded.samplesDecoded);
                        }}
                    }} catch (e) {{
                        console.warn('Decode error:', e);
                    }}
                }};
                
                ws.onerror = (e) => {{
                    console.error('WebSocket error:', e);
                    statusEl.textContent = '❌ Connection error';
                    statusEl.className = 'status error';
                }};
                
                ws.onclose = () => {{
                    if (isPlaying) {{
                        statusEl.textContent = '🔄 Reconnecting...';
                        statusEl.className = 'status buffering';
                        setTimeout(() => {{
                            if (isPlaying) start();
                        }}, 1000);
                    }}
                }};
                
            }} catch (e) {{
                console.error('Start error:', e);
                statusEl.textContent = '❌ ' + e.message;
                statusEl.className = 'status error';
                playBtn.disabled = false;
                stop();
            }}
        }}
        
        function scheduleAudio(channelData, samples) {{
            if (!audioContext || !isPlaying) return;
            
            const now = audioContext.currentTime;
            
            // Create buffer
            const buffer = audioContext.createBuffer(
                channelData.length,
                samples,
                48000
            );
            
            // Copy channel data
            for (let ch = 0; ch < channelData.length; ch++) {{
                buffer.copyToChannel(channelData[ch], ch);
            }}
            
            // Update visualizer
            updateVisualizer(channelData[0]);
            
            // Calculate play time
            if (nextPlayTime < now) {{
                // We're behind - catch up with minimal buffer
                nextPlayTime = now + (targetBufferMs / 1000);
            }}
            
            // Create and schedule source
            const source = audioContext.createBufferSource();
            source.buffer = buffer;
            source.connect(audioContext.destination);
            source.start(nextPlayTime);
            
            // Track timing
            const bufferDuration = samples / 48000;
            totalSamplesPlayed += samples;
            nextPlayTime += bufferDuration;
            
            // Update buffer stat (how far ahead we're scheduled)
            const bufferAhead = (nextPlayTime - now) * 1000;
            bufferEl.textContent = Math.round(bufferAhead);
            bufferEl.className = 'stat-value' + (bufferAhead < 30 ? ' bad' : bufferAhead < 50 ? ' warn' : '');
            
            // Estimate actual latency (network + buffer)
            const estimatedLatency = bufferAhead + 10; // +10ms for Opus frame
            latencyEl.textContent = Math.round(estimatedLatency);
            latencyEl.className = 'stat-value' + (estimatedLatency > 100 ? ' warn' : estimatedLatency > 200 ? ' bad' : '');
        }}
        
        function updateVisualizer(samples) {{
            const step = Math.floor(samples.length / NUM_BARS);
            for (let i = 0; i < NUM_BARS; i++) {{
                let sum = 0;
                for (let j = 0; j < step; j++) {{
                    sum += Math.abs(samples[i * step + j] || 0);
                }}
                const avg = sum / step;
                const height = Math.max(2, Math.min(50, avg * 200));
                bars[i].style.height = height + 'px';
            }}
        }}
        
        function startStats() {{
            statsInterval = setInterval(() => {{
                packetsPerSecond = packetsReceived - lastPacketCount;
                lastPacketCount = packetsReceived;
                packetsEl.textContent = packetsPerSecond;
            }}, 1000);
        }}
        
        function stop() {{
            isPlaying = false;
            
            if (ws) {{
                ws.close();
                ws = null;
            }}
            
            if (opusDecoder) {{
                opusDecoder.free();
                opusDecoder = null;
            }}
            
            if (audioContext) {{
                audioContext.close();
                audioContext = null;
            }}
            
            if (statsInterval) {{
                clearInterval(statsInterval);
                statsInterval = null;
            }}
            
            // Reset UI
            statusEl.textContent = '⏸ Stopped';
            statusEl.className = 'status';
            playBtn.textContent = '▶ Play';
            playBtn.className = 'play-btn';
            playBtn.disabled = false;
            latencyEl.textContent = '--';
            latencyEl.className = 'stat-value';
            bufferEl.textContent = '--';
            bufferEl.className = 'stat-value';
            packetsEl.textContent = '0';
            
            // Reset visualizer
            bars.forEach(bar => bar.style.height = '2px');
        }}
        
        // Handle page visibility for reconnection
        document.addEventListener('visibilitychange', () => {{
            if (document.hidden && isPlaying) {{
                // Could pause here if needed
            }}
        }});
    </script>
</body>
</html>"##, port)
    }

    /// Get index HTML page (legacy player)
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
        .latency-warn {{
            color: #f39c12;
        }}
        .latency-bad {{
            color: #e74c3c;
        }}
        .controls {{
            margin-top: 1rem;
            display: flex;
            gap: 10px;
            justify-content: center;
            flex-wrap: wrap;
        }}
        .latency-slider {{
            width: 100%;
            margin-top: 1rem;
        }}
        .latency-slider input {{
            width: 100%;
        }}
        .latency-slider label {{
            display: block;
            font-size: 0.8rem;
            color: #888;
            margin-bottom: 0.3rem;
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
        
        <div class="latency-slider">
            <label>🎯 Target Latency: <span id="targetLatencyValue">100</span>ms</label>
            <input type="range" id="targetLatency" min="30" max="500" value="100" step="10">
        </div>
        
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
        const targetLatencySlider = document.getElementById('targetLatency');
        const targetLatencyValue = document.getElementById('targetLatencyValue');
        
        let isPlaying = false;
        let bufferCheckInterval = null;
        let targetLatencyMs = 100; // Default target latency in ms
        
        // Load saved latency preference
        const savedLatency = localStorage.getItem('rustcast_target_latency');
        if (savedLatency) {{
            targetLatencyMs = parseInt(savedLatency);
            targetLatencySlider.value = targetLatencyMs;
            targetLatencyValue.textContent = targetLatencyMs;
        }}
        
        targetLatencySlider.addEventListener('input', (e) => {{
            targetLatencyMs = parseInt(e.target.value);
            targetLatencyValue.textContent = targetLatencyMs;
            localStorage.setItem('rustcast_target_latency', targetLatencyMs);
        }});
        
        function togglePlay() {{
            if (isPlaying) {{
                audio.pause();
                audio.src = '';
                isPlaying = false;
                playBtn.textContent = '▶ Play';
                status.textContent = '⏸ Paused';
                status.className = 'status';
                latencyInfo.textContent = 'Expected latency: ~50-100ms';
                latencyInfo.className = 'latency-info';
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
            // Aggressive buffer monitoring for ultra-low latency
            bufferCheckInterval = setInterval(() => {{
                if (!isPlaying) return;
                
                const buffered = audio.buffered;
                if (buffered.length > 0) {{
                    const bufferedEnd = buffered.end(buffered.length - 1);
                    const currentTime = audio.currentTime;
                    const bufferSize = bufferedEnd - currentTime;
                    const bufferMs = bufferSize * 1000;
                    const targetLatencySec = targetLatencyMs / 1000;
                    
                    // Update latency display with color coding
                    let className = 'latency-info';
                    if (bufferMs > 500) {{
                        className += ' latency-bad';
                    }} else if (bufferMs > 200) {{
                        className += ' latency-warn';
                    }}
                    latencyInfo.className = className;
                    
                    // Skip ahead if buffer exceeds target + 50ms tolerance
                    const skipThreshold = targetLatencySec + 0.05;
                    if (bufferSize > skipThreshold) {{
                        // Jump to near-live position (target latency from end)
                        audio.currentTime = bufferedEnd - targetLatencySec;
                        latencyInfo.textContent = `⚡ ${{bufferMs.toFixed(0)}}ms → Synced to ${{targetLatencyMs}}ms`;
                    }} else {{
                        latencyInfo.textContent = `⚡ Buffer: ${{bufferMs.toFixed(0)}}ms`;
                    }}
                }}
            }}, 50); // Check more frequently for faster response
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

/// Handle WebSocket connection for ultra-low latency streaming
fn handle_websocket(
    request: tiny_http::Request,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use sha1::{Sha1, Digest};
    use base64::Engine;
    
    // Get WebSocket key from headers
    let ws_key = request.headers()
        .iter()
        .find(|h| h.field.as_str().to_ascii_lowercase() == "sec-websocket-key")
        .map(|h| h.value.as_str().to_string())
        .ok_or("Missing Sec-WebSocket-Key")?;
    
    // Generate accept key
    let mut hasher = Sha1::new();
    hasher.update(ws_key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept_key = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
    
    // Get raw TCP stream
    let mut stream = request.into_writer();
    
    // Send WebSocket handshake response
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_key
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    
    log::info!("WebSocket handshake complete");
    
    // Stream Opus packets as binary WebSocket frames
    while let Ok(opus_packet) = rx.recv() {
        // Create WebSocket binary frame
        let frame = create_websocket_frame(&opus_packet);
        if stream.write_all(&frame).is_err() {
            break;
        }
        // Don't flush every packet - let TCP handle buffering for efficiency
    }
    
    Ok(())
}

/// Create a WebSocket binary frame
fn create_websocket_frame(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut frame = Vec::with_capacity(10 + len);
    
    // FIN + Binary opcode (0x82)
    frame.push(0x82);
    
    // Payload length (no masking for server->client)
    if len <= 125 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.push((len >> 8) as u8);
        frame.push((len & 0xFF) as u8);
    } else {
        frame.push(127);
        for i in (0..8).rev() {
            frame.push((len >> (i * 8)) as u8);
        }
    }
    
    // Payload
    frame.extend_from_slice(data);
    frame
}
