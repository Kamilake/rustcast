# RustCast - Copilot Instructions

## 프로젝트 개요
Windows 시스템 오디오를 Opus/Ogg로 실시간 스트리밍하는 서버. **더블클릭 → 즉시 실행**이 핵심 목표.

## 아키텍처
```
AudioCapture (WASAPI) → OpusEncoder (audiopus) ──┬── StreamServer (HTTP/Ogg) → 레거시 클라이언트
         └─────────────── crossbeam-channel ──────┴── WebSocket (Raw Opus) → 저지연 클라이언트
```

### 핵심 데이터 흐름
1. `audio.rs` - WASAPI 루프백으로 시스템 오디오 캡처 (48kHz, f32 스테레오)
2. `opus_encoder.rs` - PCM → Opus 실시간 인코딩 (10ms 프레임, 128kbps 기본)
3. `server.rs` - HTTP/WebSocket 스트리밍, WebSocket은 Raw Opus 패킷, HTTP는 Ogg 컨테이너
4. `tray.rs` - 시스템 트레이 UI + 콘솔 폴백
5. `gui.rs` - Windows 네이티브 설정 패널 (native-windows-gui)
6. `config.rs` - JSON 설정 (`%APPDATA%\rustcast\RustCast\config.json`)

## 빌드 명령어
```powershell
# CMake 필수 (Opus 빌드에 필요)
$env:PATH = "C:\Program Files\CMake\bin;$env:PATH"

cargo build              # 디버그 (콘솔 표시됨)
cargo build --release    # 릴리즈 (콘솔 숨김, LTO 최적화)
cargo clippy             # 린트 검사
cargo fmt                # 코드 포매팅
```

## 코드 컨벤션
- **에러 처리**: `Result<T, Box<dyn std::error::Error>>` 패턴 사용
- **스레드 통신**: `crossbeam-channel` (표준 mpsc 대신)
- **상태 플래그**: `Arc<AtomicBool>` / `Arc<AtomicUsize>`
- **로깅**: `log` 매크로 사용 (`log::info!`, `log::error!` 등)
- **Windows 전용 코드**: `#[cfg(windows)]` 어트리뷰트로 분리

## 주요 패턴

### 채널 기반 파이프라인 (저지연 최적화)
```rust
let (audio_tx, audio_rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = crossbeam_channel::bounded(4);
let (opus_tx, opus_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = crossbeam_channel::bounded(4);
```

### Opus 인코딩
- 프레임 크기: 10ms (480 샘플 @ 48kHz) - 최저 지연
- Application 모드: `LowDelay` - 실시간 스트리밍 최적화
- 샘플 버퍼링으로 완전한 프레임만 인코딩
- Raw Opus 패킷 → WebSocket (저지연) 또는 Ogg 컨테이너 (레거시)

### WebSocket 스트리밍 (저지연)
- `/ws` 엔드포인트로 WebSocket 연결
- Raw Opus 패킷을 바이너리 프레임으로 전송 (Ogg 래핑 없음)
- 클라이언트: opus-decoder WASM + Web Audio API
- Hard sync: 버퍼 > 타겟 시 스킵 (playback rate 변경 없음)

### Ogg 컨테이너 (레거시 클라이언트용)
각 클라이언트가 유니크한 Ogg 스트림을 받도록 수동으로 Ogg 페이지 생성:
```rust
// BOS 플래그는 첫 페이지에만, 이후 페이지는 continuation
OpusEncoder::create_ogg_page(data, header_type, granule, serial, sequence)
OpusEncoder::wrap_opus_packet(packet, granule, serial, sequence)  // 오디오 페이지
```

### 아이콘 로딩 (폴백 체인)
1. `resources/rustcast_envelope.ico` (개발 환경)
2. 임베디드 리소스 (`build.rs`로 임베드)
3. 콘솔 모드 (트레이 실패 시)

## 설정 구조
```rust
struct Config {
    port: u16,        // 기본값: 3000
    bitrate: u32,     // 기본값: 192 (64/96/128/160/192/256/320)
    auto_start: bool, // 기본값: true
}
```

## HTTP 엔드포인트
| 경로 | 응답 |
|------|------|
| `/` | 저지연 웹 플레이어 (WebSocket + Web Audio API) |
| `/legacy` | 레거시 HTML5 Audio 플레이어 |
| `/ws` | WebSocket 스트리밍 (Raw Opus 패킷) |
| `/stream.opus` | Opus/Ogg 오디오 스트림 (레거시) |
| `/status` | `{"clients": N, "running": true}` |

## 의존성 역할
- `cpal` - WASAPI 오디오 캡처
- `audiopus` - Opus 인코딩 (libopus 바인딩)
- `tiny_http` - 경량 HTTP 서버
- `sha1` / `base64` - WebSocket 핸드셰이크
- `native-windows-gui` - Windows 네이티브 GUI
- `crossbeam-channel` - 고성능 스레드 채널
- `directories` - 플랫폼별 설정 경로

## 빌드 요구사항
- **CMake** - Opus 네이티브 라이브러리 빌드에 필요
- **Visual Studio Build Tools** - C++ 컴파일러

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
