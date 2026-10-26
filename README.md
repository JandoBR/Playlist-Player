# 🎵 Playlist Player

A lightweight, local-first music player built with **Tauri** and **Rust**. Give it a
YouTube playlist URL and it keeps a local copy of the tracks, synced.

https://github.com/user-attachments/assets/4f7f9500-e655-44fb-a3e7-892e6786dbaa

## Features

- **YouTube sync:** point it at a playlist and it downloads and syncs the tracks locally
  with `yt-dlp`.
- **Stays up to date:** the local library follows changes in the source playlist.
- **Shuffle only:** playback is random by design, no queue management.
- **Tray icon:** <img src="https://github.com/user-attachments/assets/89b9b2c3-ac55-4754-b50f-752f70d50d7e" align="right" width="178" height="214" alt="Tray menu"> play, skip and show or hide the window without opening the main UI.
- **Frameless window** with its own controls.
- **Keyboard shortcuts:** local ones when focused, global ones while you work elsewhere.

## Requirements

- Rust and the Tauri v2 CLI
- `yt-dlp` on `PATH`
- A desktop with an audio output (Linux, macOS or Windows)

## Run it

```bash
cargo install tauri-cli --version "^2"
cargo tauri dev
```

Build a bundle with `cargo tauri build`.

## Layout

```
src/           plain HTML, CSS and JS frontend (no bundler)
src-tauri/     Rust backend: player, playlist sync, tray and shortcuts
```
