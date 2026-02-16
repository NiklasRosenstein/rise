import { useEffect, useRef, useState } from 'react';

function cx(...parts: Array<string | false | null | undefined>) {
    return parts.filter(Boolean).join(' ');
}

export function StatusBadge({ status }) {
    const statusColors = {
        Healthy: 'mono-status-ok',
        Running: 'mono-status-ok',
        Deploying: 'mono-status-warn',
        Pending: 'mono-status-warn',
        Building: 'mono-status-warn',
        Pushing: 'mono-status-warn',
        Pushed: 'mono-status-warn',
        Unhealthy: 'mono-status-bad',
        Failed: 'mono-status-bad',
        Stopped: 'mono-status-muted',
        Cancelled: 'mono-status-muted',
        Superseded: 'mono-status-muted',
        Expired: 'mono-status-muted',
        Terminating: 'mono-status-muted',
    };

    const color = statusColors[status] || 'mono-status-muted';

    return <span className={`mono-status ${color}`}>{status}</span>;
}

export function Button({
    children,
    onClick,
    variant = 'primary',
    size = 'md',
    loading = false,
    disabled = false,
    type = 'button',
    className = '',
}) {
    const baseClasses = 'mono-btn';
    const variantClasses = {
        primary: 'mono-btn-primary',
        secondary: 'mono-btn-secondary',
        danger: 'mono-btn-danger',
    };
    const sizeClasses = {
        sm: 'mono-btn-sm',
        md: 'mono-btn-md',
        lg: 'mono-btn-lg',
    };

    return (
        <button
            type={type as 'button' | 'submit' | 'reset'}
            onClick={onClick}
            disabled={disabled || loading}
            className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
        >
            {loading && <div className="mono-spinner" />}
            {children}
        </button>
    );
}

export function Modal({
    isOpen,
    onClose,
    title,
    children,
    maxWidth = 'max-w-2xl',
    modalClassName = '',
    bodyClassName = '',
}) {
    const [bodyElement, setBodyElement] = useState<HTMLDivElement | null>(null);

    useEffect(() => {
        const handleEscape = (e) => {
            if (e.key === 'Escape' && isOpen) {
                onClose();
            }
        };
        const handleNavigate = () => {
            if (isOpen) onClose();
        };
        const handleCloseAll = () => {
            if (isOpen) onClose();
        };

        document.addEventListener('keydown', handleEscape);
        window.addEventListener('rise:navigate', handleNavigate as EventListener);
        window.addEventListener('rise:close-modals', handleCloseAll as EventListener);
        return () => {
            document.removeEventListener('keydown', handleEscape);
            window.removeEventListener('rise:navigate', handleNavigate as EventListener);
            window.removeEventListener('rise:close-modals', handleCloseAll as EventListener);
        };
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

    useEffect(() => {
        if (!isOpen || !bodyElement) return;

        const timer = window.setTimeout(() => {
            const firstInput = bodyElement.querySelector<HTMLElement>(
                '[data-autofocus], input:not([type="hidden"]):not([disabled]), select:not([disabled]), textarea:not([disabled])'
            );
            firstInput?.focus();
        }, 0);

        return () => window.clearTimeout(timer);
    }, [isOpen, bodyElement]);

    if (!isOpen) return null;

    return (
        <div className="modal-backdrop" onClick={onClose}>
            <div className={cx('modal-content mono-modal-shell', maxWidth, modalClassName)} onClick={(e) => e.stopPropagation()}>
                <div className="modal-header mono-modal-header">
                    <h3 className="modal-title">{title}</h3>
                    <button onClick={onClose} className="modal-close-button" aria-label="Close modal">
                        <div
                            className="w-6 h-6 svg-mask"
                            style={{
                                maskImage: 'url(/assets/close-x.svg)',
                                WebkitMaskImage: 'url(/assets/close-x.svg)',
                            }}
                        ></div>
                    </button>
                </div>
                <div className={cx('modal-body mono-modal-body', bodyClassName)} ref={setBodyElement}>{children}</div>
            </div>
        </div>
    );
}

export function ModalSection({ children, className = '' }) {
    return <div className={cx('mono-modal-section', className)}>{children}</div>;
}

export function ModalActions({ children, className = '' }) {
    return <div className={cx('mono-modal-actions', className)}>{children}</div>;
}

export function ModalTabs({ children, className = '' }) {
    return <div className={cx('mono-modal-tabs', className)}>{children}</div>;
}

export function SegmentedRadioGroup({
    label,
    name,
    value,
    onChange,
    options = [],
    ariaLabel,
    className = '',
}) {
    return (
        <div className={cx('form-field', className)}>
            {label && <p className="mono-label">{label}</p>}
            <div className="mono-segmented mt-1" role="radiogroup" aria-label={ariaLabel || label || name}>
                {options.map((option) => (
                    <label key={option.value} className={`mono-segmented-option ${value === option.value ? 'active' : ''}`}>
                        <input
                            type="radio"
                            name={name}
                            value={option.value}
                            checked={value === option.value}
                            onChange={() => onChange(option.value)}
                            className="mono-segmented-input"
                        />
                        <span>{option.label}</span>
                    </label>
                ))}
            </div>
        </div>
    );
}

export function FormField({
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
    list = undefined,
    children = null,
}) {
    const inputClasses = `mono-input ${error ? 'mono-input-error' : ''}`;

    return (
        <div className="form-field">
            <label htmlFor={id} className="mono-label">
                {label}
                {required && <span className="text-red-300 ml-1">*</span>}
            </label>

            {type === 'select' ? (
                <select id={id} value={value} onChange={onChange} disabled={disabled} className={inputClasses}>
                    {children
                        ? children
                        : options.map((opt) => (
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
                    <input type="checkbox" id={id} checked={value} onChange={onChange} disabled={disabled} className="mono-checkbox" />
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
                    list={list}
                    className={inputClasses}
                />
            )}

            {error && <p className="mt-2 text-sm text-red-300">{error}</p>}
        </div>
    );
}

export function AutocompleteInput({
    id,
    value,
    onChange,
    options = [],
    placeholder = '',
    disabled = false,
    loading = false,
    multiValue = false,
    noMatchesText = 'No matches',
    className = '',
    onEnter,
}) {
    const [isOpen, setIsOpen] = useState(false);
    const ref = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        function handleClickOutside(e: MouseEvent) {
            if (ref.current && !ref.current.contains(e.target as Node)) {
                setIsOpen(false);
            }
        }
        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, []);

    const rawQuery = multiValue ? (value || '').split(',').pop() || '' : (value || '');
    const query = rawQuery.trim().toLowerCase();
    const uniqueOptions = Array.from(new Set((options || []).filter(Boolean)));
    const filteredOptions = uniqueOptions.filter((opt) => opt.toLowerCase().includes(query));

    const handleSelect = (selected: string) => {
        if (multiValue) {
            const chunks = (value || '').split(',');
            const prefix = chunks.slice(0, -1).map((p) => p.trim()).filter(Boolean).join(', ');
            onChange(prefix ? `${prefix}, ${selected}` : selected);
        } else {
            onChange(selected);
        }
        setIsOpen(false);
    };

    return (
        <div ref={ref} className={cx('relative', className)}>
            <input
                type="text"
                id={id}
                className="mono-input w-full"
                placeholder={placeholder}
                value={value}
                onChange={(e) => {
                    onChange(e.target.value);
                    if (!isOpen) setIsOpen(true);
                }}
                onKeyDown={(e) => {
                    if (e.key === 'Enter' && onEnter) onEnter();
                }}
                onFocus={() => setIsOpen(true)}
                onClick={() => setIsOpen(true)}
                disabled={disabled || loading}
            />
            {isOpen && (
                <div className="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded shadow-lg max-h-48 overflow-y-auto">
                    {loading ? (
                        <div className="px-3 py-2 text-sm text-gray-500">Loading...</div>
                    ) : filteredOptions.length === 0 ? (
                        <div className="px-3 py-2 text-sm text-gray-500">{noMatchesText}</div>
                    ) : (
                        filteredOptions.map((opt) => (
                            <button
                                key={opt}
                                type="button"
                                className="w-full text-left px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700"
                                onClick={() => handleSelect(opt)}
                            >
                                {opt}
                            </button>
                        ))
                    )}
                </div>
            )}
        </div>
    );
}

export function ConfirmDialog({
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
    loading = false,
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
            <ModalSection>
                <p className="text-gray-200">{message}</p>

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

                <ModalActions>
                    <Button variant="secondary" onClick={handleClose} disabled={loading}>
                        {cancelText}
                    </Button>
                    <Button variant={variant} onClick={handleConfirm} disabled={!isConfirmEnabled} loading={loading}>
                        {confirmText}
                    </Button>
                </ModalActions>
            </ModalSection>
        </Modal>
    );
}

export function Footer({ version }) {
    return (
        <footer className="mono-footer">
            <div className="mono-footer-inner">
                <div className="flex items-center gap-4">
                    <span>Rise {version?.version ? `v${version.version}` : ''}</span>
                    {version?.repository && (
                        <a href={version.repository} target="_blank" rel="noopener noreferrer" className="underline">
                            github
                        </a>
                    )}
                </div>
                <div className="text-xs">container deployment platform</div>
            </div>
        </footer>
    );
}
