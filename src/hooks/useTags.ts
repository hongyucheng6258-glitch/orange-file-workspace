import { useState, useCallback, useEffect } from 'react';
import { tagsApi } from '../api/phase1';
import type { Tag } from '../types/phase1';

export const useTags = (resourceId?: string, resourceType?: string) => {
  const [tags, setTags] = useState<Tag[]>([]);
  const [resourceTags, setResourceTags] = useState<Tag[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadTags = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const allTags = await tagsApi.listTags();
      setTags(allTags);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load tags');
      console.error('Failed to load tags:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadResourceTags = useCallback(async () => {
    if (!resourceId) return;
    
    try {
      const tagsForResource = await tagsApi.getResourceTags(resourceId);
      setResourceTags(tagsForResource);
    } catch (err) {
      console.error('Failed to load resource tags:', err);
    }
  }, [resourceId]);

  const createTag = useCallback(async (name: string, color?: string) => {
    try {
      const newTag = await tagsApi.createTag(name, color);
      setTags(prev => [...prev, newTag]);
      return newTag;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create tag');
      throw err;
    }
  }, []);

  const updateTag = useCallback(async (tagId: string, name?: string, color?: string) => {
    try {
      await tagsApi.updateTag(tagId, name, color);
      setTags(prev => prev.map(t => {
        if (t.id !== tagId) return t;
        return { ...t, name: name ?? t.name, color: color ?? t.color };
      }));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update tag');
      throw err;
    }
  }, []);

  const deleteTag = useCallback(async (tagId: string) => {
    try {
      await tagsApi.deleteTag(tagId);
      setTags(prev => prev.filter(t => t.id !== tagId));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete tag');
      throw err;
    }
  }, []);

  const attachTag = useCallback(async (tagId: string, resId: string, resType: string) => {
    try {
      await tagsApi.addTagToResource(tagId, resId);
      if (resId === resourceId && resType === resourceType) {
        const tag = tags.find(t => t.id === tagId);
        if (tag) {
          setResourceTags(prev => prev.some(t => t.id === tagId) ? prev : [...prev, tag]);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to attach tag');
      throw err;
    }
  }, [resourceId, resourceType, tags]);

  const detachTag = useCallback(async (tagId: string, resId: string, resType: string) => {
    try {
      await tagsApi.removeTagFromResource(tagId, resId);
      if (resId === resourceId && resType === resourceType) {
        setResourceTags(prev => prev.filter(t => t.id !== tagId));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to detach tag');
      throw err;
    }
  }, [resourceId, resourceType]);

  const toggleTag = useCallback(async (tagId: string, resId: string, resType: string) => {
    const isAttached = resourceTags.some(t => t.id === tagId);
    if (isAttached) {
      await detachTag(tagId, resId, resType);
    } else {
      await attachTag(tagId, resId, resType);
    }
  }, [resourceTags, attachTag, detachTag]);

  const isTagAttached = useCallback((tagId: string): boolean => {
    return resourceTags.some(t => t.id === tagId);
  }, [resourceTags]);

  useEffect(() => {
    loadTags();
  }, [loadTags]);

  useEffect(() => {
    loadResourceTags();
  }, [loadResourceTags]);

  return {
    tags,
    resourceTags,
    loading,
    error,
    createTag,
    updateTag,
    deleteTag,
    attachTag,
    detachTag,
    toggleTag,
    isTagAttached,
    reload: loadTags
  };
};
