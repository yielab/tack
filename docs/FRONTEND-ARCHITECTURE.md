# FlexPM Frontend Architecture - Bulletproof & Customizable

**Goal:** Build a production-ready, highly customizable, accessible UI that doesn't compromise on any features.

## Core Principles

1. **Bulletproof**: Robust error handling, offline support, optimistic updates
2. **Practical**: Fast, intuitive, keyboard-friendly, mobile-responsive
3. **Customizable**: Themes, layouts, keyboard shortcuts, workflows all customizable
4. **No Compromises**: Implement ALL features with proper libraries, no shortcuts

## Technology Stack Review

### Current Stack ✅
- **SolidJS** - Perfect choice (faster than React, better DX than Vue)
- **TypeScript** - Essential for bulletproof code
- **Vite** - Fast, modern build tool
- **Tailwind CSS v4** - Utility-first, highly customizable

### Additions Needed

#### 1. **Drag & Drop** - Use `@thisbeyond/solid-dnd`
**Why:** Native SolidJS drag-and-drop library (not @dnd-kit which is React-only)
- Fully compatible with SolidJS reactivity
- Touch support for mobile
- Accessibility built-in
- Customizable drag handles and drop zones

**Replace:**
```bash
npm uninstall @dnd-kit/core @dnd-kit/sortable @dnd-kit/utilities
npm install @thisbeyond/solid-dnd
```

#### 2. **State Management** - Use SolidJS Stores + Context
**Why:** Built-in, no external dependencies needed
- Reactive by default
- Nested reactivity (perfect for complex state)
- Dev tools available
- Context API for global state

**Structure:**
```
stores/
├── projectStore.ts    # Current project, board state
├── uiStore.ts         # Theme, sidebar, modals
├── settingsStore.ts   # User preferences, shortcuts
└── websocketStore.ts  # Real-time connection state
```

#### 3. **WebSocket** - Native WebSocket + Reconnection Logic
**No library needed** - WebSocket API is native
- Automatic reconnection with exponential backoff
- Queue messages during disconnection
- Heartbeat/ping to detect stale connections

#### 4. **Keyboard Shortcuts** - `@solid-primitives/keyboard`
**Why:** SolidJS-native keyboard handling
- Composable shortcuts
- Scope management (modal vs global)
- Conflict detection
- Customizable bindings

#### 5. **Accessibility** - `@kobalte/core`
**Why:** Headless UI components with full a11y
- ARIA attributes built-in
- Keyboard navigation
- Screen reader support
- Focus management

Replace custom modals with Kobalte Dialog:
```typescript
import { Dialog } from "@kobalte/core";
```

#### 6. **Form Validation** - `modular-forms`
**Why:** SolidJS-native, type-safe form library
- Zod schema integration
- Field-level validation
- Async validation
- Form state management

#### 7. **Offline Support** - Service Worker + IndexedDB
**Why:** True PWA capabilities
- Offline board access
- Background sync when online
- Cache API for static assets
- IndexedDB for local data storage

#### 8. **Virtualization** - `@tanstack/solid-virtual`
**Why:** Efficient rendering of large lists
- Smooth scrolling with 1000s of items
- Variable heights
- Horizontal + vertical virtualization

## Architecture Layers

### 1. Data Layer
```
lib/
├── api.ts              # REST API client (existing)
├── websocket.ts        # WebSocket connection manager (NEW)
├── cache.ts            # IndexedDB wrapper (NEW)
├── sync.ts             # Offline sync queue (NEW)
└── types/
    └── api.ts          # TypeScript types
```

### 2. State Layer
```
stores/
├── projectStore.ts     # Project data, board columns
├── itemsStore.ts       # Items with optimistic updates
├── uiStore.ts          # UI state (modals, sidebar, theme)
├── settingsStore.ts    # User preferences
└── syncStore.ts        # Offline sync queue
```

### 3. Component Layer
```
components/
├── ui/                 # Base UI components (Kobalte-based)
│   ├── Dialog.tsx
│   ├── Dropdown.tsx
│   ├── Toast.tsx
│   ├── Popover.tsx
│   └── Select.tsx
├── board/              # Board-specific components
│   ├── BoardColumn.tsx
│   ├── ItemCard.tsx
│   ├── DragOverlay.tsx
│   └── ColumnHeader.tsx
├── forms/              # Form components
│   ├── ProjectForm.tsx
│   ├── ItemForm.tsx
│   └── FieldTypes/
└── layout/             # Layout components
    ├── Sidebar.tsx
    ├── Header.tsx
    └── CommandPalette.tsx
```

### 4. Feature Layer
```
pages/
├── Board.tsx           # Board view with drag-and-drop
├── Projects.tsx        # Project list
├── Settings.tsx        # Settings page
└── Search.tsx          # Global search
```

## Feature Implementation Plan

### Phase 4.1: Foundation (Current Session)
- ✅ Modal system
- ✅ Project creation
- ✅ Item creation
- ⏳ Migrate to proper libraries

### Phase 4.2: Drag & Drop (Next Priority)
**Implementation:**
1. Install `@thisbeyond/solid-dnd`
2. Wrap board in `<DragDropProvider>`
3. Make columns `<Droppable>`
4. Make items `<Draggable>`
5. Handle `onDragEnd` to update item status
6. Optimistic UI updates
7. Rollback on API error

**Features:**
- Drag items between columns
- Reorder within column (sort_order)
- Visual feedback (ghost item)
- Touch support for mobile
- Accessibility (keyboard drag with Space+Arrow keys)

### Phase 4.3: WebSocket Real-Time (High Priority)
**Implementation:**
```typescript
// stores/websocketStore.ts
const [wsState, setWsState] = createStore({
  connected: false,
  reconnecting: false,
  events: [] as BoardEvent[],
});

function connectWebSocket(projectId: string) {
  const ws = new WebSocket(`ws://localhost:3210/api/projects/${projectId}/board/live`);

  ws.onopen = () => setWsState({ connected: true, reconnecting: false });
  ws.onmessage = (event) => handleBoardEvent(JSON.parse(event.data));
  ws.onclose = () => reconnectWithBackoff();
  ws.onerror = () => setWsState({ connected: false });
}
```

**Features:**
- Auto-reconnect with exponential backoff
- Event queue during disconnection
- Optimistic updates (don't wait for server)
- Conflict resolution (last-write-wins)
- Connection status indicator in UI

### Phase 4.4: Keyboard Shortcuts
**Shortcuts:**
```
Ctrl+K          → Open command palette
N               → New item (in current column)
E               → Edit selected item
Del             → Delete selected item
Esc             → Close modal/cancel
Arrow keys      → Navigate between items
Enter           → Open item details
Ctrl+S          → Quick save (if editing)
Ctrl+F          → Focus search
1-9             → Switch between columns
```

**Implementation:**
```typescript
import { createShortcut } from "@solid-primitives/keyboard";

createShortcut(["Control", "K"], () => openCommandPalette(), {
  preventDefault: true,
  requireReset: false,
});
```

### Phase 4.5: Theme Customization
**Customizable:**
- Color scheme (predefined + custom)
- Board layout (compact vs spacious)
- Font size (accessibility)
- Card density
- Column width

**Implementation:**
```typescript
// stores/uiStore.ts
const [theme, setTheme] = createStore({
  colors: {
    primary: "#9333ea",    // purple-600
    success: "#10b981",
    warning: "#f59e0b",
    danger: "#ef4444",
  },
  layout: {
    columnWidth: 320,
    cardGap: 12,
    fontSize: "base",      // sm | base | lg
  },
  density: "comfortable",  // compact | comfortable | spacious
});
```

### Phase 4.6: Advanced Search
**Features:**
- Full-text search across all projects
- Filter by: status, priority, type, tags, assignee
- Date range filters
- Save search queries
- Keyboard navigation
- Instant results (debounced)

**UI:**
```
┌─────────────────────────────────────┐
│  Search...              Ctrl+F     │
├─────────────────────────────────────┤
│  Filters                           │
│  ☐ Status: All                    │
│  ☐ Priority: All                  │
│  ☐ Tags: [Select...]              │
│  ☐ Date: Last 30 days             │
├─────────────────────────────────────┤
│  Results (23)                      │
│  ⚡ High priority bug in Project A │
│  📝 Feature request for...         │
│  🐛 Bug: Login fails when...       │
└─────────────────────────────────────┘
```

### Phase 4.7: Offline Support
**Features:**
- Cache board data in IndexedDB
- Queue mutations while offline
- Sync when back online
- Conflict detection
- Offline indicator in UI

**Implementation:**
```typescript
// lib/cache.ts
import { openDB } from 'idb';

const db = await openDB('flexpm', 1, {
  upgrade(db) {
    db.createObjectStore('boards', { keyPath: 'project_id' });
    db.createObjectStore('items', { keyPath: 'id' });
    db.createObjectStore('queue', { keyPath: 'id', autoIncrement: true });
  },
});

// Store board data
await db.put('boards', boardData);

// Queue mutation
await db.put('queue', { action: 'UPDATE_ITEM', data, timestamp: Date.now() });
```

## Accessibility Features

### WCAG 2.1 AA Compliance
- ✅ Keyboard navigation
- ✅ Screen reader support
- ✅ Focus indicators
- ✅ Color contrast (4.5:1 minimum)
- ✅ Resizable text (up to 200%)
- ✅ No keyboard traps
- ✅ ARIA labels and landmarks

### Implementation Checklist
- [ ] All interactive elements focusable
- [ ] Skip links for keyboard users
- [ ] Focus visible on all elements
- [ ] Labels on all form inputs
- [ ] Error messages associated with fields
- [ ] Toast notifications announced to screen readers
- [ ] Modal focus trap
- [ ] Drag-and-drop keyboard alternative

## Performance Targets

### Metrics
- **First Contentful Paint**: < 1.5s
- **Time to Interactive**: < 3s
- **Largest Contentful Paint**: < 2.5s
- **Cumulative Layout Shift**: < 0.1
- **First Input Delay**: < 100ms

### Optimizations
1. **Code Splitting** - Route-based lazy loading
2. **Image Optimization** - WebP with fallback
3. **Bundle Size** - < 200KB initial JS
4. **Caching** - Service worker + HTTP cache
5. **Virtualization** - For large lists
6. **Debouncing** - Search, autosave
7. **Memoization** - Expensive computations

## Testing Strategy

### Unit Tests (Vitest)
```typescript
describe('ItemCard', () => {
  it('displays item title', () => {
    const { getByText } = render(() => <ItemCard item={mockItem} />);
    expect(getByText('Test Item')).toBeInTheDocument();
  });
});
```

### Integration Tests
```typescript
describe('Board drag-and-drop', () => {
  it('moves item between columns', async () => {
    // Test drag from TODO to IN_PROGRESS
    // Verify API call
    // Verify optimistic update
  });
});
```

### E2E Tests (Playwright)
```typescript
test('create and move item', async ({ page }) => {
  await page.goto('/board?project=123');
  await page.click('text=+ Add item');
  await page.fill('input[name="title"]', 'Test Task');
  await page.click('button:has-text("Create")');

  // Drag item
  const item = page.locator('text=Test Task');
  await item.dragTo(page.locator('[data-column="in_progress"]'));

  // Verify status changed
  await expect(item).toHaveAttribute('data-status', 'in_progress');
});
```

## Mobile Responsive Design

### Breakpoints
```css
/* Tailwind breakpoints */
sm: 640px   /* Mobile landscape */
md: 768px   /* Tablet */
lg: 1024px  /* Desktop */
xl: 1280px  /* Large desktop */
```

### Mobile UX
- **Board**: Horizontal scroll + snap to columns
- **Items**: Full-width cards on mobile
- **Modals**: Full-screen on mobile
- **Navigation**: Bottom tab bar on mobile
- **Drag**: Touch-friendly drag handles
- **Search**: Slide-up panel on mobile

## Error Handling

### Levels
1. **Field-level**: Inline validation errors
2. **Form-level**: Summary at top of form
3. **API-level**: Toast notifications
4. **Connection-level**: Reconnecting banner
5. **Critical**: Error boundary with retry

### User-Friendly Messages
```typescript
const errorMessages = {
  NETWORK_ERROR: "Can't connect to server. Check your internet connection.",
  VALIDATION_ERROR: "Please check the highlighted fields.",
  PERMISSION_ERROR: "You don't have permission to do that.",
  NOT_FOUND: "That item doesn't exist anymore.",
  CONFLICT: "Someone else modified this. Refresh to see changes.",
};
```

## Deployment

### Build Optimizations
```typescript
// vite.config.ts
export default defineConfig({
  build: {
    target: 'es2020',
    minify: 'terser',
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor': ['solid-js', '@solidjs/router'],
          'ui': ['@kobalte/core'],
          'dnd': ['@thisbeyond/solid-dnd'],
        },
      },
    },
  },
});
```

### Docker Production Build
- Multi-stage build (Node → nginx)
- Gzip compression enabled
- HTTP/2 support
- Cache headers configured
- Security headers (CSP, HSTS)

## Summary

This architecture provides:
- ✅ **Bulletproof**: Offline support, error recovery, optimistic updates
- ✅ **Practical**: Fast, keyboard-friendly, mobile-responsive
- ✅ **Customizable**: Themes, shortcuts, layouts all configurable
- ✅ **No Compromises**: Proper libraries for every feature
- ✅ **Accessible**: WCAG 2.1 AA compliant
- ✅ **Tested**: Unit, integration, E2E coverage
- ✅ **Performant**: < 3s TTI, virtualization, code splitting

Next session should implement these improvements systematically, starting with the drag-and-drop migration to `@thisbeyond/solid-dnd`.
