# Copilot Instructions for Hoopline

## Commands to run
- Check Justfile first to find useful commands, specific to this repository. Always prefer those!

## Build, test, and run

- Build: `cargo build`
- Run locally: `cargo run` (serves on `0.0.0.0:3000`)
- Run all tests: `cargo test`
- Run a single integration test: `cargo test --test integration get_root_returns_ok_and_body`
- Optional local dev loop: `bacon run` (configured in `bacon.toml` to watch `src/` and `templates/`)

## High-level architecture

- `src/main.rs` starts the HTTP server and serves the Axum router.
- `src/lib.rs` is the web app entrypoint (routes + page rendering).
- `src/models.rs` holds current domain data structures and sample in-memory data.
- `src/error.rs` centralizes HTTP error responses for web handlers.
- `templates/` contains Askama HTML templates:
  - `base.html` layout shell
  - `error.html` fallback error page
  - ...
- `tests/integration.rs` validates the rendered HTML end-to-end against the router.

## Rust Conventions

- **Clippy is law.** We run pedantic clippy with `correctness` and `suspicious` as deny. Fix all warnings.
- **Error handling:** Use `anyhow` for application-level errors (binaries, handlers). Use `thiserror` for library errors in `crates/common/` that need to be matched on.
- **Use strong types.** Avoid primitive obsession - prefer newtype wrappers (e.g., `ContentId(String)`) over raw `String`/`i64` when the domain warrants it.
- **Leverage the type system.** Catch errors at compile time, not runtime. Use enums for states, `Option` for optional values, `Result` for fallible operations.
- **Keep it simple.** Avoid unnecessary traits, generics, or abstraction layers. 
- **Clone is fine.** This is a web service. Don't fight the borrow checker with complex lifetimes when a `.clone()` is clearer and has negligible performance impact.

_From [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/):_

- **Doc comments:** First sentence should be < 15 words and appear in module summaries. Include `# Errors` and `# Panics` sections when applicable.
- **Cascaded initialization:** Types requiring 4+ parameters should group them into semantic helper structs.
- **Simple abstractions:** Avoid visibly nested generics in service types (e.g., `Service<Backend<Store>>`). Keep API surfaces simple.
- **Naming:** Avoid weasel words (`Service`, `Manager`, `Factory`) that don't add meaning. `Bookings` > `BookingService`.
- **Conversions:** Follow `as_`/`to_`/`into_` conventions. Getters don't use `get_` prefix.
- **Common traits:** Implement `Clone`, `Debug`, `Default`, `PartialEq` where sensible. Have `Foo::new()` even if `Default` exists.

