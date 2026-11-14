# Local scenario tests

## Test policy

Test user outcomes through real components. Use the browser for complete user
journeys. Use HTTP tests for concurrent requests. Use generated database tests
for larger action sequences. Keep small tests for validation and time math.

Assert the saved roster, not only a success message. Check both users after a
race. Do not assume which simultaneous request wins. Check FIFO promotion,
capacity, unique members, and contiguous positions.

## Run

After installing the prerequisites listed in the root README, run:

```sh
just test-all
```

Use these commands for a shorter development loop:

| Command | Checks |
| --- | --- |
| `just verify` | Formatting, Clippy, all Rust tests, including races and the model |
| `just test-races` | Real HTTP races and generated roster sequences only |
| `just test-production-clock` | Normal builds ignore the test-only clock |
| `just test-e2e` | Fixture parser, two-user journey, and exact clock boundaries |
| `just test-ui` | Member and admin browser interactions, layout, and accessibility |

Without Just, run the commands from the matching recipe in `Justfile`.
Do not run build commands concurrently with browser tests.

After installation, these checks run locally without cloud services. HTMX is
pinned and served by the app. Scenario browsers reject external requests.

`just test-e2e` builds with the `scenario-tests` feature. This feature permits
`HOOPLINE_TEST_NOW` to set an RFC3339 application time. Production builds omit
the feature and ignore that variable. Use a normal `cargo build --release` for
deployment. The fixed time covers game visibility, signup closure, schedule
release, activity timestamps, admin lockout, and session expiry. Rate-limit
windows use monotonic time and stay enabled. Do not wait for real release or
expiry times in tests.

Each browser test, including the layout and admin suites, owns a temporary
SQLite file and an app process.
Migrations and the five-connection file database pool are the production ones.
Fixture setup inserts prerequisites directly. User actions use real routes.
No reset or seed endpoint is exposed by the application.

Playwright retains traces on failure. Scenario fixtures attach server logs.
Use the trace viewer and logs to diagnose a failed scenario. Do not hide failures
with automatic retries.

`just test-e2e` also checks the fixture listener parser with Node's test runner.
This check splits process output inside a port number. It prevents partial
stdout chunks from being treated as a complete listener announcement.
`just test-production-clock` proves that a normal build ignores the clock
override.

`tests/scenarios/pwa.spec.js` allows service workers for its isolated browser
contexts. It checks local worker registration and the manifest. It also stubs
push APIs to check signup preparation, restored pages, the shared request lock,
and identity refresh. The other browser scenarios continue to block service
workers. Rust tests exercise enrollment, the outbox, leases, retries, ownership,
and encrypted message construction without contacting push providers.

These tests do not prove operating-system notification delivery. Before relying
on alerts, test an installed iPhone web app, Android Chrome, and a desktop
browser on HTTPS with consenting testers. Check permission changes, a test alert,
member-triggered promotion, notification tap, and logout. Do not send real
alerts to members without separate approval.

## Layers

- `tests/scenarios/`: isolated browser journeys, request gates, process restarts,
  and controlled application time.
- `tests/races.rs`: real TCP requests, file-backed SQLite, and concurrent writes.
- `tests/model.rs`: generated action sequences against an independent roster
  model. Proptest shrinks failures and saves regression seeds. Commit new
  regression seed files with the fix.
- Existing Rust tests: route rendering, validation, transactions, and time math.
- Existing browser tests: detailed layout, accessibility, and interaction checks.

`just test-all` runs all these layers. Browser tests use mobile and desktop
Chromium. The mobile project emulates a phone; it is not a physical-device test.
The complete suite runs the Rust tests once. Some browser scenarios replace
older single-request Rust tests. Run `just test-all`, not only `cargo test`,
before a merge.
For more concurrency schedules, run `just test-races` repeatedly. For more
generated action sequences, use `PROPTEST_CASES=500 just test-races`.

## CI and release builds

`.github/workflows/verify.yml` runs `just test-all` on pull requests and pushes
to `main`. It installs Chromium with Linux dependencies and retains browser
failure artifacts and generated regression seeds for seven days.
The container-build job requires successful verification. It builds the normal
Docker image without the test-clock feature. It does not publish or deploy it.

Rust dependencies, Cargo's build cache, and the installed Just binary are
cached with `Swatinem/rust-cache`. Its keys include the toolchain and dependency
configuration. npm's download cache remains enabled through `setup-node`.
Docker Buildx caches intermediate layers in GitHub Actions, including
`cargo-chef` dependency builds. The first run fills each cache; later runs can
reuse it. GitHub isolates pull-request caches from the default branch.
All tests still run on every verification. Chromium and its Linux dependencies
are installed normally; caching them adds little value for this small suite.

Hostim deployment is configured outside this repository. If it deploys directly
from Git, use a CI gate if the provider supports it. Require the `Verify`
check before deployment. Otherwise, disable independent auto-deploy and deploy
only a verified commit. These external settings are not changed by this PR.
A GitHub workflow cannot gate an independent Hostim auto-deploy by itself.
Any future deployment job in this workflow must depend on `verify`.

## Limits

Request gates increase overlap. They do not control SQLite thread scheduling
or prove every interleaving. Generated sequential model tests do not replace
race tests. Re-run race tests for more schedules. A fixed input seed does not
make concurrent execution deterministic.

Keep hosted HTTPS, proxy headers, volume permissions, and real hardware failures
as separate deployment checks. Local tests cannot prove those properties.
