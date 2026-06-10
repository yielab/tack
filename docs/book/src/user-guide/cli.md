# CLI Reference

The `flexpm` CLI talks to a running API server over HTTP. All commands require the server to be reachable.

## Configuration

```sh
# Set URL and token (saved to ~/.config/flexpm/config.toml)
flexpm config --url http://127.0.0.1:3210 --token your-token

# Print current config
flexpm config --show

# Or use environment variables
export FLEXPM_API_URL=http://127.0.0.1:3210
export FLEXPM_API_TOKEN=your-token
```

**Shell completions:**

```sh
flexpm completions bash  >> ~/.bashrc
flexpm completions zsh   >> ~/.zshrc
flexpm completions fish  > ~/.config/fish/completions/flexpm.fish
```

---

## Projects

```sh
# Create a project
flexpm init "Kitchen Reno" --type construction

# Types: software · web · mobile · construction · personal · homework · maintenance · custom
```

---

## Items

```sh
# List items in a project
flexpm list --project <id>

# Add an item
flexpm add "Design login page" \
  --project <id> \
  --type task \
  --priority high

# Priorities: high · medium · low
# Types: task · epic · story · bug · feature · subtask · milestone (or vocabulary-mapped)

# Move an item to a different status column
flexpm move <item-id> "In Progress"
# The status name must exactly match the column name (case-sensitive)
```

---

## Sprints

```sh
flexpm sprint create --project <id> --name "Sprint 1"
flexpm sprint start  <sprint-id>    # Planning → Active
flexpm sprint close  <sprint-id>    # Active → Closed
```

Only one sprint can be Active per project at a time.

---

## Backup and Restore

```sh
# Download backup to current directory (timestamped filename)
flexpm backup

# Download to a specific path
flexpm backup --path /safe/place/flexpm.db

# Stage a restore (applied on next server startup)
flexpm restore /safe/place/flexpm.db
```

---

## Machine-Readable Output

All commands accept `--json` for raw JSON output:

```sh
flexpm list --project <id> --json | jq '.[] | select(.priority == "high")'
```

With `--json`, vocabulary mappings are bypassed and raw field names are returned.

---

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | General error (see stderr) |
| `2` | Configuration error (no URL, bad token) |
| `3` | API error (server returned 4xx/5xx) |
