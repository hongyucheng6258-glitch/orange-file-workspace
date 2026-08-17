import { useState, useEffect } from 'react';
import { recentApi } from '../api/phase1';
import type { RecentItem } from '../types/phase1';
import {
  File as FileIcon,
  Folder,
  FileText,
  Boxes,
  X,
} from 'lucide-react';

interface RecentItemsListProps {
  resourceTypeFilter?: 'file' | 'folder' | 'page' | 'project';
  limit?: number;
  onItemClick?: (item: RecentItem) => void;
}

const RESOURCE_TYPE_LABELS: Record<string, string> = {
  file: '文件',
  folder: '文件夹',
  page: '页面',
  project: '项目',
};

function resourceIcon(type: string) {
  switch (type) {
    case 'folder':
      return <Folder size={18} color="var(--folder)" />;
    case 'page':
      return <FileText size={18} color="var(--code)" />;
    case 'project':
      return <Boxes size={18} color="var(--primary)" />;
    default:
      return <FileIcon size={18} color="var(--file)" />;
  }
}

function formatRelativeTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);

  if (minutes < 1) return '刚刚';
  if (minutes < 60) return `${minutes} 分钟前`;
  if (hours < 24) return `${hours} 小时前`;
  if (days < 7) return `${days} 天前`;
  return date.toLocaleDateString('zh-CN');
}

export function RecentItemsList({
  resourceTypeFilter,
  limit = 20,
  onItemClick,
}: RecentItemsListProps) {
  const [items, setItems] = useState<RecentItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string | undefined>(resourceTypeFilter);

  useEffect(() => {
    loadRecentItems();
  }, [filter, limit]);

  const loadRecentItems = async () => {
    try {
      setLoading(true);
      const data = await recentApi.getRecentItems(filter, limit);
      setItems(data);
    } catch (error) {
      console.error('Failed to load recent items:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleRemoveItem = async (itemId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await recentApi.removeItem(itemId);
      setItems(items.filter((item) => item.id !== itemId));
    } catch (error) {
      console.error('Failed to remove item:', error);
    }
  };

  const handleClearAll = async () => {
    if (!confirm('确定要清空所有最近项吗？')) return;

    try {
      await recentApi.clearAll();
      setItems([]);
    } catch (error) {
      console.error('Failed to clear items:', error);
    }
  };

  const filterTabs = [
    { key: undefined, label: '全部' },
    { key: 'file', label: '文件' },
    { key: 'folder', label: '文件夹' },
    { key: 'page', label: '页面' },
    { key: 'project', label: '项目' },
  ];

  if (loading) {
    return <div className="recent-list-loading">加载中…</div>;
  }

  return (
    <div className="recent-list-container">
      {/* Filter bar */}
      <div className="recent-list-filters">
        <div className="recent-list-tabs">
          {filterTabs.map((tab) => (
            <button
              key={tab.label}
              className={`recent-list-tab ${filter === tab.key ? 'active' : ''}`}
              onClick={() => setFilter(tab.key)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        {items.length > 0 && (
          <button className="btn-link recent-list-clear" onClick={handleClearAll}>
            清空
          </button>
        )}
      </div>

      {/* Items list */}
      <div className="recent-list-body">
        {items.length === 0 ? (
          <div className="recent-list-empty">暂无最近访问记录</div>
        ) : (
          items.map((item) => (
            <div
              key={item.id}
              className={`recent-list-item ${onItemClick ? 'clickable' : ''}`}
              onClick={() => onItemClick?.(item)}
            >
              <span className="recent-list-icon">
                {resourceIcon(item.resource_type)}
              </span>
              <div className="recent-list-info">
                <span className="recent-list-name">
                  {item.name ?? item.resource_id.slice(0, 8)}
                </span>
                <span className="recent-list-meta">
                  {RESOURCE_TYPE_LABELS[item.resource_type] ?? '文件'}
                  {' · '}访问 {item.access_count} 次
                  {' · '}{formatRelativeTime(item.last_accessed_at)}
                </span>
              </div>
              <button
                className="recent-list-remove"
                onClick={(e) => handleRemoveItem(item.id, e)}
                title="移除"
              >
                <X size={14} />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
