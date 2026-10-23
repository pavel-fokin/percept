import { useLoaderData, useRouteError } from "react-router";
import { describe, getData, type Skill, type SkillEntry } from "../api";
import { Chevron } from "../components/chevron";
import { Markdown } from "../components/markdown";

export async function loadSkills() {
  return getData<{ library: string; skills: SkillEntry[] }>("/api/skills");
}

export function Skills() {
  const { library, skills } = useLoaderData<typeof loadSkills>();
  const target = decodeURIComponent(window.location.hash.slice(1));

  return (
    <>
      <h1 className="mt-8 sm:mt-10">Skills</h1>
      <p className="mt-2 font-mono text-sm text-muted wrap-anywhere">{library}</p>

      <div className="mt-8 grid gap-1">
        {skills.map((skill) =>
          "error" in skill ? (
            <div key={skill.name} id={skill.name} className="-mx-3 rounded-lg px-3 py-3">
              <h2 className="font-mono text-sm font-normal wrap-anywhere">{skill.name}</h2>
              <p className="mt-1 text-sm text-muted wrap-anywhere">{skill.error}</p>
            </div>
          ) : (
            <details key={skill.name} id={skill.name} open={skill.name === target} className="group -mx-3 rounded-lg">
              <summary className="flex min-h-11 cursor-pointer list-none items-start gap-3 rounded-lg px-3 py-3 hover:bg-surface focus-visible:bg-surface [&::-webkit-details-marker]:hidden">
                <Chevron className="mt-1.5" />
                <span className="grid min-w-0 flex-1 gap-0.5">
                  <span className="font-mono text-sm wrap-anywhere">{skill.name}</span>
                  <span className="text-muted">{skill.description}</span>
                </span>
              </summary>
              <div className="px-3 pt-2 pb-4 pl-9">
                <Markdown text={skill.instructions} />
                {skill.schema && <Schema schema={skill.schema} />}
              </div>
            </details>
          ),
        )}
      </div>
    </>
  );
}

function Schema({ schema }: { schema: NonNullable<Skill["schema"]> }) {
  return (
    <div className="mt-6 grid gap-4 border-t border-border pt-4 text-sm">
      <dl className="grid gap-2">
        {Object.entries(schema.nodes).map(([kind, properties]) => (
          <div key={kind} className="flex flex-wrap gap-x-3">
            <dt className="font-mono">{kind}</dt>
            <dd className="font-mono text-muted wrap-anywhere">{properties.join(", ")}</dd>
          </div>
        ))}
      </dl>
      <ul className="grid gap-1 font-mono">
        {Object.entries(schema.edges).map(([kind, { from, to }]) => (
          <li key={kind} className="wrap-anywhere">
            {kind} <span className="text-muted">{from} → {to}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function SkillsError() {
  return <p className="pt-6">Failed to load skills: {describe(useRouteError())}</p>;
}
