-- Teknusa ERP PostgreSQL schema (compatible with Django tables)

CREATE TABLE IF NOT EXISTS auth_user (
    id SERIAL PRIMARY KEY,
    password varchar(128) NOT NULL,
    last_login TIMESTAMP(6) DEFAULT NULL,
    is_superuser BOOLEAN NOT NULL DEFAULT FALSE,
    username varchar(150) NOT NULL UNIQUE,
    first_name varchar(150) NOT NULL DEFAULT '',
    last_name varchar(150) NOT NULL DEFAULT '',
    email varchar(254) NOT NULL DEFAULT '',
    is_staff BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    date_joined TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
);

CREATE TABLE IF NOT EXISTS entity_entity (
    id char(32) NOT NULL PRIMARY KEY,
    parent_id char(32) DEFAULT NULL,
    name varchar(200) NOT NULL,
    code varchar(50) NOT NULL UNIQUE,
    description TEXT NOT NULL,
    email varchar(254) NOT NULL DEFAULT '',
    phone varchar(50) NOT NULL DEFAULT '',
    logo varchar(100) DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT entity_entity_parent_fk FOREIGN KEY (parent_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS leads_lead (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    name varchar(200) NOT NULL,
    email varchar(254) DEFAULT NULL,
    phone varchar(20) DEFAULT NULL,
    company varchar(200) DEFAULT NULL,
    account_medsos varchar(200) DEFAULT NULL,
    lead_source varchar(50) DEFAULT NULL,
    assigned_to_id INTEGER DEFAULT NULL,
    priority varchar(10) NOT NULL DEFAULT 'warm',
    estimated_value decimal(12,2) DEFAULT NULL,
    next_follow_up date DEFAULT NULL,
    status varchar(20) NOT NULL DEFAULT 'new',
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT leads_lead_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE SET NULL,
    CONSTRAINT leads_lead_user_fk FOREIGN KEY (assigned_to_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS leads_interaction (
    id BIGSERIAL PRIMARY KEY,
    lead_id BIGINT NOT NULL,
    note TEXT NOT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT leads_interaction_lead_fk FOREIGN KEY (lead_id) REFERENCES leads_lead (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS lean_idea (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    title varchar(200) NOT NULL,
    description TEXT NOT NULL,
    assumptions TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    status varchar(20) NOT NULL DEFAULT 'draft'
);

CREATE TABLE IF NOT EXISTS lean_build (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    idea_id BIGINT NOT NULL,
    title varchar(200) NOT NULL,
    description TEXT NOT NULL,
    link varchar(200) NOT NULL DEFAULT '',
    status varchar(20) NOT NULL DEFAULT 'in_progress',
    CONSTRAINT lean_build_idea_fk FOREIGN KEY (idea_id) REFERENCES lean_idea (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS lean_product (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    build_id BIGINT NOT NULL,
    name varchar(200) NOT NULL,
    description TEXT NOT NULL,
    public BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT lean_product_build_fk FOREIGN KEY (build_id) REFERENCES lean_build (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS lean_measure (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    product_id BIGINT NOT NULL,
    metric varchar(200) NOT NULL,
    target DOUBLE PRECISION DEFAULT NULL,
    current DOUBLE PRECISION DEFAULT NULL,
    notes TEXT NOT NULL,
    CONSTRAINT lean_measure_product_fk FOREIGN KEY (product_id) REFERENCES lean_product (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS lean_datapoint (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    measure_id BIGINT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    measured_at TIMESTAMP(6) NOT NULL,
    note TEXT NOT NULL,
    CONSTRAINT lean_datapoint_measure_fk FOREIGN KEY (measure_id) REFERENCES lean_measure (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS lean_learning (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    idea_id BIGINT DEFAULT NULL,
    summary TEXT NOT NULL,
    action TEXT NOT NULL,
    CONSTRAINT lean_learning_idea_fk FOREIGN KEY (idea_id) REFERENCES lean_idea (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_account (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    account_type varchar(50) NOT NULL,
    account_name varchar(100) NOT NULL,
    coa varchar(100) NOT NULL DEFAULT 'Default CoA',
    balance_type varchar(10) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    coa_role_default varchar(100) DEFAULT NULL,
    CONSTRAINT ledger_account_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_closingperiod (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    period varchar(7) NOT NULL,
    is_closed BOOLEAN NOT NULL DEFAULT FALSE,
    closed_at TIMESTAMP(6) DEFAULT NULL,
    closed_by varchar(100) DEFAULT NULL,
    CONSTRAINT ledger_closingperiod_period_entity_uniq UNIQUE (period, entity_id),
    CONSTRAINT ledger_closingperiod_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_profitcenter (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    description TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_profitcenter_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_costcenter (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    profit_center_id BIGINT DEFAULT NULL,
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    description TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_costcenter_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE,
    CONSTRAINT ledger_costcenter_pc_fk FOREIGN KEY (profit_center_id) REFERENCES ledger_profitcenter (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS ledger_project (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    description TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_project_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_activity (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    project_id BIGINT DEFAULT NULL,
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    description TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_activity_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE,
    CONSTRAINT ledger_activity_project_fk FOREIGN KEY (project_id) REFERENCES ledger_project (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS ledger_location (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    address TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_location_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_partner (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    partner_type varchar(20) NOT NULL DEFAULT 'customer',
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    email varchar(254) DEFAULT NULL,
    phone varchar(50) DEFAULT NULL,
    address TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_partner_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_product (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    product_type varchar(20) NOT NULL DEFAULT 'goods',
    code varchar(50) NOT NULL,
    name varchar(150) NOT NULL,
    description TEXT DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT ledger_product_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_journalentry (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    date date NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    period varchar(7) DEFAULT NULL,
    is_posted BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT ledger_journalentry_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ledger_journalitem (
    id BIGSERIAL PRIMARY KEY,
    journal_entry_id BIGINT NOT NULL,
    account_id BIGINT NOT NULL,
    cost_center_id BIGINT DEFAULT NULL,
    profit_center_id BIGINT DEFAULT NULL,
    project_id BIGINT DEFAULT NULL,
    activity_id BIGINT DEFAULT NULL,
    location_id BIGINT DEFAULT NULL,
    partner_id BIGINT DEFAULT NULL,
    product_id BIGINT DEFAULT NULL,
    debit INTEGER NOT NULL DEFAULT 0,
    credit INTEGER NOT NULL DEFAULT 0,
    note varchar(255) DEFAULT NULL,
    CONSTRAINT ledger_ji_je_fk FOREIGN KEY (journal_entry_id) REFERENCES ledger_journalentry (id) ON DELETE CASCADE,
    CONSTRAINT ledger_ji_acc_fk FOREIGN KEY (account_id) REFERENCES ledger_account (id) ON DELETE RESTRICT,
    CONSTRAINT ledger_ji_cc_fk FOREIGN KEY (cost_center_id) REFERENCES ledger_costcenter (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_pc_fk FOREIGN KEY (profit_center_id) REFERENCES ledger_profitcenter (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_prj_fk FOREIGN KEY (project_id) REFERENCES ledger_project (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_act_fk FOREIGN KEY (activity_id) REFERENCES ledger_activity (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_loc_fk FOREIGN KEY (location_id) REFERENCES ledger_location (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_prt_fk FOREIGN KEY (partner_id) REFERENCES ledger_partner (id) ON DELETE SET NULL,
    CONSTRAINT ledger_ji_prd_fk FOREIGN KEY (product_id) REFERENCES ledger_product (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS post_media_akun (
    id BIGSERIAL PRIMARY KEY,
    name varchar(20) DEFAULT NULL UNIQUE,
    description TEXT DEFAULT NULL
);

CREATE TABLE IF NOT EXISTS post_media_market (
    id BIGSERIAL PRIMARY KEY,
    name varchar(20) DEFAULT NULL UNIQUE,
    description TEXT DEFAULT NULL
);

CREATE TABLE IF NOT EXISTS post_media_digitalchannel (
    id BIGSERIAL PRIMARY KEY,
    name varchar(20) DEFAULT NULL UNIQUE,
    description TEXT DEFAULT NULL
);

CREATE TABLE IF NOT EXISTS post_media_funnelstage (
    id BIGSERIAL PRIMARY KEY,
    name varchar(20) DEFAULT NULL UNIQUE,
    description TEXT DEFAULT NULL
);

CREATE TABLE IF NOT EXISTS post_media_biaya (
    id BIGSERIAL PRIMARY KEY,
    name varchar(20) DEFAULT NULL UNIQUE,
    description TEXT DEFAULT NULL
);

CREATE TABLE IF NOT EXISTS post_media_channel (
    id BIGSERIAL PRIMARY KEY,
    akun_id BIGINT DEFAULT NULL,
    for_market_id BIGINT DEFAULT NULL,
    channel_id BIGINT DEFAULT NULL,
    funnel_stage_id BIGINT DEFAULT NULL,
    kategori_biaya_id BIGINT DEFAULT NULL,
    amount decimal(12,2) DEFAULT NULL,
    rencana_tanggal_posting date DEFAULT NULL,
    tanggal_posting TIMESTAMP(6) DEFAULT NULL,
    is_posted BOOLEAN NOT NULL DEFAULT FALSE,
    jenis_konten varchar(100) NOT NULL DEFAULT '',
    judul varchar(200) NOT NULL,
    isi_konten TEXT NOT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT post_media_ch_akun_fk FOREIGN KEY (akun_id) REFERENCES post_media_akun (id) ON DELETE CASCADE,
    CONSTRAINT post_media_ch_market_fk FOREIGN KEY (for_market_id) REFERENCES post_media_market (id) ON DELETE CASCADE,
    CONSTRAINT post_media_ch_dc_fk FOREIGN KEY (channel_id) REFERENCES post_media_digitalchannel (id) ON DELETE CASCADE,
    CONSTRAINT post_media_ch_fs_fk FOREIGN KEY (funnel_stage_id) REFERENCES post_media_funnelstage (id) ON DELETE CASCADE,
    CONSTRAINT post_media_ch_biaya_fk FOREIGN KEY (kategori_biaya_id) REFERENCES post_media_biaya (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS post_media_channelperformance (
    id BIGSERIAL PRIMARY KEY,
    channel_id BIGINT NOT NULL,
    period varchar(10) NOT NULL,
    as_of_date date NOT NULL,
    metrics TEXT NOT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT post_media_perf_ch_fk FOREIGN KEY (channel_id) REFERENCES post_media_channel (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS post_media_idekonten (
    id BIGSERIAL PRIMARY KEY,
    teks varchar(255) NOT NULL,
    terkirim_telegram BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
);

CREATE TABLE IF NOT EXISTS kanban_board (
    id BIGSERIAL PRIMARY KEY,
    entity_id char(32) DEFAULT NULL,
    name varchar(200) NOT NULL,
    description TEXT NOT NULL,
    created_by_id INTEGER DEFAULT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT kanban_board_entity_fk FOREIGN KEY (entity_id) REFERENCES entity_entity (id) ON DELETE CASCADE,
    CONSTRAINT kanban_board_user_fk FOREIGN KEY (created_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS kanban_column (
    id BIGSERIAL PRIMARY KEY,
    board_id BIGINT NOT NULL,
    name varchar(100) NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT kanban_col_board_fk FOREIGN KEY (board_id) REFERENCES kanban_board (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS kanban_label (
    id BIGSERIAL PRIMARY KEY,
    name varchar(50) NOT NULL,
    color varchar(20) NOT NULL DEFAULT '#007bff'
);

CREATE TABLE IF NOT EXISTS kanban_task (
    id BIGSERIAL PRIMARY KEY,
    column_id BIGINT NOT NULL,
    title varchar(255) NOT NULL,
    description TEXT NOT NULL,
    priority varchar(20) NOT NULL DEFAULT 'medium',
    start_date date DEFAULT NULL,
    due_date date DEFAULT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    created_by_id INTEGER DEFAULT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT kanban_task_col_fk FOREIGN KEY (column_id) REFERENCES kanban_column (id) ON DELETE CASCADE,
    CONSTRAINT kanban_task_user_fk FOREIGN KEY (created_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS kanban_task_assignees (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL,
    user_id INTEGER NOT NULL,
    CONSTRAINT kanban_task_assignees_uniq UNIQUE (task_id, user_id),
    CONSTRAINT kanban_ta_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT kanban_ta_user_fk FOREIGN KEY (user_id) REFERENCES auth_user (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS kanban_task_labels (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL,
    label_id BIGINT NOT NULL,
    CONSTRAINT kanban_task_labels_uniq UNIQUE (task_id, label_id),
    CONSTRAINT kanban_tl_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT kanban_tl_label_fk FOREIGN KEY (label_id) REFERENCES kanban_label (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS kanban_taskcomment (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL,
    author_id INTEGER NOT NULL,
    comment TEXT NOT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT kanban_tc_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT kanban_tc_user_fk FOREIGN KEY (author_id) REFERENCES auth_user (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS kanban_taskattachment (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL,
    file varchar(100) NOT NULL,
    uploaded_by_id INTEGER DEFAULT NULL,
    uploaded_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT kanban_tatt_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT kanban_tatt_user_fk FOREIGN KEY (uploaded_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS kanban_taskchecklistitem (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL,
    title varchar(255) NOT NULL,
    is_completed BOOLEAN NOT NULL DEFAULT FALSE,
    position INTEGER NOT NULL DEFAULT 0,
    created_by_id INTEGER DEFAULT NULL,
    completed_by_id INTEGER DEFAULT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    completed_at TIMESTAMP(6) DEFAULT NULL,
    CONSTRAINT kanban_chk_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT kanban_chk_cby_fk FOREIGN KEY (created_by_id) REFERENCES auth_user (id) ON DELETE SET NULL,
    CONSTRAINT kanban_chk_doneby_fk FOREIGN KEY (completed_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS gantt_taskdependency (
    id BIGSERIAL PRIMARY KEY,
    predecessor_id BIGINT NOT NULL,
    successor_id BIGINT NOT NULL,
    created_by_id INTEGER DEFAULT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT gantt_dep_pred_succ_uniq UNIQUE (predecessor_id, successor_id),
    CONSTRAINT gantt_dep_pred_fk FOREIGN KEY (predecessor_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT gantt_dep_succ_fk FOREIGN KEY (successor_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT gantt_dep_user_fk FOREIGN KEY (created_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS gantt_milestone (
    id BIGSERIAL PRIMARY KEY,
    board_id BIGINT NOT NULL,
    task_id BIGINT DEFAULT NULL,
    title varchar(200) NOT NULL,
    date date NOT NULL,
    is_done BOOLEAN NOT NULL DEFAULT FALSE,
    created_by_id INTEGER DEFAULT NULL,
    created_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT gantt_ms_board_fk FOREIGN KEY (board_id) REFERENCES kanban_board (id) ON DELETE CASCADE,
    CONSTRAINT gantt_ms_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT gantt_ms_user_fk FOREIGN KEY (created_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS gantt_taskbaseline (
    id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL UNIQUE,
    planned_start date NOT NULL,
    planned_end date NOT NULL,
    set_by_id INTEGER DEFAULT NULL,
    set_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT gantt_bl_task_fk FOREIGN KEY (task_id) REFERENCES kanban_task (id) ON DELETE CASCADE,
    CONSTRAINT gantt_bl_user_fk FOREIGN KEY (set_by_id) REFERENCES auth_user (id) ON DELETE SET NULL
);

