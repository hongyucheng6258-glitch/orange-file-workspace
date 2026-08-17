import { useState, useCallback, useEffect } from 'react';
import { gitApi } from '../api/phase1';
import type { GitRepository, GitFileStatus } from '../types/phase1';

export const useGitStatus = (projectId: string, repoPath: string) => {
  const [repository, setRepository] = useState<GitRepository | null>(null);
  const [fileStatuses, setFileStatuses] = useState<GitFileStatus[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadGitData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // Load or register repository
      let repo = await gitApi.getRepository(projectId);
      if (!repo) {
        repo = await gitApi.registerRepository(projectId, repoPath);
      }
      setRepository(repo);

      // Load file statuses
      const statuses = await gitApi.listFileStatuses(repo.id);
      setFileStatuses(statuses);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load Git data');
      console.error('Failed to load Git data:', err);
    } finally {
      setLoading(false);
    }
  }, [projectId, repoPath]);

  const updateRepositoryStatus = useCallback(async (
    currentBranch: string,
    hasUncommitted: boolean,
    aheadCount: number,
    behindCount: number
  ) => {
    if (!repository) return;

    try {
      await gitApi.updateStatus(
        repository.id,
        currentBranch,
        hasUncommitted,
        aheadCount,
        behindCount
      );
      // Reload to get updated state
      await loadGitData();
    } catch (err) {
      console.error('Failed to update repository status:', err);
      throw err;
    }
  }, [repository, loadGitData]);

  const saveFileStatus = useCallback(async (
    filePath: string,
    status: string,
    staged: boolean
  ) => {
    if (!repository) return;

    try {
      const fileStatus = await gitApi.saveFileStatus(
        repository.id,
        filePath,
        status,
        staged
      );
      setFileStatuses(prev => {
        const existing = prev.find(f => f.file_path === filePath);
        if (existing) {
          return prev.map(f => f.file_path === filePath ? fileStatus : f);
        }
        return [...prev, fileStatus];
      });
      return fileStatus;
    } catch (err) {
      console.error('Failed to save file status:', err);
      throw err;
    }
  }, [repository]);

  const getStagedFiles = useCallback((): GitFileStatus[] => {
    return fileStatuses.filter(f => f.staged);
  }, [fileStatuses]);

  const getUnstagedFiles = useCallback((): GitFileStatus[] => {
    return fileStatuses.filter(f => !f.staged);
  }, [fileStatuses]);

  const getFilesByStatus = useCallback((status: string): GitFileStatus[] => {
    return fileStatuses.filter(f => f.status === status);
  }, [fileStatuses]);

  useEffect(() => {
    loadGitData();
  }, [loadGitData]);

  return {
    repository,
    fileStatuses,
    loading,
    error,
    updateRepositoryStatus,
    saveFileStatus,
    getStagedFiles,
    getUnstagedFiles,
    getFilesByStatus,
    reload: loadGitData
  };
};
