import { useState, useEffect } from 'react';
import { recentApi } from '../api/phase1';
import type { RecentItem } from '../types/phase1';

interface RecentItemsListProps {
  resourceTypeFilter?: 'file' | 'folder' | 'page' | 'project';
  limit?: number;
  onItemClick?: (item: RecentItem) => void;
}

const RESOURCE_TYPE_ICONS: Record<string, string> = {
  file: '📄',
  folder: '📁',
  page: '📝',
  project: '📦',
};

const RESOURCE_TYPE_LABELS: Record<string, string> = {
  file: '文件',
  folder: '文件夹',
  page: '页面',
  project: '项目',
};

export function RecentItemsList({
  resourceTypeFilter,
  limit = 20,
  onItemClick,
}: RecentItemsListProps) {
  const [items, setItems] = useState<RecentItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadRecentItems();
  }, [resourceTypeFilter, limit]);

  const loadRecentItems = async () => {
    try {
      setLoading(true);
      const data = await recentApi.getRecentItems(resourceTypeFilter, limit);
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

  const formatTime = (timestamp: number) => {
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
  };

  if (loading) {
    return (
      <div
        style={{
          padding: '32px',
          textAlign: 'center',
          color: '#5C635D',
          fontFamily: 'Inter, sans-serif',
          fontWeight: 300,
        }}
      >
        加载中...
      </div>
    );
  }

  return (
    <div
      style={{
        backgroundColor: '#FBF9F5',
        borderRadius: '12px',
        border: '1px solid #E7E1D7',
        overflow: 'hidden',
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: '16px 20px',
          borderBottom: '1px solid #E7E1D7',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <h3
          style={{
            margin: 0,
            fontSize: '16px',
            fontWeight: 500,
            color: '#1F2421',
            fontFamily: 'Inter, sans-serif',
          }}
        >
          最近访问
        </h3>
        {items.length > 0 && (
          <button
            onClick={handleClearAll}
            style={{
              padding: '6px 12px',
              borderRadius: '999px',
              border: '1px solid #E7E1D7',
              backgroundColor: 'transparent',
              color: '#5C635D',
              fontSize: '12px',
              fontFamily: 'Inter, sans-serif',
              fontWeight: 400,
              cursor: 'pointer',
              transition: 'all 0.15s ease',
            }}
          >
            清空
          </button>
        )}
      </div>

      {/* Items List */}
      <div style={{ maxHeight: '500px', overflowY: 'auto' }}>
        {items.length === 0 ? (
          <div
            style={{
              padding: '48px 20px',
              textAlign: 'center',
              color: '#5C635D',
              fontSize: '14px',
              fontFamily: 'Inter, sans-serif',
              fontWeight: 300,
            }}
          >
            暂无最近访问记录
          </div>
        ) : (
          items.map((item) => (
            <div
              key={item.id}
              onClick={() => onItemClick?.(item)}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '12px 20px',
                borderBottom: '1px solid #E7E1D7',
                cursor: onItemClick ? 'pointer' : 'default',
                transition: 'background-color 0.15s ease',
                backgroundColor: 'transparent',
              }}
              onMouseEnter={(e) => {
                if (onItemClick) {
                  e.currentTarget.style.backgroundColor = '#F2E3D6';
                }
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = 'transparent';
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px', flex: 1 }}>
                <span style={{ fontSize: '24px' }}>
                  {RESOURCE_TYPE_ICONS[item.resource_type] || '📄'}
                </span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: '14px',
                      fontWeight: 400,
                      color: '#1F2421',
                      fontFamily: 'Inter, sans-serif',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    资源 {item.resource_id.slice(0, 8)}
                  </div>
                  <div
                    style={{
                      fontSize: '12px',
                      color: '#5C635D',
                      marginTop: '2px',
                      fontFamily: 'Inter, sans-serif',
                      fontWeight: 300,
                    }}
                  >
                    {RESOURCE_TYPE_LABELS[item.resource_type]} · 访问 {item.access_count}{' '}
                    次 · {formatTime(item.last_accessed_at)}
                  </div>
                </div>
              </div>
              <button
                onClick={(e) => handleRemoveItem(item.id, e)}
                style={{
                  background: 'none',
                  border: 'none',
                  color: '#5C635D',
                  cursor: 'pointer',
                  padding: '4px',
                  fontSize: '18px',
                  opacity: 0.6,
                  transition: 'opacity 0.15s ease',
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.opacity = '1';
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.opacity = '0.6';
                }}
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
