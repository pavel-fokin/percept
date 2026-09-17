import type { ButtonHTMLAttributes } from "react";

/** A `<button>` that defaults to `type="button"`, so a click inside a
 * form never submits it by accident. Every other attribute, including
 * `className`, passes straight through. */
export default function Button({ type = "button", ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type={type} {...props} />;
}
