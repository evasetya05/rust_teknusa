# Teknusa ERP (Rust Port)

Porting penuh modul internal ERP dari project Python Django (`teknusa`) ke Rust (`Axum` + `SQLx MySQL/MariaDB` + `Tera`).

## Modul yang Tersedia
- **Accounts & Auth (`/login/`, `/register/`, `/logout/`)**: Kompatibel dengan tabel `auth_user` dan hash password Django `pbkdf2_sha256`, plus cookie sesi bertanda HMAC-SHA256.
- **Entity / Multi-Perusahaan (`/entity/`)**: CRUD perusahaan (`entity_entity`), upload logo, dan switcher perusahaan aktif berbasis sesi (`/entity/select/`).
- **Dashboard (`/dashboard/`)**: Ringkasan harian & AI Agent briefing + floating Report Assistant Chat untuk admin.
- **CRM & Leads (`/leads/`)**: Daftar lead, detail & catatan interaksi, analisis KPI, grafik tren 14 hari, serta FullCalendar follow-up (`/leads/calendar/`).
- **Lean Innovation (`/lean/`)**: Idea, Build, Product, Measure, DataPoint, dan Learning.
- **Accounting & Ledger (`/ledger/`)**: Chart of Accounts, Jurnal Multi-Dimensi (7 dimensi), Closing Period + penyesuaian Retained Earnings otomatis, Buku Besar, Laba Rugi, Neraca Saldo, Profitabilitas, Solvabilitas, dan Analisis Dimensi.
- **Post Media (`/post_media/`)**: Daftar konten per market & channel, metrik performa (`24h`, `1w`, `1m`), analitik & export CSV, serta generator ide konten AI (`/post_media/ide-konten/`).
- **Kanban (`/kanban/`)**: Board, kolom otomatis, drag-and-drop card, komentar, lampiran file, checklist AJAX, dan Kalender Kanban bulanan.
- **Gantt Chart (`/gantt/`)**: Timeline interaktif, drag/resize tanggal task, dependency arrows dengan deteksi siklus DFS, milestone, dan baseline snapshot.
- **AI Agent & LLM (`/agent/briefing/`, `/agent/llm/`, `/agent/chat/`)**: Kolektor metrik deterministik, alert traffic-light (`green`/`yellow`/`red`), narator Gemini & Ollama dengan validasi anti-halusinasi angka, dan chat laporan.

## Menjalankan Secara Lokal

```bash
cp .env.example .env
# Sesuaikan DB_HOST, DB_USER, DB_PASSWORD, DB_NAME di .env
cargo run --release
```

Server berjalan di `http://0.0.0.0:8000` (atau port sesuai `PORT`). Jika tabel `auth_user` masih kosong, user admin default (`admin` / `admin123`) dibuat otomatis saat startup.

## Menjalankan dengan Docker Compose

```bash
docker compose up -d --build
```

Untuk menyertakan layanan lokal Ollama:

```bash
docker compose --profile llm up -d --build
```

