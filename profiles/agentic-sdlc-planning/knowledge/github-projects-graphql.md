# GitHub Projects v2 GraphQL API

## Mutation: Adding Status Options

### What Works

The `updateProjectV2Field` mutation successfully updates a ProjectV2 SingleSelectField (like Status):

```bash
gh api graphql -f query="
  mutation {
    updateProjectV2Field(input: {
      fieldId: \"PVTSSF_lADOD6L7Gs4BSitmzhAC5bo\"
      singleSelectOptions: [
        {name: \"bug:investigate\", color: GRAY, description: \"QE investigating bug\"}
      ]
    }) {
      projectV2Field {
        ... on ProjectV2SingleSelectField {
          name
          options { name }
        }
      }
    }
  }
"
```

**Result:**
```json
{
  "data": {
    "updateProjectV2Field": {
      "projectV2Field": {
        "name": "Status",
        "options": [
          {"name": "bug:investigate"}
        ]
      }
    }
  }
}
```

### Key Findings

1. **Inline mutation works** - No need for GraphQL variables
2. **Required fields in ProjectV2SingleSelectFieldOptionInput:**
   - `name` (String!)
   - `color` (ProjectV2SingleSelectFieldOptionColor!) - enum values like GRAY, RED, BLUE, etc.
   - `description` (String!) - human-readable description
3. **The mutation REPLACES all options** - It doesn't append. The entire option list must be provided.
4. **The 'id' field is NOT accepted** - Only name, color, and description are valid in the input

### The Problem

To add new bug statuses without losing existing ones, we need to:
1. Read ALL existing status options with their colors and descriptions
2. Include them in the singleSelectOptions array along with the new ones
3. Submit the complete list via updateProjectV2Field

**Current limitation:** We can only read option names via `gh project field-list`, not their colors or descriptions.

## Query: Reading Status Options

Use the `node()` query with the field ID to get full option metadata (name, color, description):

```bash
gh api graphql -f query='
{
  node(id: "PVTSSF_lADOD6L7Gs4BSitmzhAC5bo") {
    ... on ProjectV2SingleSelectField {
      name
      options {
        id
        name
        color
        description
      }
    }
  }
}'
```

**Result:**
```json
{
  "data": {
    "node": {
      "name": "Status",
      "options": [
        {
          "id": "8760f5dd",
          "name": "bug:investigate",
          "color": "GRAY",
          "description": "QE investigating bug"
        }
      ]
    }
  }
}
```

**Key points:**
- `gh project field-list` only returns option names — NOT colors or descriptions
- The `node()` query with inline fragment `... on ProjectV2SingleSelectField` returns the full metadata
- Use this query to read existing options before calling `updateProjectV2Field` to preserve them

## Available Colors

Query the enum with introspection:

```bash
gh api graphql -f query='{
  __type(name: "ProjectV2SingleSelectFieldOptionColor") {
    enumValues { name }
  }
}'
```

Values: `GRAY`, `BLUE`, `GREEN`, `YELLOW`, `ORANGE`, `RED`, `PINK`, `PURPLE`

## Safe Add Pattern

To add new options without losing existing ones:

1. **Read** current options via `node()` query (get name, color, description for each)
2. **Append** new options to the list
3. **Write** the complete list via `updateProjectV2Field` mutation

## Native Issue Types

GitHub's native issue types replace `kind/*` labels. Query and create via GraphQL with `GraphQL-Features: issue_types` header.

### Query Available Types

```bash
gh api graphql \
  -f owner="$OWNER" -f repo="$REPO_NAME" \
  -H "GraphQL-Features: issue_types" \
  -f query='query($owner: String!, $repo: String!) {
    repository(owner: $owner, name: $repo) {
      issueTypes(first: 20) {
        nodes { id name description color isEnabled }
      }
    }
  }'
```

### Query Issue Type of an Existing Issue

```bash
gh api graphql \
  -f owner="$OWNER" -f repo="$REPO_NAME" -F number=3 \
  -H "GraphQL-Features: issue_types" \
  -f query='query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      issue(number: $number) { issueType { id name } }
    }
  }'
```

### Create Issue with Type + Parent (Single Mutation)

```bash
gh api graphql \
  -H "GraphQL-Features: issue_types,sub_issues" \
  -f query='mutation {
    createIssue(input: {
      repositoryId: "<REPO_ID>"
      title: "Issue title"
      body: "Description"
      issueTypeId: "<TYPE_ID>"
      parentIssueId: "<PARENT_ID>"
    }) {
      issue { number title issueType { name } }
    }
  }'
```

**Key:** `CreateIssueInput` accepts both `issueTypeId` and `parentIssueId`, so type + sub-issue link happens in one call.

### Type Mapping

| Team Kind | GitHub Issue Type |
|-----------|-------------------|
| epic | Epic |
| story | Task |
| bug | Bug |

---

*Last updated: 2026-03-24*
