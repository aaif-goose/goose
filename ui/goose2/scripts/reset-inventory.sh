#!/usr/bin/env bash
# Reset the provider inventory tables to empty, as if migration 12 just ran.
# This lets you test the first-use experience (cold inventory).
#
# Usage: ./scripts/reset-inventory.sh

set -euo pipefail

DB="${GOOSE_DB:-$HOME/.local/share/goose/sessions/sessions.db}"

if [ ! -f "$DB" ]; then
  echo "Database not found at $DB"
  echo "Set GOOSE_DB to override the path."
  exit 1
fi

echo "Database: $DB"
echo ""
echo "Before:"
echo "  provider_inventory_entries: $(sqlite3 "$DB" 'SELECT COUNT(*) FROM provider_inventory_entries;')"
echo "  provider_inventory_models:  $(sqlite3 "$DB" 'SELECT COUNT(*) FROM provider_inventory_models;')"

# ON DELETE CASCADE on provider_inventory_models means deleting entries clears both tables.
# Delete models first since CASCADE isn't reliable in all sqlite3 builds,
# then delete entries.
sqlite3 "$DB" "DELETE FROM provider_inventory_models; DELETE FROM provider_inventory_entries;"

echo ""
echo "After:"
echo "  provider_inventory_entries: $(sqlite3 "$DB" 'SELECT COUNT(*) FROM provider_inventory_entries;')"
echo "  provider_inventory_models:  $(sqlite3 "$DB" 'SELECT COUNT(*) FROM provider_inventory_models;')"
echo ""
echo "Inventory tables are empty. Restart goose to test first-use flow."
