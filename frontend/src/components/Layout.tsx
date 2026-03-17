import { type Component, type JSX } from 'solid-js';
import Sidebar from './Sidebar';
import SearchBar from './SearchBar';
import ToastContainer from './ToastContainer';

interface LayoutProps {
  children: JSX.Element;
}

const Layout: Component<LayoutProps> = (props) => {
  return (
    <div class="flex h-screen bg-gray-50 dark:bg-gray-900">
      <Sidebar />
      <main class="flex-1 overflow-auto pt-14 lg:pt-0">
        {/* Top Navigation Bar with Search */}
        <div class="sticky top-0 z-40 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 px-4 py-3">
          <div class="container mx-auto max-w-7xl flex items-center justify-between gap-4">
            <div class="flex items-center gap-3">
              <h2 class="text-lg font-semibold text-gray-900 dark:text-white hidden sm:block">
                FlexPM
              </h2>
            </div>
            <SearchBar placeholder="Search items... (Ctrl+/)" />
          </div>
        </div>

        <div class="container mx-auto px-4 py-8 max-w-7xl">
          {props.children}
        </div>
      </main>

      {/* Toast Notifications */}
      <ToastContainer />
    </div>
  );
};

export default Layout;
