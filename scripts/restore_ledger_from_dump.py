#!/usr/bin/env python3
"""
Restore Presisi Entity & Ledger dari file ledger_backup.sql ke PostgreSQL
Menangani perbedaan urutan kolom MariaDB vs PostgreSQL, boolean (TRUE/FALSE), dan quote escaping.
"""

import os
import re
import subprocess
import sys

POSTGRES_CONTAINER = os.environ.get("POSTGRES_CONTAINER", "teknusa_rust_postgres")
POSTGRES_USER = os.environ.get("POSTGRES_USER", "teknusas_teknusa")
POSTGRES_DB = os.environ.get("POSTGRES_DB", "teknusas_teknusa")

BACKUP_PATHS = [
    "ledger_backup.sql",
    "/srv/apps/teknusa/ledger_backup.sql",
    "../teknusa/ledger_backup.sql",
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
    print(" Migrasi Entity & Ledger dari ledger_backup.sql")
    print("=" * 60)

    backup_file = None
    for p in BACKUP_PATHS:
        if os.path.exists(p):
            backup_file = p
            break

    if not backup_file:
        print(f"❌ Error: File ledger_backup.sql tidak ditemukan di: {BACKUP_PATHS}")
        sys.exit(1)

    print(f"1. Membaca {backup_file}...")
    with open(backup_file, "r", encoding="utf-8", errors="ignore") as f:
        content = f.read()

    print("2. Memproses data tabel...")
    pg_commands = [
        "SET standard_conforming_strings = off;\n",
        "SET session_replication_role = 'replica';\n",
    ]

    # Truncate tables first
    tables_to_truncate = list(TABLE_CONFIG.keys())
    pg_commands.append(f"TRUNCATE TABLE {', '.join(tables_to_truncate)} CASCADE;\n")

    total_rows = 0
    for tbl, (cols, bool_indices) in TABLE_CONFIG.items():
        pattern = rf"INSERT INTO `{tbl}` VALUES\s*([\s\S]*?);\n"
        matches = re.findall(pattern, content)
        if not matches:
            # Coba tanpa semicolon newline ketat
            matches = re.findall(rf"INSERT INTO `{tbl}` VALUES\s*([\s\S]*?);", content)

        if not matches:
            print(f"   -> {tbl}: 0 baris (tidak ada data di backup)")
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
        total_rows += len(converted_tuples)

    # Sequences
    for tbl in TABLE_CONFIG.keys():
        if tbl != "entity_entity":
            pg_commands.append(f"SELECT setval('{tbl}_id_seq', COALESCE((SELECT MAX(id) FROM {tbl}), 1));\n")

    pg_commands.append("SET session_replication_role = 'origin';\n")

    print("\n3. Memasukkan data ke PostgreSQL...")
    full_sql = "".join(pg_commands)
    out = run_psql(full_sql)
    print(out)

    print("4. Verifikasi Data Akhir di PostgreSQL:")
    check_sql = "\nUNION ALL\n".join([f"SELECT '{t}' AS tabel, COUNT(*) FROM {t}" for t in TABLE_CONFIG.keys()]) + ";"
    print(run_psql(check_sql))
    print("✅ Selesai!")

if __name__ == "__main__":
    main()

