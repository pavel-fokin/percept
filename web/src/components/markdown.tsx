import { createElement, memo, type ComponentType, type JSX } from "react";
import ReactMarkdown, { type Components, type ExtraProps } from "react-markdown";
import remarkGfm from "remark-gfm";
import { textLink } from "./ui/text-link";

type Tag = keyof JSX.IntrinsicElements;

// react-markdown passes the syntax node as a prop; it must not reach the DOM.
function styled<T extends Tag>(Tag: T, className: string): ComponentType<JSX.IntrinsicElements[T] & ExtraProps> {
  return ({ node: _, ...props }) => createElement(Tag, { className, ...props });
}

// Every level is body size: text inside a message must not outrank the page's own structure.
const heading = "mt-6 mb-2 font-semibold";
const nested = "[&_ul]:mt-2 [&_ul]:mb-0 [&_ol]:mt-2 [&_ol]:mb-0";

const components: Components = {
  h1: styled("h1", heading),
  h2: styled("h2", heading),
  h3: styled("h3", heading),
  h4: styled("h4", heading),
  h5: styled("h5", heading),
  h6: styled("h6", heading),
  p: styled("p", "mb-4"),
  ul: styled("ul", `mb-4 list-disc pl-[22px] ${nested}`),
  ol: styled("ol", `mb-4 list-decimal pl-[22px] ${nested}`),
  li: styled("li", "mb-2 marker:text-muted"),
  a: styled("a", textLink),
  code: styled("code", "rounded bg-surface px-[5px] py-px font-mono text-sm"),
  pre: styled(
    "pre",
    "mb-4 overflow-x-auto rounded-[10px] bg-surface px-4 py-3.5 font-mono text-[13.5px] leading-[22px] [&>code]:rounded-none [&>code]:bg-transparent [&>code]:p-0 [&>code]:text-[13.5px]",
  ),
  table: ({ node: _, ...props }) => (
    <div className="mb-4 overflow-x-auto">
      <table className="border-collapse" {...props} />
    </div>
  ),
  th: styled("th", "border-b border-border py-2 pr-5 text-left font-semibold text-muted"),
  td: styled("td", "border-b border-border py-2 pr-5"),
};

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="min-w-0 leading-[26px] wrap-anywhere [&>:first-child]:mt-0 [&>:last-child]:mb-0">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
