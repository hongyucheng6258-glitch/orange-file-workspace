-- 页面升级为富文本文档：pages 表新增 content_json（TipTap 文档 JSON）。
-- 一次性把旧版 page_blocks 按块存储的内容迁移为富文本文档。

ALTER TABLE pages ADD COLUMN content_json TEXT;

UPDATE pages SET content_json = (
  SELECT json_object(
    'type', 'doc',
    'content', json_group_array(
      CASE pb.block_type
        WHEN 'heading' THEN json_object(
          'type', 'heading', 'attrs', json_object('level', 2),
          'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
        )
        WHEN 'bullet' THEN json_object(
          'type', 'bulletList',
          'content', json_array(json_object(
            'type', 'listItem',
            'content', json_array(json_object(
              'type', 'paragraph',
              'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
            ))
          ))
        )
        WHEN 'numbered' THEN json_object(
          'type', 'orderedList',
          'content', json_array(json_object(
            'type', 'listItem',
            'content', json_array(json_object(
              'type', 'paragraph',
              'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
            ))
          ))
        )
        WHEN 'quote' THEN json_object(
          'type', 'blockquote',
          'content', json_array(json_object(
            'type', 'paragraph',
            'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
          ))
        )
        WHEN 'code' THEN json_object(
          'type', 'codeBlock',
          'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
        )
        ELSE json_object(
          'type', 'paragraph',
          'content', json_array(json_object('type', 'text', 'text', COALESCE(pb.plain_text, '')))
        )
      END
    )
  )
  FROM page_blocks pb
  WHERE pb.page_id = pages.resource_id
  ORDER BY pb.block_order ASC
)
WHERE content_json IS NULL OR content_json = '';
