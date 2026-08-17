import { useState, useEffect } from 'react';
import { tagsApi } from '../api/phase1';
import type { Tag } from '../types/phase1';

interface TagManagerProps {
  resourceId: string;
  onClose?: () => void;
  embedded?: boolean;
}

const TAG_COLORS = [
  '#C4612F', // terracotta
  '#2E7D32', // green
  '#1976D2', // blue
  '#D32F2F', // red
  '#7B1FA2', // purple
  '#F57C00', // orange
  '#0097A7', // cyan
  '#5D4037', // brown
];

export function TagManager({ resourceId, onClose, embedded }: TagManagerProps) {
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [resourceTags, setResourceTags] = useState<Tag[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  const [newTagName, setNewTagName] = useState('');
  const [selectedColor, setSelectedColor] = useState(TAG_COLORS[0]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadTags();
  }, [resourceId]);

  const loadTags = async () => {
    try {
      setLoading(true);
      const [all, resource] = await Promise.all([
        tagsApi.listTags(),
        tagsApi.getResourceTags(resourceId),
      ]);
      setAllTags(all);
      setResourceTags(resource);
    } catch (error) {
      console.error('Failed to load tags:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleCreateTag = async () => {
    if (!newTagName.trim()) return;

    try {
      const newTag = await tagsApi.createTag(newTagName.trim(), selectedColor);
      await tagsApi.addTagToResource(newTag.id, resourceId);
      setAllTags([...allTags, newTag]);
      setResourceTags([...resourceTags, newTag]);
      setNewTagName('');
      setIsCreating(false);
    } catch (error) {
      console.error('Failed to create tag:', error);
    }
  };

  const handleToggleTag = async (tag: Tag) => {
    const isAttached = resourceTags.some((t) => t.id === tag.id);

    try {
      if (isAttached) {
        await tagsApi.removeTagFromResource(tag.id, resourceId);
        setResourceTags(resourceTags.filter((t) => t.id !== tag.id));
      } else {
        await tagsApi.addTagToResource(tag.id, resourceId);
        setResourceTags([...resourceTags, tag]);
      }
    } catch (error) {
      console.error('Failed to toggle tag:', error);
    }
  };

  const handleDeleteTag = async (tagId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm('确定要删除这个标签吗？')) return;

    try {
      await tagsApi.deleteTag(tagId);
      setAllTags(allTags.filter((t) => t.id !== tagId));
      setResourceTags(resourceTags.filter((t) => t.id !== tagId));
    } catch (error) {
      console.error('Failed to delete tag:', error);
    }
  };

  if (loading) {
    return <div className="tag-mgr-loading">加载中...</div>;
  }

  return (
    <div className={`tag-mgr${embedded ? ' embedded' : ''}`}>
      {/* Header */}
      <div className="tag-mgr-head">
        <h3 className="tag-mgr-title">管理标签</h3>
        {onClose && (
          <button className="tag-mgr-close icon-btn" onClick={onClose}>
            ×
          </button>
        )}
      </div>

      {/* Current Tags */}
      {resourceTags.length > 0 && (
        <div className="tag-mgr-section">
          <div className="tag-mgr-label">已添加的标签</div>
          <div className="tag-chip-list">
            {resourceTags.map((tag) => (
              <span
                key={tag.id}
                className="tag-chip"
                style={{ backgroundColor: tag.color || TAG_COLORS[0] }}
              >
                {tag.name}
                <button
                  className="tag-chip-remove"
                  onClick={() => handleToggleTag(tag)}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Available Tags */}
      <div className="tag-mgr-list">
        <div className="tag-mgr-label">所有标签</div>
        {allTags.map((tag) => {
          const isAttached = resourceTags.some((t) => t.id === tag.id);
          return (
            <div
              key={tag.id}
              className={`tag-mgr-row${isAttached ? ' attached' : ''}`}
              onClick={() => handleToggleTag(tag)}
            >
              <div className="tag-mgr-row-body">
                <span
                  className="tag-mgr-dot"
                  style={{ backgroundColor: tag.color || TAG_COLORS[0] }}
                />
                <span className="tag-mgr-name">{tag.name}</span>
                {tag.resource_count !== undefined && (
                  <span className="tag-mgr-count">({tag.resource_count})</span>
                )}
              </div>
              <button
                className="tag-mgr-delete"
                onClick={(e) => handleDeleteTag(tag.id, e)}
              >
                🗑
              </button>
            </div>
          );
        })}
      </div>

      {/* Create New Tag */}
      <div className="tag-mgr-footer">
        {!isCreating ? (
          <button
            className="tag-mgr-create-btn"
            onClick={() => setIsCreating(true)}
          >
            + 创建新标签
          </button>
        ) : (
          <div className="tag-mgr-form">
            <input
              type="text"
              className="tag-mgr-input"
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleCreateTag()}
              placeholder="标签名称"
              autoFocus
            />
            <div className="tag-color-picker">
              {TAG_COLORS.map((color) => (
                <div
                  key={color}
                  className={`tag-color-swatch${
                    selectedColor === color ? ' selected' : ''
                  }`}
                  onClick={() => setSelectedColor(color)}
                  style={{ backgroundColor: color }}
                />
              ))}
            </div>
            <div className="tag-mgr-actions">
              <button
                className="btn btn-primary"
                onClick={handleCreateTag}
                disabled={!newTagName.trim()}
              >
                创建
              </button>
              <button
                className="btn btn-ghost"
                onClick={() => {
                  setIsCreating(false);
                  setNewTagName('');
                }}
              >
                取消
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
