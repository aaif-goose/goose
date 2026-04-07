import type { MCPServer } from "../types/server";

const SERVERS_URL = "/servers.json";

export async function fetchMCPServers(): Promise<MCPServer[]> {
  try {
    const response = await fetch(SERVERS_URL);
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    const data = await response.json();
    return data;
  } catch (error) {
    console.error("Error fetching MCP servers:", error);
    throw error;
  }
}

export async function searchMCPServers(query: string): Promise<MCPServer[]> {
  const servers = await fetchMCPServers();
  const normalizedQuery = query.toLowerCase();

  return servers.filter((server) => {
    const normalizedName = server.name.toLowerCase();
    const normalizedDescription = server.description.toLowerCase();

    return (
      normalizedName.includes(normalizedQuery) ||
      normalizedDescription.includes(normalizedQuery)
    );
  });
}
