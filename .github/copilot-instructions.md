# RustCast - Copilot Instructions

## 프로젝트 개요
Windows 시스템 오디오를 MP3로 실시간 스트리밍하는 서버. **더블클릭 → 즉시 실행**이 핵심 목표.

## 아키텍처
```
AudioCapture (WASAPI) → Mp3Encoder (LAME) → StreamServer (HTTP) → 클라이언트
         └─────────────── crossbeam-channel ───────────────┘
```

### 핵심 데이터 흐름
1. `audio.rs` - WASAPI 루프백으로 시스템 오디오 캡처 (48kHz, f32 스테레오)
2. `encoder.rs` - PCM → MP3 실시간 인코딩 (LAME, 192kbps 기본)
3. `server.rs` - HTTP 스트리밍 (`/stream.mp3`), 다중 클라이언트 브로드캐스트
4. `tray.rs` - 시스템 트레이 UI + 콘솔 폴백
5. `config.rs` - JSON 설정 (`%APPDATA%\rustcast\RustCast\config.json`)

## 빌드 명령어
```powershell
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

### 채널 기반 파이프라인
```rust
let (audio_tx, audio_rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = crossbeam_channel::bounded(64);
let (mp3_tx, mp3_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = crossbeam_channel::bounded(64);
```

### MP3 인코더 버퍼 처리
`mp3lame-encoder`는 `MaybeUninit<u8>` 버퍼를 요구:
```rust
let mut mp3_buffer: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); size];
// 인코딩 후 unsafe { m.assume_init() }로 변환
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
| `/` | 웹 플레이어 HTML |
| `/stream.mp3` | MP3 오디오 스트림 |
| `/status` | `{"clients": N, "running": true}` |

## 의존성 역할
- `cpal` - WASAPI 오디오 캡처
- `mp3lame-encoder` - LAME MP3 인코딩
- `tiny_http` - 경량 HTTP 서버
- `tray-item` - 시스템 트레이
- `crossbeam-channel` - 고성능 스레드 채널
- `directories` - 플랫폼별 설정 경로

## 알려진 제한사항
- Windows 전용 (WASAPI 의존)
- 기본 출력 장치만 캡처 (장치 선택 미지원)
- MP3 인코딩 레이턴시 ~1초

## 테스트 방법
현재 유닛 테스트 없음. 수동 테스트:
1. `cargo run`으로 실행
2. `http://localhost:3000` 접속
3. 시스템 오디오 재생 후 스트리밍 확인
