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

export function PlatformAccessDenied({ userEmail }: { userEmail: string }) {
    return (
        <div className="mono-login-wrap">
            <div className="mono-login-card">
                <div className="text-center mb-8">
                    <div
                        className="mono-login-logo svg-mask mx-auto mb-4"
                        aria-hidden="true"
                        style={{
                            maskImage: 'url(/assets/logo.svg)',
                            WebkitMaskImage: 'url(/assets/logo.svg)',
                        }}
                    ></div>
                    <div className="text-4xl mb-4" style={{ color: '#ff6b6b' }}>[!]</div>
                    <h1 className="text-2xl font-bold mb-4" style={{ color: '#ff6b6b' }}>Platform Access Denied</h1>
                </div>

                <p className="text-gray-300 mb-4 text-center">
                    Your account is not authorized to access Rise platform features.
                </p>

                <p className="text-gray-300 mb-6 text-center">
                    You can authenticate to access deployed applications, but you cannot use
                    the Rise Dashboard, CLI, or API.
                </p>

                <div className="bg-gray-900 border border-gray-700 p-3 mb-6 text-center font-mono text-sm break-all">
                    {userEmail}
                </div>

                <p className="text-gray-500 text-sm text-center">
                    If you believe this is an error, please contact your administrator.
                </p>
            </div>
        </div>
    );
}

