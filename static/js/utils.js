// Utility functions for Rise Dashboard
// This file depends on React being loaded first

// Date formatting
function formatDate(dateString) {
    const date = new Date(dateString);
    return date.toLocaleString();
}

function formatTimeRemaining(expiresAt) {
    if (!expiresAt) return null;

    const now = new Date();
    const expiryDate = new Date(expiresAt);
    const diffMs = expiryDate - now;
    const diffSec = Math.floor(Math.abs(diffMs) / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);

    const isExpired = diffMs < 0;
    const prefix = isExpired ? 'expired ' : 'in ';
    const suffix = isExpired ? ' ago' : '';

    if (diffDay > 0) {
        return `${prefix}${diffDay} day${diffDay > 1 ? 's' : ''}${suffix}`;
    } else if (diffHour > 0) {
        return `${prefix}${diffHour} hour${diffHour > 1 ? 's' : ''}${suffix}`;
    } else if (diffMin > 0) {
        return `${prefix}${diffMin} minute${diffMin > 1 ? 's' : ''}${suffix}`;
    } else {
        return `${prefix}${diffSec} second${diffSec !== 1 ? 's' : ''}${suffix}`;
    }
}

// Navigation helpers
function useHashLocation() {
    const { useState, useEffect } = React;
    const [hash, setHash] = useState(window.location.hash.slice(1) || 'projects');

    useEffect(() => {
        const handleHashChange = () => {
            setHash(window.location.hash.slice(1) || 'projects');
        };
        window.addEventListener('hashchange', handleHashChange);
        return () => window.removeEventListener('hashchange', handleHashChange);
    }, []);

    return hash;
}
