# CLI Reference

`tack` is a single binary that is both the **server** and the **CLI client**.
Run `tack` with no arguments (or `tack serve`) to start the server + web UI;
run `tack <command>` to use the CLI.

The CLI commands below talk to a running server over HTTP, so start the server
first (`tack serve`) — all client commands require it to be reachable.

## Configuration

```sh
# Set URL and token (saved to ~/.config/tack/config.toml)
tack config --url http://127.0.0.1:3210 --token your-token

# Print current config
tack config --show

# Or use environment variables
export TACK_API_URL=http://127.0.0.1:3210
export TACK_API_TOKEN=your-token
```

**Shell completions:**

```sh
tack completions bash  >> ~/.bashrc
tack completions zsh   >> ~/.zshrc
tack completions fish  > ~/.config/fish/completions/tack.fish
```

---

## Projects

```sh
# Create a project
tack init "Kitchen Reno" --type construction

# Types: software · web · mobile · construction · personal · homework · maintenance · custom
```

---

## Items

```sh
# List items in a project
tack list --project <id>

# Add an item
tack add "Design login page" \
  --project <id> \
  --type task \
  --priority high

# Priorities: high · medium · low
# Types: task · epic · story · bug · feature · subtask · milestone (or vocabulary-mapped)

# Move an item to a different status column
tack move <item-id> "In Progress"
# The status name must exactly match the column name (case-sensitive)
```

---

## Sprints

```sh
tack sprint create --project <id> --name "Sprint 1"
tack sprint start  <sprint-id>    # Planning → Active
tack sprint close  <sprint-id>    # Active → Closed
```

Only one sprint can be Active per project at a time.

---

## Templates

```sh
# List available templates (built-in + user-created)
tack template list
tack template list --type construction   # filter by project type

# Show a template's full details
tack template show <template-id>

# Create a new project from a template
tack template create-from <template-id> "My New Project"
tack template create-from <template-id> "My New Project" --description "Optional description"
```

---

## Roles

Roles represent specialties or disciplines (Designer, Engineer, Reviewer, …) that can be
assigned to items for tracking who is responsible.

```sh
# List roles in a project
tack role list --project <project-id>

# Create a role
tack role create "Designer" --project <project-id>
tack role create "Engineer" --project <project-id> --color "#4A90D9"

# Assign / unassign a role on an item
tack role assign  <item-id> <role-id>
tack role unassign <item-id> <role-id>

# Delete a role (removes all its item assignments)
tack role delete <role-id>
```

---

## Comments

```sh
# List comments on an item
tack comment list <item-id>

# Add a comment
tack comment add <item-id> "Looks good to merge"
tack comment add <item-id> "Blocked on client sign-off" --author "Alice"
```

---

## Custom Fields

Custom fields extend items with project-specific metadata.

```sh
# List field definitions for a project
tack field list --project <project-id>

# Create a field definition
tack field create "Client Name"   --project <project-id> --type text
tack field create "Story Points"  --project <project-id> --type number --required
tack field create "Phase"         --project <project-id> --type select \
    --options "Design,Development,QA,Done"

# Types: text · long_text · number · date · boolean · select · multi_select · url · email

# List all custom field values set on an item
tack field values <item-id>

# Set a value (parsed as JSON if valid, otherwise treated as a string)
tack field set <item-id> <field-id> "Acme Corp"
tack field set <item-id> <field-id> 8          # number
tack field set <item-id> <field-id> true        # boolean
tack field set <item-id> <field-id> '"Design"'  # string that looks like JSON — quote it

# Remove a value
tack field unset <item-id> <field-id>

# Delete a field definition (also removes all item values for that field)
tack field delete <field-id>
```

---

## Backup and Restore

```sh
# Download backup to current directory (timestamped filename)
tack backup

# Download to a specific path
tack backup --path /safe/place/tack.db

# Stage a restore (applied on next server startup)
tack restore /safe/place/tack.db
```

---

## Machine-Readable Output

All commands accept `--json` for raw JSON output:

```sh
tack list --project <id> --json | jq '.[] | select(.priority == "high")'
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
