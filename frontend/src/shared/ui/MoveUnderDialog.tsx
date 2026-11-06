import { type Component, Show } from 'solid-js';
import type { ParentOption } from '../types';
import Modal from './Modal';
import ParentPicker from './ParentPicker';

export interface MoveUnderDialogProps {
  projectId: string;
  /** The item being moved; the dialog is shut while it is null. */
  item: { id: string; title: string; parent_id?: string | null } | null;
  /** The parent the person chose, or null for the top of the project. */
  onPick: (parent: ParentOption | null) => void;
  onClose: () => void;
}

/**
 * The keyboard's way to do what dragging does: a dialog over `ParentPicker`, which offers exactly the
 * parents the server admits for the item. Every view that lets an item be dragged under another opens
 * this from the item's menu.
 */
const MoveUnderDialog: Component<MoveUnderDialogProps> = (props) => (
  <Modal isOpen={props.item != null} onClose={props.onClose} title={`Move under…: ${props.item?.title ?? ''}`} size="md">
    <Show when={props.item}>
      {(moving) => (
        <ParentPicker
          projectId={props.projectId}
          of={{ item: moving().id }}
          current={moving().parent_id ?? null}
          onPick={props.onPick}
        />
      )}
    </Show>
  </Modal>
);

export default MoveUnderDialog;
