# Spotify Client (Rust + Iced)

A lightweight, high-performance Spotify client built with Rust and Iced, designed to provide the full Spotify experience with significantly lower RAM usage and better performance than the official client while mimicking its interface and features.

## 🎯 Project Goals

- **Performance**: Leverage Rust's efficiency for minimal resource consumption
- **Native Feel**: Fast, responsive UI using the Iced GUI framework
- **Feature Parity**: Implement core Spotify functionality matching the official client
- **Open Source**: Community-driven development and transparency

## ⚡ Current Status

**Early Development** - This project is in active development. Currently implemented:

- [ ] Authentication with Spotify Web API
- [ ] Display user playlists
- [ ] Playlist detail view
- [ ] Track playback controls
- [ ] Search functionality
- [ ] Album/Artist browsing
- [ ] Queue management

## 🚀 Getting Started

### Prerequisites

- Rust 1.70+ ([Install Rust](https://rustup.rs/))
- Spotify Premium account (required for playback features)
- Spotify Developer App credentials

### Spotify API Setup

1. Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Create a new app
3. Note your **Client ID** and **Client Secret**
4. Add `http://localhost:8888/callback` to Redirect URIs
5. Create a `.env` file in the project root:

```env
SPOTIFY_CLIENT_ID=your_client_id_here
SPOTIFY_CLIENT_SECRET=your_client_secret_here
SPOTIFY_REDIRECT_URI=http://localhost:8888/callback
```

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/spotify-client
cd spotify-client

# Build and run
cargo run --release
```

## 🏗️ Architecture

The project is structured around three main layers:

```
src/
├── main.rs           # Application entry point and Iced setup
├── spotify.rs        # Spotify Web API client wrapper
├── playback.rs       # Audio playback engine (future)
└── ui/
    ├── mod.rs        # UI module exports
    ├── playlist.rs   # Playlist views
    ├── player.rs     # Playback controls
    └── search.rs     # Search interface
```

### Key Dependencies

- **iced** - Cross-platform GUI framework
- **rspotify** - Spotify Web API client
- **tokio** - Async runtime
- **librespot** (planned) - Audio playback via Spotify Connect

## 🎵 Playback Approach

The client will support two playback modes:

1. **Device Control Mode** (Current)
   - Control playback on existing Spotify devices
   - Uses official Web API (ToS compliant)
   - Requires another device running Spotify

2. **Integrated Playback** (Planned)
   - Direct audio playback using librespot
   - Registers as a Spotify Connect device
   - Full offline support and lower latency

## 🛠️ Development

### Running in Development

```bash
cargo run
```

### Building for Release

```bash
cargo build --release
```

The optimized binary will be available at `target/release/spotify-client`

### Code Style

This project uses standard Rust formatting:

```bash
cargo fmt
cargo clippy
```

## 🤝 Contributing

Contributions are welcome! This project is in early stages, and there's plenty to build.

### Areas for Contribution

- UI/UX improvements
- Additional Spotify API features
- Performance optimizations
- Cross-platform testing
- Documentation

Please open an issue before starting work on major features.

## 📋 Roadmap

### Phase 1: Foundation (Current)
- [x] Project setup
- [ ] Basic authentication flow
- [ ] Playlist listing
- [ ] Simple UI layout

### Phase 2: Core Features
- [ ] Track playback controls
- [ ] Search functionality
- [ ] Album and artist views
- [ ] Queue management
- [ ] Playlist editing

### Phase 3: Advanced Features
- [ ] Integrated playback (librespot)
- [ ] Offline mode
- [ ] Custom themes
- [ ] Keyboard shortcuts
- [ ] System media controls integration

### Phase 4: Polish
- [ ] Performance profiling and optimization
- [ ] Cross-platform packaging
- [ ] User preferences and settings
- [ ] Lyrics support
- [ ] Social features (friend activity)

## 📊 Performance Goals

Target metrics compared to official Spotify client:

- **RAM Usage**: < 150MB (vs ~500MB for official client)
- **Startup Time**: < 2s
- **CPU Usage (idle)**: < 1%
- **Binary Size**: < 30MB

## ⚖️ Legal & Licensing

This project is licensed under the MIT License - see LICENSE file for details.

**Important Notes:**
- This is an unofficial client and is not affiliated with Spotify
- Using librespot may violate Spotify's Terms of Service
- Requires a Spotify Premium subscription for full functionality
- This project is for educational purposes and personal use

## 🙏 Acknowledgments

- [librespot](https://github.com/librespot-org/librespot) - Spotify streaming library
- [rspotify](https://github.com/ramsayleung/rspotify) - Spotify Web API wrapper
- [Iced](https://github.com/iced-rs/iced) - GUI framework
- The Rust community for excellent tooling and libraries

## 📫 Contact

For questions or suggestions, please open an issue on GitHub.

---

**Status**: 🚧 Early Development | **License**: MIT | **Language**: Rust 🦀
