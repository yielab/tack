import { type Component, type JSX, lazy, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import Sidebar from '../shared/ui/Sidebar';
import SearchBar from '../shared/ui/SearchBar';
import ToastContainer from '../shared/ui/ToastContainer';
import { ProjectProvider } from '../shared/state/projectContext';

// Lazy so the drawer + its tabs stay out of the entry bundle until first opened.
const ItemDetailDrawer = lazy(() => import('../features/item-detail/ItemDetailDrawer'));

interface LayoutProps {
  children?: JSX.Element;
}

const Layout: Component<LayoutProps> = (props) => {
  const [searchParams] = useSearchParams();
  return (
    <ProjectProvider>
    <div class="flex h-screen" style={{ "background-color": "var(--color-bg-app)" }}>
      <Sidebar />
      <main class="flex-1 overflow-auto pt-14 lg:pt-0">
        {/* Top Navigation Bar with Search */}
        <div
          class="sticky top-0 z-40 border-b px-4 py-3 shadow-sm"
          style={{
            "background-color": "var(--color-bg-base)",
            "border-color": "var(--color-border-light)"
          }}
        >
          <div class="container mx-auto max-w-7xl flex items-center justify-between gap-4">
            <div class="flex items-center gap-3">
              <h2 class="text-lg font-semibold hidden sm:block" style={{ color: "var(--color-text-primary)" }}>
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

      {/* Item detail drawer — lazily mounted only while ?item= is present */}
      <Show when={searchParams.item}>
        <ItemDetailDrawer />
      </Show>

      {/* Toast Notifications */}
      <ToastContainer />
    </div>
    </ProjectProvider>
  );
};

export default Layout;
