verify:
  cargo fmt --check
  cargo clippy --locked --all-targets --all-features
  cargo test --locked --all-targets --all-features

# Run `npm ci` and `npx playwright install chromium` before the first run.
test-ui:
    cargo build --locked --features scenario-tests
    npm run test:ui

# Real browser journeys. Each test owns its server and file-backed database.
test-e2e:
    cargo build --locked --features scenario-tests
    npm run test:fixtures
    npm run test:e2e

# Real HTTP concurrency and generated roster-model tests.
test-races:
    cargo test --locked --test races --test model

# Prove that normal builds ignore the scenario-only clock environment.
test-production-clock:
    cargo test --locked --no-default-features --test clock_process

# Run the complete local check set after installing browser prerequisites.
test-all: verify test-production-clock test-e2e test-ui

# Stop this checkout's dev server on port 5050, but not other applications.
stop-dev:
  #!/usr/bin/env bash
  set -eu
  pids=$(lsof -nP -tiTCP:5050 -sTCP:LISTEN || true)
  if [ -z "$pids" ]; then
    echo "Port 5050 is free."
    exit 0
  fi
  for pid in $pids; do
    if ! lsof -a -p "$pid" -d txt -Fn | grep -Fxq "n$PWD/target/debug/hoopline"; then
      echo "Port 5050 is used by another process (PID $pid); not stopping it." >&2
      exit 1
    fi
  done
  kill $pids
  for i in {1..50}; do
    if ! lsof -nP -tiTCP:5050 -sTCP:LISTEN >/dev/null; then
      echo "Stopped Hoopline on port 5050."
      exit 0
    fi
    sleep 0.1
  done
  echo "Port 5050 is still in use." >&2
  exit 1

# Start a disposable preview with a full game and three people waiting.
preview: stop-dev
  ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}" DATABASE_URL=sqlite::memory: PREVIEW_SAMPLE=1 PORT=5050 cargo run

# Rebuild and restart the disposable preview when source files change.
dev: stop-dev
  ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}" DATABASE_URL=sqlite::memory: PREVIEW_SAMPLE=1 PORT=5050 bacon run

healthcheck db_mode="memory":
  db_mode="{{db_mode}}"; \
  if [ "$db_mode" = "file" ]; then \
    mkdir -p tmp; \
    db_url="sqlite://tmp/hoopline.db"; \
  else \
    db_url="sqlite::memory:"; \
  fi; \
  echo "healthcheck using $db_mode db ($db_url)"; \
  ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}" DATABASE_URL="$db_url" cargo run >/tmp/hoopline-healthcheck.log 2>&1 & pid=$!; \
  i=0; \
  until curl -fsS http://127.0.0.1:5050/healthz >/dev/null 2>&1; do \
    i=$((i+1)); \
    if [ $i -ge 60 ]; then \
      echo "healthcheck failed; recent server logs:"; \
      tail -n 40 /tmp/hoopline-healthcheck.log; \
      kill $pid; \
      wait $pid 2>/dev/null || true; \
      exit 1; \
    fi; \
    sleep 0.25; \
  done; \
  echo "healthcheck ok"; \
  kill $pid; \
  wait $pid 2>/dev/null || true
