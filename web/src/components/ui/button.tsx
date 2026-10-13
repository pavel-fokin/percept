type Variant = "primary" | "secondary" | "ghost";

type ButtonProps = React.ComponentProps<"button"> & { variant: Variant };

const base =
  "inline-flex cursor-pointer items-center justify-center gap-2 rounded-md text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-50";

const variants: Record<Variant, string> = {
  primary: "bg-text font-semibold text-bg hover:bg-text/85",
  secondary: "border border-border bg-surface text-text hover:border-muted",
  ghost: "text-muted hover:bg-surface hover:text-text",
};

export function Button({ variant, className = "", ...props }: ButtonProps) {
  return <button type="button" className={`${base} h-8 px-3 ${variants[variant]} ${className}`} {...props} />;
}

// An icon alone tells no one what the button does, so the label is required.
export function IconButton({ variant, label, ...props }: Omit<ButtonProps, "aria-label" | "title" | "className"> & { label: string }) {
  return <button type="button" aria-label={label} title={label} className={`${base} size-11 ${variants[variant]}`} {...props} />;
}
