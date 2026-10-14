# Use your own AI app with Tenun

`tenun mcp` lets an AI app that you already use — Claude Desktop, Claude Code, or any app that
speaks the Model Context Protocol (MCP) — read a Tenun workspace and build dashboards in it. You
ask in your own words ("make a dashboard of revenue by store, with a forecast"), and the AI app
calls Tenun's tools to do it.

**Tenun calls no AI and sends nothing anywhere.** The AI app reads what the tools answer — table
names, sample rows, the numbers of a question — and sends it to its own company, under that
company's terms. If your data must not leave your computer, do not connect an AI app to it.

## Set it up

`tenun mcp` takes the path of a `.tenun` workspace. The AI app starts it and talks to it over its
standard input and output. Nothing listens on the network.

**Claude Code:**

```sh
claude mcp add tenun -- tenun mcp /path/to/sales.tenun
```

**Claude Desktop, and most other MCP apps:** add this to the app's MCP settings
(`claude_desktop_config.json` for Claude Desktop), then restart the app:

```json
{
  "mcpServers": {
    "tenun": { "command": "tenun", "args": ["mcp", "/path/to/sales.tenun"] }
  }
}
```

On Windows, write the path with forward slashes or doubled backslashes:
`"C:/Users/you/Documents/sales.tenun"`.

## When the Tenun app is open

If the Tenun app (or `tenun serve`) has the same workspace open, `tenun mcp` sends every call to it.
So one program writes the file, and you see each change in the app at once, with a notice:
"An AI app added the tile Amount by store to Stores." The notice has an **Undo** button.

If the app is not open, `tenun mcp` opens the workspace itself and saves each change to the file.
Open the workspace in the app after that, or before; avoid editing it in both at the same time.

## What the AI app can do

| Tool | What it does |
| --- | --- |
| `list_tables`, `describe_table` | The tables, their columns and types, and the first 5 rows of one. |
| `list_relationships` | How the tables link. |
| `query` | A question as a tile asks it: values grouped by fields, with filters. At most 500 rows. |
| `check_formula` | Checks a formula and shows its type and a preview, or the error. |
| `list_dashboards`, `read_dashboard` | The dashboards, and what each tile shows as the page opens. |
| `add_measure` | A named measure (a formula of totals) that tiles can use. |
| `add_dashboard`, `add_tile`, `update_tile`, `remove_tile` | Builds dashboards. A new or changed tile answers with its result, so the AI app sees what it made. A tile that does not work is not kept. |
| `undo` | Takes back the last change an AI app made, if nothing changed after it. |

The AI app may use every chart type and option, Advanced ones too. The tiles work the same way
as tiles you make.

## What it cannot do

- **Run SQL**, or any query except Tenun's own questions.
- **Read or write files.** It reaches only the workspace that `tenun mcp` was started with, and
  only its tables, not the paths of its sources.
- **See a secret.** Database passwords and web link keys are never in an answer. A column whose
  name says it holds a password or a key (`password`, `token`, `api_key`, …) is never read: its
  values are not shown, and a question about it is refused.
- **Publish**, change who can see a published dashboard, or send alerts and emails.
- **Delete** a dashboard, a table or a source.

## The protocol

`tenun mcp` speaks MCP over stdio, one JSON-RPC message a line. It serves the current revision
(`2026-07-28`, no handshake; `server/discover` answers what it supports) and the revisions before it,
which open with `initialize` (`2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`).
