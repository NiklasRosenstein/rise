// @ts-nocheck
import { EmptyState } from './states';
import { StatusBadge } from './ui';
import { MonoSortButton, MonoTable, MonoTableBody, MonoTableEmptyRow, MonoTableFrame, MonoTableHead, MonoTableRow, MonoTd, MonoTh } from './table';

function OwnerCell({ project }) {
    const ownerType = project.owner_user_email ? 'user' : project.owner_team_name ? 'team' : null;
    const ownerLabel = project.owner_user_email || project.owner_team_name || '-';

    return (
        <span className="inline-flex items-center gap-2">
            {ownerType === 'user' && (
                <span
                    className="w-3 h-3 svg-mask inline-block"
                    aria-hidden="true"
                    style={{
                        maskImage: 'url(/assets/user.svg)',
                        WebkitMaskImage: 'url(/assets/user.svg)',
                    }}
                />
            )}
            {ownerType === 'team' && (
                <svg className="w-3 h-3" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                    <path d="M7 10a3 3 0 1 0-3-3 3 3 0 0 0 3 3Zm6 0a3 3 0 1 0-3-3 3 3 0 0 0 3 3ZM1.5 16.5a5.5 5.5 0 0 1 11 0v.5h-11Zm12 0a5.5 5.5 0 0 1 5-5.48 5.53 5.53 0 0 1 .5.02V17h-5.5Z" />
                </svg>
            )}
            <span>{ownerLabel}</span>
        </span>
    );
}

export function ProjectTable({
    projects,
    sortKey,
    sortDirection,
    requestSort,
    onRowClick,
    onKeyDown,
    activeIndex,
    setActiveIndex,
    emptyMessage = 'No projects found.',
    emptyActionLabel,
    onEmptyAction,
}) {
    const sortableHeader = (label, key) => (
        requestSort ? (
            <MonoSortButton
                label={label}
                active={sortKey === key}
                direction={sortDirection}
                onClick={() => requestSort(key)}
            />
        ) : label
    );

    return (
        <MonoTableFrame>
            <MonoTable className="mono-sticky-table mono-table--sticky" onKeyDown={onKeyDown}>
                <MonoTableHead>
                    <tr>
                        <MonoTh stickyCol className="px-6 py-3 text-left">{sortableHeader('Name', 'name')}</MonoTh>
                        <MonoTh className="px-6 py-3 text-left">{sortableHeader('Status', 'status')}</MonoTh>
                        <MonoTh className="px-6 py-3 text-left">Owner</MonoTh>
                        <MonoTh className="px-6 py-3 text-left">Access Class</MonoTh>
                        <MonoTh className="px-6 py-3 text-left">URL</MonoTh>
                    </tr>
                </MonoTableHead>
                <MonoTableBody>
                    {projects.length === 0 ? (
                        <MonoTableEmptyRow colSpan={5}>
                            {emptyActionLabel && onEmptyAction ? (
                                <EmptyState message={emptyMessage} actionLabel={emptyActionLabel} onAction={onEmptyAction} />
                            ) : (
                                emptyMessage
                            )}
                        </MonoTableEmptyRow>
                    ) : (
                        projects.map((project, idx) => (
                            <MonoTableRow
                                key={project.id}
                                onClick={() => onRowClick(project)}
                                onFocus={() => setActiveIndex?.(idx)}
                                tabIndex={0}
                                aria-label={`Project ${project.name}`}
                                interactive
                                active={activeIndex === idx}
                                className={activeIndex === idx ? 'mono-row-active transition-colors' : 'transition-colors'}
                            >
                                <MonoTd stickyCol className="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100">{project.name}</MonoTd>
                                <MonoTd className="px-6 py-4 whitespace-nowrap text-sm"><StatusBadge status={project.status} /></MonoTd>
                                <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300"><OwnerCell project={project} /></MonoTd>
                                <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{project.access_class || '-'}</MonoTd>
                                <MonoTd className="px-6 py-4 whitespace-nowrap text-sm">
                                    {project.primary_url ? (
                                        <a
                                            href={project.primary_url}
                                            target="_blank"
                                            rel="noopener noreferrer"
                                            className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300"
                                            onClick={(e) => e.stopPropagation()}
                                        >
                                            {project.primary_url}
                                        </a>
                                    ) : '-'}
                                </MonoTd>
                            </MonoTableRow>
                        ))
                    )}
                </MonoTableBody>
            </MonoTable>
        </MonoTableFrame>
    );
}
