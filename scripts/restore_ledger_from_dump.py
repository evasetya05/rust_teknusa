#!/usr/bin/env python3
"""
Restore Presisi Entity, Ledger, dan CRM ke PostgreSQL
Mengubah tipe data id/entity_id menjadi VARCHAR(64) agar UUID 36-karakter muat,
menyesuaikan urutan kolom, dan mengimpor seluruh data.
"""

import os
import re
import subprocess
import sys

POSTGRES_CONTAINER = os.environ.get("POSTGRES_CONTAINER", "teknusa_rust_postgres")
POSTGRES_USER = os.environ.get("POSTGRES_USER", "teknusas_teknusa")
POSTGRES_DB = os.environ.get("POSTGRES_DB", "teknusas_teknusa")

BACKUP_PATHS = [
    "/srv/apps/teknusa/ledger_backup.sql",
    "ledger_backup.sql",
    "../ledger_backup.sql",
]

CRM_BACKUP_PATHS = [
    "/srv/apps/teknusa/crm_backup.sql",
    "crm_backup.sql",
    "../crm_backup.sql",
]

TABLE_CONFIG = {
    "entity_entity": (
        ["id", "name", "code", "description", "email", "phone", "logo", "is_active", "created_at", "updated_at", "parent_id"],
        [7]
    ),
    "ledger_account": (
        ["id", "account_name", "account_type", "active", "balance_type", "coa", "coa_role_default", "locked", "entity_id"],
        [3, 7]
    ),
    "ledger_closingperiod": (
        ["id", "period", "is_closed", "closed_at", "closed_by", "entity_id"],
        [2]
    ),
    "ledger_profitcenter": (
        ["id", "code", "name", "description", "is_active", "created_at", "updated_at", "entity_id"],
        [4]
    ),
    "ledger_costcenter": (
        ["id", "code", "name", "description", "is_active", "created_at", "updated_at", "entity_id", "profit_center_id"],
        [4]
    ),
    "ledger_project": (
        ["id", "code", "name", "description", "is_active", "created_at", "updated_at", "entity_id"],
        [4]
    ),
    "ledger_activity": (
        ["id", "code", "name", "description", "is_active", "created_at", "updated_at", "entity_id", "project_id"],
        [4]
    ),
    "ledger_location": (
        ["id", "code", "name", "address", "is_active", "created_at", "updated_at", "entity_id"],
        [4]
    ),
    "ledger_partner": (
        ["id", "partner_type", "code", "name", "email", "phone", "address", "is_active", "created_at", "updated_at", "entity_id"],
        [7]
    ),
    "ledger_product": (
        ["id", "product_type", "code", "name", "description", "is_active", "created_at", "updated_at", "entity_id"],
        [5]
    ),
    "ledger_journalentry": (
        ["id", "date", "description", "created_at", "is_posted", "period", "entity_id"],
        [4]
    ),
    "ledger_journalitem": (
        ["id", "debit", "credit", "account_id", "journal_entry_id", "note", "activity_id", "cost_center_id", "location_id", "partner_id", "product_id", "profit_center_id", "project_id"],
        []
    )
}

def extract_tuples(values_block):
    tuples = []
    current = []
    depth = 0
    in_quote = False
    escape = False
    for ch in values_block:
        if escape:
            current.append(ch)
            escape = False
        elif ch == "\\":
            current.append(ch)
            escape = True
        elif ch == "'":
            current.append(ch)
            in_quote = not in_quote
        elif ch == "(" and not in_quote:
            depth += 1
            current.append(ch)
        elif ch == ")" and not in_quote:
            depth -= 1
            current.append(ch)
            if depth == 0:
                tuples.append("".join(current).strip())
                current = []
        elif depth > 0:
            current.append(ch)
    return tuples

def convert_row(row_str, bool_indices):
    s = row_str.strip()
    if s.startswith("(") and s.endswith(")"):
        s = s[1:-1]
    tokens = []
    current = []
    in_quote = False
    escape = False
    for ch in s:
        if escape:
            current.append(ch)
            escape = False
        elif ch == "\\":
            current.append(ch)
            escape = True
        elif ch == "'":
            current.append(ch)
            in_quote = not in_quote
        elif ch == "," and not in_quote:
            tokens.append("".join(current).strip())
            current = []
        else:
            current.append(ch)
    tokens.append("".join(current).strip())

    for idx in bool_indices:
        if idx < len(tokens):
            if tokens[idx] == "1":
                tokens[idx] = "TRUE"
            elif tokens[idx] == "0":
                tokens[idx] = "FALSE"

    for i in range(len(tokens)):
        t = tokens[i]
        if t.startswith("'") and t.endswith("'"):
            inner = t[1:-1]
            inner = inner.replace("\\'", "''").replace('\\"', '"')
            tokens[i] = f"'{inner}'"

    return "(" + ", ".join(tokens) + ")"

def run_psql(sql):
    proc = subprocess.run(
        ["docker", "exec", "-i", POSTGRES_CONTAINER, "psql", "-U", POSTGRES_USER, "-d", POSTGRES_DB],
        input=sql, text=True, capture_output=True
    )
    if proc.stderr:
        print("[PSQL Stderr]:", proc.stderr, file=sys.stderr)
    return proc.stdout

def main():
    print("=" * 60)
    print(" Migrasi Entity, Ledger & CRM ke PostgreSQL")
    print("=" * 60)

    backup_file = None
    for p in BACKUP_PATHS:
        if os.path.exists(p):
            backup_file = p
            break

    if not backup_file:
        print(f"❌ Error: File ledger_backup.sql tidak ditemukan di: {BACKUP_PATHS}")
        sys.exit(1)

    print("1. Menyesuaikan tipe kolom UUID menjadi VARCHAR(64) di PostgreSQL...")
    alter_sql = """
    ALTER TABLE entity_entity ALTER COLUMN id TYPE VARCHAR(64);
    ALTER TABLE entity_entity ALTER COLUMN parent_id TYPE VARCHAR(64);
    ALTER TABLE ledger_account ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_closingperiod ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_profitcenter ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_costcenter ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_project ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_activity ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_location ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_partner ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_product ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE ledger_journalentry ALTER COLUMN entity_id TYPE VARCHAR(64);
    ALTER TABLE leads_lead ALTER COLUMN entity_id TYPE VARCHAR(64);
    """
    run_psql(alter_sql)

    print(f"2. Membaca {backup_file}...")
    with open(backup_file, "r", encoding="utf-8", errors="ignore") as f:
        content = f.read()

    print("3. Memproses data tabel Entity & Ledger...")
    pg_commands = [
        "SET standard_conforming_strings = off;\n",
        "SET session_replication_role = 'replica';\n",
    ]

    tables_to_truncate = list(TABLE_CONFIG.keys())
    pg_commands.append(f"TRUNCATE TABLE {', '.join(tables_to_truncate)} CASCADE;\n")

    for tbl, (cols, bool_indices) in TABLE_CONFIG.items():
        matches = re.findall(rf"INSERT INTO `{tbl}` VALUES\s*([\s\S]*?);", content)

        if not matches:
            print(f"   -> {tbl}: 0 baris")
            continue

        converted_tuples = []
        for val_block in matches:
            extracted = extract_tuples(val_block)
            for t in extracted:
                converted_tuples.append(convert_row(t, bool_indices))

        if not converted_tuples:
            print(f"   -> {tbl}: 0 baris")
            continue

        cols_str = ", ".join(cols)
        vals_str = ",\n".join(converted_tuples)
        stmt = f"INSERT INTO {tbl} ({cols_str}) VALUES\n{vals_str};\n"
        pg_commands.append(stmt)
        print(f"   -> ✅ {tbl}: {len(converted_tuples)} baris berhasil diproses.")

    for tbl in TABLE_CONFIG.keys():
        if tbl != "entity_entity":
            pg_commands.append(f"SELECT setval('{tbl}_id_seq', COALESCE((SELECT MAX(id) FROM {tbl}), 1));\n")

    # Pulihkan juga CRM jika file backup CRM ada
    crm_file = None
    for cp in CRM_BACKUP_PATHS:
        if os.path.exists(cp):
            crm_file = cp
            break

    if crm_file:
        print(f"4. Memulihkan data CRM dari {crm_file}...")
        with open(crm_file, "r", encoding="utf-8", errors="ignore") as f:
            crm_content = f.read()

        leads_matches = re.findall(r"INSERT INTO `leads_lead`[\s\S]*?;", crm_content)
        for m in leads_matches:
            stmt = m.replace("`leads_lead`", "leads_lead (id, name, email, phone, company, status, created_at, assigned_to_id, estimated_value, lead_source, next_follow_up, priority, account_medsos, entity_id)")
            pg_commands.append(stmt + "\n")

        inter_matches = re.findall(r"INSERT INTO `leads_interaction`[\s\S]*?;", crm_content)
        for m in inter_matches:
            stmt = m.replace("`leads_interaction`", "leads_interaction (id, note, created_at, lead_id)")
            pg_commands.append(stmt + "\n")

        pg_commands.append("SELECT setval('leads_lead_id_seq', COALESCE((SELECT MAX(id) FROM leads_lead), 1));\n")
        pg_commands.append("SELECT setval('leads_interaction_id_seq', COALESCE((SELECT MAX(id) FROM leads_interaction), 1));\n")

    pg_commands.append("SET session_replication_role = 'origin';\n")

    print("5. Memasukkan data ke PostgreSQL...")
    full_sql = "".join(pg_commands)
    out = run_psql(full_sql)
    print(out)

    print("6. Verifikasi Data Akhir di PostgreSQL:")
    check_tables = list(TABLE_CONFIG.keys()) + ["leads_lead", "leads_interaction"]
    check_sql = "\nUNION ALL\n".join([f"SELECT '{t}' AS tabel, COUNT(*) FROM {t}" for t in check_tables]) + ";"
    print(run_psql(check_sql))
    print("✅ Selesai!")

if __name__ == "__main__":
    main()
