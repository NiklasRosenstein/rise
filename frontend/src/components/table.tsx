import type { HTMLAttributes, KeyboardEventHandler, ReactNode } from 'react';

type Classy = { className?: string };

function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(' ');
}

export function MonoTableFrame({ children, className }: { children: ReactNode } & Classy) {
  return <div className={cx('mono-table-wrap', className)}>{children}</div>;
}

export function MonoTable({ children, className, onKeyDown }: { children: ReactNode; onKeyDown?: KeyboardEventHandler<HTMLElement> } & Classy) {
  return (
    <table className={cx('mono-table', className)} onKeyDown={onKeyDown}>
      {children}
    </table>
  );
}

export function MonoTableHead({ children, className }: { children: ReactNode } & Classy) {
  return <thead className={cx('mono-table-head', className)}>{children}</thead>;
}

export function MonoTableBody({ children, className }: { children: ReactNode } & Classy) {
  return <tbody className={cx('mono-table-body', className)}>{children}</tbody>;
}

export function MonoTableRow({
  children,
  className,
  interactive,
  active,
  highlight,
  ...rest
}: { children: ReactNode; interactive?: boolean; active?: boolean; highlight?: boolean } & Classy & HTMLAttributes<HTMLTableRowElement>) {
  return (
    <tr
      className={cx(
        interactive && 'mono-table-row--interactive',
        active && 'mono-table-row--active',
        highlight && 'mono-table-row--highlight',
        className,
      )}
      {...rest}
    >
      {children}
    </tr>
  );
}

export function MonoTh({ children, className, stickyCol, ...rest }: { children: ReactNode; stickyCol?: boolean } & Classy & HTMLAttributes<HTMLTableCellElement>) {
  return (
    <th className={cx('mono-table-th', stickyCol && 'mono-sticky-col', className)} {...rest}>
      {children}
    </th>
  );
}

export function MonoTd({ children, className, stickyCol, mono, ...rest }: { children: ReactNode; stickyCol?: boolean; mono?: boolean } & Classy & HTMLAttributes<HTMLTableCellElement>) {
  return (
    <td className={cx(stickyCol && 'mono-sticky-col', mono && 'font-mono', className)} {...rest}>
      {children}
    </td>
  );
}

export function MonoSortButton({ label, active, direction, onClick, className }: { label: string; active?: boolean; direction?: 'asc' | 'desc'; onClick: () => void } & Classy) {
  return (
    <button type="button" className={cx('mono-sort-button', className)} onClick={onClick}>
      {label} {active ? (direction === 'asc' ? '↑' : '↓') : ''}
    </button>
  );
}

export function MonoTableEmptyRow({ colSpan, children, className }: { colSpan: number; children: ReactNode } & Classy) {
  return (
    <tr>
      <td colSpan={colSpan} className={cx('mono-table-empty px-6 py-8', className)}>
        {children}
      </td>
    </tr>
  );
}
