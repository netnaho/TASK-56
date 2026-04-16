#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# Entrypoint for Scholarly Backend
# ============================================================================
# Waits for MySQL, runs migrations and seeds, then starts the API server.
# ============================================================================

# ---------------------------------------------------------------------------
# Parse DATABASE_URL into components
# Format: mysql://user:password@host:port/database
# ---------------------------------------------------------------------------
parse_database_url() {
    local url="${DATABASE_URL:?DATABASE_URL environment variable is required}"

    # Strip the protocol prefix
    local without_proto="${url#mysql://}"

    # Extract user:password
    local userpass="${without_proto%%@*}"
    DB_USER="${userpass%%:*}"
    DB_PASS="${userpass#*:}"

    # Extract host:port/database
    local hostportdb="${without_proto#*@}"
    local hostport="${hostportdb%%/*}"
    DB_HOST="${hostport%%:*}"
    DB_PORT="${hostport#*:}"
    DB_NAME="${hostportdb#*/}"

    echo "Database config: host=${DB_HOST} port=${DB_PORT} user=${DB_USER} db=${DB_NAME}"
}

parse_database_url

# ---------------------------------------------------------------------------
# Wait for MySQL to be ready
# ---------------------------------------------------------------------------
echo "Waiting for MySQL at ${DB_HOST}:${DB_PORT}..."

MAX_RETRIES=60
RETRY_COUNT=0

until mysql -h "${DB_HOST}" -P "${DB_PORT}" -u "${DB_USER}" -p"${DB_PASS}" -e "SELECT 1" "${DB_NAME}" > /dev/null 2>&1; do
    RETRY_COUNT=$((RETRY_COUNT + 1))
    if [ "${RETRY_COUNT}" -ge "${MAX_RETRIES}" ]; then
        echo "ERROR: MySQL not available after ${MAX_RETRIES} attempts. Exiting."
        exit 1
    fi
    echo "  MySQL not ready yet (attempt ${RETRY_COUNT}/${MAX_RETRIES}). Retrying in 2s..."
    sleep 2
done

echo "MySQL is ready."

# ---------------------------------------------------------------------------
# Run migration SQL files in order
# ---------------------------------------------------------------------------
echo "Running migrations..."

for migration in /app/migrations/*.sql; do
    if [ -f "${migration}" ]; then
        echo "  Applying migration: $(basename "${migration}")"
        mysql -h "${DB_HOST}" -P "${DB_PORT}" -u "${DB_USER}" -p"${DB_PASS}" "${DB_NAME}" < "${migration}" || {
            # Migrations use CREATE TABLE which will fail if already applied;
            # log and continue so the entrypoint is re-entrant.
            echo "  Warning: migration $(basename "${migration}") returned non-zero (may already be applied)."
        }
    fi
done

echo "Migrations complete."

# ---------------------------------------------------------------------------
# Run seed SQL files in order
# ---------------------------------------------------------------------------
echo "Running seeds..."

for seed in /app/seeds/*.sql; do
    if [ -f "${seed}" ]; then
        echo "  Applying seed: $(basename "${seed}")"
        mysql -h "${DB_HOST}" -P "${DB_PORT}" -u "${DB_USER}" -p"${DB_PASS}" "${DB_NAME}" < "${seed}" || {
            echo "  Warning: seed $(basename "${seed}") returned non-zero."
        }
    fi
done

echo "Seeds complete."

# ---------------------------------------------------------------------------
# Ensure Rocket's temp directory exists (needed for TempFile multipart uploads)
# ---------------------------------------------------------------------------
if [ -n "${ROCKET_TEMP_DIR:-}" ]; then
    mkdir -p "${ROCKET_TEMP_DIR}"
fi

# ---------------------------------------------------------------------------
# Start the backend server
# ---------------------------------------------------------------------------
echo "Starting Scholarly backend..."
exec /app/scholarly-backend
