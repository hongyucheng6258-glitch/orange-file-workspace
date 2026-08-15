-- 清理孤立的 resource_locations 和 file_metadata 记录
-- 这些记录对应的 resources 已被删除，但 location 残留未清理，
-- 导致幂等导入逻辑错误跳过所有项目
DELETE FROM file_metadata WHERE resource_id NOT IN (SELECT id FROM resources);
DELETE FROM resource_locations WHERE resource_id NOT IN (SELECT id FROM resources);
