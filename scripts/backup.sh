#!/usr/bin/env bash
# Pale Server PostgreSQL Backup Script
#
# Usage:
#   ./scripts/backup.sh
#
# Environment variables:
#   PGHOST          PostgreSQL host (default: localhost)
#   PGPORT          PostgreSQL port (default: 5432)
#   PGUSER          PostgreSQL user (default: pale)
#   PGPASSWORD      PostgreSQL password
#   PGDATABASE      PostgreSQL database (default: pale)
#   BACKUP_DIR      Backup directory (default: ./backups)
#   RETENTION_DAYS  Number of days to keep backups (default: 30)
#
# Examples:
#   # Local backup
#   PGPASSWORD=mypassword ./scripts/backup.sh
#
#   # Docker Compose
#   docker compose exec postgres pg_dump -U pale pale | gzip > backup.sql.gz

set -euo pipefail

PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-pale}"
PGDATABASE="${PGDATABASE:-pale}"
BACKUP_DIR="${BACKUP_DIR:-./backups}"
RETENTION_DAYS="${RETENTION_DAYS:-30}"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/pale_${TIMESTAMP}.sql.gz"

echo "=== Pale Server Backup ==="
echo "Host: ${PGHOST}:${PGPORT}"
echo "Database: ${PGDATABASE}"
echo "Backup: ${BACKUP_FILE}"
echo ""

# Create backup directory
mkdir -p "${BACKUP_DIR}"

# Perform backup
echo "Starting backup..."
pg_dump \
    -h "${PGHOST}" \
    -p "${PGPORT}" \
    -U "${PGUSER}" \
    -d "${PGDATABASE}" \
    --format=plain \
    --no-owner \
    --no-privileges \
    --verbose \
    2>/dev/null \
    | gzip > "${BACKUP_FILE}"

BACKUP_SIZE=$(du -h "${BACKUP_FILE}" | cut -f1)
echo "Backup complete: ${BACKUP_FILE} (${BACKUP_SIZE})"

# Cleanup old backups
echo ""
echo "Cleaning backups older than ${RETENTION_DAYS} days..."
DELETED=$(find "${BACKUP_DIR}" -name "pale_*.sql.gz" -mtime "+${RETENTION_DAYS}" -delete -print | wc -l)
echo "Deleted ${DELETED} old backup(s)"

# List remaining backups
echo ""
echo "Current backups:"
ls -lh "${BACKUP_DIR}"/pale_*.sql.gz 2>/dev/null || echo "  (none)"

echo ""
echo "=== Backup Complete ==="
