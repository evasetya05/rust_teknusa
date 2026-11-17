-- Teknusa ERP MariaDB Schema (compatible with Django tables)

CREATE TABLE IF NOT EXISTS `auth_user` (
    `id` int(11) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `password` varchar(128) NOT NULL,
    `last_login` datetime(6) DEFAULT NULL,
    `is_superuser` tinyint(1) NOT NULL DEFAULT 0,
    `username` varchar(150) NOT NULL UNIQUE,
    `first_name` varchar(150) NOT NULL DEFAULT '',
    `last_name` varchar(150) NOT NULL DEFAULT '',
    `email` varchar(254) NOT NULL DEFAULT '',
    `is_staff` tinyint(1) NOT NULL DEFAULT 0,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `date_joined` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `entity_entity` (
    `id` char(32) NOT NULL PRIMARY KEY,
    `parent_id` char(32) DEFAULT NULL,
    `name` varchar(200) NOT NULL,
    `code` varchar(50) NOT NULL UNIQUE,
    `description` longtext NOT NULL,
    `email` varchar(254) NOT NULL DEFAULT '',
    `phone` varchar(50) NOT NULL DEFAULT '',
    `logo` varchar(100) DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `entity_entity_parent_fk` FOREIGN KEY (`parent_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `leads_lead` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `name` varchar(200) NOT NULL,
    `email` varchar(254) DEFAULT NULL,
    `phone` varchar(20) DEFAULT NULL,
    `company` varchar(200) DEFAULT NULL,
    `account_medsos` varchar(200) DEFAULT NULL,
    `lead_source` varchar(50) DEFAULT NULL,
    `assigned_to_id` int(11) DEFAULT NULL,
    `priority` varchar(10) NOT NULL DEFAULT 'warm',
    `estimated_value` decimal(12,2) DEFAULT NULL,
    `next_follow_up` date DEFAULT NULL,
    `status` varchar(20) NOT NULL DEFAULT 'new',
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `leads_lead_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE SET NULL,
    CONSTRAINT `leads_lead_user_fk` FOREIGN KEY (`assigned_to_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `leads_interaction` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `lead_id` bigint(20) NOT NULL,
    `note` longtext NOT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `leads_interaction_lead_fk` FOREIGN KEY (`lead_id`) REFERENCES `leads_lead` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_idea` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `title` varchar(200) NOT NULL,
    `description` longtext NOT NULL,
    `assumptions` longtext NOT NULL,
    `priority` int(11) NOT NULL DEFAULT 0,
    `status` varchar(20) NOT NULL DEFAULT 'draft'
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_build` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `idea_id` bigint(20) NOT NULL,
    `title` varchar(200) NOT NULL,
    `description` longtext NOT NULL,
    `link` varchar(200) NOT NULL DEFAULT '',
    `status` varchar(20) NOT NULL DEFAULT 'in_progress',
    CONSTRAINT `lean_build_idea_fk` FOREIGN KEY (`idea_id`) REFERENCES `lean_idea` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_product` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `build_id` bigint(20) NOT NULL,
    `name` varchar(200) NOT NULL,
    `description` longtext NOT NULL,
    `public` tinyint(1) NOT NULL DEFAULT 0,
    CONSTRAINT `lean_product_build_fk` FOREIGN KEY (`build_id`) REFERENCES `lean_build` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_measure` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `product_id` bigint(20) NOT NULL,
    `metric` varchar(200) NOT NULL,
    `target` double DEFAULT NULL,
    `current` double DEFAULT NULL,
    `notes` longtext NOT NULL,
    CONSTRAINT `lean_measure_product_fk` FOREIGN KEY (`product_id`) REFERENCES `lean_product` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_datapoint` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `measure_id` bigint(20) NOT NULL,
    `value` double NOT NULL,
    `measured_at` datetime(6) NOT NULL,
    `note` longtext NOT NULL,
    CONSTRAINT `lean_datapoint_measure_fk` FOREIGN KEY (`measure_id`) REFERENCES `lean_measure` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `lean_learning` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    `idea_id` bigint(20) DEFAULT NULL,
    `summary` longtext NOT NULL,
    `action` longtext NOT NULL,
    CONSTRAINT `lean_learning_idea_fk` FOREIGN KEY (`idea_id`) REFERENCES `lean_idea` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_account` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `account_type` varchar(50) NOT NULL,
    `account_name` varchar(100) NOT NULL,
    `coa` varchar(100) NOT NULL DEFAULT 'Default CoA',
    `balance_type` varchar(10) NOT NULL,
    `active` tinyint(1) NOT NULL DEFAULT 1,
    `locked` tinyint(1) NOT NULL DEFAULT 0,
    `coa_role_default` varchar(100) DEFAULT NULL,
    CONSTRAINT `ledger_account_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_closingperiod` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `period` varchar(7) NOT NULL,
    `is_closed` tinyint(1) NOT NULL DEFAULT 0,
    `closed_at` datetime(6) DEFAULT NULL,
    `closed_by` varchar(100) DEFAULT NULL,
    UNIQUE KEY `ledger_closingperiod_period_entity_uniq` (`period`, `entity_id`),
    CONSTRAINT `ledger_closingperiod_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_profitcenter` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `description` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_profitcenter_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_costcenter` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `profit_center_id` bigint(20) DEFAULT NULL,
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `description` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_costcenter_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE,
    CONSTRAINT `ledger_costcenter_pc_fk` FOREIGN KEY (`profit_center_id`) REFERENCES `ledger_profitcenter` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_project` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `description` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_project_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_activity` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `project_id` bigint(20) DEFAULT NULL,
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `description` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_activity_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE,
    CONSTRAINT `ledger_activity_project_fk` FOREIGN KEY (`project_id`) REFERENCES `ledger_project` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_location` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `address` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_location_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_partner` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `partner_type` varchar(20) NOT NULL DEFAULT 'customer',
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `email` varchar(254) DEFAULT NULL,
    `phone` varchar(50) DEFAULT NULL,
    `address` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_partner_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_product` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `product_type` varchar(20) NOT NULL DEFAULT 'goods',
    `code` varchar(50) NOT NULL,
    `name` varchar(150) NOT NULL,
    `description` longtext DEFAULT NULL,
    `is_active` tinyint(1) NOT NULL DEFAULT 1,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `ledger_product_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_journalentry` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `date` date NOT NULL,
    `description` longtext NOT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `period` varchar(7) DEFAULT NULL,
    `is_posted` tinyint(1) NOT NULL DEFAULT 0,
    CONSTRAINT `ledger_journalentry_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `ledger_journalitem` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `journal_entry_id` bigint(20) NOT NULL,
    `account_id` bigint(20) NOT NULL,
    `cost_center_id` bigint(20) DEFAULT NULL,
    `profit_center_id` bigint(20) DEFAULT NULL,
    `project_id` bigint(20) DEFAULT NULL,
    `activity_id` bigint(20) DEFAULT NULL,
    `location_id` bigint(20) DEFAULT NULL,
    `partner_id` bigint(20) DEFAULT NULL,
    `product_id` bigint(20) DEFAULT NULL,
    `debit` int(11) NOT NULL DEFAULT 0,
    `credit` int(11) NOT NULL DEFAULT 0,
    `note` varchar(255) DEFAULT NULL,
    CONSTRAINT `ledger_ji_je_fk` FOREIGN KEY (`journal_entry_id`) REFERENCES `ledger_journalentry` (`id`) ON DELETE CASCADE,
    CONSTRAINT `ledger_ji_acc_fk` FOREIGN KEY (`account_id`) REFERENCES `ledger_account` (`id`) ON DELETE RESTRICT,
    CONSTRAINT `ledger_ji_cc_fk` FOREIGN KEY (`cost_center_id`) REFERENCES `ledger_costcenter` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_pc_fk` FOREIGN KEY (`profit_center_id`) REFERENCES `ledger_profitcenter` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_prj_fk` FOREIGN KEY (`project_id`) REFERENCES `ledger_project` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_act_fk` FOREIGN KEY (`activity_id`) REFERENCES `ledger_activity` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_loc_fk` FOREIGN KEY (`location_id`) REFERENCES `ledger_location` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_prt_fk` FOREIGN KEY (`partner_id`) REFERENCES `ledger_partner` (`id`) ON DELETE SET NULL,
    CONSTRAINT `ledger_ji_prd_fk` FOREIGN KEY (`product_id`) REFERENCES `ledger_product` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_akun` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(20) DEFAULT NULL UNIQUE,
    `description` longtext DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_market` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(20) DEFAULT NULL UNIQUE,
    `description` longtext DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_digitalchannel` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(20) DEFAULT NULL UNIQUE,
    `description` longtext DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_funnelstage` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(20) DEFAULT NULL UNIQUE,
    `description` longtext DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_biaya` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(20) DEFAULT NULL UNIQUE,
    `description` longtext DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_channel` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `akun_id` bigint(20) DEFAULT NULL,
    `for_market_id` bigint(20) DEFAULT NULL,
    `channel_id` bigint(20) DEFAULT NULL,
    `funnel_stage_id` bigint(20) DEFAULT NULL,
    `kategori_biaya_id` bigint(20) DEFAULT NULL,
    `amount` decimal(12,2) DEFAULT NULL,
    `rencana_tanggal_posting` date DEFAULT NULL,
    `tanggal_posting` datetime(6) DEFAULT NULL,
    `is_posted` tinyint(1) NOT NULL DEFAULT 0,
    `jenis_konten` varchar(100) NOT NULL DEFAULT '',
    `judul` varchar(200) NOT NULL,
    `isi_konten` longtext NOT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `post_media_ch_akun_fk` FOREIGN KEY (`akun_id`) REFERENCES `post_media_akun` (`id`) ON DELETE CASCADE,
    CONSTRAINT `post_media_ch_market_fk` FOREIGN KEY (`for_market_id`) REFERENCES `post_media_market` (`id`) ON DELETE CASCADE,
    CONSTRAINT `post_media_ch_dc_fk` FOREIGN KEY (`channel_id`) REFERENCES `post_media_digitalchannel` (`id`) ON DELETE CASCADE,
    CONSTRAINT `post_media_ch_fs_fk` FOREIGN KEY (`funnel_stage_id`) REFERENCES `post_media_funnelstage` (`id`) ON DELETE CASCADE,
    CONSTRAINT `post_media_ch_biaya_fk` FOREIGN KEY (`kategori_biaya_id`) REFERENCES `post_media_biaya` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_channelperformance` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `channel_id` bigint(20) NOT NULL,
    `period` varchar(10) NOT NULL,
    `as_of_date` date NOT NULL,
    `metrics` longtext NOT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `post_media_perf_ch_fk` FOREIGN KEY (`channel_id`) REFERENCES `post_media_channel` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `post_media_idekonten` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `teks` varchar(255) NOT NULL,
    `terkirim_telegram` tinyint(1) NOT NULL DEFAULT 0,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_board` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `entity_id` char(32) DEFAULT NULL,
    `name` varchar(200) NOT NULL,
    `description` longtext NOT NULL,
    `created_by_id` int(11) DEFAULT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `kanban_board_entity_fk` FOREIGN KEY (`entity_id`) REFERENCES `entity_entity` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_board_user_fk` FOREIGN KEY (`created_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_column` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `board_id` bigint(20) NOT NULL,
    `name` varchar(100) NOT NULL,
    `position` int(10) unsigned NOT NULL DEFAULT 0,
    CONSTRAINT `kanban_col_board_fk` FOREIGN KEY (`board_id`) REFERENCES `kanban_board` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_label` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `name` varchar(50) NOT NULL,
    `color` varchar(20) NOT NULL DEFAULT '#007bff'
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_task` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `column_id` bigint(20) NOT NULL,
    `title` varchar(255) NOT NULL,
    `description` longtext NOT NULL,
    `priority` varchar(20) NOT NULL DEFAULT 'medium',
    `start_date` date DEFAULT NULL,
    `due_date` date DEFAULT NULL,
    `position` int(10) unsigned NOT NULL DEFAULT 0,
    `completed` tinyint(1) NOT NULL DEFAULT 0,
    `created_by_id` int(11) DEFAULT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `updated_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `kanban_task_col_fk` FOREIGN KEY (`column_id`) REFERENCES `kanban_column` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_task_user_fk` FOREIGN KEY (`created_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_task_assignees` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL,
    `user_id` int(11) NOT NULL,
    UNIQUE KEY `kanban_task_assignees_uniq` (`task_id`, `user_id`),
    CONSTRAINT `kanban_ta_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_ta_user_fk` FOREIGN KEY (`user_id`) REFERENCES `auth_user` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_task_labels` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL,
    `label_id` bigint(20) NOT NULL,
    UNIQUE KEY `kanban_task_labels_uniq` (`task_id`, `label_id`),
    CONSTRAINT `kanban_tl_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_tl_label_fk` FOREIGN KEY (`label_id`) REFERENCES `kanban_label` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_taskcomment` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL,
    `author_id` int(11) NOT NULL,
    `comment` longtext NOT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `kanban_tc_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_tc_user_fk` FOREIGN KEY (`author_id`) REFERENCES `auth_user` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_taskattachment` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL,
    `file` varchar(100) NOT NULL,
    `uploaded_by_id` int(11) DEFAULT NULL,
    `uploaded_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `kanban_tatt_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_tatt_user_fk` FOREIGN KEY (`uploaded_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `kanban_taskchecklistitem` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL,
    `title` varchar(255) NOT NULL,
    `is_completed` tinyint(1) NOT NULL DEFAULT 0,
    `position` int(10) unsigned NOT NULL DEFAULT 0,
    `created_by_id` int(11) DEFAULT NULL,
    `completed_by_id` int(11) DEFAULT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    `completed_at` datetime(6) DEFAULT NULL,
    CONSTRAINT `kanban_chk_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `kanban_chk_cby_fk` FOREIGN KEY (`created_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL,
    CONSTRAINT `kanban_chk_doneby_fk` FOREIGN KEY (`completed_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `gantt_taskdependency` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `predecessor_id` bigint(20) NOT NULL,
    `successor_id` bigint(20) NOT NULL,
    `created_by_id` int(11) DEFAULT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    UNIQUE KEY `gantt_dep_pred_succ_uniq` (`predecessor_id`, `successor_id`),
    CONSTRAINT `gantt_dep_pred_fk` FOREIGN KEY (`predecessor_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `gantt_dep_succ_fk` FOREIGN KEY (`successor_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `gantt_dep_user_fk` FOREIGN KEY (`created_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `gantt_milestone` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `board_id` bigint(20) NOT NULL,
    `task_id` bigint(20) DEFAULT NULL,
    `title` varchar(200) NOT NULL,
    `date` date NOT NULL,
    `is_done` tinyint(1) NOT NULL DEFAULT 0,
    `created_by_id` int(11) DEFAULT NULL,
    `created_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT `gantt_ms_board_fk` FOREIGN KEY (`board_id`) REFERENCES `kanban_board` (`id`) ON DELETE CASCADE,
    CONSTRAINT `gantt_ms_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `gantt_ms_user_fk` FOREIGN KEY (`created_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `gantt_taskbaseline` (
    `id` bigint(20) NOT NULL AUTO_INCREMENT PRIMARY KEY,
    `task_id` bigint(20) NOT NULL UNIQUE,
    `planned_start` date NOT NULL,
    `planned_end` date NOT NULL,
    `set_by_id` int(11) DEFAULT NULL,
    `set_at` datetime(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    CONSTRAINT `gantt_bl_task_fk` FOREIGN KEY (`task_id`) REFERENCES `kanban_task` (`id`) ON DELETE CASCADE,
    CONSTRAINT `gantt_bl_user_fk` FOREIGN KEY (`set_by_id`) REFERENCES `auth_user` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
