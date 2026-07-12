#!/usr/bin/env bash
# Non-destructive restore drill: restore a dump into a temporary database and
# run a minimal health query. Does not overwrite production by default.
#
# Usage:
#   export PGPASSWORD=...
#   ./scripts/restore-drill.sh path/to/pale_YYYYMMDD.sql.gz
#
# Optional:
#   PGHOST PGPORT PGUSER (defaults: localhost 5432 pale)
#   DRILL_DB=pale_restore_drill (temp database name)
set -euo pipefail

DUMP="${1:?Usage: $0 backup.sql.gz}"
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-pale}"
DRILL_DB="${DRILL_DB:-pale_restore_drill_$(date +%Y%m%d%H%M%S)}"

if [ ! -f "$DUMP" ]; then
  echo "error: dump not found: $DUMP" >&2
  exit 1
fi

echo "=== Pale restore drill ==="
echo "Host: $PGHOST:$PGPORT"
echo "Dump: $DUMP"
echo "Temp DB: $DRILL_DB"
echo

psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d postgres -v ON_ERROR_STOP=1 \
  -c "DROP DATABASE IF EXISTS \"${DRILL_DB}\";" \
  -c "CREATE DATABASE \"${DRILL_DB}\";"

echo "Restoring..."
if [[ "$DUMP" == *.gz ]]; then
  gunzip -c "$DUMP" | psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$DRILL_DB" -v ON_ERROR_STOP=1 -q
else
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$DRILL_DB" -v ON_ERROR_STOP=1 -q -f "$DUMP"
fi

echo "Verifying tables..."
COUNT=$(psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$DRILL_DB" -tAc \
  "SELECT count(*) FROM information_schema.tables WHERE table_schema='public';")
echo "Public tables: $COUNT"

if [ "${COUNT:-0}" -lt 1 ]; then
  echo "FAIL: no tables restored"
  exit 1
fi

echo
echo "PASS: restore drill succeeded into $DRILL_DB"
echo "Drop when finished:"
echo "  psql -h $PGHOST -p $PGPORT -U $PGUSER -d postgres -c 'DROP DATABASE \"${DRILL_DB}\";'"
