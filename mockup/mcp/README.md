# MCP responses

What the MCP tools return for the mock-up project, written before the server exists.
Like `../outputs/`, they become the reference files of the MCP server's end-to-end tests.

Each response has two parts: `content`, compact text written for a model to read (the `.txt` file),
and `structuredContent`, the same data as JSON for programs (the `.json` file, when there is one).

| Call | Files | Size |
| --- | --- | --- |
| `project_map()` | [txt](project_map.txt), [json](project_map.json) | ~500 tokens |
| `project_map(resource: "orders")` | [txt](project_map-orders.txt), [json](project_map-orders.json) | ~540 tokens |
| `search_api(query: "paginate")` | [txt](search_api-paginate.txt), [json](search_api-paginate.json) | ~240 tokens |
| `search_api(query: "Json")`, a name from Axum | [txt](search_api-Json.txt) | ~130 tokens |
| `search_api(query: "save")`, a name from Active Record | [txt](search_api-save.txt) | ~90 tokens |
| `search_api(query: "cache_page")`, a name that exists nowhere | [txt](search_api-cache_page.txt) | ~90 tokens |
| `explain(code: "DB0042")` | [txt](explain-DB0042.txt) | ~180 tokens |
| `explain(topic: "budgets")` | [txt](explain-budgets.txt) | ~175 tokens |

Sizes are estimated at 3.5 characters per token. The JSON form of the same data is 1.5 to 2 times larger,
which is why the model reads the text. On the reference project (8 resources, 54 routes), `project_map()`
is about 2,000 tokens.

## Rules these responses follow

- **Two levels.** `project_map()` gives one line per route and per model; `project_map(resource)` gives
  the details of one resource: fields, rules, owner, budgets, errors, tests, front-end pages.
- **Everything points to a file and a line**, so the agent can open it without searching.
- **Effective values, with their origin**: a rule inherited from the model says so, a budget says
  whether it is a default or an override.
- **`search_api` answers invented names**: it says the name does not exist, where it comes from, and
  what to write instead, with an example from the project's own code.
- **A search with no result says so plainly**, then suggests related names and how to search better.
- **Every response ends with where to go next** (`more: explain("…")`, `project_map(resource: …)`).
