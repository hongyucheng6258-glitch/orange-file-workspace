/**
 * 重复文件检测服务
 *
 * 利用 resource_locations.content_hash (SHA-256) 分组查找重复文件。
 * 调用前应先通过 hash_resources 命令计算哈希。
 */

use std::path::PathBuf;

use rusqlite::{Connection, Result as SqliteResult};

use crate::db::models::{DuplicateEntry, DuplicateGroup};
use crate::services::hash_service;

/// 为尚未计算内容哈希的受管文件补齐 SHA-256 哈希。
///
/// 返回本次实际计算的文件数量。用于在重复检测前自动补齐哈希，
/// 避免因 content_hash 为 NULL 而漏检重复文件。
pub fn ensure_all_hashed(conn: &mut Connection) -> SqliteResult<usize> {
    // 找出所有缺哈希的受管文件位置
    let missing: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, path FROM resource_locations
             WHERE source_type = 'managed'
               AND (content_hash IS NULL OR content_hash = '')",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let mut hashed = 0;
    let tx = conn.transaction()?;
    {
        let mut update = tx.prepare(
            "UPDATE resource_locations SET content_hash = ?2, hash_algorithm = 'sha256' WHERE id = ?1",
        )?;
        for (loc_id, path) in missing {
            let p = PathBuf::from(&path);
            if !p.exists() || !p.is_file() {
                continue;
            }
            if let Ok(h) = hash_service::sha256_file(&p) {
                update.execute(rusqlite::params![loc_id, h])?;
                hashed += 1;
            }
        }
    }
    tx.commit()?;
    Ok(hashed)
}

/// 查找所有重复文件分组。
///
/// 逻辑：按 content_hash 分组，仅保留 hash 非空且出现次数 > 1 的组，
/// 每组返回该 hash 下的所有 location + 资源名称。
pub fn find_duplicates(conn: &Connection) -> SqliteResult<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash, COUNT(*) as cnt
         FROM resource_locations
         WHERE content_hash IS NOT NULL AND content_hash != ''
         GROUP BY content_hash
         HAVING cnt > 1
         ORDER BY cnt DESC",
    )?;

    let hash_rows: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    drop(stmt);

    let mut groups = Vec::with_capacity(hash_rows.len());
    for (hash, _cnt) in hash_rows {
        let entries = get_entries_for_hash(conn, &hash)?;
        if entries.len() < 2 {
            continue;
        }
        let size = entries.first().map(|e| e.size_bytes).unwrap_or(0);
        groups.push(DuplicateGroup {
            content_hash: hash,
            size_bytes: size,
            entries,
        });
    }

    Ok(groups)
}

/// 获取指定 content_hash 下的所有条目（含资源名称、路径、大小）。
fn get_entries_for_hash(conn: &Connection, hash: &str) -> SqliteResult<Vec<DuplicateEntry>> {
    let mut stmt = conn.prepare(
        "SELECT
            r.id, r.name,
            rl.path, rl.file_size, rl.source_type
         FROM resource_locations rl
         JOIN resources r ON r.id = rl.resource_id
         WHERE rl.content_hash = ?1
         ORDER BY r.name",
    )?;

    let entries = stmt
        .query_map([hash], |row| {
            Ok(DuplicateEntry {
                resource_id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                size_bytes: row.get(3).unwrap_or(0),
                source_type: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

/// 统计已计算哈希的文件数和总文件数 (hashed, total)。
pub fn hash_stats(conn: &Connection) -> SqliteResult<(i64, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM resource_locations WHERE source_type = 'managed'",
        [],
        |row| row.get(0),
    )?;
    let hashed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM resource_locations
         WHERE content_hash IS NOT NULL AND content_hash != ''",
        [],
        |row| row.get(0),
    )?;
    Ok((hashed, total))
}
