type Variant = "primary" | "secondary" | "ghost";

type ButtonProps = React.ComponentProps<"button"> & { variant: Variant };

const base =
  "inline-flex cursor-pointer items-center justify-center gap-2 rounded-md text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-50";

const ghost = "text-muted hover:bg-border/50 hover:text-text";

// primary: the one main action of a screen; secondary: an ordinary action; ghost: a quiet action in a toolbar or panel.
const variants: Record<Variant, string> = {
  primary: "bg-text font-semibold text-bg hover:bg-text/85",
  secondary: "border border-border bg-surface text-text hover:border-muted",
  ghost,
};

// Height follows the pointer: 44px on touch, 32px with a mouse.
export function Button({ variant, className = "", ...props }: ButtonProps) {
  return <button type="button" className={`${base} h-11 px-3 pointer-fine:h-8 ${variants[variant]} ${className}`} {...props} />;
}

// An icon-only action. An icon alone tells no one what the button does, so the label is required.
export function IconButton({ variant, label, ...props }: Omit<ButtonProps, "aria-label" | "title" | "className"> & { label: string }) {
  return <button type="button" aria-label={label} title={label} className={`${base} size-11 pointer-fine:size-8 ${variants[variant]}`} {...props} />;
}
