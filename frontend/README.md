# FlexPM Frontend

Modern, responsive frontend for FlexPM built with SolidJS, TypeScript, and Tailwind CSS.

## Tech Stack

- **Framework:** [SolidJS](https://www.solidjs.com/) - Fast, reactive UI framework
- **Build Tool:** [Vite](https://vitejs.dev/) - Next-generation frontend tooling
- **Language:** [TypeScript](https://www.typescriptlang.org/) - Type-safe JavaScript
- **Styling:** [Tailwind CSS v4](https://tailwindcss.com/) - Utility-first CSS framework
- **Routing:** [@solidjs/router](https://github.com/solidjs/solid-router) - Client-side routing
- **Icons:** [solid-icons](https://github.com/x64Bits/solid-icons) - Icon library

## Features

### ✅ Implemented (Phase 4 - 25%)

- **Responsive Layout**
  - Mobile-first design
  - Collapsible sidebar with hamburger menu
  - Dark mode support (via Tailwind)

- **Projects Page**
  - Grid layout showing all projects
  - Project cards with type badges
  - Create new project button (UI only)

- **Board View (Kanban)**
  - Column-based layout
  - Items grouped by status
  - WIP limit indicators
  - Priority and type badges

- **List View** (Placeholder)
- **Settings Page** (Placeholder)

- **API Integration**
  - Type-safe API client
  - Resource-based data fetching
  - Environment configuration

### 🚧 In Progress

- Drag-and-drop for Kanban board
- Create/edit modals
- WebSocket real-time updates
- Complete List view
- Project settings UI

## Getting Started

### Prerequisites

- Node.js 18+ and npm
- FlexPM backend running (default: `http://localhost:3210`)

### Installation

```bash
cd frontend
npm install
```

### Development

```bash
npm run dev
```

The app will be available at `http://localhost:5173`

### Build for Production

```bash
npm run build
```

The optimized build will be in the `dist/` directory.

### Preview Production Build

```bash
npm run preview
```

## Configuration

### Environment Variables

Create a `.env` file in the frontend directory:

```env
VITE_API_URL=http://localhost:3210/api
```

**Note:** For production, update this to your deployed backend URL.

## Project Structure

```
frontend/
├── src/
│   ├── components/       # Reusable UI components
│   │   ├── Layout.tsx    # Main layout wrapper
│   │   └── Sidebar.tsx   # Navigation sidebar
│   ├── pages/            # Page components (routes)
│   │   ├── Projects.tsx  # Projects list
│   │   ├── Board.tsx     # Kanban board view
│   │   ├── List.tsx      # List view
│   │   └── Settings.tsx  # Settings page
│   ├── lib/              # Utilities and helpers
│   │   └── api.ts        # API client
│   ├── types/            # TypeScript type definitions
│   │   └── api.ts        # API types matching backend
│   ├── App.tsx           # Root component with routing
│   ├── index.tsx         # App entry point
│   └── index.css         # Global styles (Tailwind)
├── public/               # Static assets
├── .env                  # Environment variables
├── package.json          # Dependencies
├── tsconfig.json         # TypeScript configuration
├── tailwind.config.js    # Tailwind configuration
├── postcss.config.js     # PostCSS configuration
└── vite.config.ts        # Vite configuration
```

## API Client Usage

The API client is located in `src/lib/api.ts` and provides type-safe methods for all backend endpoints:

```typescript
import { api } from '../lib/api';

// List all projects
const projects = await api.listProjects();

// Get board state
const board = await api.getBoard(projectId);

// Create an item
const newItem = await api.createItem(projectId, {
  title: 'New task',
  item_type: 'task',
  priority: 'high',
});

// Update item
await api.updateItem(itemId, {
  status: 'in_progress',
});

// WebSocket connection
const ws = api.createBoardSocket(projectId);
ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);
  // Handle real-time updates
};
```

## Routing

Routes are defined in `App.tsx`:

| Route | Component | Description |
|-------|-----------|-------------|
| `/` | Projects | Projects list page |
| `/board` | Board | Kanban board view |
| `/list` | List | List/table view |
| `/settings` | Settings | Project settings |

**Query Parameters:**
- `/board?project={id}` - Load specific project board
- `/list?project={id}` - Load specific project list

## Development

For detailed API documentation and examples, see [docs/API-EXAMPLES.md](../docs/API-EXAMPLES.md)

---

**Current Status:** Phase 4 - 25% Complete

**Next Milestone:** Drag-and-drop + Create/Edit modals → 50% Complete
