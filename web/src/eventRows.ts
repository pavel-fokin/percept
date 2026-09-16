import type { Event } from "./types";

/** Who a row's header names, and the tier its label is set in - `You`
 * in `accent`, everyone else in `muted`. */
export interface Speaker {
  label: string;
  toneClass: "text-accent" | "text-muted";
}

/** `event`'s speaker, from `actor.kind` alone: `You` for a human, the
 * writing client's name for an agent (`claude-code` reads `CLAUDE
 * CODE` under the row's own `uppercase`), `percept` for the system. */
export function speakerOf(event: Event): Speaker {
  switch (event.actor.kind) {
    case "human":
      return { label: "You", toneClass: "text-accent" };
    case "agent":
      return { label: event.source.name.replace(/-/g, " "), toneClass: "text-muted" };
    case "system":
      return { label: "percept", toneClass: "text-muted" };
  }
}

/** The key consecutive events share a header under - the speaker's
 * kind, plus the writing client's name for an agent, since two agents
 * writing to the same project are two speakers. */
function speakerKey(event: Event): string {
  return event.actor.kind === "agent" ? `agent:${event.source.name}` : event.actor.kind;
}

/** One run of consecutive events by the same speaker - what shares one
 * header down the page. */
export interface Run {
  speaker: Speaker;
  events: Event[];
}

/** `events`, folded into runs of consecutive events by the same
 * speaker - the header each row carries is the run's, not the
 * event's. */
export function groupRuns(events: Event[]): Run[] {
  const runs: Run[] = [];
  let key: string | null = null;
  for (const event of events) {
    const nextKey = speakerKey(event);
    const last = runs[runs.length - 1];
    if (last && key === nextKey) {
      last.events.push(event);
    } else {
      runs.push({ speaker: speakerOf(event), events: [event] });
    }
    key = nextKey;
  }
  return runs;
}

/** One kind, in the plain words the details line carries - `null` when
 * the row already says it: the quotes and the serif on a message, the
 * content itself on a session. Falls back to the raw type for a kind
 * this page does not know. */
export function kindWords(event: Event): string | null {
  switch (event.type) {
    case "message.received":
      return null;
    case "thought.recorded":
      return "thought";
    case "tool.called":
      return "ran a tool";
    case "tool.resulted":
      return "a tool answered";
    case "node.added": {
      const kind = stringField(event.payload, "kind") || "node";
      return `${kind} created`;
    }
    case "node.changed":
      return "changed";
    case "node.removed":
      return "removed";
    case "edge.added":
      return "linked";
    case "edge.removed":
      return "unlinked";
    case "model.called":
      return "called a model";
    // The row's content is already the words `session started`; a
    // details line repeating them says nothing twice.
    case "session.started":
      return null;
    case "file.cited":
      return "cited a file";
    default:
      return event.type;
  }
}

/** How a row's content is set - the face says what produced it:
 * `quote` for a thing said (serif, quoted), `mono` for what a machine
 * emitted, `plain` for a thing percept recorded. */
export type ContentVariant = "quote" | "mono" | "plain";

export interface Content {
  text: string;
  variant: ContentVariant;
}

/** `event`'s content, per its type - what a row's full-weight line
 * shows. */
export function contentOf(event: Event): Content {
  const payload = event.payload;
  switch (event.type) {
    case "message.received":
      return { text: stringField(payload, "content"), variant: "quote" };
    case "thought.recorded":
      return { text: stringField(payload, "content"), variant: "plain" };
    // A tool's output is not prose. Set as one it is the loudest thing
    // on a page it makes up most of, and the envelope it arrives in
    // belongs to the tool, so unwrapping it would be right for one tool
    // and wrong for the rest.
    case "tool.resulted":
      return { text: stringField(payload, "content"), variant: "mono" };
    case "tool.called": {
      const tool = stringField(payload, "tool");
      const argument = firstArgument(payload);
      return { text: argument ? `${tool} ${argument}` : tool, variant: "mono" };
    }
    case "node.added":
    case "node.changed":
    case "node.removed":
      return { text: stringField(payload, "name") || stringField(payload, "node"), variant: "plain" };
    // An edge's ends are node ids, and resolving them to names needs the
    // map this view does not fold. The kind is the part a reader can use
    // without it; both ends are one tap away in the row's own panel.
    case "edge.added":
    case "edge.removed":
      return { text: stringField(payload, "kind"), variant: "plain" };
    case "file.cited":
      return { text: stringField(payload, "path"), variant: "plain" };
    case "session.started":
      return { text: "session started", variant: "plain" };
    case "model.called":
      return { text: stringField(payload, "model"), variant: "plain" };
    default:
      return { text: event.type, variant: "plain" };
  }
}

/** `payload[key]` as a string, or "" when it isn't one - every content
 * and kind derivation above reads a payload field through this rather
 * than trusting its shape. */
function stringField(payload: Record<string, unknown>, key: string): string {
  const value = payload[key];
  return typeof value === "string" ? value : "";
}

/** `tool.called`'s first argument value, in call order - what the
 * model actually passed, not the tool's name. `arguments` is a JSON
 * object, and both the log and the browser keep an object's string
 * keys in insertion order, so "first" means what the log wrote first. */
function firstArgument(payload: Record<string, unknown>): string {
  const args = payload.arguments;
  if (!args || typeof args !== "object") return "";
  const first = Object.values(args as Record<string, unknown>)[0];
  if (typeof first === "string") return first;
  return first === undefined ? "" : JSON.stringify(first);
}

/** The basename of `event.source.path` - the project the details line
 * names. */
export function projectOf(event: Event): string {
  const trimmed = event.source.path.replace(/\/+$/, "");
  const parts = trimmed.split("/");
  return parts[parts.length - 1] || trimmed;
}

/** The interface is written in English, so its dates are formatted in
 * English too - an open locale would read `September 16` on one machine
 * and `16 September` on the next, in a row whose every other word is
 * fixed. */
const LOCALE = "en-GB";

const TIME = new Intl.DateTimeFormat(LOCALE, { hour: "2-digit", minute: "2-digit", hour12: false });

/** `event.created_at` as `HH:MM`, 24-hour, in the browser's local
 * time. */
export function timeOf(event: Event): string {
  return TIME.format(new Date(event.created_at));
}

const DAY_MONTH = new Intl.DateTimeFormat(LOCALE, { day: "numeric", month: "long" });

/** The local calendar date `iso` falls on, as a grouping key - not a
 * display string. */
function localDayKey(iso: string): string {
  const date = new Date(iso);
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

/** One day's events, newest first, with the heading the day reads
 * under. */
export interface DayGroup {
  key: string;
  label: string;
  events: Event[];
}

/** `events` - oldest first, as `GET /api/events` returns them - folded
 * into days newest first, each day's own events newest first. `now`
 * is the moment "Today" is measured against, a parameter so a test
 * does not depend on the clock. */
export function groupByDay(events: Event[], now: Date = new Date()): DayGroup[] {
  const groups: DayGroup[] = [];
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    const key = localDayKey(event.created_at);
    const last = groups[groups.length - 1];
    if (last && last.key === key) {
      last.events.push(event);
    } else {
      groups.push({ key, label: dayLabel(event.created_at, now), events: [event] });
    }
  }
  return groups;
}

/** `iso`'s calendar date against `now` - `Today, 16 September` when
 * they land on the same local day, else `12 September`. */
function dayLabel(iso: string, now: Date): string {
  const date = new Date(iso);
  const isToday =
    date.getFullYear() === now.getFullYear() && date.getMonth() === now.getMonth() && date.getDate() === now.getDate();
  const formatted = DAY_MONTH.format(date);
  return isToday ? `Today, ${formatted}` : formatted;
}
