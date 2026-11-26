# Agent Instructions for Hoopline

## Communication

- Use ASD-STE100 Simplified Technical English. Use short sentences and active voice.
- State the result first. Report which checks passed, failed, or did not run.

## Git worktrees

- Before editing files for a new task, create a separate Git worktree with a new branch. Work in that worktree.
- If the session already uses a worktree for that task, reuse it. Skip worktree setup for read-only requests or when the user specifies a checkout.

## Run and test

- Start with `Justfile`. Use `just preview` for sample data in a disposable database. Never seed a database that contains real bookings.
- Run the smallest relevant checks first. Use `just verify` for Rust, `just test-ui` for browser UI, and `just test-e2e` for full user journeys. Run `just test-all` before merging app changes. See [local testing](docs/testing.md) for setup and test layers.
- Use the isolated scenario fixtures for browser journeys. For a redirect or an HTMX update, wait for the expected page or fragment before the next action. Do not use a fixed sleep.
- Use file-backed race tests and the test clock for booking and time-boundary changes. Check the saved roster, not only the page message.
- Browser emulation does not verify push delivery on a real device. Report that check separately.

## App changes

- Start in `src/lib.rs` for Axum routes, `src/db.rs` for SQLite booking changes, and `templates/` for Askama views.
- Update both English and German copy in `src/i18n.rs`, including accessible names and status messages. Keep normal form submission working when JavaScript does not load.
- Check `origin/main` and relevant open PRs for SQLx migration number conflicts. Do not change an applied migration.

## Pull requests and releases

- Opening a PR does not authorize a merge or a deploy. Before an authorized merge, wait for GitHub's **Release gate** check on the final PR commit. It requires **Verify** and **Container build** to pass.
- After an authorized Hostim deploy, check the running commit and `/healthz`. A healthy response alone can come from the previous release.

## Rust

- Fix Clippy warnings. Use domain types and enums. Avoid unnecessary traits and generics. Clone when it is simpler than borrowing.
- Follow the [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/). Keep doc summaries short. Document errors and panics when applicable.
