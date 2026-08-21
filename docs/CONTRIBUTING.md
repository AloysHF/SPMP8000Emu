# Contributing

## How to Contribute

1. **Fork** this repository
2. **Create** a feature branch (`git checkout -b feature/your-feature`)
3. **Commit** your changes (`git commit -m 'Add your feature'`)
4. **Push** to the branch (`git push origin feature/your-feature`)
5. **Open** a Pull Request

## Code Style

- Use English for all comments and documentation
- Use `snake_case` for functions and variables
- Use `PascalCase` for types and structs
- Prefer `anyhow::Result` for error handling
- Use `log` crate for logging (not `println!`)

## Areas That Need Help

- **Game compatibility testing** — test more games and report issues with screenshots
- **HLE API implementation** — some system API calls are not yet fully implemented
- **ARM CPU emulation** — improve instruction accuracy for edge cases
- **Platform ports** — macOS, Linux, Android, and iOS testing and packaging
- **Documentation** — improve docs and code comments
- **Bug reports** — if you find a game that doesn't work correctly, please open an issue
- **RetroArch integration** — compatibility testing across frontends

## Getting Started

Check the [open issues](https://github.com/AloysHF/SPMP8000Emu/issues) for
tasks labeled `good first issue` or `help wanted`. If you have questions, feel
free to open a discussion issue.

To understand the SPMP8000 game file format (NGame1.0), see
[Game File Formats](Game-File-Formats.md).
