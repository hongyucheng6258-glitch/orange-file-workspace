/**
 * 批量操作服务 — 批量重命名 + 操作历史/撤销
 *
 * 批量重命名流程：
 *   1. 预检（dry_run）：验证新名称合法性、检测冲突
 *   2. 执行：逐个 fs::rename + db 更新，在事务中记录 before/after 快照
 *   3. 撤销：从 operation_history 读取 before_state，逆向重命名
 */
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::db::connection::now_unix;
use crate::db::models::{new_id, OperationHistory};
use crate::db::repositories;
use crate::error::AppError;
use crate::services::file_service as fsutil;

/// 单个资源的重命名条目（预览/执行用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameItem {
    pub resource_id: String,
    pub old_name: String,
    pub new_name: String,
    pub old_path: String,
    pub new_path: String,
    /// 预检状态：ok / conflict / invalid / missing
    pub status: String,
    pub error: Option<String>,
}

/// 批量重命名预览（dry-run）。
///
/// 不执行任何文件系统或数据库操作，
/// 仅验证名称合法性并检测目标路径冲突。
pub fn preview_batch_rename(
    conn: &Connection,
    items: &[(String, String)], // (resource_id, new_name)
) -> Result<Vec<RenameItem>, AppError> {
    let mut results = Vec::with_capacity(items.len());
    let mut new_names_in_batch: HashSet<String> = HashSet::new();

    for (resource_id, new_name) in items {
        let mut item = RenameItem {
            resource_id: resource_id.clone(),
            old_name: String::new(),
            new_name: new_name.clone(),
            old_path: String::new(),
            new_path: String::new(),
            status: String::new(),
            error: None,
        };

        // 取资源信息
        let resource = match repositories::get_resource(conn, resource_id) {
            Ok(Some(r)) => r,
            _ => {
                item.status = "missing".into();
                item.error = Some("资源不存在".into());
                results.push(item);
                continue;
            }
        };
        item.old_name = resource.name.clone();

        // 取 location
        let locations = repositories::list_locations(conn, resource_id)?;
        let loc = match locations.first() {
            Some(l) => l,
            None => {
                item.status = "missing".into();
                item.error = Some("无文件位置".into());
                results.push(item);
                continue;
            }
        };
        let old_path = PathBuf::from(&loc.path);
        item.old_path = loc.path.clone();

        // 名称合法性
        if new_name.is_empty()
            || new_name.contains('/')
            || new_name.contains('\\')
            || new_name.contains('\0')
        {
            item.status = "invalid".into();
            item.error = Some("名称为空或包含非法字符".into());
            results.push(item);
            continue;
        }

        if *new_name == resource.name {
            item.status = "ok".into();
            item.new_path = loc.path.clone();
            results.push(item);
            continue;
        }

        // 同批次内冲突
        let batch_key = format!(
            "{}|{}",
            old_path.parent().unwrap_or(Path::new("")).to_string_lossy(),
            new_name.to_lowercase()
        );
        if new_names_in_batch.contains(&batch_key) {
            item.status = "conflict".into();
            item.error = Some("同批次内有重名".into());
            results.push(item);
            continue;
        }
        new_names_in_batch.insert(batch_key);

        // 磁盘冲突
        let new_path = old_path.parent().unwrap_or(Path::new("")).join(new_name);
        item.new_path = new_path.to_string_lossy().to_string();

        if new_path.exists() {
            item.status = "conflict".into();
            item.error = Some("目标路径已存在".into());
        } else {
            item.status = "ok".into();
        }

        results.push(item);
    }

    Ok(results)
}

/// 执行批量重命名。
///
/// 仅处理 status == "ok" 的条目，跳过其他。
/// 成功后将 before/after 快照写入 operation_history。
pub fn execute_batch_rename(
    conn: &mut Connection,
    items: &[RenameItem],
    description: Option<&str>,
) -> Result<OperationHistory, AppError> {
    let tx = conn.transaction()?;

    let now = now_unix();
    let op_id = new_id();

    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut affected = 0i64;

    for item in items {
        if item.status != "ok" {
            continue;
        }
        if item.old_name == item.new_name {
            continue;
        }

        let old_path = PathBuf::from(&item.old_path);
        let new_path = PathBuf::from(&item.new_path);

        // 文件系统重命名
        if old_path.exists() {
            fs::rename(&old_path, &new_path).map_err(|e| {
                AppError::new(
                    "io_error",
                    format!("重命名失败 {}: {}", old_path.display(), e),
                )
            })?;
        }

        // 更新 location 路径
        let canonical = fsutil::canonical_path_key(&item.new_path);
        repositories::update_location_path(&tx, &item.resource_id, &item.new_path, &canonical)?;

        // 更新资源名称
        repositories::rename_resource(&tx, &item.resource_id, &item.new_name, now)?;

        before.push(serde_json::json!({
            "resource_id": item.resource_id,
            "old_name": item.old_name,
            "old_path": item.old_path,
        }));
        after.push(serde_json::json!({
            "resource_id": item.resource_id,
            "new_name": item.new_name,
            "new_path": item.new_path,
        }));
        affected += 1;
    }

    // 记录操作历史
    let before_json = serde_json::to_string(&before).unwrap_or_default();
    let after_json = serde_json::to_string(&after).unwrap_or_default();

    tx.execute(
        "INSERT INTO operation_history
         (id, operation_type, description, before_state, after_state, affected_count, created_at)
         VALUES (?1, 'batch_rename', ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![op_id, description, before_json, after_json, affected, now],
    )?;

    tx.commit()?;

    // 返回操作记录
    Ok(OperationHistory {
        id: op_id,
        operation_type: "batch_rename".into(),
        description: description.map(String::from),
        before_state: before_json,
        after_state: after_json,
        affected_count: affected,
        created_at: now,
        undone_at: None,
    })
}

/// 撤销操作 — 从 before_state 恢复文件名和路径。
pub fn undo_operation(conn: &mut Connection, op_id: &str) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    let now = now_unix();

    // 读取操作记录
    let (_op_type, before_state): (String, String) = tx
        .query_row(
            "SELECT operation_type, before_state FROM operation_history WHERE id = ?1",
            [op_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| AppError::from(e))?;

    if tx
        .query_row::<i64, _, _>(
            "SELECT 1 FROM operation_history WHERE id = ?1 AND undone_at IS NOT NULL",
            [op_id],
            |_| Ok(1),
        )
        .is_ok()
    {
        return Err(AppError::new("invalid_input", "该操作已被撤销"));
    }

    let before_items: Vec<serde_json::Value> =
        serde_json::from_str(&before_state).unwrap_or_default();

    let mut _undone = 0;
    for item in &before_items {
        let resource_id = item["resource_id"].as_str().unwrap_or("");
        let old_name = item["old_name"].as_str().unwrap_or("");
        let old_path = item["old_path"].as_str().unwrap_or("");

        if resource_id.is_empty() || old_name.is_empty() {
            continue;
        }

        // 取当前位置
        let locations = repositories::list_locations(&tx, resource_id)?;
        let loc = match locations.first() {
            Some(l) => l,
            None => continue,
        };
        let current_path = PathBuf::from(&loc.path);
        let restore_path = PathBuf::from(old_path);

        // 如果当前文件存在且路径不同，执行重命名
        if current_path.exists() && current_path != restore_path {
            // 确保目标目录存在
            if let Some(parent) = restore_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::rename(&current_path, &restore_path)
                .map_err(|e| AppError::new("io_error", format!("撤销重命名失败: {}", e)))?;
        }

        // 更新 location 路径
        let canonical = fsutil::canonical_path_key(old_path);
        repositories::update_location_path(&tx, resource_id, old_path, &canonical)?;

        // 恢复资源名称
        repositories::rename_resource(&tx, resource_id, old_name, now)?;
        _undone += 1;
    }

    // 标记为已撤销
    tx.execute(
        "UPDATE operation_history SET undone_at = ?2 WHERE id = ?1",
        rusqlite::params![op_id, now],
    )?;

    tx.commit()?;
    Ok(())
}

/// 列出最近的操作历史。
pub fn list_operation_history(
    conn: &Connection,
    limit: i64,
) -> SqliteResult<Vec<OperationHistory>> {
    let mut stmt = conn.prepare(
        "SELECT id, operation_type, description, before_state, after_state,
                affected_count, created_at, undone_at
         FROM operation_history
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map([limit], |row| {
        Ok(OperationHistory {
            id: row.get(0)?,
            operation_type: row.get(1)?,
            description: row.get(2)?,
            before_state: row.get(3)?,
            after_state: row.get(4)?,
            affected_count: row.get(5)?,
            created_at: row.get(6)?,
            undone_at: row.get(7)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}
