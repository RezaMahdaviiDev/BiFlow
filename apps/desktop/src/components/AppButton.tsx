import type { ButtonHTMLAttributes, ReactNode } from "react";

export const BUTTON_ICON_PX = 18;

const withIcon = "inline-flex items-center justify-center gap-2 text-start";

export function AppButton({
  icon,
  children,
  className = "",
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { icon?: ReactNode }) {
  return (
    <button type={type} className={`${withIcon} ${className}`} {...props}>
      {icon}
      {children}
    </button>
  );
}

export function IconOnlyButton({
  label,
  children,
  className = "",
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type={type}
      aria-label={label}
      title={label}
      className={className}
      {...props}
    >
      {children}
    </button>
  );
}
