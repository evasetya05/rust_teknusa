#!/usr/bin/env python3
"""
Migrasi Lengkap Module Entity & Ledger (termasuk Closing Period & 7 Dimensi)
dari MariaDB (teknusa) ke PostgreSQL (rust_teknusa).
"""

import re
import subprocess
import sys

POSTGRES_CONTAINER = "teknusa_rust_postgres"
POSTGRES_USER = "teknusas_teknusa"
POSTGRES_DB = "teknusas_teknusa"
MARIADB_CONTAINER = "teknusa_mariadb"
MARIADB_USER = "teknusas_teknusa"
MARIADB_PASS = "@Pontianak123"
MARIADB_DB = "teknusas_teknusa"

def run_psql(sql):
    proc = subprocess.run(
        ["docker", "exec", "-i", POSTGRES_CONTAINER, "psql", "-U", POSTGRES_USER, "-d", POSTGRES_DB],
        input=sql,
        text=True,
        capture_output=True
    )
    if proc.stderr:
        print("[PSQL Stderr]:", proc.stderr, file=sys.stderr)
    return proc.stdout

def main():
    print("=" * 60)
    print(" Migrasi Entity & Ledger: MariaDB -> PostgreSQL")
    print("=" * 60)

    # 1. Pastikan Schema Entity dan Ledger dibuat di PostgreSQL
    print("1. Menyiapkan schema tabel Entity & Ledger di PostgreSQL...")
    schema_sql = """
    SET standard_conforming_strings = off;

    -- Entity
    CREATE TABLE IF NOT EXISTS entity_entity (
        id VARCHAR(64) PRIMARY KEY,
        parent_id VARCHAR(64),
        name VARCHAR(200) NOT NULL,
        code VARCHAR(50) NOT NULL,
        description TEXT DEFAULT '',
        email VARCHAR(254) DEFAULT '',
        phone VARCHAR(50) DEFAULT '',
        logo VARCHAR(100),
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    -- Ledger Account (CoA)
    CREATE TABLE IF NOT EXISTS ledger_account (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        account_type VARCHAR(50) NOT NULL,
        account_name VARCHAR(100) NOT NULL,
        coa VARCHAR(100) NOT NULL DEFAULT 'Default CoA',
        balance_type VARCHAR(10) NOT NULL,
        active BOOLEAN NOT NULL DEFAULT TRUE,
        locked BOOLEAN NOT NULL DEFAULT FALSE,
        coa_role_default VARCHAR(100)
    );

    -- Closing Period
    CREATE TABLE IF NOT EXISTS ledger_closingperiod (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        period VARCHAR(7) NOT NULL,
        is_closed BOOLEAN NOT NULL DEFAULT FALSE,
        closed_at TIMESTAMP WITHOUT TIME ZONE,
        closed_by VARCHAR(100)
    );

    -- 7 Dimensions
    CREATE TABLE IF NOT EXISTS ledger_profitcenter (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_costcenter (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        profit_center_id BIGINT,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_project (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_activity (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        project_id BIGINT,
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_location (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        address TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_partner (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        partner_type VARCHAR(20) NOT NULL DEFAULT 'customer',
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        email VARCHAR(254),
        phone VARCHAR(50),
        address TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE TABLE IF NOT EXISTS ledger_product (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        product_type VARCHAR(20) NOT NULL DEFAULT 'goods',
        code VARCHAR(50) NOT NULL,
        name VARCHAR(150) NOT NULL,
        description TEXT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    -- Journal Entry & Items
    CREATE TABLE IF NOT EXISTS ledger_journalentry (
        id BIGSERIAL PRIMARY KEY,
        entity_id VARCHAR(64),
        date DATE NOT NULL,
        description TEXT NOT NULL,
        created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
        period VARCHAR(7),
        is_posted BOOLEAN NOT NULL DEFAULT FALSE
    );

    CREATE TABLE IF NOT EXISTS ledger_journalitem (
        id BIGSERIAL PRIMARY KEY,
        journal_entry_id BIGINT NOT NULL,
        account_id BIGINT NOT NULL,
        cost_center_id BIGINT,
        profit_center_id BIGINT,
        project_id BIGINT,
        activity_id BIGINT,
        location_id BIGINT,
        partner_id BIGINT,
        product_id BIGINT,
        debit INTEGER NOT NULL DEFAULT 0,
        credit INTEGER NOT NULL DEFAULT 0,
        note VARCHAR(255)
    );
    """
    run_psql(schema_sql)
    print("   -> Schema berhasil dibuat/diverifikasi.")

    # 2. Baca file backup jika ada, atau dump langsung
    dump_file = "ledger_backup.sql"
    print(f"2. Membaca data dari {dump_file}...")
    try:
        with open(dump_file, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()
    except Exception as e:
        print(f"❌ Error membaca {dump_file}: {e}")
        return

    # 3. Cari dan konversi seluruh statement INSERT INTO `...`
    # Daftar tabel yang ingin dimigrasikan sesuai urutan dependensi
    target_tables = [
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
        "ledger_journalitem"
    ]

    pg_commands = [
        "SET standard_conforming_strings = off;\n",
        "SET session_replication_role = 'replica';\n",
    ]

    # Truncate tabel-tabel target terlebih dahulu
    trunc_list = ", ".join(target_tables)
    pg_commands.append(f"TRUNCATE TABLE {trunc_list} CASCADE;\n")

    found_count = 0
    for tbl in target_tables:
        pattern = rf"INSERT INTO `{tbl}`[\s\S]*?;"
        matches = re.findall(pattern, content)
        for m in matches:
            # Ganti `tabel` menjadi tabel biasa
            clean_stmt = m.replace(f"`{tbl}`", tbl)
            pg_commands.append(clean_stmt + "\n")
            found_count += 1

    print(f"   -> Ditemukan {found_count} blok INSERT untuk dimigrasikan.")

    pg_commands.append("SET session_replication_role = 'origin';\n")

    # Reset sequences untuk tabel ber-ID serial
    for tbl in target_tables:
        if tbl != "entity_entity":
            pg_commands.append(f"SELECT setval('{tbl}_id_seq', COALESCE((SELECT MAX(id) FROM {tbl}), 1));\n")

    full_sql = "".join(pg_commands)
    print("3. Memasukkan data ke PostgreSQL...")
    out = run_psql(full_sql)
    print(out)

    print("\n4. Verifikasi Data di PostgreSQL:")
    check_sql = "\nUNION ALL\n".join([f"SELECT '{t}' AS tabel, COUNT(*) FROM {t}" for t in target_tables]) + ";"
    print(run_psql(check_sql))
    print("✅ Migrasi Entity & Ledger Selesai!")

if __name__ == "__main__":
    main()

