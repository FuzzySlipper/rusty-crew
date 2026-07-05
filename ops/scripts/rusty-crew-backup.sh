#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: rusty-crew-backup.sh --root PATH --backend postgres|sqlite [options]

Options:
  --root PATH             Runtime root, e.g. /home/system/rusty-crew
  --backend BACKEND       postgres or sqlite
  --service-env PATH      Service env file. Defaults to ROOT/config/service.env
  --database-env PATH     Optional database secret env file for Postgres
  --output-dir PATH       Backup directory. Defaults to RUSTY_CREW_BACKUP_DIR or ROOT/backups
  --sqlite-db PATH        SQLite db path. Defaults to ROOT/data/engine/coordination.sqlite3
  --postgres-schema NAME  Schema label for filename. Defaults to RUSTY_CREW_POSTGRES_SCHEMA or rusty_crew
  --help                  Show this help
EOF
}

root=""
backend=""
service_env=""
database_env=""
output_dir=""
sqlite_db=""
postgres_schema=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      root="${2:?--root requires a path}"
      shift 2
      ;;
    --backend)
      backend="${2:?--backend requires postgres or sqlite}"
      shift 2
      ;;
    --service-env)
      service_env="${2:?--service-env requires a path}"
      shift 2
      ;;
    --database-env)
      database_env="${2:?--database-env requires a path}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:?--output-dir requires a path}"
      shift 2
      ;;
    --sqlite-db)
      sqlite_db="${2:?--sqlite-db requires a path}"
      shift 2
      ;;
    --postgres-schema)
      postgres_schema="${2:?--postgres-schema requires a name}"
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$root" || -z "$backend" ]]; then
  usage >&2
  exit 2
fi

service_env="${service_env:-$root/config/service.env}"
if [[ -f "$service_env" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$service_env"
  set +a
fi
if [[ -n "$database_env" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$database_env"
  set +a
fi

output_dir="${output_dir:-${RUSTY_CREW_BACKUP_DIR:-$root/backups}}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$output_dir"

case "$backend" in
  postgres)
    database_url_var="${RUSTY_CREW_POSTGRES_DATABASE_URL_ENV:-RUSTY_CREW_DATABASE_URL}"
    database_url="${!database_url_var:-}"
    if [[ -z "$database_url" ]]; then
      echo "missing Postgres database URL env $database_url_var" >&2
      exit 1
    fi
    postgres_schema="${postgres_schema:-${RUSTY_CREW_POSTGRES_SCHEMA:-rusty_crew}}"
    output="$output_dir/rusty-crew-postgres-${postgres_schema}-${timestamp}.dump"
    pg_dump --format=custom --file="$output" "$database_url"
    sha256sum "$output" > "$output.sha256"
    ;;
  sqlite)
    sqlite_db="${sqlite_db:-${RUSTY_CREW_ENGINE_DATA_DIR:-$root/data/engine}/coordination.sqlite3}"
    if [[ ! -f "$sqlite_db" ]]; then
      echo "missing SQLite database $sqlite_db" >&2
      exit 1
    fi
    output="$output_dir/rusty-crew-sqlite-${timestamp}.sqlite3"
    sqlite3 "$sqlite_db" ".backup '$output'"
    sha256sum "$output" > "$output.sha256"
    ;;
  *)
    echo "unsupported backend: $backend" >&2
    usage >&2
    exit 2
    ;;
esac

cat <<EOF
{
  "ok": true,
  "backend": "$backend",
  "root": "$root",
  "output": "$output",
  "sha256": "$output.sha256"
}
EOF
