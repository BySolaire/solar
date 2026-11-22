import { describe, expect, it } from "vitest";
import type { Block } from "./session";
import { solarToolCall, solarWorkSummary } from "./solarToolCall";

function shell(text: string): Block {
  return { id: text, role: "tool", text, tool: { kind: "shell" } };
}

describe("Solar CLI tool calls", () => {
  it("recognizes app actions with absolute, quoted, or bare executables", () => {
    expect(
      solarToolCall(
        shell(
          "/repo/target/debug/Solar.app/Contents/MacOS/solar app notes.list --json '{}'",
        ),
      )?.label,
    ).toBe("List notes");
    expect(
      solarToolCall(
        shell(
          "'/Applications/Solar App/solar' app folders.move --input -",
        ),
      )?.label,
    ).toBe("Move a session");
    expect(
      solarToolCall(
        shell('"C:\\Program Files\\Solar\\solar.exe" app notes.list'),
      )?.label,
    ).toBe("List notes");
    expect(solarToolCall(shell("solar app --help"))?.label).toBe(
      "View CLI commands",
    );
    expect(
      solarToolCall(shell("solar app sessions.read --json '{}'"))?.label,
    ).toBe("Read a session");
    expect(
      solarToolCall(shell("solar app sessions.send --json '{}'"))?.label,
    ).toBe("Continue a session");
    expect(
      solarToolCall(shell("solar app sessions.draft --json '{}'"))?.label,
    ).toBe("Save a draft");
    for (const [action, label] of [
      ["sessions.stop", "Stop a session"],
      ["sessions.archive", "Archive a session"],
      ["sessions.delete", "Delete a session"],
    ]) {
      expect(
        solarToolCall(shell(`solar app ${action} --json '{}'`))?.label,
      ).toBe(label);
    }
    expect(
      solarToolCall(shell("solar app notes.write --input -"))?.label,
    ).toBe("Write a note");
    expect(
      solarToolCall({
        id: "generic",
        role: "tool",
        text: "Run command:",
        tool: { kind: "other", title: "solar app sessions.start" },
      })?.label,
    ).toBe("Start a session");
    expect(
      solarToolCall({
        id: "codex-action",
        role: "tool",
        text: "List",
        tool: {
          kind: "execute",
          preview: { kind: "shell", title: "solar app notes.list" },
        },
      })?.label,
    ).toBe("List notes");
  });

  it("does not restyle unrelated commands or text mentioning the CLI", () => {
    expect(
      solarToolCall(shell("echo solar app notes.list")),
    ).toBeUndefined();
    expect(solarToolCall(shell("solar control list"))).toBeUndefined();
    expect(
      solarToolCall({
        id: "prose",
        role: "assistant",
        text: "solar app notes.list",
      }),
    ).toBeUndefined();
  });

  it("does not compact compound shell commands or hide a longer shell preview", () => {
    for (const command of [
      "solar app notes.list && echo extra",
      "solar app notes.list; echo extra",
      "solar app notes.list | cat",
      "solar app notes.list\necho extra",
      "solar app notes.list --json \"$(echo extra)\"",
    ]) {
      expect(solarToolCall(shell(command))).toBeUndefined();
    }
    expect(
      solarToolCall({
        id: "short-title",
        role: "tool",
        text: "solar app notes.list",
        tool: {
          kind: "shell",
          title: "solar app notes.list",
          preview: {
            kind: "shell",
            title: "solar app notes.list && echo extra",
          },
        },
      }),
    ).toBeUndefined();
    expect(
      solarToolCall(
        shell("solar app sessions.send --json '{\"prompt\":\"a; b\"}'"),
      )?.command,
    ).toBe("solar app sessions.send --json '{\"prompt\":\"a; b\"}'");
  });

  it("names a group only when all its tool calls use Solar", () => {
    const calls = [
      shell("solar app --help"),
      shell("solar app notes.list"),
    ];
    expect(solarWorkSummary(calls, true)).toBe("Using Solar");
    expect(solarWorkSummary(calls, false)).toBe("Used Solar");
    expect(
      solarWorkSummary([...calls, shell("git status")], true),
    ).toBeUndefined();
  });
});
