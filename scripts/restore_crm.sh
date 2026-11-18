#!/usr/bin/env bash
set -e

# ==============================================================================
# Script Migrasi / Restore Tabel CRM dari Python teknusa ke rust_teknusa
# ==============================================================================

SOURCE_CONTAINER="${SOURCE_CONTAINER:-teknusa_mariadb}"
TARGET_CONTAINER="${TARGET_CONTAINER:-teknusa_rust_mysql}"
DB_NAME="${DB_NAME:-teknusas_teknusa}"
DB_USER="${DB_USER:-teknusas_teknusa}"
DB_PASS="${DB_PASS:-@Pontianak123}"

WITH_DEPS=false
if [[ "$1" == "--with-deps" ]]; then
    WITH_DEPS=true
fi

echo "========================================================"
echo " CRM Table Restore: teknusa -> rust_teknusa"
echo " Source Container: $SOURCE_CONTAINER"
echo " Target Container: $TARGET_CONTAINER"
echo " Database        : $DB_NAME"
echo "========================================================"

# Pastikan container berjalan
if ! docker ps --format '{{.Names}}' | grep -q "^${SOURCE_CONTAINER}$"; then
    echo "❌ Error: Container sumber '$SOURCE_CONTAINER' tidak sedang berjalan!"
    echo "   Jalankan: docker compose -f ../teknusa/compose.yaml up -d db"
    exit 1
fi

if ! docker ps --format '{{.Names}}' | grep -q "^${TARGET_CONTAINER}$"; then
    echo "❌ Error: Container tujuan '$TARGET_CONTAINER' tidak sedang berjalan!"
    echo "   Jalankan: docker compose -f ./compose.yaml up -d db"
    exit 1
fi

# Tentukan tabel yang akan diexport
if [ "$WITH_DEPS" = true ]; then
    TABLES="auth_user entity_entity leads_lead leads_interaction"
    echo "📦 Mode: Menyertakan tabel dependensi relasi ($TABLES)"
else
    TABLES="leads_lead leads_interaction"
    echo "📦 Mode: Hanya tabel CRM ($TABLES)"
fi

BACKUP_FILE="/tmp/teknusa_crm_migration_$(date +%Y%m%d_%H%M%S).sql"

echo "📥 1. Melakukan dump data dari '$SOURCE_CONTAINER'..."
docker exec "$SOURCE_CONTAINER" mysqldump \
    -u "$DB_USER" \
    -p"$DB_PASS" \
    "$DB_NAME" \
    $TABLES \
    --single-transaction \
    --quick \
    --skip-add-drop-table \
    --replace \
    --no-create-info > "$BACKUP_FILE"

# Tambahkan pembungkus disable foreign key checks agar import aman
TMP_IMPORT_FILE="/tmp/teknusa_crm_import_ready.sql"
echo "SET FOREIGN_KEY_CHECKS=0;" > "$TMP_IMPORT_FILE"
cat "$BACKUP_FILE" >> "$TMP_IMPORT_FILE"
echo "SET FOREIGN_KEY_CHECKS=1;" >> "$TMP_IMPORT_FILE"

echo "📤 2. Merestore data ke '$TARGET_CONTAINER'..."
docker exec -i "$TARGET_CONTAINER" mysql \
    -u "$DB_USER" \
    -p"$DB_PASS" \
    "$DB_NAME" < "$TMP_IMPORT_FILE"

echo "🔍 3. Verifikasi jumlah data di target database:"
for tbl in $TABLES; do
    COUNT=$(docker exec -i "$TARGET_CONTAINER" mysql -u "$DB_USER" -p"$DB_PASS" -N -e "SELECT COUNT(*) FROM \`$tbl\`;" "$DB_NAME" 2>/dev/null || echo "0")
    echo "   - $tbl: $COUNT records"
done

# Bersihkan file sementara
rm -f "$BACKUP_FILE" "$TMP_IMPORT_FILE"

echo "========================================================"
echo "✅ Restore tabel CRM selesai dengan sukses!"
echo "========================================================"

