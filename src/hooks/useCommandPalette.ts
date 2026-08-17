import { useState, useCallback, useEffect } from 'react';
import { commandPaletteApi } from '../api/phase1';
import type { Command, CommandHistory } from '../types/phase1';

export const useCommandPalette = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [recentCommands, setRecentCommands] = useState<CommandHistory[]>([]);
  const [frequentCommands, setFrequentCommands] = useState<CommandHistory[]>([]);
  const [loading, setLoading] = useState(false);

  const loadCommandHistory = useCallback(async () => {
    try {
      setLoading(true);
      const [recent, frequent] = await Promise.all([
        commandPaletteApi.getRecentCommands(10),
        commandPaletteApi.getFrequentCommands(10)
      ]);
      setRecentCommands(recent);
      setFrequentCommands(frequent);
    } catch (error) {
      console.error('Failed to load command history:', error);
    } finally {
      setLoading(false);
    }
  }, []);

  const open = useCallback(() => {
    setIsOpen(true);
    loadCommandHistory();
  }, [loadCommandHistory]);

  const close = useCallback(() => {
    setIsOpen(false);
  }, []);

  const toggle = useCallback(() => {
    if (isOpen) {
      close();
    } else {
      open();
    }
  }, [isOpen, open, close]);

  const executeCommand = useCallback(async (command: Command) => {
    await command.action();
    await commandPaletteApi.recordExecution(
      command.id,
      command.label,
      command.category
    );
    close();
  }, [close]);

  const searchCommands = useCallback(async (query: string) => {
    if (!query.trim()) {
      return [];
    }
    return commandPaletteApi.searchCommands(query, 20);
  }, []);

  const clearHistory = useCallback(async () => {
    await commandPaletteApi.clearHistory();
    setRecentCommands([]);
    setFrequentCommands([]);
  }, []);

  // Setup keyboard shortcut listener
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        toggle();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggle]);

  return {
    isOpen,
    open,
    close,
    toggle,
    executeCommand,
    searchCommands,
    recentCommands,
    frequentCommands,
    clearHistory,
    loading
  };
};
