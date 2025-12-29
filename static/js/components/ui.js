// Reusable UI components for Rise Dashboard
// This file depends on React being loaded first

const { useState, useEffect } = React;

// Status Badge Component
function StatusBadge({ status }) {
    const statusColors = {
        'Healthy': 'bg-green-600',
        'Running': 'bg-green-600',
        'Deploying': 'bg-yellow-600',
        'Pending': 'bg-yellow-600',
        'Building': 'bg-yellow-600',
        'Pushing': 'bg-yellow-600',
        'Pushed': 'bg-yellow-600',
        'Unhealthy': 'bg-red-600',
        'Failed': 'bg-red-600',
        'Stopped': 'bg-gray-600',
        'Cancelled': 'bg-gray-600',
        'Superseded': 'bg-gray-600',
        'Expired': 'bg-gray-600',
        'Terminating': 'bg-gray-600',
    };

    const color = statusColors[status] || 'bg-gray-600';

    return (
        <span className={`${color} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
            {status}
        </span>
    );
}

// Button Component
function Button({
    children,
    onClick,
    variant = 'primary',
    size = 'md',
    loading = false,
    disabled = false,
    type = 'button',
    className = ''
}) {
    const baseClasses = 'font-semibold rounded transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-50 dark:focus:ring-offset-gray-900 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2';

    const variantClasses = {
        primary: 'bg-indigo-600 hover:bg-indigo-700 text-white focus:ring-indigo-500',
        secondary: 'bg-gray-600 hover:bg-gray-700 text-white focus:ring-gray-500',
        danger: 'bg-red-600 hover:bg-red-700 text-white focus:ring-red-500',
    };

    const sizeClasses = {
        sm: 'px-3 py-1.5 text-sm',
        md: 'px-4 py-2 text-sm',
        lg: 'px-6 py-3 text-base',
    };

    return (
        <button
            type={type}
            onClick={onClick}
            disabled={disabled || loading}
            className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
        >
            {loading && (
                <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
            )}
            {children}
        </button>
    );
}

// Modal Component
function Modal({ isOpen, onClose, title, children, maxWidth = 'max-w-2xl' }) {
    useEffect(() => {
        const handleEscape = (e) => {
            if (e.key === 'Escape' && isOpen) {
                onClose();
            }
        };

        document.addEventListener('keydown', handleEscape);
        return () => document.removeEventListener('keydown', handleEscape);
    }, [isOpen, onClose]);

    useEffect(() => {
        if (isOpen) {
            document.body.style.overflow = 'hidden';
        } else {
            document.body.style.overflow = 'unset';
        }
        return () => {
            document.body.style.overflow = 'unset';
        };
    }, [isOpen]);

    if (!isOpen) return null;

    return (
        <div className="modal-backdrop" onClick={onClose}>
            <div className={`modal-content ${maxWidth}`} onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h3 className="modal-title">{title}</h3>
                    <button onClick={onClose} className="modal-close-button">
                        <div className="w-6 h-6 svg-mask" style={{
                            maskImage: 'url(/assets/close-x.svg)',
                            WebkitMaskImage: 'url(/assets/close-x.svg)'
                        }}></div>
                    </button>
                </div>
                <div className="modal-body">
                    {children}
                </div>
            </div>
        </div>
    );
}

// FormField Component
function FormField({
    label,
    id,
    type = 'text',
    value,
    onChange,
    error,
    required = false,
    placeholder,
    disabled = false,
    options = [],
    rows = 3,
    children
}) {
    const inputClasses = `w-full bg-white dark:bg-gray-800 border ${error ? 'border-red-500' : 'border-gray-300 dark:border-gray-700'} rounded px-3 py-2 text-gray-900 dark:text-gray-100 placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed`;

    return (
        <div className="form-field">
            <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                {label}
                {required && <span className="text-red-500 ml-1">*</span>}
            </label>

            {type === 'select' ? (
                <select
                    id={id}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                    className={inputClasses}
                >
                    {children ? children : options.map(opt => (
                        <option key={opt.value} value={opt.value}>
                            {opt.label}
                        </option>
                    ))}
                </select>
            ) : type === 'textarea' ? (
                <textarea
                    id={id}
                    value={value}
                    onChange={onChange}
                    placeholder={placeholder}
                    disabled={disabled}
                    rows={rows}
                    className={inputClasses}
                />
            ) : type === 'checkbox' ? (
                <div className="flex items-center">
                    <input
                        type="checkbox"
                        id={id}
                        checked={value}
                        onChange={onChange}
                        disabled={disabled}
                        className="w-4 h-4 bg-white dark:bg-gray-800 border-gray-300 dark:border-gray-700 rounded text-indigo-600 focus:ring-indigo-500 focus:ring-offset-gray-50 dark:focus:ring-offset-gray-900"
                    />
                    <label htmlFor={id} className="ml-2 text-sm text-gray-700 dark:text-gray-300">
                        {placeholder}
                    </label>
                </div>
            ) : (
                <input
                    type={type}
                    id={id}
                    value={value}
                    onChange={onChange}
                    placeholder={placeholder}
                    disabled={disabled}
                    className={inputClasses}
                />
            )}

            {error && (
                <p className="mt-2 text-sm text-red-500">{error}</p>
            )}
        </div>
    );
}

// ConfirmDialog Component
function ConfirmDialog({
    isOpen,
    onClose,
    onConfirm,
    title,
    message,
    confirmText = 'Confirm',
    cancelText = 'Cancel',
    variant = 'danger',
    requireConfirmation = false,
    confirmationText = '',
    loading = false
}) {
    const [inputValue, setInputValue] = useState('');
    const [error, setError] = useState('');

    const handleConfirm = () => {
        if (requireConfirmation && inputValue !== confirmationText) {
            setError(`Please type "${confirmationText}" to confirm`);
            return;
        }
        onConfirm();
    };

    const handleClose = () => {
        setInputValue('');
        setError('');
        onClose();
    };

    useEffect(() => {
        if (!isOpen) {
            setInputValue('');
            setError('');
        }
    }, [isOpen]);

    const isConfirmEnabled = !requireConfirmation || inputValue === confirmationText;

    return (
        <Modal isOpen={isOpen} onClose={handleClose} title={title} maxWidth="max-w-md">
            <div className="space-y-4">
                <p className="text-gray-700 dark:text-gray-300">{message}</p>

                {requireConfirmation && (
                    <FormField
                        label={`Type "${confirmationText}" to confirm`}
                        id="confirm-input"
                        value={inputValue}
                        onChange={(e) => setInputValue(e.target.value)}
                        error={error}
                        placeholder={confirmationText}
                    />
                )}

                <div className="flex justify-end gap-3 pt-4">
                    <Button
                        variant="secondary"
                        onClick={handleClose}
                        disabled={loading}
                    >
                        {cancelText}
                    </Button>
                    <Button
                        variant={variant}
                        onClick={handleConfirm}
                        disabled={!isConfirmEnabled}
                        loading={loading}
                    >
                        {confirmText}
                    </Button>
                </div>
            </div>
        </Modal>
    );
}

// Footer Component
function Footer({ version }) {
    return (
        <footer className="bg-white dark:bg-gray-900 border-t border-gray-200 dark:border-gray-800 mt-auto">
            <div className="container mx-auto px-4 py-4">
                <div className="flex flex-col sm:flex-row items-center justify-between gap-3 text-sm text-gray-600 dark:text-gray-400">
                    <div className="flex items-center gap-4">
                        <span className="text-gray-900 dark:text-gray-300">
                            Rise {version?.version ? `v${version.version}` : ''}
                        </span>
                        {version?.repository && (
                            <a
                                href={version.repository}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="flex items-center gap-1.5 hover:text-indigo-600 dark:hover:text-indigo-400 transition-colors"
                            >
                                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
                                </svg>
                                GitHub
                            </a>
                        )}
                    </div>
                    <div className="text-xs text-gray-500 dark:text-gray-500">
                        Container Deployment Platform
                    </div>
                </div>
            </div>
        </footer>
    );
}
