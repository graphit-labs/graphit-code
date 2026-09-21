import type { ReactNode } from "react";
import { ArrowRight, Search } from "lucide-react";
import { cn } from "@/lib/utils";
import "./engineering.css";

export function WorkPage({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={cn("work-page", className)}>{children}</div>;
}
export function WorkHeader({
  title,
  description,
  actions,
  context,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
  context?: ReactNode;
}) {
  return (
    <header className="work-header">
      <div>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      <div className="work-header-actions">
        {context}
        {actions}
      </div>
    </header>
  );
}
export function WorkSection({
  title,
  description,
  children,
  actions,
  id,
  className,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  actions?: ReactNode;
  id?: string;
  className?: string;
}) {
  return (
    <section id={id} className={cn("work-section", className)}>
      <header>
        <div>
          <h2>{title}</h2>
          {description && <p>{description}</p>}
        </div>
        {actions}
      </header>
      {children}
    </section>
  );
}
export function FactList({ items }: { items: Array<[string, ReactNode]> }) {
  return (
    <dl className="work-facts">
      {items.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value == null || value === "" ? "—" : value}</dd>
        </div>
      ))}
    </dl>
  );
}
export function WorkNotice({
  title,
  children,
  tone = "neutral",
}: {
  title: string;
  children?: ReactNode;
  tone?: "neutral" | "error" | "success";
}) {
  return (
    <div
      className={`work-notice ${tone}`}
      role={tone === "error" ? "alert" : undefined}
    >
      <strong>{title}</strong>
      {children && <div>{children}</div>}
    </div>
  );
}
export function WorkSearch({
  value,
  onChange,
  label,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  label: string;
  placeholder?: string;
}) {
  return (
    <label className="work-search">
      <Search size={17} />
      <input
        aria-label={label}
        placeholder={placeholder || label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
export function WorkEmpty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="work-empty">
      <span aria-hidden="true" className="work-empty-mark">
        [ ]
      </span>
      <h2>{title}</h2>
      <p>{children}</p>
      {action}
    </div>
  );
}
export function RecordLink({
  title,
  meta,
  onClick,
  selected,
  children,
}: {
  title: string;
  meta?: ReactNode;
  onClick: () => void;
  selected?: boolean;
  children?: ReactNode;
}) {
  return (
    <button
      className={cn("record-link", selected && "selected")}
      onClick={onClick}
    >
      <span>
        <strong>{title}</strong>
        {meta && <small>{meta}</small>}
      </span>
      {children}
      <ArrowRight size={16} />
    </button>
  );
}
export function WorkTabs({
  value,
  onChange,
  items,
  label,
}: {
  value: string;
  onChange: (id: string) => void;
  items: Array<[string, string]>;
  label: string;
}) {
  return (
    <div className="work-tabs" role="tablist" aria-label={label}>
      {items.map(([id, title], index) => (
        <button
          key={id}
          role="tab"
          aria-selected={value === id}
          tabIndex={value === id ? 0 : -1}
          onClick={() => onChange(id)}
          onKeyDown={(e) => {
            let next = index;
            if (e.key === "ArrowRight") next = (index + 1) % items.length;
            else if (e.key === "ArrowLeft")
              next = (index + items.length - 1) % items.length;
            else if (e.key === "Home") next = 0;
            else if (e.key === "End") next = items.length - 1;
            else return;
            e.preventDefault();
            onChange(items[next][0]);
            (
              e.currentTarget.parentElement?.children[next] as HTMLElement
            )?.focus();
          }}
        >
          {title}
        </button>
      ))}
    </div>
  );
}
