# Projects & Teams

## Projects

A project represents a deployable application.

### Create

```bash
rise-cli project create my-app --visibility public
rise-cli project create secret-app --visibility private --owner team:devops
```

Ownership defaults to current user if not specified.

### List & Show

```bash
rise-cli project list
rise-cli project show my-app
```

### Update

```bash
# Rename
rise-cli project update my-app --name my-new-app

# Change visibility
rise-cli project update my-app --visibility private

# Transfer ownership
rise-cli project update my-app --owner user:alice@example.com
rise-cli project update my-app --owner team:devops
```

### Fuzzy Matching

Typos are corrected with suggestions:

```bash
$ rise-cli project show my-ap
Project 'my-ap' not found

Did you mean one of these?
  - my-app
```

## Teams

Teams enable shared project ownership.

### Create

```bash
rise-cli team create devops --owners alice@example.com --members bob@example.com
```

Owners have full control; members have read access. Current user is automatically added as owner if not specified.

### Manage Members

```bash
# Add members
rise-cli team update devops --add-members charlie@example.com

# Remove members
rise-cli team update devops --remove-members bob@example.com

# Add/remove owners
rise-cli team update devops --add-owners charlie@example.com
```

### Lookup by Name or ID

```bash
rise-cli team show devops           # by name
rise-cli team show abc123 --by-id   # by ID
```
