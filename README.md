# ClipScraper Downloader

App to download content from Youtube, TikTok, Instagram, Pinterest

## Features:

### Single download:

- Giving the url of a specific youtube, tiktok or instagram content it should download it to the selected folder (by default, download folder). Selected option may apply to only get the audio if the link points to a video.

### Bulk download:

- Using the companion extension (currently only available on Chromium based browsers) the app can accept a list of urls scraped from pages/profiles/bookmarks and download them sequentially, or in parallel, and output files sorted by platform, and collection

## Local Development

### Recommended IDE Setup

This template should help get you started developing with Tauri and Yew.

[VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

To run the app in development mode with hot-reloading:

```bash
sh run.sh
```

This script handles the setup of necessary configuration files and starts the Tauri development server.

### Building the Application

You can build a production-ready, optimized version of your application for your current operating system.

#### Standard Build

This command creates an optimized executable and associated installer.

```bash
cargo tauri build
```

The output will be located in `src-tauri/target/release/bundle/`.

### Recompiling

To ensure you are starting from a clean slate, you can clean the build artifacts before recompiling.

```bash
# Clean previous build artifacts
cargo clean

# Build the application again
cargo tauri build
```

### Cross-Platform Compilation

Tauri can build your application for different platforms from a single machine.

#### Building for Windows (from macOS/Linux)

```bash
cargo tauri build --target x86_64-pc-windows-msvc
```

#### Building for macOS (from Windows/Linux)

**Note**: Building for macOS from a non-macOS machine is complex and often requires setting up a cross-compilation toolchain and a macOS SDK. It's generally recommended to build for macOS on a macOS machine.

```bash
cargo tauri build --target x86_64-apple-darwin
# For Apple Silicon
cargo tauri build --target aarch64-apple-darwin
```

#### Building for Linux (from macOS/Windows) -- Untested yet

```bash
cargo tauri build --target x86_64-unknown-linux-gnu
```

The output for cross-platform builds will also be in the `src-tauri/target/release/bundle/` directory, under the respective target's folder.

## Roadmap

For a detailed list of planned features, ongoing tasks, and future ideas, please see the project's roadmap.

[View the Roadmap (TODO.MD)](./TODO.MD)

# Contributing:

I'm open to contributions. Issues listed in TODO.MD are my priority but feel free to look into them.
All new contributions will require testing to be approved.
