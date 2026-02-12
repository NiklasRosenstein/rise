import { useMemo, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';

export function useSortableData<T extends Record<string, any>>(items: T[], initialKey: keyof T | null = null, initialDirection: 'asc' | 'desc' = 'asc') {
    const [sortKey, setSortKey] = useState<keyof T | null>(initialKey);
    const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>(initialDirection);

    const sortedItems = useMemo(() => {
        if (!sortKey) return items;

        const sorted = [...items].sort((a, b) => {
            const av = a[sortKey];
            const bv = b[sortKey];

            if (av == null && bv == null) return 0;
            if (av == null) return 1;
            if (bv == null) return -1;

            if (typeof av === 'number' && typeof bv === 'number') {
                return av - bv;
            }

            return String(av).localeCompare(String(bv), undefined, { numeric: true, sensitivity: 'base' });
        });

        return sortDirection === 'asc' ? sorted : sorted.reverse();
    }, [items, sortKey, sortDirection]);

    const requestSort = (key: keyof T) => {
        if (sortKey === key) {
            setSortDirection((prev) => (prev === 'asc' ? 'desc' : 'asc'));
            return;
        }
        setSortKey(key);
        setSortDirection('asc');
    };

    return { sortedItems, sortKey, sortDirection, requestSort };
}

export function useRowKeyboardNavigation(onSelect: (index: number) => void, rowCount: number) {
    const [activeIndex, setActiveIndex] = useState(-1);

    const onKeyDown = (e: ReactKeyboardEvent<HTMLElement>) => {
        if (rowCount === 0) return;

        if (e.key === 'ArrowDown') {
            e.preventDefault();
            setActiveIndex((prev) => Math.min(prev + 1, rowCount - 1));
            return;
        }

        if (e.key === 'ArrowUp') {
            e.preventDefault();
            setActiveIndex((prev) => Math.max(prev - 1, 0));
            return;
        }

        if (e.key === 'Enter') {
            e.preventDefault();
            if (activeIndex >= 0) {
                onSelect(activeIndex);
            }
        }
    };

    return { activeIndex, setActiveIndex, onKeyDown };
}
