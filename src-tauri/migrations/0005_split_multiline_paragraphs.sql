-- 补充迁移：将 0004 迁移生成的含换行单段落，拆分为多个段落，
-- 保留原始换行结构，避免长文本在文档编辑器中挤成一段。

UPDATE pages SET content_json = (
  SELECT json_object(
    'type', 'doc',
    'content', json_group_array(
      json_object(
        'type', 'paragraph',
        'content', json_array(json_object('type', 'text', 'text', line))
      )
    )
  )
  FROM (
    WITH RECURSIVE split(rest, line, ord) AS (
      SELECT plain, '', 0
      FROM (SELECT json_extract(content_json, '$.content[0].content[0].text') AS plain)
      UNION ALL
      SELECT
        CASE WHEN instr(rest, char(10)) > 0
             THEN substr(rest, instr(rest, char(10)) + 1) ELSE '' END,
        CASE WHEN instr(rest, char(10)) > 0
             THEN substr(rest, 1, instr(rest, char(10)) - 1) ELSE rest END,
        ord + 1
      FROM split
      WHERE rest != ''
    )
    SELECT line FROM split WHERE ord > 0
  )
)
WHERE json_extract(content_json, '$.content[0].type') = 'paragraph'
  AND json_extract(content_json, '$.content[0].content[0].text') LIKE '%' || char(10) || '%';
