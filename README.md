# 🎵 RustCast

**Windows System Audio Streaming Server**

더블클릭 → 바로 시스템 오디오 송출 서버 ON! 🚀

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows-0078d4.svg)](https://www.microsoft.com/windows)

## ✨ Features

- 🖱️ **원클릭 실행** - exe 더블클릭만으로 서버 시작
- 🔊 **시스템 오디오 캡처** - PC에서 재생되는 모든 소리를 스트리밍
- 🌐 **웹 기반 플레이어** - 브라우저에서 바로 재생
- 📱 **모바일 지원** - 스마트폰, 태블릿 등 어디서든 접속
- 🔧 **시스템 트레이** - 백그라운드 실행 + 우클릭 메뉴
- ⚙️ **설정 가능** - 포트, 비트레이트 등 커스터마이즈

## 🚀 Quick Start

### 사전 준비

1. [Rust 설치](https://rustup.rs/) (1.70 이상)

### 빌드 & 실행

```bash
# 클론
git clone https://github.com/your-username/rustcast.git
cd rustcast

# 빌드 (릴리즈)
cargo build --release

# 실행
./target/release/rustcast.exe
```

### 사용 방법

1. `rustcast.exe` 더블클릭
2. 시스템 트레이에 아이콘 생성됨
3. 브라우저가 자동으로 열림 (http://localhost:3000)
4. 음악 재생! 🎶

## 🔧 Configuration

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
| `port` | HTTP 서버 포트 | 3000 |
| `bitrate` | MP3 인코딩 비트레이트 (kbps) | 192 |
| `auto_start` | 실행 시 자동 스트리밍 시작 | true |

## 📱 접속 방법

### 같은 네트워크 내 다른 기기에서 접속

1. PC의 IP 주소 확인 (예: `192.168.1.100`)
2. 브라우저에서 `http://192.168.1.100:3000` 접속
3. Windows 방화벽에서 포트 허용 필요할 수 있음

### 직접 스트림 URL

- 웹 플레이어: `http://[IP]:3000/`
- MP3 스트림: `http://[IP]:3000/stream.mp3`

## 🛠️ System Tray Menu

| 메뉴 | 기능 |
|------|------|
| 🌐 Open in Browser | 웹 플레이어 열기 |
| ⏯️ Toggle Streaming | 스트리밍 시작/중지 |
| ⚙️ Settings... | 설정 확인 |
| ❌ Quit | 프로그램 종료 |

## 📦 Dependencies

- [cpal](https://crates.io/crates/cpal) - 크로스 플랫폼 오디오 캡처
- [tiny_http](https://crates.io/crates/tiny_http) - 경량 HTTP 서버
- [mp3lame-encoder](https://crates.io/crates/mp3lame-encoder) - MP3 인코딩
- [tray-item](https://crates.io/crates/tray-item) - 시스템 트레이

## 🤝 Contributing

기여 환영합니다! Issue나 PR을 남겨주세요.

## 📄 License

MIT License - 자유롭게 사용하세요!

---

Made with ❤️ and 🦀 Rust
