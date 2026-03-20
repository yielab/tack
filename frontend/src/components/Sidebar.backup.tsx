import { A } from '@solidjs/router';
import { FiHome, FiGrid, FiList, FiSettings, FiMenu, FiX } from 'solid-icons/fi';
import { createSignal, Show, type Component } from 'solid-js';
import clsx from 'clsx';

const Sidebar: Component = () => {
  const [isOpen, setIsOpen] = createSignal(false);

  const navigation = [
    { name: 'Projects', href: '/', icon: FiHome },
    { name: 'Board', href: '/board', icon: FiGrid },
    { name: 'List', href: '/list', icon: FiList },
    { name: 'Settings', href: '/settings', icon: FiSettings },
  ];

  return (
    <>
      {/* Mobile menu button */}
      <div class="lg:hidden fixed top-0 left-0 right-0 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 z-20">
        <div class="flex items-center justify-between px-4 py-3">
          <h1 class="text-xl font-bold text-gray-900 dark:text-white">FlexPM</h1>
          <button
            onClick={() => setIsOpen(!isOpen())}
            class="p-2 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
          >
            <Show when={isOpen()} fallback={<FiMenu size={24} />}>
              <FiX size={24} />
            </Show>
          </button>
        </div>
      </div>

      {/* Sidebar */}
      <div
        class={clsx(
          'fixed inset-y-0 left-0 z-10 w-64 bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700 transform transition-transform duration-200 ease-in-out',
          'lg:translate-x-0 lg:static',
          isOpen() ? 'translate-x-0' : '-translate-x-full'
        )}
      >
        <div class="flex flex-col h-full">
          {/* Logo */}
          <div class="hidden lg:flex items-center px-6 py-5 border-b border-gray-200 dark:border-gray-700">
            <h1 class="text-2xl font-bold text-gray-900 dark:text-white">FlexPM</h1>
          </div>

          {/* Navigation */}
          <nav class="flex-1 px-4 py-4 space-y-1 overflow-y-auto mt-14 lg:mt-0">
            {navigation.map((item) => (
              <A
                href={item.href}
                activeClass="bg-purple-50 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400"
                inactiveClass="text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
                class="flex items-center px-4 py-3 text-sm font-medium rounded-lg transition-colors"
                onClick={() => setIsOpen(false)}
              >
                <item.icon class="mr-3" size={20} />
                {item.name}
              </A>
            ))}
          </nav>

          {/* Footer */}
          <div class="px-4 py-4 border-t border-gray-200 dark:border-gray-700">
            <p class="text-xs text-gray-500 dark:text-gray-400">
              FlexPM v0.1.0
            </p>
          </div>
        </div>
      </div>

      {/* Mobile overlay */}
      <Show when={isOpen()}>
        <div
          class="fixed inset-0 bg-black bg-opacity-50 z-0 lg:hidden"
          onClick={() => setIsOpen(false)}
        />
      </Show>
    </>
  );
};

export default Sidebar;
