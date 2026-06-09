import { createSignal, For, Show, type Component, createEffect } from 'solid-js';
import { Portal } from 'solid-js/web';

export interface Command {
  id: string;
  label: string;
  description?: string;
  icon?: string;
  action: () => void;
  shortcut?: string;
}

export interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  commands: Command[];
}

const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [search, setSearch] = createSignal('');
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;

  // Reset when opened
  createEffect(() => {
    if (props.isOpen) {
      setSearch('');
      setSelectedIndex(0);
      // Focus input when opened
      setTimeout(() => inputRef?.focus(), 10);
    }
  });

  // Filter commands based on search
  const filteredCommands = () => {
    const query = search().toLowerCase();
    if (!query) return props.commands;

    return props.commands.filter((cmd) => {
      const labelMatch = cmd.label.toLowerCase().includes(query);
      const descMatch = cmd.description?.toLowerCase().includes(query);
      return labelMatch || descMatch;
    });
  };

  // Keep selected index in bounds
  createEffect(() => {
    const maxIndex = Math.max(0, filteredCommands().length - 1);
    if (selectedIndex() > maxIndex) {
      setSelectedIndex(maxIndex);
    }
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    const commands = filteredCommands();

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, commands.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (commands[selectedIndex()]) {
          commands[selectedIndex()].action();
          props.onClose();
        }
        break;
      case 'Escape':
        e.preventDefault();
        props.onClose();
        break;
    }
  };

  const handleCommandClick = (cmd: Command) => {
    cmd.action();
    props.onClose();
  };

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onClose();
    }
  };

  return (
    <Show when={props.isOpen}>
      <Portal>
        <div
          class="fixed inset-0 z-50 flex items-start justify-center pt-[20vh] px-4 bg-black/50 backdrop-blur-sm"
          onClick={handleBackdropClick}
        >
          <div class="bg-white dark:bg-gray-900 rounded-lg shadow-2xl w-full max-w-2xl overflow-hidden border border-gray-200 dark:border-gray-700">
            {/* Search Input */}
            <div class="p-4 border-b border-gray-200 dark:border-gray-700">
              <input
                ref={inputRef}
                type="text"
                class="w-full px-4 py-3 text-lg bg-transparent border-none outline-none text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500"
                placeholder="Search commands..."
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
                onKeyDown={handleKeyDown}
              />
            </div>

            {/* Commands List */}
            <div class="max-h-[60vh] overflow-y-auto">
              <Show
                when={filteredCommands().length > 0}
                fallback={
                  <div class="px-4 py-8 text-center text-gray-500 dark:text-gray-400">
                    No commands found
                  </div>
                }
              >
                <For each={filteredCommands()}>
                  {(cmd, index) => (
                    <button
                      class="w-full px-4 py-3 flex items-center justify-between text-left transition-colors hover:bg-gray-100 dark:hover:bg-gray-800"
                      classList={{
                        'bg-purple-50 dark:bg-purple-900/20': index() === selectedIndex(),
                      }}
                      onClick={() => handleCommandClick(cmd)}
                      onMouseEnter={() => setSelectedIndex(index())}
                    >
                      <div class="flex items-center gap-3 flex-1">
                        {cmd.icon && (
                          <span class="text-gray-500 dark:text-gray-400">{cmd.icon}</span>
                        )}
                        <div class="flex-1">
                          <div class="font-medium text-gray-900 dark:text-white">
                            {cmd.label}
                          </div>
                          {cmd.description && (
                            <div class="text-sm text-gray-500 dark:text-gray-400">
                              {cmd.description}
                            </div>
                          )}
                        </div>
                      </div>
                      {cmd.shortcut && (
                        <div class="text-xs text-gray-400 dark:text-gray-500 font-mono">
                          {cmd.shortcut}
                        </div>
                      )}
                    </button>
                  )}
                </For>
              </Show>
            </div>

            {/* Footer */}
            <div class="px-4 py-2 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
              <div class="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
                <span>Use ↑ ↓ to navigate</span>
                <span>↵ to select · Esc to close</span>
              </div>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default CommandPalette;
