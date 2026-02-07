# 🎵 RustCast 기여 가이드

RustCast에 관심을 가져주셔서 감사합니다! 이 문서는 프로젝트의 구조, 개발 과정, 그리고 기여 방법에 대해 설명합니다.

## 📖 목차

- [프로젝트 개요](#프로젝트-개요)
- [기술 스택](#기술-스택)
- [아키텍처](#아키텍처)
- [개발 환경 설정](#개발-환경-설정)
- [빌드 방법](#빌드-방법)
- [코드 구조](#코드-구조)
- [핵심 모듈 설명](#핵심-모듈-설명)
- [설정 파일](#설정-파일)
- [기여 방법](#기여-방법)
- [알려진 이슈 및 제한사항](#알려진-이슈-및-제한사항)
- [향후 개선 사항](#향후-개선-사항)

---

## 프로젝트 개요

RustCast는 **"더블클릭 → 바로 시스템 오디오 송출 서버 ON"** 을 목표로 만들어진 Windows 전용 오디오 스트리밍 서버입니다.

### 핵심 목표
- 🖱️ **제로 설정** - exe 파일 실행만으로 즉시 작동
- 🔊 **시스템 오디오 캡처** - PC에서 재생되는 모든 소리를 스트리밍
- 🌐 **웹 기반 접근** - 별도 앱 설치 없이 브라우저로 접속
- 📱 **크로스 디바이스** - 같은 네트워크 내 모든 기기에서 접속 가능

### 사용 시나리오
- 📺 PC 소리를 다른 방에서 듣기
- 🎮 게임/영상 소리를 스마트폰으로 스트리밍
- 🎧 무선 스피커가 없을 때 스마트폰을 스피커로 활용

---

## 기술 스택

| 구성요소 | 라이브러리 | 용도 |
|---------|-----------|------|
| 오디오 캡처 | [cpal](https://crates.io/crates/cpal) v0.15 | WASAPI를 통한 시스템 오디오 루프백 캡처 |
| MP3 인코딩 | [mp3lame-encoder](https://crates.io/crates/mp3lame-encoder) v0.2 | 실시간 PCM → MP3 변환 |
| HTTP 서버 | [tiny_http](https://crates.io/crates/tiny_http) v0.12 | 경량 HTTP 스트리밍 서버 |
| 시스템 트레이 | [tray-item](https://crates.io/crates/tray-item) v0.10 | Windows 시스템 트레이 아이콘 |
| 설정 관리 | [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) | JSON 설정 파일 처리 |
| 스레드 통신 | [crossbeam-channel](https://crates.io/crates/crossbeam-channel) v0.5 | 고성능 멀티 스레드 채널 |
| 로깅 | [log](https://crates.io/crates/log) + [env_logger](https://crates.io/crates/env_logger) | 구조화된 로깅 |
| Windows API | [windows-sys](https://crates.io/crates/windows-sys) v0.52 | 아이콘 로딩 등 Windows 네이티브 기능 |

---

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                         RustCast                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ Audio        │    │ MP3          │    │ HTTP         │       │
│  │ Capture      │───▶│ Encoder      │───▶│ Server       │───▶ 🌐│
│  │ (WASAPI)     │    │ (LAME)       │    │ (tiny_http)  │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│         │                                       │                │
│         │           ┌──────────────┐           │                │
│         └──────────▶│ System Tray  │◀──────────┘                │
│                     │ (tray-item)  │                            │
│                     └──────────────┘                            │
│                            │                                     │
│                     ┌──────────────┐                            │
│                     │ Config       │                            │
│                     │ (serde_json) │                            │
│                     └──────────────┘                            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 데이터 흐름

1. **오디오 캡처** (audio.rs)
   - WASAPI를 통해 기본 출력 장치의 오디오를 캡처
   - 48kHz, 스테레오, f32 포맷으로 샘플링
   - `crossbeam-channel`을 통해 인코더로 전송

2. **MP3 인코딩** (encoder.rs)
   - LAME 라이브러리로 실시간 MP3 인코딩
   - 기본 192kbps (설정 가능: 64~320kbps)
   - 인코딩된 청크를 서버로 전송

3. **HTTP 스트리밍** (server.rs)
   - `/` - 웹 플레이어 HTML 제공
   - `/stream.mp3` - MP3 오디오 스트림
   - `/status` - 서버 상태 JSON API
   - 다중 클라이언트 동시 지원

4. **시스템 트레이** (tray.rs)
   - 백그라운드 실행 관리
   - 우클릭 메뉴로 조작
   - 아이콘 파일 또는 임베디드 리소스 사용

---

## 개발 환경 설정

### 필수 요구사항

- **OS**: Windows 10/11 (WASAPI 필요)
- **Rust**: 1.70 이상 (권장: 최신 stable)
- **Build Tools**: Visual Studio Build Tools (C++ 컴파일러)

### Rust 설치

```powershell
# Rustup 설치 (https://rustup.rs)
winget install Rustlang.Rustup

# 또는 공식 인스톨러 사용
# https://www.rust-lang.org/tools/install

# 최신 버전으로 업데이트
rustup update stable

# 버전 확인
rustc --version  # rustc 1.93.0 이상
cargo --version
```

### 프로젝트 클론

```powershell
git clone https://github.com/your-username/rustcast.git
cd rustcast
```

---

## 빌드 방법

### 개발 빌드 (디버그)

```powershell
cargo build
cargo run
```

- 빠른 컴파일
- 디버그 심볼 포함
- 콘솔 창 표시됨

### 릴리즈 빌드

```powershell
cargo build --release
```

- 최적화됨 (LTO 활성화)
- 실행 파일 크기: ~2.3MB
- 콘솔 창 숨김 (`windows_subsystem = "windows"`)
- 출력 위치: `target/release/rustcast.exe`

### 아이콘 임베딩

`build.rs`가 자동으로 `resources/rustcast_envelope.ico`를 실행 파일에 임베드합니다.

```rust
// build.rs
res.set_icon("resources/rustcast_envelope.ico");
```

---

## 코드 구조

```
rustcast/
├── src/
│   ├── main.rs        # 진입점, 앱 초기화 및 이벤트 루프
│   ├── audio.rs       # WASAPI 오디오 캡처
│   ├── encoder.rs     # MP3 인코딩 (LAME)
│   ├── server.rs      # HTTP 스트리밍 서버
│   ├── config.rs      # 설정 파일 관리
│   └── tray.rs        # 시스템 트레이 아이콘
├── resources/
│   ├── rustcast_envelope.ico    # 트레이/앱 아이콘
│   └── rustcast_envelope_*.png  # 다양한 크기 PNG
├── build.rs           # Windows 리소스 컴파일
├── Cargo.toml         # 의존성 정의
├── README.md          # 사용자 가이드
├── CONTRIBUTING.md    # 이 파일
└── LICENSE            # MIT 라이선스
```

---

## 핵심 모듈 설명

### `audio.rs` - 오디오 캡처

```rust
pub struct AudioCapture {
    stream: Option<Stream>,
    pub sample_rate: u32,    // 48000Hz
    pub channels: u16,       // 2 (스테레오)
    is_capturing: Arc<AtomicBool>,
}
```

**주요 기능:**
- `new()` - 기본 출력 장치 감지 및 초기화
- `start(tx)` - 오디오 캡처 시작, 샘플을 채널로 전송
- `stop()` - 캡처 중지

**WASAPI 루프백:**
```rust
let host = cpal::host_from_id(cpal::HostId::Wasapi)?;
let device = host.default_output_device()?;
```

### `encoder.rs` - MP3 인코딩

```rust
pub struct Mp3Encoder {
    encoder: Encoder,
    channels: u16,
}
```

**주요 기능:**
- `new(sample_rate, channels, bitrate)` - LAME 인코더 초기화
- `encode(samples)` - f32 PCM → MP3 바이트 변환
- `flush()` - 남은 버퍼 플러시

**지원 비트레이트:** 64, 96, 128, 160, 192, 256, 320 kbps

### `server.rs` - HTTP 서버

```rust
pub struct StreamServer {
    port: u16,
    is_running: Arc<AtomicBool>,
    client_count: Arc<AtomicUsize>,
}
```

**엔드포인트:**
| 경로 | 응답 |
|------|------|
| `/` | HTML 웹 플레이어 |
| `/stream.mp3` | MP3 오디오 스트림 |
| `/status` | `{"clients": N, "running": true}` |

**멀티 클라이언트 지원:**
- 브로드캐스트 패턴으로 모든 클라이언트에 동일 스트림 전송
- 연결/해제 시 클라이언트 수 자동 추적

### `config.rs` - 설정 관리

```rust
pub struct Config {
    pub port: u16,        // 기본값: 3000
    pub bitrate: u32,     // 기본값: 192
    pub auto_start: bool, // 기본값: true
}
```

**설정 파일 위치:**
```
%APPDATA%\rustcast\RustCast\config.json
```

### `tray.rs` - 시스템 트레이

```rust
pub enum TrayAction {
    OpenBrowser,    // 브라우저 열기
    ToggleStream,   // 스트리밍 토글
    Settings,       // 설정 표시
    Quit,           // 종료
}
```

**아이콘 로딩 순서:**
1. `resources/rustcast_envelope.ico` (개발 시)
2. 임베디드 리소스 (릴리즈 빌드)
3. 콘솔 모드 폴백 (아이콘 없을 시)

---

## 설정 파일

### 위치
```
%APPDATA%\rustcast\RustCast\config.json
```

### 예시
```json
{
  "port": 3000,
  "bitrate": 192,
  "auto_start": true
}
```

### 옵션 설명

| 옵션 | 타입 | 기본값 | 설명 |
|------|------|--------|------|
| `port` | u16 | 3000 | HTTP 서버 포트 (1-65535) |
| `bitrate` | u32 | 192 | MP3 비트레이트 kbps (64/96/128/160/192/256/320) |
| `auto_start` | bool | true | 실행 시 자동으로 스트리밍 시작 |

---

## 기여 방법

### 이슈 등록

버그 리포트나 기능 제안은 GitHub Issues를 사용해주세요:

- 🐛 **버그 리포트**: 재현 단계, 예상 동작, 실제 동작 포함
- 💡 **기능 제안**: 사용 사례와 기대 효과 설명
- 📚 **문서 개선**: 오타, 불명확한 설명 수정

### Pull Request

1. **Fork** 후 브랜치 생성
   ```bash
   git checkout -b feature/amazing-feature
   ```

2. **코드 작성** 및 테스트
   ```bash
   cargo build
   cargo clippy  # 린트 검사
   cargo fmt     # 포매팅
   ```

3. **커밋** (Conventional Commits 권장)
   ```bash
   git commit -m "feat: add volume control"
   git commit -m "fix: handle audio device disconnection"
   git commit -m "docs: update README with new feature"
   ```

4. **Push** 및 PR 생성
   ```bash
   git push origin feature/amazing-feature
   ```

### 코드 스타일

- `cargo fmt`로 포매팅
- `cargo clippy`로 린트 통과
- 주요 함수에 문서 주석 (`///`) 작성
- 에러 처리는 `Result`/`Option` 사용

---

## 알려진 이슈 및 제한사항

### 현재 제한사항

| 이슈 | 설명 | 상태 |
|------|------|------|
| Windows 전용 | WASAPI는 Windows에서만 사용 가능 | 설계상 제한 |
| 단일 출력 장치 | 현재 기본 출력 장치만 캡처 | 향후 개선 예정 |
| 레이턴시 | MP3 인코딩으로 인한 ~1초 지연 | 트레이드오프 |
| 콘솔 창 (디버그) | 개발 빌드에서 콘솔 표시 | 릴리즈에서 해결 |

### 빌드 시 경고

현재 일부 unused 경고가 있습니다 (향후 기능 확장용 코드):

```
warning: method `save` is never used
warning: method `is_capturing` is never used
warning: methods `client_count` and `is_running` are never used
```

이 코드들은 UI 개선 시 사용될 예정입니다.

---

## 향후 개선 사항

### 계획된 기능

- [ ] **볼륨 조절** - 서버 측 게인 컨트롤
- [ ] **장치 선택** - 특정 오디오 장치 캡처
- [ ] **실시간 설정 변경** - 재시작 없이 설정 적용
- [ ] **Opus 코덱** - 더 낮은 레이턴시 옵션
- [ ] **QR 코드** - 모바일 접속 편의성
- [ ] **HTTPS** - 보안 연결 지원
- [ ] **인스톨러** - MSI/NSIS 패키지

### 기여 환영 분야

- 🎨 UI/UX 개선 (웹 플레이어 디자인)
- 🧪 테스트 코드 작성
- 📱 PWA 지원
- 🌍 다국어 지원
- 📦 패키지 배포 (Chocolatey, Scoop, winget)

---

## 개발 히스토리

이 프로젝트는 2026년 2월 7일에 시작되었습니다.

### 초기 개발 과정

1. **Rust 환경 설정** - rustup으로 1.93.0 버전 설치
2. **프로젝트 구조 설계** - 모듈화된 아키텍처 설계
3. **핵심 기능 구현**
   - WASAPI 오디오 캡처 (cpal)
   - MP3 인코딩 (mp3lame-encoder)
   - HTTP 스트리밍 서버 (tiny_http)
4. **시스템 트레이** - tray-item + Windows API 통합
5. **아이콘 통합** - 프로페셔널 아이콘 리소스 임베딩
6. **빌드 최적화** - LTO, 스트리핑으로 2.3MB 달성

### 기술적 도전

- **MaybeUninit 처리**: mp3lame-encoder가 `MaybeUninit<u8>` 버퍼를 요구해서 변환 로직 구현
- **Windows API 타입**: windows-sys의 HICON이 isize 타입이라 호환 처리 필요
- **트레이 아이콘 로딩**: 파일/리소스 두 가지 방식 모두 지원하도록 폴백 구현

---

## 라이선스

MIT License - 자유롭게 사용, 수정, 배포할 수 있습니다.

---

## 연락처

- GitHub Issues: [프로젝트 이슈 페이지]
- 이메일: [이메일 주소]

감사합니다! 🎵🦀
