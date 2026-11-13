# Isolated browser scenarios

See [local testing](../../docs/testing.md) for commands and test policy.

Each test uses a new temporary SQLite file, an ephemeral port, and its own app
process. The app applies the real migrations first. The fixture then stops the
app and seeds that private database with Python's standard `sqlite3` module.
No test endpoint or shared database reset is used. Rate limits stay enabled.
The first process uses `PORT=0`. The fixture reads the assigned port from
`HOOPLINE_LISTENING` in its stdout. Restarts reuse that port and verify the
listener announcement before a health check.

The fixture sets `HOOPLINE_TEST_NOW=2030-01-07T12:00:00Z`. Its default `legacy`
seed keeps the migration users and creates three empty future games. It
disables generated schedules. Use `test.use({ seedProfile: 'last-spot' })` for
one future game with 14 of 15 players. The `release` profile uses one active
Monday schedule instead. Set the initial clock with `test.use({ now: '...' })`.
`app.restart()` keeps the same database, port, and clock.
`app.restart('2030-01-08T00:00:00Z')` changes the server clock.

The exported `test` also sets the normal Playwright `baseURL` fixture. Existing
tests can import `test` and `expect` here and keep using `page` and `request`.
Use `app.query(sql, parameters)` for read-only database checks.
Use `app.seed(sql, parameters)` for one parameterized SQL statement. It stops
the app, commits the statement with foreign keys enabled, and restarts the app.
This permits prerequisite setup without rate-limited user selection requests.

`app.newActor(name)` opens a separate browser context and selects a seeded name
through the UI. Use `{ admin: true }` as its second argument for an admin actor.
The seeded names are `Scenario Alice`, `Scenario Bob`, and `Scenario Admin`.
The admin password is `admin`.

Do not replace `target/debug/hoopline` with another build while tests run.
