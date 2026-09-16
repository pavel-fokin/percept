import type { Event } from "./types";

/** The interface is written in English, so its numbers and dates are
 * formatted in English too - an open locale would read `September 16`
 * on one machine and `16 September` on the next, in a row whose every
 * other word is fixed. */
const LOCALE = "en-GB";

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

/** One row of the log: an event, and the tool result the log records
 * as caused by it, when there is one. A call and its answer are one
 * act - the answer is not a second thing that happened. */
export interface Row {
  event: Event;
  answer?: Event;
}

/** `events` as rows, one per event - `GET /api/events` never returns a
 * folded event (a `tool.resulted`, today) among them - with `carried`
 * matched onto the row whose id its `causation_id` names. */
export function foldResults(events: Event[], carried: Event[]): Row[] {
  const answerByCause = new Map(carried.map((event) => [event.causation_id, event]));
  return events.map((event) => ({ event, answer: answerByCause.get(event.id) }));
}

/** `event`'s `content`, and whether the list shortened it. `preview`
 * is present only when the server cut, and its `len` is the whole
 * length - so a row knows both what it has and what it is missing. */
export function contentText(event: Event): { text: string; cut: boolean } {
  const key = event.type === "file.cited" ? "excerpt" : "content";
  return { text: stringField(event.payload, key), cut: event.preview !== undefined };
}

/** How much a tool answered with, for the details line. The whole
 * length, not the preview's: `preview.len` is what the server cut
 * from, and is absent when nothing was cut. */
export function answerSize(answer: Event): string {
  const length = answer.preview?.len ?? stringField(answer.payload, "content").length;
  return length === 0 ? "no output" : `${length.toLocaleString(LOCALE)} characters`;
}

/** The key consecutive events share a header under. A source change
 * starts a new run even when both events have the same actor label. */
function speakerKey(event: Event): string {
  return JSON.stringify([event.actor.kind, event.source.name, event.source.path]);
}

const RUN_GAP_MS = 30 * 60 * 1000;

function withinRunGap(previous: Event, next: Event): boolean {
  return Math.abs(Date.parse(previous.created_at) - Date.parse(next.created_at)) <= RUN_GAP_MS;
}

/** One run of nearby rows with the same attributed actor and source. */
export interface Run {
  speaker: Speaker;
  rows: [Row, ...Row[]];
}

/** `rows`, in display order, folded into runs by the same source.
 * Events more than 30 minutes apart start separate runs even when
 * their source matches. */
export function groupRuns(rows: Row[]): Run[] {
  const runs: Run[] = [];
  let key: string | null = null;
  for (const row of rows) {
    const nextKey = speakerKey(row.event);
    const last = runs[runs.length - 1];
    if (last && key === nextKey && withinRunGap(last.rows[last.rows.length - 1].event, row.event)) {
      last.rows.push(row);
    } else {
      runs.push({ speaker: speakerOf(row.event), rows: [row] });
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

/** The count line's words: `shown` alone once everything matching has
 * loaded, `shown` of `total` while more remains behind `Show earlier`. */
export function eventsCountLabel(shown: number, total: number): string {
  const loaded = shown.toLocaleString(LOCALE);
  return shown >= total ? `${loaded} events, newest first.` : `${loaded} of ${total.toLocaleString(LOCALE)} events, newest first.`;
}

/** The last segment of a path - how a project is named on screen. */
export function basename(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const parts = trimmed.split("/");
  return parts[parts.length - 1] || trimmed;
}

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

/** One day's rows, newest first, with the heading the day reads
 * under. */
export interface DayGroup {
  key: string;
  label: string;
  rows: Row[];
}

/** `rows` - oldest first, as `GET /api/events` returns them - folded
 * into days newest first, each day's own rows newest first. `now` is
 * the moment "Today" is measured against, a parameter so a test does
 * not depend on the clock. */
export function groupByDay(rows: Row[], now: Date = new Date()): DayGroup[] {
  const groups: DayGroup[] = [];
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i];
    const key = localDayKey(row.event.created_at);
    const last = groups[groups.length - 1];
    if (last && last.key === key) {
      last.rows.push(row);
    } else {
      groups.push({ key, label: dayLabel(row.event.created_at, now), rows: [row] });
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
