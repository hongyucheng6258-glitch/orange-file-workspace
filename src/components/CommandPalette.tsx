import { useEffect, useState, useCallback, useRef } from 'react';
import { commandPaletteApi } from '../api/phase1';
import type { Command, CommandHistory } from '../types/phase1';

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: Command[];
}

export function CommandPalette({ isOpen, onClose, commands }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [filteredCommands, setFilteredCommands] = useState<Command[]>([]);
  const [recentCommands, setRecentCommands] = useState<CommandHistory[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load recent commands
  useEffect(() => {
    if (isOpen) {
      commandPaletteApi.getRecentCommands(5).then(setRecentCommands);
    }
  }, [isOpen]);

  // Filter commands based on query
  useEffect(() => {
    if (!query.trim()) {
      setFilteredCommands(commands);
      setSelectedIndex(0);
      return;
    }

    const lowerQuery = query.toLowerCase();
    const filtered = commands.filter((cmd) => {
      const labelMatch = cmd.label.toLowerCase().includes(lowerQuery);
      const categoryMatch = cmd.category.toLowerCase().includes(lowerQuery);
      const keywordsMatch = cmd.keywords?.some((kw) =>
        kw.toLowerCase().includes(lowerQuery)
      );
      return labelMatch || categoryMatch || keywordsMatch;
    });

    // Sort by relevance (exact label match first)
    filtered.sort((a, b) => {
      const aExact = a.label.toLowerCase().startsWith(lowerQuery) ? 0 : 1;
      const bExact = b.label.toLowerCase().startsWith(lowerQuery) ? 0 : 1;
      return aExact - bExact;
    });

    setFilteredCommands(filtered);
    setSelectedIndex(0);
  }, [query, commands]);

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
      setQuery('');
    }
  }, [isOpen]);

  const executeCommand = useCallback(
    async (command: Command) => {
      try {
        await command.action();
        await commandPaletteApi.recordExecution(
          command.id,
          command.label,
          command.category
        );
      } catch (error) {
        console.error('Command execution failed:', error);
      } finally {
        onClose();
      }
    },
    [onClose]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filteredCommands.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const selected = filteredCommands[selectedIndex];
        if (selected) {
          executeCommand(selected);
        }
      }
    },
    [onClose, filteredCommands, selectedIndex, executeCommand]
  );

  if (!isOpen) return null;

  return (
    <div className="cmd-overlay" onClick={onClose}>
      <div className="cmd-palette" onClick={(e) => e.stopPropagation()}>
        {/* Search Input */}
        <div className="cmd-input-wrap">
          <input
            ref={inputRef}
            type="text"
            className="cmd-input"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入命令或搜索..."
          />
        </div>

        {/* Command List */}
        <div className="cmd-list">
          {!query && recentCommands.length > 0 && (
            <div className="cmd-section-label">最近使用</div>
          )}

          {filteredCommands.map((cmd, index) => (
            <div
              key={cmd.id}
              className={`cmd-item${index === selectedIndex ? ' selected' : ''}`}
              onClick={() => executeCommand(cmd)}
              onMouseEnter={() => setSelectedIndex(index)}
            >
              <div className="cmd-item-body">
                {cmd.icon && <span className="cmd-item-icon">{cmd.icon}</span>}
                <div className="cmd-item-text">
                  <div className="cmd-item-label">{cmd.label}</div>
                  <div className="cmd-item-category">{cmd.category}</div>
                </div>
              </div>
              {cmd.shortcut && <kbd className="cmd-kbd">{cmd.shortcut}</kbd>}
            </div>
          ))}

          {filteredCommands.length === 0 && (
            <div className="cmd-empty">未找到匹配的命令</div>
          )}
        </div>
      </div>
    </div>
  );
}
