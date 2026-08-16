use rusqlite::{params, Connection, OptionalExtension};

use crate::db::connection::now_unix;
use crate::error::AppError;

/// 单个迁移的定义。SQL 通过 include_str! 编译进二进制，
/// 运行时不需要访问文件系统，对打包后的应用更可靠。
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("../../migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "editor_sessions_unique",
        sql: include_str!("../../migrations/0002_editor_sessions_unique.sql"),
    },
    Migration {
        version: 3,
        name: "cleanup_orphan_locations",
        sql: include_str!("../../migrations/0003_cleanup_orphan_locations.sql"),
    },
    Migration {
        version: 4,
        name: "page_document_content",
        sql: include_str!("../../migrations/0004_page_document_content.sql"),
    },
    Migration {
        version: 5,
        name: "split_multiline_paragraphs",
        sql: include_str!("../../migrations/0005_split_multiline_paragraphs.sql"),
    },
    Migration {
        version: 6,
        name: "global_search_indexes",
        sql: include_str!("../../migrations/0006_global_search.sql"),
    },
    Migration {
        version: 7,
        name: "backup_source",
        sql: include_str!("../../migrations/0007_backup_source.sql"),
    },
    Migration {
        version: 8,
        name: "project_run_history",
        sql: include_str!("../../migrations/0008_project_run_history.sql"),
    },
];

/// 应用所有未执行的迁移。每个迁移在独立事务中执行，失败即回滚。
pub fn run_migrations(conn: &mut Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );",
    )?;

    for migration in MIGRATIONS {
        let applied: Option<bool> = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [migration.version],
                |row| row.get(0),
            )
            .optional()?;

        if applied.unwrap_or(false) {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, now_unix()],
        )?;
        tx.commit()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_conn() -> Connection {
        Connection::open_in_memory().expect("open in-memory db")
    }

    #[test]
    fn applies_all_migrations() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations should apply");

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
            .expect("max version");
        assert_eq!(version, MIGRATIONS.last().expect("migrations").version);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("first run");
        run_migrations(&mut conn).expect("second run should be no-op");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let mut conn = in_memory_conn();
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
        run_migrations(&mut conn).expect("migrations");

        // 插入指向不存在父资源的记录必须失败
        let result = conn.execute(
            "INSERT INTO resources (id, kind, name, parent_id, created_at, updated_at)
             VALUES ('child', 'file', 'x', 'missing-parent', 1, 1)",
            [],
        );
        assert!(result.is_err(), "orphan resource should be rejected");
    }

    #[test]
    fn all_expected_tables_exist() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations");

        let expected = [
            "resources",
            "resource_locations",
            "file_metadata",
            "pages",
            "page_blocks",
            "resource_relations",
            "tags",
            "resource_tags",
            "projects",
            "editor_sessions",
            "tasks",
            "task_items",
            "thumbnails",
            "app_settings",
            "backup_records",
            "schema_migrations",
        ];

        for table in expected {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM sqlite_master
                        WHERE type='table' AND name=?1
                    )",
                    [table],
                    |r| r.get(0),
                )
                .expect("query sqlite_master");
            assert!(exists, "table {table} should exist");
        }
    }

    #[test]
    fn cascade_delete_cleans_dependents() {
        let mut conn = in_memory_conn();
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
        run_migrations(&mut conn).expect("migrations");

        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at)
             VALUES ('p1', 'file', 'a.txt', 1, 1)",
            [],
        )
        .expect("insert resource");
        conn.execute(
            "INSERT INTO resource_locations (id, resource_id, source_type, path, created_at)
             VALUES ('l1', 'p1', 'managed', '/x/a.txt', 1)",
            [],
        )
        .expect("insert location");
        conn.execute(
            "INSERT INTO file_metadata (resource_id, size_bytes)
             VALUES ('p1', 42)",
            [],
        )
        .expect("insert metadata");

        conn.execute("DELETE FROM resources WHERE id = 'p1'", [])
            .expect("delete resource");

        let leftover: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM resource_locations)
                      + (SELECT COUNT(*) FROM file_metadata)",
                [],
                |r| r.get(0),
            )
            .expect("count leftovers");
        assert_eq!(leftover, 0, "dependents should be cascade-deleted");
    }

    #[test]
    fn batch_rollback_on_error() {
        let mut conn = in_memory_conn();
        conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
        run_migrations(&mut conn).expect("migrations");

        // 第二条插入违反 NOT NULL，整个事务必须回滚
        let result = (|| -> rusqlite::Result<()> {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO resources (id, kind, name, created_at, updated_at)
                 VALUES ('r1', 'file', 'ok', 1, 1)",
                [],
            )?;
            tx.execute(
                "INSERT INTO resources (id, kind, created_at, updated_at)
                 VALUES ('r2', 'file', 1, 1)",
                [],
            )?;
            tx.commit()
        })();

        assert!(result.is_err(), "transaction should fail");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "partial inserts must be rolled back");
    }

    #[test]
    fn global_search_tables_exist() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations");
        for table in ["system_search_entries", "system_search_apps", "system_search_scan_state"] {
            let ok: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |r| r.get(0),
                )
                .expect("query");
            assert!(ok, "table {table} should exist");
        }
    }

    #[test]
    fn global_search_key_constraints_apply() {
        let mut conn = in_memory_conn();
        run_migrations(&mut conn).expect("migrations");
        // canonical_path UNIQUE 且大小写不敏感
        conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\Docs\\a.txt', 'a.txt', 'file', 'v1', 1, 1)",
            [],
        )
        .expect("insert");
        let dup = conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('C:\\DOCS\\A.TXT', 'a.txt', 'file', 'v1', 1, 1)",
            [],
        );
        assert!(dup.is_err(), "大小写不同的同一路径应被 UNIQUE COLLATE NOCASE 拒绝");
        // CHECK 约束拒绝非法类型与状态
        let bad_kind = conn.execute(
            "INSERT INTO system_search_entries
             (canonical_path, display_name, entry_kind, volume_id, scan_generation, indexed_at)
             VALUES ('c:\\x\\b.txt', 'b.txt', 'link', 'v1', 1, 1)",
            [],
        );
        assert!(bad_kind.is_err(), "非法 entry_kind 应被 CHECK 拒绝");
        let bad_status = conn.execute(
            "INSERT INTO system_search_scan_state (volume_id, root_path, status) VALUES ('v1', 'C:\\', 'weird')",
            [],
        );
        assert!(bad_status.is_err(), "非法 status 应被 CHECK 拒绝");
    }
}
