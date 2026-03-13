verify:
  cargo fmt
  cargo clippy --all-targets --all-features
  cargo test --all-targets --all-features

healthcheck db_mode="memory":
  db_mode="{{db_mode}}"; \
  if [ "$db_mode" = "file" ]; then \
    mkdir -p tmp; \
    db_url="sqlite://tmp/hoopline.db"; \
  else \
    db_url="sqlite::memory:"; \
  fi; \
  echo "healthcheck using $db_mode db ($db_url)"; \
  DATABASE_URL="$db_url" cargo run >/tmp/hoopline-healthcheck.log 2>&1 & pid=$!; \
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
