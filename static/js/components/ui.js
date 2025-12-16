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
    const baseClasses = 'font-semibold rounded transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-900 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2';

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
                        <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
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
    const inputClasses = `w-full bg-gray-800 border ${error ? 'border-red-500' : 'border-gray-700'} rounded px-3 py-2 text-gray-100 placeholder-gray-500 focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed`;

    return (
        <div className="form-field">
            <label htmlFor={id} className="block text-sm font-medium text-gray-300 mb-2">
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
                        className="w-4 h-4 bg-gray-800 border-gray-700 rounded text-indigo-600 focus:ring-indigo-500 focus:ring-offset-gray-900"
                    />
                    <label htmlFor={id} className="ml-2 text-sm text-gray-300">
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
                <p className="text-gray-300">{message}</p>

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
