# RustCast - Copilot Instructions

## Overview
Windows system audio streaming server using Opus codec. **Double-click → Instant streaming** is the core goal.

## Architecture
```
AudioCapture (WASAPI) → OpusEncoder (audiopus) ──┬── StreamServer (HTTP/Ogg) → Legacy clients
         └─────────────── crossbeam-channel ──────┴── WebSocket (Raw Opus) → Low-latency clients
```

### Source Files
| File | Purpose |
|------|---------|
| `main.rs` | Entry point, GUI initialization, thread orchestration |
| `audio.rs` | WASAPI loopback capture (48kHz, f32 stereo) |
| `opus_encoder.rs` | PCM → Opus encoding (20ms frames, 192kbps default) |
| `server.rs` | HTTP/WebSocket server, embedded HTML players |
| `gui.rs` | Native Windows settings panel + system tray (native-windows-gui) |
| `config.rs` | JSON config at `%APPDATA%\rustcast\RustCast\config.json` |

## Build Commands
```powershell
# CMake required for Opus native library
$env:PATH = "C:\Program Files\CMake\bin;$env:PATH"

cargo build              # Debug (console visible)
cargo build --release    # Release (console hidden, LTO enabled)
cargo clippy             # Lint
cargo fmt                # Format
```

## Code Conventions
- **Error handling**: `Result<T, Box<dyn std::error::Error>>`
- **Thread communication**: `crossbeam_channel::bounded(4)` (not std::mpsc)
- **State flags**: `Arc<AtomicBool>` / `Arc<AtomicUsize>`
- **Logging**: `log::info!`, `log::error!` macros
- **Windows-only code**: `#[cfg(windows)]` attribute

## Key Patterns

### Low-Latency Channel Pipeline
```rust
// Small bounded buffers to minimize latency
let (audio_tx, audio_rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = crossbeam_channel::bounded(4);
let (opus_tx, opus_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = crossbeam_channel::bounded(4);
```

### Opus Encoding (`opus_encoder.rs`)
- Frame size: 20ms (960 samples @ 48kHz)
- Application mode: `LowDelay` for real-time streaming
- Accumulate samples in buffer until full frame
- Output: Raw Opus packets (WebSocket) or Ogg container (legacy HTTP)

### Manual Ogg Page Generation
Each client gets unique Ogg stream with proper headers:
```rust
OpusEncoder::create_ogg_page(data, serial, granule, page_sequence, is_bos)
```

### WebSocket Streaming (`/ws`)
- Raw Opus packets as binary frames (no Ogg wrapping)
- Client: opus-decoder WASM + Web Audio API
- Hard sync: skip frames when buffer > target (no playback rate changes)

## HTTP Endpoints
| Path | Response |
|------|----------|
| `/` | Low-latency player (WebSocket + Web Audio API) |
| `/legacy` | HTML5 Audio player (Ogg stream) |
| `/ws` | WebSocket (Raw Opus packets) |
| `/stream.opus` | Opus/Ogg audio stream |
| `/status` | `{"clients": N, "running": true}` |

## Config Structure
```rust
struct Config {
    port: u16,        // default: 3000
    bitrate: u32,     // default: 192 (kbps)
    auto_start: bool, // default: true
}
```

## Dependencies
| Crate | Purpose |
|-------|---------|
| `cpal` | WASAPI audio capture |
| `audiopus` | Opus encoding (libopus bindings) |
| `tiny_http` | Lightweight HTTP server |
| `sha1` / `base64` | WebSocket handshake |
| `native-windows-gui` | Windows native GUI + tray |
| `crossbeam-channel` | High-performance bounded channels |

## Build Requirements
- **CMake** - Required for Opus native library build
- **Visual Studio Build Tools** - C++ compiler

## Known Limitations
- Windows only (WASAPI dependency)
- Default output device only (no device selection)
- WebSocket latency: ~50-100ms | Legacy HTTP: ~2000-3000ms

## 알려진 제한사항
- Windows 전용 (WASAPI 의존)
- 기본 출력 장치만 캡처 (장치 선택 미지원)
- WebSocket 저지연 플레이어: ~50-100ms (Web Audio API)
- 레거시 HTTP 플레이어: ~2000-3000ms (브라우저 버퍼링)

## 테스트 방법
현재 유닛 테스트 없음. 수동 테스트:
1. `cargo run`으로 실행
2. `http://localhost:3000` 접속
3. 시스템 오디오 재생 후 스트리밍 확인
4. Chrome/Firefox 모두에서 재생 확인
