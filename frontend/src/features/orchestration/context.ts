import { createContext, useContext, type Accessor } from 'solid-js';
import type { AgentProfileSummary, ExecutionSummary } from '../../shared/execution/api';
import type { HarnessCapability } from '../../shared/execution';
import type { TeamEntry } from '../../shared/execution/teams';
import type { FlowLane, StationKey } from './flow';

/** A task being sent from one lane to another: just what the rules for sending need. */
export interface MoveSource {
  id: string;
  title: string;
  lane: string | null;
  inFlight: boolean;
  station: StationKey;
}

/** What every lane and token of the view reads and does, so they need no long chain of props. */
export interface FlowUi {
  /** The clock the ages are measured against. */
  now: Accessor<number>;
  /** The lanes, to offer as places a task can be sent. */
  lanes: Accessor<FlowLane[]>;
  /** The task whose blocker is being pointed at: the blocker lights up. */
  highlight: Accessor<string | null>;
  setHighlight: (id: string | null) => void;
  /** Tasks that changed station on the last refresh: they arrive with a short transition. */
  arrived: Accessor<ReadonlySet<string>>;
  /** The task being dragged, if any. */
  dragged: Accessor<MoveSource | null>;
  setDragged: (source: MoveSource | null) => void;
  /** Opens the task's page on its Execution tab, or on the run's question when it asks one. */
  open: (itemId: string, run?: Pick<ExecutionSummary, 'request_id' | 'state'>) => void;
  /** Asks to send a task to a lane; the person confirms first. */
  requestMove: (source: MoveSource, lane: FlowLane) => void;
  /** Sets or clears the profile chosen on a task, with no question asked: the picker is the choice. */
  chooseProfile: (itemId: string, profile: AgentProfileSummary | null) => void;
  profiles: Accessor<readonly AgentProfileSummary[]>;
  teams: Accessor<readonly TeamEntry[]>;
  /** What the computers report, for the picker's capability marks. */
  reports: Accessor<readonly HarnessCapability[]>;
}

export const FlowUiContext = createContext<FlowUi>();

export function useFlowUi(): FlowUi {
  const ctx = useContext(FlowUiContext);
  if (!ctx) throw new Error('useFlowUi must be used within the Orchestration view');
  return ctx;
}
