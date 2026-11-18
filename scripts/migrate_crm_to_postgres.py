#!/usr/bin/env python3
"""
Migrasi Data CRM (leads_lead & leads_interaction) dari MariaDB (teknusa) ke PostgreSQL (rust_teknusa)
Bisa dijalankan langsung di server production:
    python3 scripts/migrate_crm_to_postgres.py
"""

import os
import subprocess
import sys

MARIADB_CONTAINER = os.environ.get("MARIADB_CONTAINER", "teknusa_mariadb")
MARIADB_USER = os.environ.get("MARIADB_USER", "teknusas_teknusa")
MARIADB_PASS = os.environ.get("MARIADB_PASS", "@Pontianak123")
MARIADB_DB = os.environ.get("MARIADB_DB", "teknusas_teknusa")

POSTGRES_CONTAINER = os.environ.get("POSTGRES_CONTAINER", "teknusa_rust_postgres")
POSTGRES_USER = os.environ.get("POSTGRES_USER", "postgres")
POSTGRES_DB = os.environ.get("POSTGRES_DB", "teknusas_teknusa")

def run_cmd(cmd, input_data=None):
    res = subprocess.run(cmd, shell=True, input=input_data, text=True, capture_output=True)
    if res.returncode != 0:
        print(f"Error executing: {cmd}\n{res.stderr}", file=sys.stderr)
    return res

def main():
    print("=" * 60)
    print(" Migrasi CRM: MariaDB -> PostgreSQL")
    print(f" Source MariaDB : {MARIADB_CONTAINER} ({MARIADB_DB})")
    print(f" Target Postgres: {POSTGRES_CONTAINER} ({POSTGRES_DB})")
    print("=" * 60)

    # 1. DDL Schema PostgreSQL untuk CRM
    pg_schema = """
-- Buat tabel auth_user jika belum ada (untuk foreign key user)
CREATE TABLE IF NOT EXISTS auth_user (
    id SERIAL PRIMARY KEY,
    password VARCHAR(128) NOT NULL,
    last_login TIMESTAMPTZ,
    is_superuser BOOLEAN NOT NULL DEFAULT FALSE,
    username VARCHAR(150) NOT NULL UNIQUE,
    first_name VARCHAR(150) NOT NULL DEFAULT '',
    last_name VARCHAR(150) NOT NULL DEFAULT '',
    email VARCHAR(254) NOT NULL DEFAULT '',
    is_staff BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    date_joined TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Buat tabel entity_entity jika ada relasi
CREATE TABLE IF NOT EXISTS entity_entity (
    id CHAR(32) PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Buat tabel leads_lead
CREATE TABLE IF NOT EXISTS leads_lead (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    email VARCHAR(254),
    phone VARCHAR(20),
    company VARCHAR(200),
    status VARCHAR(20) NOT NULL DEFAULT 'new',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    assigned_to_id INTEGER,
    estimated_value NUMERIC(12,2),
    lead_source VARCHAR(50),
    next_follow_up DATE,
    priority VARCHAR(10) NOT NULL DEFAULT 'warm',
    account_medsos VARCHAR(200),
    entity_id CHAR(32)
);

-- Buat tabel leads_interaction
CREATE TABLE IF NOT EXISTS leads_interaction (
    id BIGSERIAL PRIMARY KEY,
    note TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    lead_id BIGINT NOT NULL
);

-- Reset sequence ke max id nantinya
"""

    print("1. Menyiapkan schema tabel CRM di PostgreSQL...")
    run_cmd(f'docker exec -i {POSTGRES_CONTAINER} psql -U {POSTGRES_USER} -d {POSTGRES_DB}', input_data=pg_schema)

    # 2. Export leads_lead dari MariaDB dalam format TSV / tab-separated
    print("2. Mengambil data 'leads_lead' dari MariaDB...")
    query_lead = """
    SELECT 
        id, 
        REPLACE(REPLACE(IFNULL(name, ''), '\\t', ' '), '\\n', ' '),
        IFNULL(email, '\\N'),
        IFNULL(phone, '\\N'),
        REPLACE(REPLACE(IFNULL(company, ''), '\\t', ' '), '\\n', ' '),
        IFNULL(status, 'new'),
        created_at,
        IFNULL(assigned_to_id, '\\N'),
        IFNULL(estimated_value, '\\N'),
        IFNULL(lead_source, '\\N'),
        IFNULL(next_follow_up, '\\N'),
        IFNULL(priority, 'warm'),
        REPLACE(REPLACE(IFNULL(account_medsos, ''), '\\t', ' '), '\\n', ' ')
    FROM leads_lead;
    """
    res_leads = run_cmd(f'docker exec {MARIADB_CONTAINER} mysql -u {MARIADB_USER} -p{MARIADB_PASS} -N -e "{query_lead}" {MARIADB_DB}')
    lead_rows = res_leads.stdout.strip()
    
    if lead_rows:
        copy_leads_cmd = f"""
        COPY leads_lead (id, name, email, phone, company, status, created_at, assigned_to_id, estimated_value, lead_source, next_follow_up, priority, account_medsos)
        FROM STDIN WITH (FORMAT text, NULL '\\N');
        SELECT setval('leads_lead_id_seq', COALESCE((SELECT MAX(id) FROM leads_lead), 1));
        """
        run_cmd(f'docker exec -i {POSTGRES_CONTAINER} psql -U {POSTGRES_USER} -d {POSTGRES_DB} -c "{copy_leads_cmd}"', input_data=lead_rows)
        print("   -> Data leads_lead berhasil di-copy ke PostgreSQL!")
    else:
        print("   -> Tabel leads_lead kosong atau tidak ada data.")

    # 3. Export leads_interaction dari MariaDB
    print("3. Mengambil data 'leads_interaction' dari MariaDB...")
    query_inter = """
    SELECT 
        id,
        lead_id,
        REPLACE(REPLACE(IFNULL(note, ''), '\\t', ' '), '\\n', ' '),
        created_at
    FROM leads_interaction;
    """
    res_inter = run_cmd(f'docker exec {MARIADB_CONTAINER} mysql -u {MARIADB_USER} -p{MARIADB_PASS} -N -e "{query_inter}" {MARIADB_DB}')
    inter_rows = res_inter.stdout.strip()

    if inter_rows:
        copy_inter_cmd = f"""
        COPY leads_interaction (id, lead_id, note, created_at)
        FROM STDIN WITH (FORMAT text, NULL '\\N');
        SELECT setval('leads_interaction_id_seq', COALESCE((SELECT MAX(id) FROM leads_interaction), 1));
        """
        run_cmd(f'docker exec -i {POSTGRES_CONTAINER} psql -U {POSTGRES_USER} -d {POSTGRES_DB} -c "{copy_inter_cmd}"', input_data=inter_rows)
        print("   -> Data leads_interaction berhasil di-copy ke PostgreSQL!")
    else:
        print("   -> Tabel leads_interaction kosong atau tidak ada data.")

    # 4. Verifikasi
    print("\n4. Verifikasi Jumlah Data di PostgreSQL:")
    check_cmd = """
    SELECT 'leads_lead' AS tabel, COUNT(*) FROM leads_lead
    UNION ALL
    SELECT 'leads_interaction', COUNT(*) FROM leads_interaction;
    """
    verif = run_cmd(f'docker exec -i {POSTGRES_CONTAINER} psql -U {POSTGRES_USER} -d {POSTGRES_DB} -c "{check_cmd}"')
    print(verif.stdout)
    print("✅ Selesai!")

if __name__ == "__main__":
    main()

