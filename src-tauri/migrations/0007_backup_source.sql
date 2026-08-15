-- 备份记录来源：manual（手动）、auto（自动）、protect（恢复前保护备份）
ALTER TABLE backup_records ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
