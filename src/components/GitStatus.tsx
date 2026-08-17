import React, { useState, useEffect, useCallback } from 'react';
import { gitApi } from '../api/phase1';
import type { GitRepository, GitFileStatus } from '../types/phase1';

interface GitStatusProps {
  projectId: string;
  repoPath: string;
  onRefresh?: () => void;
}

type FileStatusFilter = 'all' | 'untracked' | 'modified' | 'added' | 'deleted' | 'renamed' | 'conflicted';

export const GitStatus: React.FC<GitStatusProps> = ({ projectId, repoPath, onRefresh }) => {
  const [repository, setRepository] = useState<GitRepository | null>(null);
  const [fileStatuses, setFileStatuses] = useState<GitFileStatus[]>([]);
  const [filter, setFilter] = useState<FileStatusFilter>('all');
  const [loading, setLoading] = useState(true);
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
      
      // Update repository status
      await gitApi.updateStatus(
        repo.id,
        repo.current_branch || 'main',
        repo.has_uncommitted,
        repo.ahead_count,
        repo.behind_count
      );
      
      // Reload after update
      repo = await gitApi.getRepository(projectId);
      setRepository(repo);

      // Load file statuses
      const statuses = await gitApi.listFileStatuses(repo!.id);
      setFileStatuses(statuses);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load Git data');
      console.error('Git data load error:', err);
    } finally {
      setLoading(false);
    }
  }, [projectId, repoPath]);

  useEffect(() => {
    loadGitData();
  }, [loadGitData]);

  const handleRefresh = useCallback(async () => {
    await loadGitData();
    onRefresh?.();
  }, [loadGitData, onRefresh]);

  const getFilteredFiles = useCallback((staged: boolean): GitFileStatus[] => {
    let filtered = fileStatuses.filter(f => f.staged === staged);
    
    if (filter !== 'all') {
      filtered = filtered.filter(f => f.status === filter);
    }
    
    return filtered;
  }, [fileStatuses, filter]);

  const stagedFiles = getFilteredFiles(true);
  const unstagedFiles = getFilteredFiles(false);

  const getStatusIcon = (status: string): string => {
    const icons: Record<string, string> = {
      untracked: '?',
      modified: 'M',
      added: 'A',
      deleted: 'D',
      renamed: 'R',
      conflicted: 'C'
    };
    return icons[status] || '•';
  };

  const getStatusColor = (status: string): string => {
    const colors: Record<string, string> = {
      untracked: '#5C635D',
      modified: '#C4612F',
      added: '#2E7D32',
      deleted: '#D32F2F',
      renamed: '#1976D2',
      conflicted: '#7B1FA2'
    };
    return colors[status] || '#1F2421';
  };

  if (loading) {
    return (
      <div style={{
        padding: '2rem',
        fontFamily: 'Inter, sans-serif',
        fontSize: '0.875rem',
        color: '#5C635D'
      }}>
        Loading Git status...
      </div>
    );
  }

  if (error) {
    return (
      <div style={{
        padding: '2rem',
        fontFamily: 'Inter, sans-serif',
        fontSize: '0.875rem',
        color: '#D32F2F'
      }}>
        Error: {error}
      </div>
    );
  }

  if (!repository) {
    return (
      <div style={{
        padding: '2rem',
        fontFamily: 'Inter, sans-serif',
        fontSize: '0.875rem',
        color: '#5C635D'
      }}>
        No Git repository found
      </div>
    );
  }

  return (
    <div style={{
      padding: '1.5rem',
      fontFamily: 'Inter, sans-serif',
      backgroundColor: '#FBF9F5',
      borderRadius: '8px',
      border: '1px solid #E7E1D7'
    }}>
      {/* Repository header */}
      <div style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        marginBottom: '1.5rem'
      }}>
        <div>
          <div style={{
            fontSize: '1rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '0.25rem'
          }}>
            {repository.current_branch || 'No branch'}
          </div>
          <div style={{
            fontSize: '0.75rem',
            color: '#5C635D',
            display: 'flex',
            gap: '1rem'
          }}>
            {repository.ahead_count > 0 && (
              <span>↑ {repository.ahead_count} ahead</span>
            )}
            {repository.behind_count > 0 && (
              <span>↓ {repository.behind_count} behind</span>
            )}
            {repository.has_uncommitted && (
              <span style={{ color: '#C4612F' }}>● uncommitted</span>
            )}
          </div>
        </div>
        <button
          onClick={handleRefresh}
          style={{
            padding: '0.5rem 1rem',
            backgroundColor: '#FFFFFF',
            border: '1px solid #E7E1D7',
            borderRadius: '999px',
            fontSize: '0.75rem',
            fontWeight: 400,
            color: '#1F2421',
            cursor: 'pointer',
            transition: 'all 0.15s ease'
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = 'translateY(-1px)';
            e.currentTarget.style.boxShadow = '0 2px 8px rgba(0,0,0,0.08)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'translateY(0)';
            e.currentTarget.style.boxShadow = 'none';
          }}
        >
          Refresh
        </button>
      </div>

      {/* Filter pills */}
      <div style={{
        display: 'flex',
        gap: '0.5rem',
        marginBottom: '1.5rem',
        flexWrap: 'wrap'
      }}>
        {(['all', 'untracked', 'modified', 'added', 'deleted', 'renamed', 'conflicted'] as FileStatusFilter[]).map(f => (
          <button
            key={f}
            onClick={() => setFilter(f)}
            style={{
              padding: '0.375rem 0.75rem',
              backgroundColor: filter === f ? '#F2E3D6' : '#FFFFFF',
              border: `1px solid ${filter === f ? '#C4612F' : '#E7E1D7'}`,
              borderRadius: '999px',
              fontSize: '0.75rem',
              fontWeight: filter === f ? 500 : 400,
              color: filter === f ? '#C4612F' : '#5C635D',
              cursor: 'pointer',
              transition: 'all 0.15s ease',
              textTransform: 'capitalize'
            }}
            onMouseEnter={(e) => {
              if (filter !== f) {
                e.currentTarget.style.borderColor = '#C4612F';
                e.currentTarget.style.color = '#C4612F';
              }
            }}
            onMouseLeave={(e) => {
              if (filter !== f) {
                e.currentTarget.style.borderColor = '#E7E1D7';
                e.currentTarget.style.color = '#5C635D';
              }
            }}
          >
            {f}
          </button>
        ))}
      </div>

      {/* Staged files */}
      {stagedFiles.length > 0 && (
        <div style={{ marginBottom: '1.5rem' }}>
          <div style={{
            fontSize: '0.875rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '0.75rem'
          }}>
            Staged ({stagedFiles.length})
          </div>
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '0.25rem'
          }}>
            {stagedFiles.map(file => (
              <div
                key={file.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.75rem',
                  padding: '0.5rem 0.75rem',
                  backgroundColor: '#FFFFFF',
                  borderRadius: '4px',
                  fontSize: '0.8125rem'
                }}
              >
                <span
                  style={{
                    fontFamily: 'monospace',
                    fontSize: '0.75rem',
                    fontWeight: 600,
                    color: getStatusColor(file.status),
                    minWidth: '1.25rem',
                    textAlign: 'center'
                  }}
                >
                  {getStatusIcon(file.status)}
                </span>
                <span style={{
                  flex: 1,
                  color: '#1F2421',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap'
                }}>
                  {file.file_path}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Unstaged files */}
      {unstagedFiles.length > 0 && (
        <div>
          <div style={{
            fontSize: '0.875rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '0.75rem'
          }}>
            Unstaged ({unstagedFiles.length})
          </div>
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '0.25rem'
          }}>
            {unstagedFiles.map(file => (
              <div
                key={file.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.75rem',
                  padding: '0.5rem 0.75rem',
                  backgroundColor: '#FFFFFF',
                  borderRadius: '4px',
                  fontSize: '0.8125rem'
                }}
              >
                <span
                  style={{
                    fontFamily: 'monospace',
                    fontSize: '0.75rem',
                    fontWeight: 600,
                    color: getStatusColor(file.status),
                    minWidth: '1.25rem',
                    textAlign: 'center'
                  }}
                >
                  {getStatusIcon(file.status)}
                </span>
                <span style={{
                  flex: 1,
                  color: '#1F2421',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap'
                }}>
                  {file.file_path}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Empty state */}
      {stagedFiles.length === 0 && unstagedFiles.length === 0 && (
        <div style={{
          textAlign: 'center',
          padding: '2rem',
          color: '#5C635D',
          fontSize: '0.875rem'
        }}>
          {filter === 'all' ? 'Working tree clean' : `No ${filter} files`}
        </div>
      )}
    </div>
  );
};
