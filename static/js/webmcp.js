// WebMCP: expose the docs site's key actions to in-browser AI agents.
// https://webmachinelearning.github.io/webmcp/
(() => {
  // Older Chrome builds shipped the API on `navigator`.
  const modelContext = document.modelContext ?? navigator.modelContext;
  if (!modelContext || typeof modelContext.registerTool !== "function") {
    return;
  }

  const controller = new AbortController();
  window.addEventListener("pagehide", () => controller.abort(), { once: true });

  const text = (value) => ({
    content: [{ type: "text", text: JSON.stringify(value, null, 2) }],
  });

  async function getJson(path, params) {
    const url = new URL(path, window.location.origin);
    for (const [key, value] of Object.entries(params ?? {})) {
      if (value !== undefined && value !== null && value !== "") {
        url.searchParams.set(key, String(value));
      }
    }
    const response = await fetch(url, { headers: { accept: "application/json" } });
    if (!response.ok) {
      throw new Error(`Request to ${url.pathname} failed with status ${response.status}`);
    }
    return response.json();
  }

  const tools = [
    {
      name: "search_autumn_docs",
      description:
        "Search the Autumn and Autumn Harvest guides. Every term must appear in a guide to match, so use two or three distinctive words. Returns slugs and snippets; pass a slug to get_autumn_doc.",
      inputSchema: {
        type: "object",
        properties: {
          q: { type: "string", description: "Words to search for." },
          limit: { type: "integer", minimum: 1, maximum: 50, description: "Maximum hits (default 10)." },
        },
        required: ["q"],
      },
      annotations: { readOnlyHint: true },
      execute: async ({ q, limit }) => text(await getJson("/api/search", { q, limit })),
    },
    {
      name: "list_autumn_docs",
      description: "List the bundled guides with slug, title, description and sidebar group.",
      inputSchema: {
        type: "object",
        properties: {
          group: { type: "string", description: "Only guides in this sidebar group." },
        },
      },
      annotations: { readOnlyHint: true },
      execute: async ({ group }) => text(await getJson("/api/docs", { group })),
    },
    {
      name: "get_autumn_doc",
      description:
        "Read one guide as Markdown by slug. Large guides return an outline; pass a section id to read a part.",
      inputSchema: {
        type: "object",
        properties: {
          slug: { type: "string", description: "Guide slug, e.g. getting-started." },
          section: { type: "string", description: "Section id from a previous response." },
        },
        required: ["slug"],
      },
      annotations: { readOnlyHint: true },
      execute: async ({ slug, section }) =>
        text(await getJson(`/api/docs/${encodeURIComponent(slug)}`, { section })),
    },
    {
      name: "open_autumn_doc",
      description: "Navigate the browser to a guide page by slug.",
      inputSchema: {
        type: "object",
        properties: { slug: { type: "string", description: "Guide slug." } },
        required: ["slug"],
      },
      execute: async ({ slug }) => {
        window.location.assign(`/docs/${encodeURIComponent(slug)}`);
        return text({ navigatedTo: `/docs/${slug}` });
      },
    },
  ];

  for (const tool of tools) {
    Promise.resolve(modelContext.registerTool(tool, { signal: controller.signal })).catch(
      () => {},
    );
  }
})();
