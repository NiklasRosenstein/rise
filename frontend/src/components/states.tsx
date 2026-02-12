import { Button } from './ui';

export function LoadingState({ label = 'Loading...' }: { label?: string }) {
    return (
        <div className="text-center py-8">
            <div className="inline-block w-8 h-8 border-2 border-gray-300 border-t-transparent rounded-full animate-spin"></div>
            <p className="mt-3 text-sm text-gray-400">{label}</p>
        </div>
    );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
    return (
        <div className="mono-state mono-state-error">
            <p>{message}</p>
            {onRetry && (
                <Button variant="secondary" size="sm" onClick={onRetry}>
                    Retry
                </Button>
            )}
        </div>
    );
}

export function EmptyState({
    message,
    actionLabel,
    onAction,
}: {
    message: string;
    actionLabel?: string;
    onAction?: () => void;
}) {
    return (
        <div className="mono-state">
            <p>{message}</p>
            {actionLabel && onAction && (
                <Button variant="primary" size="sm" onClick={onAction}>
                    {actionLabel}
                </Button>
            )}
        </div>
    );
}

