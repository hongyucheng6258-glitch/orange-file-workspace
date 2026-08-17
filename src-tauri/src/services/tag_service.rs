/// 标签服务：标签 CRUD、资源关联、标签筛选
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTag {
    pub resource_id: String,
    pub tag_id: String,
    pub created_at: i64,
}

/// 创建标签
pub fn create_tag(conn: &Connection, name: &str, color: Option<&str>) -> Result<Tag, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    
    conn.execute(
        "INSERT INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, color, now],
    )?;
    
    Ok(Tag {
        id,
        name: name.to_string(),
        color: color.map(String::from),
        created_at: now,
        resource_count: Some(0),
    })
}

/// 获取所有标签（带资源计数）
pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.color, t.created_at, COUNT(rt.resource_id) as resource_count
         FROM tags t
         LEFT JOIN resource_tags rt ON t.id = rt.tag_id
         GROUP BY t.id
         ORDER BY t.name COLLATE NOCASE"
    )?;
    
    let tags = stmt.query_map([], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            created_at: row.get(3)?,
            resource_count: Some(row.get(4)?),
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;
    
    Ok(tags)
}

/// 根据 ID 获取标签
pub fn get_tag(conn: &Connection, tag_id: &str) -> Result<Option<Tag>, AppError> {
    let tag = conn.query_row(
        "SELECT t.id, t.name, t.color, t.created_at, COUNT(rt.resource_id) as resource_count
         FROM tags t
         LEFT JOIN resource_tags rt ON t.id = rt.tag_id
         WHERE t.id = ?1
         GROUP BY t.id",
        params![tag_id],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
                resource_count: Some(row.get(4)?),
            })
        }
    ).optional()?;
    
    Ok(tag)
}

/// 更新标签
pub fn update_tag(
    conn: &Connection,
    tag_id: &str,
    name: Option<&str>,
    color: Option<Option<&str>>,
) -> Result<(), AppError> {
    if let Some(new_name) = name {
        conn.execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![new_name, tag_id],
        )?;
    }
    
    if let Some(new_color) = color {
        conn.execute(
            "UPDATE tags SET color = ?1 WHERE id = ?2",
            params![new_color, tag_id],
        )?;
    }
    
    Ok(())
}

/// 删除标签（级联删除关联）
pub fn delete_tag(conn: &Connection, tag_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![tag_id])?;
    Ok(())
}

/// 为资源添加标签
pub fn add_tag_to_resource(
    conn: &Connection,
    resource_id: &str,
    tag_id: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    
    conn.execute(
        "INSERT OR IGNORE INTO resource_tags (resource_id, tag_id, created_at) 
         VALUES (?1, ?2, ?3)",
        params![resource_id, tag_id, now],
    )?;
    
    Ok(())
}

/// 从资源移除标签
pub fn remove_tag_from_resource(
    conn: &Connection,
    resource_id: &str,
    tag_id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM resource_tags WHERE resource_id = ?1 AND tag_id = ?2",
        params![resource_id, tag_id],
    )?;
    
    Ok(())
}

/// 获取资源的所有标签
pub fn get_resource_tags(conn: &Connection, resource_id: &str) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.color, t.created_at
         FROM tags t
         INNER JOIN resource_tags rt ON t.id = rt.tag_id
         WHERE rt.resource_id = ?1
         ORDER BY t.name COLLATE NOCASE"
    )?;
    
    let tags = stmt.query_map(params![resource_id], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            created_at: row.get(3)?,
            resource_count: None,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;
    
    Ok(tags)
}

/// 按标签查询资源 ID
pub fn get_resources_by_tag(conn: &Connection, tag_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT resource_id FROM resource_tags WHERE tag_id = ?1"
    )?;
    
    let ids = stmt.query_map(params![tag_id], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;
    use crate::db::migrations::run_migrations;

    fn setup_db() -> Connection {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    #[test]
    fn test_create_and_list_tags() {
        let conn = setup_db();
        
        let tag1 = create_tag(&conn, "工作", Some("#FF5733")).unwrap();
        let tag2 = create_tag(&conn, "个人", Some("#33C3FF")).unwrap();
        
        assert_eq!(tag1.name, "工作");
        assert_eq!(tag1.color, Some("#FF5733".to_string()));
        assert_eq!(tag1.resource_count, Some(0));
        
        let tags = list_tags(&conn).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().any(|t| t.name == "工作"));
    }

    #[test]
    fn test_tag_resource_association() {
        let conn = setup_db();
        
        // 创建测试资源
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at) 
             VALUES ('res1', 'file', 'test.txt', 0, 0)",
            [],
        ).unwrap();
        
        let tag = create_tag(&conn, "重要", None).unwrap();
        
        add_tag_to_resource(&conn, "res1", &tag.id).unwrap();
        
        let tags = get_resource_tags(&conn, "res1").unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "重要");
        
        let resources = get_resources_by_tag(&conn, &tag.id).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0], "res1");
    }

    #[test]
    fn test_remove_tag_from_resource() {
        let conn = setup_db();
        
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at) 
             VALUES ('res1', 'file', 'test.txt', 0, 0)",
            [],
        ).unwrap();
        
        let tag = create_tag(&conn, "临时", None).unwrap();
        add_tag_to_resource(&conn, "res1", &tag.id).unwrap();
        
        let tags_before = get_resource_tags(&conn, "res1").unwrap();
        assert_eq!(tags_before.len(), 1);
        
        remove_tag_from_resource(&conn, "res1", &tag.id).unwrap();
        
        let tags_after = get_resource_tags(&conn, "res1").unwrap();
        assert_eq!(tags_after.len(), 0);
    }

    #[test]
    fn test_delete_tag_cascades() {
        let conn = setup_db();
        
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at) 
             VALUES ('res1', 'file', 'test.txt', 0, 0)",
            [],
        ).unwrap();
        
        let tag = create_tag(&conn, "删除测试", None).unwrap();
        add_tag_to_resource(&conn, "res1", &tag.id).unwrap();
        
        delete_tag(&conn, &tag.id).unwrap();
        
        let tags = get_resource_tags(&conn, "res1").unwrap();
        assert_eq!(tags.len(), 0);
    }
}
