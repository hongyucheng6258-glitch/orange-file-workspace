import { useState, useCallback, useEffect } from 'react';
import { workspaceSessionsApi } from '../api/phase1';
import type { WorkspaceSession } from '../types/phase1';

export const useWorkspaceSession = (projectId: string) => {
  const [session, setSession] = useState<WorkspaceSession | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadSession = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const existingSession = await workspaceSessionsApi.get(projectId);
      setSession(existingSession);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load session');
      console.error('Failed to load workspace session:', err);
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  const saveSession = useCallback(async (data: {
    openFilesJson?: string;
    activeFileId?: string;
    terminalTabsJson?: string;
    activeTerminalIndex?: number;
    runningTasksJson?: string;
    panelLayoutJson?: string;
    scrollPositionsJson?: string;
  }) => {
    try {
      const savedSession = await workspaceSessionsApi.save(
        projectId,
        data.openFilesJson,
        data.activeFileId,
        data.terminalTabsJson,
        data.activeTerminalIndex,
        data.runningTasksJson,
        data.panelLayoutJson,
        data.scrollPositionsJson
      );
      setSession(savedSession);
      return savedSession;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save session');
      throw err;
    }
  }, [projectId]);

  const updateLastAccess = useCallback(async () => {
    try {
      await workspaceSessionsApi.markRestored(projectId);
    } catch (err) {
      console.error('Failed to update last access:', err);
    }
  }, [projectId]);

  const deleteSession = useCallback(async () => {
    try {
      await workspaceSessionsApi.delete(projectId);
      setSession(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete session');
      throw err;
    }
  }, [projectId]);

  const autoSave = useCallback((data: {
    openFilesJson?: string;
    activeFileId?: string;
    terminalTabsJson?: string;
    activeTerminalIndex?: number;
    runningTasksJson?: string;
    panelLayoutJson?: string;
    scrollPositionsJson?: string;
  }) => {
    // Debounced auto-save - in production, wrap this with a debounce utility
    saveSession(data).catch(err => {
      console.error('Auto-save failed:', err);
    });
  }, [saveSession]);

  useEffect(() => {
    loadSession();
  }, [loadSession]);

  return {
    session,
    loading,
    error,
    saveSession,
    updateLastAccess,
    deleteSession,
    autoSave,
    reload: loadSession
  };
};
