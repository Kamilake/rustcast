# 🎵 RustCast

**Windows 시스템 오디오 스트리밍 서버**

더블클릭 → 바로 시스템 오디오 스트리밍 시작! 🚀

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows-0078d4.svg)](https://www.microsoft.com/windows)

## ✨ 주요 기능

- 🖱️ **원클릭 실행** - exe 더블클릭만으로 서버 시작
- 🔊 **시스템 오디오 캡처** - PC에서 재생되는 모든 소리를 스트리밍
- ⚡ **저지연 스트리밍** - WebSocket + Opus 코덱으로 ~50-100ms 지연
- 🌐 **웹 기반 플레이어** - 브라우저에서 바로 재생 (설치 불필요)
- 📱 **모바일 지원** - 스마트폰, 태블릿 등 어디서든 접속
- 🔧 **시스템 트레이** - 백그라운드 실행 + 설정 패널

## 🏗️ 아키텍처

```
WASAPI (오디오 캡처) → Opus 인코더 ──┬── WebSocket (저지연) → 웹 플레이어
                                    └── HTTP/Ogg (레거시) → HTML5 Audio
```

## 🚀 빠른 시작

### 사전 준비

1. [Rust 설치](https://rustup.rs/) (1.70 이상)
2. [CMake 설치](https://cmake.org/download/) (Opus 빌드에 필요)
3. Visual Studio Build Tools (C++ 컴파일러)

### 빌드 & 실행

```powershell
# 클론
git clone https://github.com/Kamilake/rustcast.git
cd rustcast

# CMake 경로 설정 (필요시)
$env:PATH = "C:\Program Files\CMake\bin;$env:PATH"

# 릴리즈 빌드
cargo build --release

# 실행
./target/release/rustcast.exe
```

### 사용 방법

1. `rustcast.exe` 더블클릭
2. 시스템 트레이에 아이콘 생성
3. 브라우저가 자동으로 열림 (`http://localhost:3000`)
4. 음악 재생! 🎶

## 🔧 설정

설정 파일 위치: `%APPDATA%\rustcast\RustCast\config.json`

```json
{
  "port": 3000,
  "bitrate": 192,
  "auto_start": true
}
```

| 설정 | 설명 | 기본값 |
|------|------|--------|
| `port` | HTTP/WebSocket 서버 포트 | 3000 |
| `bitrate` | Opus 인코딩 비트레이트 (kbps) | 192 |
| `auto_start` | 실행 시 자동 스트리밍 시작 | true |

## 🌐 HTTP 엔드포인트

| 경로 | 설명 |
|------|------|
| `/` | 저지연 웹 플레이어 (WebSocket + Web Audio API) |
| `/legacy` | 레거시 HTML5 Audio 플레이어 |
| `/ws` | WebSocket 스트리밍 (Raw Opus 패킷) |
| `/stream.opus` | Opus/Ogg 오디오 스트림 |
| `/status` | 서버 상태 JSON |

## 📱 다른 기기에서 접속

### 같은 네트워크 내 접속

1. PC의 IP 주소 확인 (예: `192.168.1.100`)
2. 브라우저에서 `http://192.168.1.100:3000` 접속
3. Windows 방화벽에서 포트 허용 필요

### 지연 시간 비교

| 플레이어 | 지연 시간 | 비고 |
|----------|-----------|------|
| 저지연 (`/`) | ~50-100ms | WebSocket + Web Audio API |
| 레거시 (`/legacy`) | ~2000-3000ms | HTML5 Audio 버퍼링 |

## 📦 의존성

| 크레이트 | 용도 |
|----------|------|
| `cpal` | WASAPI 오디오 캡처 |
| `audiopus` | Opus 인코딩 |
| `tiny_http` | 경량 HTTP 서버 |
| `native-windows-gui` | Windows 네이티브 GUI |
| `crossbeam-channel` | 고성능 채널 통신 |

## 🛠️ 시스템 트레이 메뉴

| 메뉴 | 기능 |
|------|------|
| 🌐 브라우저에서 열기 | 웹 플레이어 열기 |
| ⏯️ 스트리밍 토글 | 스트리밍 시작/중지 |
| ⚙️ 설정 | 설정 패널 열기 |
| ❌ 종료 | 프로그램 종료 |

## 🤝 기여하기

기여 환영합니다! [CONTRIBUTING.md](CONTRIBUTING.md)를 참고해주세요.

## 📄 라이선스

MIT License - 자유롭게 사용하세요!

---

Made with ❤️ and 🦀 Rust
