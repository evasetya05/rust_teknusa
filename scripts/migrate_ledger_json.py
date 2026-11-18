#!/usr/bin/env python3
"""
Migrasi Presisi Entity & Ledger (MariaDB -> PostgreSQL)
Membaca data dalam format JSON dari MariaDB, menyesuaikan tipe data boolean dan kutipan string PostgreSQL,
lalu memasukkannya dengan aman.
"""

import json
import re
import subprocess
import sys

MARIADB_CONTAINER = "teknusa_mariadb"
MARIADB_USER = "teknusas_teknusa"
MARIADB_PASS = "@Pontianak123"
MARIADB_DB = "teknusas_teknusa"

POSTGRES_CONTAINER = "teknusa_rust_postgres"
POSTGRES_USER = "teknusas_teknusa"
POSTGRES_DB = "teknusas_teknusa"

BOOL_COLUMNS = {"is_active", "active", "locked", "is_closed", "is_posted"}

TARGET_TABLES = [
    "entity_entity",
    "ledger_account",
    "ledger_closingperiod",
    "ledger_profitcenter",
    "ledger_costcenter",
    "ledger_project",
    "ledger_activity",
    "ledger_location",
    "ledger_partner",
    "ledger_product",
    "ledger_journalentry",
    "ledger_journalitem",
]

def sql_quote(val, is_bool=False):
    if val is None:
        return "NULL"
    if is_bool:
        return "TRUE" if str(val).lower() in ("1", "true", "t") else "FALSE"
    if isinstance(val, (int, float)):
        return str(val)
    # Double single-quotes for safe PostgreSQL strings
    s = str(val).replace("'", "''")
    return f"'{s}'"

def run_psql(sql):
    proc = subprocess.run(
        ["docker", "exec", "-i", POSTGRES_CONTAINER, "psql", "-U", POSTGRES_USER, "-d", POSTGRES_DB],
        input=sql,
        text=True,
        capture_output=True,
    )
    if proc.stderr:
        print("[PSQL Stderr]:", proc.stderr, file=sys.stderr)
    return proc.stdout

def main():
    print("=" * 60)
    print(" Migrasi Entity & Ledger (MariaDB -> PostgreSQL)")
    print("=" * 60)

    # 1. Pastikan skema tabel di PostgreSQL sudah memiliki kolom & tipe data yang benar
    print("1. Menyiapkan / memvalidasi schema di PostgreSQL...")
    schema_sql = """
    SET standard_conforming_strings = off;
    SET session_replication_role = 'replica';

    CREATE TABLE IF NOT EXISTS entity_entity (
        id VARCHAR(64) PRIMARY KEY,
        name VARCHAR(200) NOT NULL,
        code VARCHAR(50) NOT NULL,
        description TEXT DEFAULT '',
        email VARCHAR(254) DEFAULT '',
        phone VARCHAR(50) DEFAULT '',
        logo VARCHAR(100),
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        parent_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_account (
        id BIGSERIAL PRIMARY KEY,
        account_name VARCHAR(100) NOT NULL,
        account_type VARCHAR(50) NOT NULL,
        active BOOLEAN NOT NULL DEFAULT TRUE,
        balance_type VARCHAR(10) NOT NULL,
        coa VARCHAR(100) NOT NULL DEFAULT 'Default CoA',
        coa_role_default VARCHAR(100),
        locked BOOLEAN NOT NULL DEFAULT FALSE,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_closingperiod (
        id BIGSERIAL PRIMARY KEY,
        period VARCHAR(7) NOT NULL,
        is_closed BOOLEAN NOT NULL DEFAULT FALSE,
        closed_at TIMESTAMP WITHOUT TIME ZONE,
        closed_by VARCHAR(100),
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_profitcenter (
        id BIGSERIAL PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_costcenter (
        id BIGSERIAL PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64),
        profit_center_id BIGINT
    );

    CREATE TABLE IF NOT EXISTS ledger_project (
        id BIGSERIAL PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_activity (
        id BIGSERIAL PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64),
        project_id BIGINT
    );

    CREATE TABLE IF NOT EXISTS ledger_location (
        id BIGSERIAL PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        address TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_partner (
        id BIGSERIAL PRIMARY KEY,
        partner_type VARCHAR(20) NOT NULL DEFAULT 'customer',
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        email VARCHAR(254),
        phone VARCHAR(50),
        address TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_product (
        id BIGSERIAL PRIMARY KEY,
        product_type VARCHAR(20) NOT NULL DEFAULT 'goods',
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_journalentry (
        id BIGSERIAL PRIMARY KEY,
        date DATE NOT NULL,
        description TEXT NOT NULL,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        is_posted BOOLEAN NOT NULL DEFAULT FALSE,
        period VARCHAR(7),
        entity_id VARCHAR(64)
    );

    CREATE TABLE IF NOT EXISTS ledger_journalitem (
        id BIGSERIAL PRIMARY KEY,
        debit INTEGER NOT NULL DEFAULT 0,
        credit INTEGER NOT NULL DEFAULT 0,
        account_id BIGINT NOT NULL,
        journal_entry_id BIGINT NOT NULL,
        note VARCHAR(255),
        activity_id BIGINT,
        cost_center_id BIGINT,
        location_id BIGINT,
        partner_id BIGINT,
        product_id BIGINT,
        profit_center_id BIGINT,
        project_id BIGINT
    );
    """
    run_psql(schema_sql)

    # 2. Ambil data per tabel dari MariaDB & Generate INSERT untuk PostgreSQL
    print("2. Mentransfer data per tabel dari MariaDB ke PostgreSQL...")
    total_migrated = 0

    for tbl in TARGET_TABLES:
        # Cek apakah tabel ada di MariaDB
        chk = subprocess.run(
            ["docker", "exec", MARIADB_CONTAINER, "mariadb", "-u", MARIADB_USER, f"-p{MARIADB_PASS}", "-N", "-e",
             f"SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='{MARIADB_DB}' AND table_name='{tbl}';"],
            capture_output=True, text=True
        )
        if chk.stdout.strip() != "1":
            print(f"   -> [SKIP] Tabel `{tbl}` tidak ditemukan di MariaDB.")
            continue

        # Ambil data JSON dari MariaDB
        fetch = subprocess.run(
            ["docker", "exec", MARIADB_CONTAINER, "mariadb", "-u", MARIADB_USER, f"-p{MARIADB_PASS}", "--json", "-e",
             f"SELECT * FROM `{tbl}`;", MARIADB_DB],
            capture_output=True, text=True
        )

        raw = fetch.stdout.strip()
        if not raw:
            print(f"   -> {tbl}: 0 rows (kosong)")
            continue

        # Cari JSON array
        m = re.search(r"\[\s*\{.*\}\s*\]", raw, re.DOTALL)
        if not m:
            print(f"   -> {tbl}: 0 rows (tidak ada baris)")
            continue

        try:
            rows = json.loads(m.group(0))
        except Exception as e:
            print(f"   -> ⚠️ Gagal decode JSON untuk {tbl}: {e}")
            continue

        if not rows:
            print(f"   -> {tbl}: 0 rows")
            continue

        # Bersihkan tabel target di PostgreSQL
        run_psql(f"TRUNCATE TABLE {tbl} CASCADE;")

        # Susun perintah INSERT
        statements = ["SET standard_conforming_strings = off;\nSET session_replication_role = 'replica';\n"]
        for row in rows:
            cols = list(row.keys())
            vals = [sql_quote(row[c], is_bool=(c in BOOL_COLUMNS)) for c in cols]
            cols_str = ", ".join(cols)
            vals_str = ", ".join(vals)
            statements.append(f"INSERT INTO {tbl} ({cols_str}) VALUES ({vals_str});\n")

        # Reset sequence jika ada id serial
        if tbl != "entity_entity":
            statements.append(f"SELECT setval('{tbl}_id_seq', COALESCE((SELECT MAX(id) FROM {tbl}), 1));\n")

        statements.append("SET session_replication_role = 'origin';\n")
        full_sql = "".join(statements)
        run_psql(full_sql)

        print(f"   -> ✅ {tbl}: {len(rows)} baris berhasil dimigrasikan.")
        total_migrated += len(rows)

    print(f"\n3. Total data ditransfer: {total_migrated} baris.")
    print("4. Verifikasi Data di PostgreSQL:")
    check_sql = "\nUNION ALL\n".join([f"SELECT '{t}' AS tabel, COUNT(*) FROM {t}" for t in TARGET_TABLES]) + ";"
    print(run_psql(check_sql))
    print("✅ Selesai!")

if __name__ == "__main__":
    main()

