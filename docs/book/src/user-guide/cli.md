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

## Templates

```sh
# List available templates (built-in + user-created)
flexpm template list
flexpm template list --type construction   # filter by project type

# Show a template's full details
flexpm template show <template-id>

# Create a new project from a template
flexpm template create-from <template-id> "My New Project"
flexpm template create-from <template-id> "My New Project" --description "Optional description"
```

---

## Roles

Roles represent specialties or disciplines (Designer, Engineer, Reviewer, …) that can be
assigned to items for tracking who is responsible.

```sh
# List roles in a project
flexpm role list --project <project-id>

# Create a role
flexpm role create "Designer" --project <project-id>
flexpm role create "Engineer" --project <project-id> --color "#4A90D9"

# Assign / unassign a role on an item
flexpm role assign  <item-id> <role-id>
flexpm role unassign <item-id> <role-id>

# Delete a role (removes all its item assignments)
flexpm role delete <role-id>
```

---

## Comments

```sh
# List comments on an item
flexpm comment list <item-id>

# Add a comment
flexpm comment add <item-id> "Looks good to merge"
flexpm comment add <item-id> "Blocked on client sign-off" --author "Alice"
```

---

## Custom Fields

Custom fields extend items with project-specific metadata.

```sh
# List field definitions for a project
flexpm field list --project <project-id>

# Create a field definition
flexpm field create "Client Name"   --project <project-id> --type text
flexpm field create "Story Points"  --project <project-id> --type number --required
flexpm field create "Phase"         --project <project-id> --type select \
    --options "Design,Development,QA,Done"

# Types: text · long_text · number · date · boolean · select · multi_select · url · email

# List all custom field values set on an item
flexpm field values <item-id>

# Set a value (parsed as JSON if valid, otherwise treated as a string)
flexpm field set <item-id> <field-id> "Acme Corp"
flexpm field set <item-id> <field-id> 8          # number
flexpm field set <item-id> <field-id> true        # boolean
flexpm field set <item-id> <field-id> '"Design"'  # string that looks like JSON — quote it

# Remove a value
flexpm field unset <item-id> <field-id>

# Delete a field definition (also removes all item values for that field)
flexpm field delete <field-id>
```

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
