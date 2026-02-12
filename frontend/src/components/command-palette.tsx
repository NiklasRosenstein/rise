import { useEffect, useMemo, useState } from 'react';

export type CommandItem = {
    id: string;
    label: string;
    keywords?: string[];
    run: () => void;
};

export function CommandPalette({
    isOpen,
    onClose,
    items,
}: {
    isOpen: boolean;
    onClose: () => void;
    items: CommandItem[];
}) {
    const [query, setQuery] = useState('');
    const [activeIndex, setActiveIndex] = useState(0);

    useEffect(() => {
        if (isOpen) {
            setQuery('');
            setActiveIndex(0);
        }
    }, [isOpen]);

    const filteredItems = useMemo(() => {
        const q = query.trim().toLowerCase();
        if (!q) return items;
        return items.filter((item) => {
            const haystack = [item.label, ...(item.keywords || [])].join(' ').toLowerCase();
            return haystack.includes(q);
        });
    }, [items, query]);

    useEffect(() => {
        if (!isOpen) return;

        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                onClose();
                return;
            }
            if (e.key === 'ArrowDown') {
                e.preventDefault();
                setActiveIndex((prev) => Math.min(prev + 1, Math.max(0, filteredItems.length - 1)));
                return;
            }
            if (e.key === 'ArrowUp') {
                e.preventDefault();
                setActiveIndex((prev) => Math.max(prev - 1, 0));
                return;
            }
            if (e.key === 'Enter' && filteredItems[activeIndex]) {
                e.preventDefault();
                filteredItems[activeIndex].run();
                onClose();
            }
        };

        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isOpen, activeIndex, filteredItems, onClose]);

    if (!isOpen) return null;

    return (
        <div className="mono-palette-backdrop" onClick={onClose}>
            <div className="mono-palette" onClick={(e) => e.stopPropagation()}>
                <input
                    autoFocus
                    aria-label="Command palette"
                    value={query}
                    onChange={(e) => {
                        setQuery(e.target.value);
                        setActiveIndex(0);
                    }}
                    placeholder="Type a command..."
                    className="mono-palette-input"
                />
                <div className="mono-palette-list">
                    {filteredItems.length === 0 ? (
                        <p className="mono-palette-empty">No commands found</p>
                    ) : (
                        filteredItems.map((item, idx) => (
                            <button
                                key={item.id}
                                className={`mono-palette-item ${idx === activeIndex ? 'active' : ''}`}
                                onMouseEnter={() => setActiveIndex(idx)}
                                onClick={() => {
                                    item.run();
                                    onClose();
                                }}
                            >
                                {item.label}
                            </button>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
}
