// Toast notification system for Rise Dashboard
// This file depends on React being loaded first

const { useState, useEffect, useCallback } = React;
const ToastContext = React.createContext(null);

function ToastProvider({ children }) {
    const [toasts, setToasts] = useState([]);

    const showToast = useCallback((message, type = 'info') => {
        const id = Date.now() + Math.random();
        const toast = { id, message, type };

        setToasts(prev => [...prev, toast]);

        // Auto-dismiss after 4 seconds
        setTimeout(() => {
            setToasts(prev => prev.filter(t => t.id !== id));
        }, 4000);
    }, []);

    const removeToast = useCallback((id) => {
        setToasts(prev => prev.filter(t => t.id !== id));
    }, []);

    return (
        <ToastContext.Provider value={{ showToast }}>
            {children}
            <div className="toast-container">
                {toasts.map(toast => (
                    <Toast key={toast.id} toast={toast} onClose={() => removeToast(toast.id)} />
                ))}
            </div>
        </ToastContext.Provider>
    );
}

function Toast({ toast, onClose }) {
    const typeClasses = {
        success: 'toast-success',
        error: 'toast-error',
        info: 'toast-info',
    };

    return (
        <div className={`toast ${typeClasses[toast.type] || 'toast-info'}`}>
            <div className="toast-content">
                {toast.type === 'success' && (
                    <div className="toast-icon svg-mask" style={{
                        maskImage: 'url(/assets/check.svg)',
                        WebkitMaskImage: 'url(/assets/check.svg)'
                    }}></div>
                )}
                {toast.type === 'error' && (
                    <div className="toast-icon svg-mask" style={{
                        maskImage: 'url(/assets/close-x.svg)',
                        WebkitMaskImage: 'url(/assets/close-x.svg)'
                    }}></div>
                )}
                {toast.type === 'info' && (
                    <div className="toast-icon svg-mask" style={{
                        maskImage: 'url(/assets/info.svg)',
                        WebkitMaskImage: 'url(/assets/info.svg)'
                    }}></div>
                )}
                <span className="toast-message">{toast.message}</span>
            </div>
            <button onClick={onClose} className="toast-close">
                <div className="w-4 h-4 svg-mask" style={{
                    maskImage: 'url(/assets/close-x.svg)',
                    WebkitMaskImage: 'url(/assets/close-x.svg)'
                }}></div>
            </button>
        </div>
    );
}

// Hook to use toast from any component
function useToast() {
    const context = React.useContext(ToastContext);
    if (!context) {
        throw new Error('useToast must be used within ToastProvider');
    }
    return context;
}
