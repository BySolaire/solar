# Solar

Solar is a native app for agentic development: conversations, projects and agents (Claude Code, Codex, Grok and others) in one Rust + GPUI window, with a companion mobile app.

Solar is derived from [MonoCode](https://github.com/hardbeat920/monocode) (MIT). See [NOTICE](NOTICE) and [LICENSE](LICENSE).

## Build and run

Requires Node.js 22+, stable Rust and, on macOS, Xcode with macOS components. Install and sign in to at least one agent CLI.

```sh
npm run native:dev     # launch with a separate development database
npm run native:build   # optimized app
npm run native:check   # format, lint and tests
```

Development builds keep their data in `Solar Development` (or `SOLAR_NATIVE_DATA_DIR`). Release builds use `com.bysolaire.solar` in the platform's application data folder. Neither touches an installed MonoCode.

The native app lives in `crates/solar` (UI) and `crates/solar-core` (core), the mobile app in `mobile/` and `crates/mobile-link`. See [docs/native](docs/native/README.md).

## License

MIT. See [LICENSE](LICENSE).
