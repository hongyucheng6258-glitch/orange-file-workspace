import { useState, useCallback, useEffect } from 'react';
import { recentApi } from '../api/phase1';
import type { RecentItem } from '../types/phase1';

export const useRecentItems = (
  resourceType?: 'file' | 'folder' | 'page' | 'project',
  limit: number = 20
) => {
  const [items, setItems] = useState<RecentItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadRecentItems = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const recentItems = await recentApi.getRecentItems(resourceType, limit);
      setItems(recentItems);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load recent items');
      console.error('Failed to load recent items:', err);
    } finally {
      setLoading(false);
    }
  }, [limit, resourceType]);

  const recordAccess = useCallback(async (
    resourceId: string,
    resourceType: 'file' | 'folder' | 'page' | 'project'
  ) => {
    try {
      await recentApi.recordAccess(resourceId, resourceType);
      // Reload to get updated list with new access count
      await loadRecentItems();
    } catch (err) {
      console.error('Failed to record access:', err);
    }
  }, [loadRecentItems]);

  const removeItem = useCallback(async (itemId: string) => {
    try {
      await recentApi.removeItem(itemId);
      setItems(prev => prev.filter(item => item.id !== itemId));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to remove item');
      throw err;
    }
  }, []);

  const clearAll = useCallback(async () => {
    try {
      await recentApi.clearAll();
      setItems([]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to clear items');
      throw err;
    }
  }, []);

  const cleanupOld = useCallback(async (daysToKeep: number = 30) => {
    try {
      const deletedCount = await recentApi.cleanupOld(daysToKeep);
      await loadRecentItems();
      return deletedCount;
    } catch (err) {
      console.error('Failed to cleanup old items:', err);
      return 0;
    }
  }, [loadRecentItems]);

  useEffect(() => {
    loadRecentItems();
  }, [loadRecentItems]);

  return {
    items,
    loading,
    error,
    recordAccess,
    removeItem,
    clearAll,
    cleanupOld,
    reload: loadRecentItems
  };
};
