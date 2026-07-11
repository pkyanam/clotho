import type { ReactNode } from "react";

/** Consistent page width + padding for the console. */
export function PageFrame({
  children,
  className = "",
  wide = false,
}: {
  children: ReactNode;
  className?: string;
  wide?: boolean;
}) {
  return (
    <div
      className={`mx-auto px-4 py-8 sm:px-6 sm:py-10 ${
        wide ? "max-w-7xl" : "max-w-7xl"
      } ${className}`}
    >
      {children}
    </div>
  );
}

export function PageTitle({
  title,
  description,
  actions,
  eyebrow,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  eyebrow?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-5 border-b border-kumo-hairline pb-7">
      <div className="min-w-0 max-w-3xl">
        {eyebrow && (
          <div className="clotho-display mb-2 text-[0.75rem] uppercase tracking-[0.14em] text-accent-strong">
            {eyebrow}
          </div>
        )}
        <h1
          className="text-balance font-medium leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          {title}
        </h1>
        {description && (
          <p className="mt-2 max-w-2xl text-[0.9375rem] leading-relaxed text-kumo-subtle">
            {description}
          </p>
        )}
      </div>
      {actions && (
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      )}
    </div>
  );
}

export function SectionHeader({
  title,
  meta,
  actions,
}: {
  title: string;
  meta?: string;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-baseline justify-between gap-3">
      <div className="flex items-baseline gap-3">
        <h2 className="text-[0.9375rem] font-medium text-kumo-default">
          {title}
        </h2>
        {meta && (
          <span className="text-[0.8125rem] text-kumo-inactive">{meta}</span>
        )}
      </div>
      {actions}
    </div>
  );
}

export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-enter clotho-panel px-6 py-12 text-center">
      <p className="text-[0.9375rem] text-kumo-default">{title}</p>
      {description && (
        <p className="mx-auto mt-2 max-w-md text-[0.875rem] leading-relaxed text-kumo-inactive">
          {description}
        </p>
      )}
      {action && <div className="mt-6 flex justify-center">{action}</div>}
    </div>
  );
}

export function Panel({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <section className={`clotho-panel ${className}`}>{children}</section>;
}

export function StatCell({
  label,
  value,
  muted = false,
}: {
  label: string;
  value: string | number;
  muted?: boolean;
}) {
  return (
    <div className="clotho-panel overflow-hidden px-4 py-4">
      <div className="clotho-display text-[0.75rem] uppercase tracking-[0.12em] text-kumo-inactive">
        {label}
      </div>
      <div
        className={`mt-1 truncate text-[1.0625rem] font-medium ${
          muted ? "text-kumo-inactive" : "text-kumo-default"
        }`}
      >
        {value}
      </div>
    </div>
  );
}

export function SettingsSection({
  title,
  description,
  children,
  badge,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  badge?: ReactNode;
}) {
  return (
    <section className="clotho-panel overflow-hidden">
      <div className="border-b border-kumo-hairline px-5 py-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-[0.9375rem] font-medium">{title}</h2>
          {badge}
        </div>
        {description && (
          <p className="mt-1 text-[0.8125rem] leading-relaxed text-kumo-inactive">
            {description}
          </p>
        )}
      </div>
      <div className="px-5 py-4">{children}</div>
    </section>
  );
}

export function MetaRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="grid gap-1 border-b border-kumo-hairline py-3 last:border-0 sm:grid-cols-[160px_minmax(0,1fr)] sm:gap-4">
      <dt className="text-[0.8125rem] text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-all text-[0.875rem] text-kumo-default">
        {value}
      </dd>
    </div>
  );
}
