import { useState } from "react";
import { useNavigate } from "react-router";
import { describe, postData } from "../api";
import { field } from "./ui/field";

/** A project-name input; Enter creates the project and opens home. Escape or blurring it empty calls `onCancel`. */
export function NewProjectForm({ onCreated, onCancel }: { onCreated: () => void; onCancel: () => void }) {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  const [creating, setCreating] = useState(false);

  const create = async (e: React.SyntheticEvent) => {
    e.preventDefault();
    if (creating || !name.trim()) return;
    setCreating(true);

    try {
      await postData("/api/projects", { name });
      onCancel();
      void navigate("/");
      onCreated();
    } catch (err) {
      setError(describe(err));
    } finally {
      setCreating(false);
    }
  };

  return (
    <form onSubmit={(e) => void create(e)} className="grid gap-1 px-1">
      <input
        autoFocus
        value={name}
        readOnly={creating}
        aria-label="Project name"
        aria-describedby={error ? "new-project-error" : undefined}
        placeholder="Project name"
        onChange={(e) => {
          setName(e.target.value);
          setError("");
        }}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
        onBlur={() => !name.trim() && onCancel()}
        className={field}
      />
      {error && (
        <p id="new-project-error" role="alert" className="px-1 text-sm text-muted wrap-anywhere">
          {error}
        </p>
      )}
    </form>
  );
}
