import type { Component } from 'solid-js';
import {
  IconBoard, IconList, IconTable, IconCalendar, IconTimeline, IconSprint,
  IconOverview, IconProjects, IconTemplates, IconAgent, IconSettings, type IconProps,
} from './icons';

export type DestinationIcon = Component<IconProps>;

/** Looks up a word from the project's vocabulary (for example what a sprint is called). */
type Translate = (key: string) => string;

/** A place the sidebar links to. The sidebar and the command palette both read these lists, so a
 *  place has one label, one icon and one route everywhere. */
export interface Destination {
  id: string;
  label: (t: Translate) => string;
  icon: DestinationIcon;
  /** The route. A project view takes the project id; a workspace page ignores it. */
  path: (projectId: string) => string;
  /** Active only on an exact match (the all-projects page, which is the parent of every project). */
  end?: boolean;
}

/** The views of one project, in sidebar order. */
export const PROJECT_DESTINATIONS: Destination[] = [
  { id: 'board', label: () => 'Board', icon: IconBoard, path: (p) => `/projects/${p}/board` },
  { id: 'list', label: () => 'List', icon: IconList, path: (p) => `/projects/${p}/list` },
  { id: 'table', label: () => 'Table', icon: IconTable, path: (p) => `/projects/${p}/table` },
  { id: 'calendar', label: () => 'Calendar', icon: IconCalendar, path: (p) => `/projects/${p}/calendar` },
  { id: 'timeline', label: () => 'Timeline', icon: IconTimeline, path: (p) => `/projects/${p}/timeline` },
  { id: 'sprint', label: (t) => t('sprint'), icon: IconSprint, path: (p) => `/projects/${p}/sprint` },
  { id: 'overview', label: () => 'Overview', icon: IconOverview, path: (p) => `/projects/${p}/overview` },
  { id: 'factory', label: () => 'Factory metrics', icon: IconOverview, path: (p) => `/projects/${p}/factory` },
  { id: 'project-settings', label: () => 'Project settings', icon: IconSettings, path: (p) => `/projects/${p}/settings` },
];

/** The places outside any project, in sidebar order. */
export const WORKSPACE_DESTINATIONS: Destination[] = [
  { id: 'all-projects', label: () => 'All projects', icon: IconProjects, path: () => '/projects', end: true },
  { id: 'templates', label: () => 'Templates', icon: IconTemplates, path: () => '/templates' },
  { id: 'agents', label: () => 'Agents', icon: IconAgent, path: () => '/agents' },
  { id: 'settings', label: () => 'Settings', icon: IconSettings, path: () => '/settings' },
];
