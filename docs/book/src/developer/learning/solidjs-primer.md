# SolidJS for Frontend Developers

FlexPM's frontend uses SolidJS. If you know React, the JSX syntax will feel familiar — but the execution model is fundamentally different. This chapter explains what changes and why it matters when reading FlexPM code.

---

## SolidJS is not React

The surface similarity is deceptive. React and SolidJS both use JSX and look similar in small examples. But:

- **React** re-renders components on state changes — the component function runs again, producing a new virtual DOM, which is diffed against the previous one.
- **SolidJS** runs component functions exactly once, on mount. DOM updates are surgical: only the specific DOM node that reads a changed value updates.

The mental model shift is from "what should render?" to "what will update?". Components are not render functions — they are setup functions that establish reactive bindings and return a DOM structure. After mount, the component function never runs again.

This has practical consequences for reading FlexPM code:
- Variables declared inside a component do not "reset" on re-render — there is no re-render
- Reactive values are signals, not state variables — you call them as functions to read them
- Control flow uses dedicated components, not ternaries and `.map()`

---

## Signals — useState, but reactive

```tsx
// React
const [count, setCount] = useState(0)
// Reading count in JSX triggers a re-render of the whole component
<div>{count}</div>

// SolidJS
const [count, setCount] = createSignal(0)
// Reading count() in JSX creates a reactive binding at that exact DOM node
<div>{count()}</div>  // only this text node updates when count changes
```

The key differences:

1. `count` in SolidJS is a getter function — you call it with `()` to read the current value. This is how SolidJS's reactivity system tracks which signals you read — it wraps reads in a subscription.
2. When `setCount(1)` is called, SolidJS does not re-run the component function. It directly updates the DOM nodes that read `count()` and nothing else.

You will see this throughout FlexPM's frontend. From `Board.tsx`:

```tsx
const [isDragging, setIsDragging] = createSignal(false)

// In JSX:
classList={{ 'opacity-40': isDragging() }}
```

`isDragging()` — the parens are always there. If you see a function call in JSX without obvious arguments, it is almost certainly reading a signal.

---

## createResource — async data fetching

`createResource` is SolidJS's built-in primitive for async data. It is similar to React Query's `useQuery`, but built into the framework:

```tsx
// From Projects.tsx

const [projects, { refetch }] = createResource(() => api.projects.list())
```

- `projects()` returns `undefined` while loading, the data when loaded
- `projects.loading` is `true` while the fetch is in progress
- `projects.error` holds the error if the fetch failed
- `refetch()` triggers a new fetch

In `projectContext.tsx`, the active project is loaded once and shared across all views:

```tsx
const [project, { refetch }] = createResource(
    projectId,                               // source signal — re-fetches when projectId changes
    (id) => (id ? api.projects.get(id) : null),
)
```

The first argument is a *source signal*. When `projectId()` changes (because the user navigated to a different project), the resource automatically refetches with the new ID. This replaces the `useEffect(() => { fetch() }, [projectId])` pattern from React.

---

## createMemo — derived state with no dependency array

```tsx
// React — you must declare dependencies:
const doubled = useMemo(() => count * 2, [count])

// SolidJS — dependencies are tracked automatically:
const doubled = createMemo(() => count() * 2)
```

SolidJS tracks which signals are read inside `createMemo`. When any of them change, the memo recomputes. No dependency array to forget to update.

`createMemo` returns a read-only signal (called with `doubled()`). It caches the result and only recomputes when its dependencies change — equivalent to React's `useMemo` in behavior, but without manual dependency management.

---

## createEffect — side effects with auto-tracking

```tsx
// React — must list dependencies:
useEffect(() => {
    document.title = `${count} items`
}, [count])

// SolidJS — tracks automatically:
createEffect(() => {
    document.title = `${count()} items`
})
```

Like `createMemo`, effects track their signal reads automatically. The effect re-runs whenever any signal read inside it changes.

One important difference from React's `useEffect`: SolidJS effects run synchronously after DOM updates, not asynchronously in a microtask. In most cases this distinction does not matter, but it means you will not see the "stale closure" bugs that are common in React.

---

## Control flow primitives — Show and For

SolidJS does not use JavaScript's native ternary operator or `.map()` for control flow in JSX. It uses components:

**Conditional rendering — `<Show>`:**

```tsx
// React
{items.length > 0 ? <ItemList items={items} /> : <EmptyState />}

// SolidJS (from Projects.tsx)
<Show
    when={projects() && projects()!.length > 0}
    fallback={<div>No projects yet...</div>}
>
    <div class="grid grid-cols-3 gap-6">
        {/* children render only when `when` is truthy */}
    </div>
</Show>
```

`<Show>` is cleaner than a ternary for the `fallback` case and also avoids React's famous `0` rendering bug (where `{count && <Component />}` renders `0` when `count` is 0).

**List rendering — `<For>`:**

```tsx
// React
{items.map(item => <ItemCard key={item.id} item={item} />)}

// SolidJS (from Projects.tsx)
<For each={projects()}>
    {(project) => (
        <a href={`/projects/${project.id}/board`}>
            {project.name}
        </a>
    )}
</For>
```

`<For>` is keyed by identity (the object reference). When the `projects()` array updates, only the DOM nodes for items that actually changed are updated. It is not a re-render of the whole list — it is a targeted reconciliation.

**Loading states — the `fallback` prop:**

```tsx
// From Projects.tsx

<Show when={!projects.loading} fallback={<ProjectsGridSkeleton />}>
    <Show when={projects()?.length > 0} fallback={<EmptyState />}>
        <For each={projects()}>
            {(project) => <ProjectCard project={project} />}
        </For>
    </Show>
</Show>
```

Three states, three `<Show>` components — no `if/else` chains in the JSX.

---

## Context — same idea, signals inside

SolidJS context works the same way as React context conceptually:

```tsx
// From frontend/src/shared/state/projectContext.tsx

const ProjectContext = createContext<ProjectContextValue>()

export const ProjectProvider: ParentComponent = (props) => {
    const params = useParams()
    const projectId = () => params.id as string | undefined

    const [project, { refetch }] = createResource(
        projectId,
        (id) => (id ? api.projects.get(id) : null),
    )

    const value: ProjectContextValue = {
        projectId,
        project,
        workflow:   () => project()?.workflow,
        vocabulary: () => project()?.vocabulary,
        refetch:    () => { void refetch() },
    }

    return (
        <ProjectContext.Provider value={value}>
            {props.children}
        </ProjectContext.Provider>
    )
}

export function useProject(): ProjectContextValue {
    const ctx = useContext(ProjectContext)
    if (!ctx) throw new Error('useProject must be used within a ProjectProvider')
    return ctx
}
```

Notice that `workflow` and `vocabulary` are accessor functions — `() => project()?.workflow`. When a consumer calls `workflow()`, they are reading `project()` inside a reactive context, so any change to the project resource will automatically propagate to components that call `workflow()`.

---

## The vocabulary hook

The `useVocab()` hook builds on `useProject()` to provide reactive label translation:

```tsx
// From frontend/src/shared/vocab/useVocab.ts

export function useVocab(): Vocab {
    const { vocabulary } = useProject()
    return {
        t: (key: string) => resolveLabel(vocabulary(), key),
        // ...
    }
}
```

Usage in a component:

```tsx
const vocab = useVocab()

// In JSX:
<label>{vocab.t('task')}</label>
// Renders "Task" for software projects, "Work Order" for construction, etc.
// Updates automatically the moment the project vocabulary is saved in Settings —
// no page reload, no manual refresh.
```

When `vocabulary()` changes (after a settings save), every component that calls `vocab.t('...')` inside a reactive context (JSX, `createMemo`, `createEffect`) will update automatically. This is the SolidJS reactivity model in practice — you did not write any subscription or effect code. The signal tracking handled it.

---

## Routing with @solidjs/router

The mental model from React Router v6 transfers directly:

```tsx
// Route definitions (from src/app/routes.tsx)
<Route path="/projects" component={Projects} />
<Route path="/projects/:id/board" component={Board} />
<Route path="/projects/:id/settings" component={Settings} />
```

Reading route parameters:

```tsx
// useParams returns a reactive object — reading params.id inside JSX is reactive
const params = useParams()
const projectId = () => params.id  // signal-like accessor

// Equivalent React Router:
const { id: projectId } = useParams()
```

Programmatic navigation:

```tsx
const navigate = useNavigate()
navigate(`/projects/${project.id}/board`)

// Equivalent React Router:
const navigate = useNavigate()
navigate(`/projects/${project.id}/board`)
```

Linking:

```tsx
// SolidJS
<A href="/projects">Projects</A>

// React Router
<Link to="/projects">Projects</Link>
```

---

## What to keep in mind when reading FlexPM code

- **`items()`** with parens — reading a signal. The value changes reactively; no re-render needed.
- **Component functions run once** — anything in a component body runs at mount time, not on every update. If you want something to run reactively, it belongs in `createEffect`, `createMemo`, or JSX.
- **`<Show>`, `<For>`, `<Switch>`** are the primary control flow constructs — not ternaries or `.map()`.
- **`createResource`** is the data-fetching primitive — covers loading/error/data states without a library.
- **`useProject()` / `useVocab()`** are signal-based context hooks — any component that reads from them updates automatically when the project data changes.
- **No `key` prop on `<For>` items** — SolidJS tracks by identity, not by a `key` string. If you see `<For each={...}>`, the identity tracking is implicit.
