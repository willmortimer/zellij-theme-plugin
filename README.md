# Zellij Theme Selector Plugin

<<<<<<< 
A terminal user interface (TUI) plugin for [Zellij](https://github.com/zellij-org/zellij) that allows you to preview and switch between themes in real-time.
=======
A terminal user interface (TUI) plugin for [Zellij](https://github.com/zellij-org/zellij) that allows you to preview and switch between themes in real-time. 

## Features

- 🎨 Interactive theme selection with live preview
- 📡 Fetches latest themes directly from Zellij's repository
- 💾 Local theme caching for faster startup
- ⌨️ Vim-style navigation (j/k or arrow keys)
- 🔄 Force refresh option for theme updates
- 🏃 Fast and responsive TUI using ratatui

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/zellij-theme-plugin
cd zellij-theme-plugin

# Build the plugin
cargo build --release

# Copy the plugin to your Zellij plugins directory
mkdir -p ~/.config/zellij/plugins
cp target/release/libzellij_theme_selector.so ~/.config/zellij/plugins/
```

## Usage

1. Start Zellij
2. Press `Ctrl+o` to enter session mode
3. Press `p` to open the plugin manager
4. Select "Theme Selector" to launch the plugin

### Navigation

- `↑/k`: Move selection up
- `↓/j`: Move selection down
- `Enter`: Apply selected theme
- `q`: Quit the plugin

### Command Line Options

- `--force-refresh`: Force refresh theme list from GitHub

## Implementation Details

The plugin is implemented in Rust and uses:

- `ratatui`: Terminal user interface library
- `crossterm`: Terminal manipulation
- `reqwest`: HTTP client for fetching themes
- `tokio`: Async runtime
- `kdl`: Configuration file parsing
- `serde`: Serialization for theme caching

### Architecture

1. **Theme Discovery**
<<<<<<< HEAD

=======
>>>>>>> b6f5dc0aeb099e2d568e472ebbb0973623363147
   - Fetches themes from Zellij's GitHub repository
   - Parses KDL theme files to extract theme names
   - Caches results locally with hourly expiration

2. **Configuration Management**
<<<<<<< HEAD

=======
>>>>>>> b6f5dc0aeb099e2d568e472ebbb0973623363147
   - Automatically locates Zellij config file
   - Uses KDL parser for safe config modifications
   - Creates theme directory if needed

3. **User Interface**
   - Built with ratatui for a modern TUI experience
   - Responsive design with status updates
   - Vim-style keybindings

## Development

```bash
# Run with logging
RUST_LOG=debug cargo run

# Run with force refresh
cargo run -- --force-refresh

# Build for release
cargo build --release
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## License

[MIT](LICENSE)

## Acknowledgments

- [Zellij](https://github.com/zellij-org/zellij) - The terminal workspace
- [ratatui](https://github.com/ratatui-org/ratatui) - TUI library
- Theme authors who contributed to Zellij
