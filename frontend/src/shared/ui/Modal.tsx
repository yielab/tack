import { type Component, type JSX, Show } from 'solid-js';
import { Portal } from 'solid-js/web';

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: JSX.Element;
  size?: 'sm' | 'md' | 'lg' | 'xl';
}

const Modal: Component<ModalProps> = (props) => {
  const sizeClasses = () => {
    switch (props.size || 'md') {
      case 'sm':
        return 'max-w-md';
      case 'md':
        return 'max-w-lg';
      case 'lg':
        return 'max-w-2xl';
      case 'xl':
        return 'max-w-4xl';
      default:
        return 'max-w-lg';
    }
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
          class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm"
          onClick={handleBackdropClick}
        >
          <div
            class={`bg-white dark:bg-gray-900 rounded-lg shadow-xl w-full ${sizeClasses()} max-h-[90vh] flex flex-col`}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div class="flex items-center justify-between p-6 border-b border-gray-200 dark:border-gray-700">
              <h2 class="text-xl font-semibold text-gray-900 dark:text-white">
                {props.title}
              </h2>
              <button
                onClick={props.onClose}
                class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                aria-label="Close"
              >
                <svg
                  class="w-6 h-6"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div class="flex-1 overflow-y-auto p-6">{props.children}</div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default Modal;
