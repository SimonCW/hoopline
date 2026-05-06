#!/bin/sh
set -eu

DB_PATH="${1:-/data/hoopline.db}"
BACKUP_DIR="${2:-/data/backups}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"

if [ ! -f "$DB_PATH" ]; then
    echo "error: database file not found: $DB_PATH" >&2
    exit 1
fi

mkdir -p "$BACKUP_DIR"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_path="$BACKUP_DIR/hoopline-$timestamp.db"

if command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 "$DB_PATH" ".backup '$backup_path'"
else
    cp "$DB_PATH" "$backup_path"
fi

find "$BACKUP_DIR" -type f -name "hoopline-*.db" -mtime +"$RETENTION_DAYS" -delete
echo "backup written: $backup_path"
