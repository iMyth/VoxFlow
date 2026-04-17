//! Database schema management and migrations.

use rusqlite::Connection;

use super::super::error::AppError;

/// Check whether a column already exists on a table via `PRAGMA table_info`.
pub fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let sql = format!("PRAGMA table_info({})", table);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| AppError::Database(e.to_string()))?
        .any(|name| name.map(|n| n == column).unwrap_or(false));
    Ok(exists)
}

/// Add a column to a table only if it does not already exist.
pub fn add_column_if_not_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    col_def: &str,
) -> Result<(), AppError> {
    if !has_column(conn, table, column)? {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_def);
        conn.execute_batch(&sql)
            .map_err(|e| AppError::Database(e.to_string()))?;
    }
    Ok(())
}

/// Run schema migrations.
/// Uses a `schema_migrations` table to track applied migrations, so upgrades
/// never re-run old steps and users don't need to manually edit SQLite.
pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    // Ensure schema_migrations table exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)",
        [],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Get current version
    let current_version: i32 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Define migrations: (version, sql)
    let migrations: &[(i32, &str)] = &[
        (
            1,
            "
            CREATE TABLE IF NOT EXISTS projects (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS characters (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name        TEXT NOT NULL,
                tts_model   TEXT NOT NULL DEFAULT 'qwen3-tts-flash',
                voice_name  TEXT NOT NULL,
                speed       REAL NOT NULL DEFAULT 1.0,
                pitch       REAL NOT NULL DEFAULT 1.0,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS script_lines (
                id            TEXT PRIMARY KEY,
                project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                line_order    INTEGER NOT NULL,
                text          TEXT NOT NULL,
                character_id  TEXT REFERENCES characters(id) ON DELETE SET NULL,
                gap_after_ms  INTEGER NOT NULL DEFAULT 500,
                updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS audio_fragments (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                line_id     TEXT NOT NULL REFERENCES script_lines(id) ON DELETE CASCADE,
                file_path   TEXT NOT NULL,
                duration_ms INTEGER,
                source      TEXT NOT NULL DEFAULT 'tts',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS bgm_files (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                file_path   TEXT NOT NULL,
                name        TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS user_settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        ),
        // Migration 2–6: see programmatic handling below
    ];

    for (version, sql) in migrations {
        if *version <= current_version {
            continue;
        }

        conn.execute_batch(sql)
            .map_err(|e| AppError::Database(format!("Migration {} failed: {}", version, e)))?;

        conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            rusqlite::params![version],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // Programmatic migrations that require column-existence checks
    // to avoid "duplicate column" errors on fresh installs where
    // CREATE TABLE already includes these columns.

    let alter_migrations: &[(i32, &str, &[(&str, &str, &str)])] = &[
        // (version, extra_sql_before, &[(table, column, definition)])
        (2, "", &[("projects", "outline", "TEXT NOT NULL DEFAULT ''")]),
        (3, "", &[("script_lines", "instructions", "TEXT NOT NULL DEFAULT ''")]),
        (
            4,
            "CREATE TABLE IF NOT EXISTS script_sections (
                id            TEXT PRIMARY KEY,
                project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                title         TEXT NOT NULL,
                section_order INTEGER NOT NULL
            );",
            &[("script_lines", "section_id", "TEXT REFERENCES script_sections(id) ON DELETE SET NULL")],
        ),
        (
            5,
            "CREATE TABLE IF NOT EXISTS story_kb (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                text        TEXT NOT NULL,
                embedding   TEXT NOT NULL,
                kb_type     TEXT NOT NULL DEFAULT 'plot',
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_story_kb_project ON story_kb(project_id);
            CREATE INDEX IF NOT EXISTS idx_story_kb_type ON story_kb(kb_type);",
            &[],
        ),
        (6, "", &[("audio_fragments", "source", "TEXT NOT NULL DEFAULT 'tts'")]),
    ];

    for (version, extra_sql, columns) in alter_migrations {
        if *version <= current_version {
            continue;
        }

        if !extra_sql.is_empty() {
            conn.execute_batch(extra_sql)
                .map_err(|e| AppError::Database(format!("Migration {} failed: {}", version, e)))?;
        }

        for (table, column, col_def) in *columns {
            add_column_if_not_exists(conn, table, column, col_def).map_err(|e| {
                AppError::Database(format!("Migration {} failed adding {}.{}: {}", version, table, column, e))
            })?;
        }

        conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            rusqlite::params![version],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(())
}
