import Layout from "@theme/Layout";
import { Download, Terminal, Star, ArrowLeft, Info, BookOpen } from "lucide-react";
import { Button } from "@site/src/components/ui/button";
import { Badge } from "@site/src/components/ui/badge";
import { getGooseInstallLink } from "@site/src/utils/install-links";
import { useLocation } from "@docusaurus/router";
import { useEffect, useState } from "react";
import type { MCPServer } from "@site/src/types/server";
import { fetchMCPServers } from "@site/src/utils/mcp-servers";
import { fetchGitHubStars, formatStarCount } from "@site/src/utils/github-stars";
import Link from "@docusaurus/Link"; 

function ExtensionDetail({ server }: { server: MCPServer }) {
  const [githubStars, setGithubStars] = useState<number | null>(null);


// outliers in naming
const overrides: Record<string, string> = {
  'computercontroller': 'computer-controller-mcp',
  'pdf-read': 'pdf-mcp',
  'knowledge-graph-memory': 'knowledge-graph-mcp',
  'vscode': 'vs-code-mcp',
};

const getDocumentationPath = (serverId: string): string => {
  let filename = serverId.replace(/_/g, '-');
  filename = overrides[filename] ?? filename;

  if (!filename.endsWith('-mcp')) {
    filename += '-mcp';
  }

  return filename;
};


  useEffect(() => {
    if (server.link) {
      fetchGitHubStars(server.link).then(stars => {
        setGithubStars(stars);
      });
    }
  }, [server.link]);

  return (
    <Layout>
      <div className="min-h-screen flex items-start justify-center py-16">
        <div className="container max-w-5xl mx-auto px-4">
          <div className="flex gap-8">
            <div>
              <Link to="/extensions" className="no-underline">
                <Button
                    className="flex items-center gap-2 hover:cursor-pointer"
                  >
                  <ArrowLeft className="h-4 w-4" />
                  Back
                </Button>
              </Link>
            </div>

            <div className="server-card flex-1">
              <div className="card p-8 relative">
                <Link
                  to={`/docs/mcp/${getDocumentationPath(server.id)}`}
                  className="absolute top-4 right-4 flex items-center gap-2 text-textSubtle hover:text-textProminent transition-colors no-underline"
                  title="View tutorial"
                >
                  <BookOpen className="h-5 w-5" />
                </Link>
                <div className="card-header mb-6">
                  <div className="flex items-center gap-4">
                    <h1 className="font-medium text-5xl text-textProminent m-0">
                      {server.name}
                    </h1>
                    {server.is_builtin && (
                      <Badge variant="secondary" className="text-sm">
                        Built-in
                      </Badge>
                    )}
                  </div>
                </div>

                <div className="card-content space-y-6">
                  <div>
                    <p className="text-xl text-textSubtle m-0">
                      {server.description}
                    </p>
                  </div>

                  {server.installation_notes && (
                    <div>
                      <p className="text-md text-textSubtle m-0">
                        {server.installation_notes}
                      </p>
                    </div>
                  )}

                  <div className="space-y-2">
                    {server.is_builtin ? (
                      <div className="flex items-center gap-2">
                        <Info className="h-4 w-4 text-textSubtle shrink-0" />
                        <span className="text-sm text-textSubtle">
                           Can be enabled on the Extensions page in goose
                        </span>
                      </div>
                    ) : server.show_install_command !== false ? (
                      <>
                        <div className="flex items-center gap-2 text-textStandard">
                          <Terminal className="h-4 w-4" />
                          <h4 className="font-medium m-0">Command</h4>
                        </div>
                        <div className="command-content">
                          {(server.type === "local" || !server.type) ? (
                            <code className="text-sm block">
                              {`goose session --with-extension "${server.command}"`}
                            </code>
                          ) : server.type === "remote" ? (
                            <code className="text-sm block">
                              {`goose session --with-remote-extension "${server.url}"`}
                            </code>
                          ) : server.type === "streamable-http" ? (
                            <code className="text-sm block">
                              {`goose session --with-streamable-http-extension "${server.url}"`}
                            </code>
                          ) : (
                            <code className="text-sm block">
                              No command available
                            </code>
                          )}
                        </div>
                      </>
                    ) : null}
                  </div>

                  {server.environmentVariables && (
                    <div className="space-y-4">
                      <h2 className="text-lg font-medium text-textStandard m-0">
                        Environment Variables
                      </h2>
                      {server.environmentVariables.length > 0 ? (
                        <div>
                          {server.environmentVariables.map((env) => (
                            <div
                              key={env.name}
                              className="border-b border-borderSubtle py-4 first:pt-0 last:border-0"
                            >
                              <div className="text-sm text-textStandard font-medium">
                                {env.name}
                              </div>
                              <div className="text-textSubtle text-sm mt-1">
                                {env.description}
                              </div>
                              {env.required && (
                                <Badge variant="secondary" className="mt-2">
                                  Required
                                </Badge>
                              )}
                            </div>
                          ))}
                        </div>
                      ) : (
                        <div className="text-textSubtle text-sm flex items-center gap-2">
                          <Info className="h-4 w-4" />
                          No environment variables needed
                        </div>
                      )}
                    </div>
                  )}

                  {server.headers && server.headers.length > 0 && (
                    <div className="space-y-4">
                      <h2 className="text-lg font-medium text-textStandard m-0">
                        Request Headers
                      </h2>
                      <div>
                        {server.headers.map((header) => (
                          <div
                            key={header.name}
                            className="border-b border-borderSubtle py-4 first:pt-0 last:border-0"
                          >
                            <div className="text-sm text-textStandard font-medium">
                              {header.name}
                            </div>
                            <div className="text-textSubtle text-sm mt-1">
                              {header.description}
                            </div>
                            {header.required && (
                              <Badge variant="secondary" className="mt-2">
                                Required
                              </Badge>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  <div className="card-footer">
                    {githubStars !== null && (
                      <a
                        href={server.link}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="card-stats"
                      >
                        <Star className="h-4 w-4" />
                        <span>{formatStarCount(githubStars)} on GitHub</span>
                      </a>
                    )}

                    {server.is_builtin ? (
                      <div
                        className="built-in-badge"
                        title="This extension is built into Goose and can be enabled on the Extensions page"
                      >
                        Built-in
                      </div>
                    ) : server.show_install_link !== false ? (
                      <a
                        href={getGooseInstallLink(server)}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="install-button"
                      >
                        <span>Install</span>
                        <Download className="h-4 w-4" />
                      </a>
                    ) : (
                      <div
                        className="manual-setup-badge"
                        title="This extension requires manual setup. See the installation notes above."
                      >
                        Manual setup
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Layout>
  );
}

export default function DetailPage(): JSX.Element {
  const location = useLocation();
  const [server, setServer] = useState<MCPServer | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadServer = async () => {
      try {
        setLoading(true);
        setError(null);
        const servers = await fetchMCPServers();
        // Get the ID from the query parameter
        const params = new URLSearchParams(location.search);
        const id = params.get("id");
        if (!id) {
          setError("No extension ID provided");
          return;
        }
        const foundServer = servers.find((s) => s.id === id);
        if (foundServer) {
          setServer(foundServer);
        } else {
          setError("Extension not found");
        }
      } catch (err) {
        setError("Failed to load extension details");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    loadServer();
  }, [location]);

  if (loading) {
    return (
      <Layout>
        <div className="min-h-screen flex items-start justify-center py-16">
          <div className="container max-w-5xl mx-auto px-4">
            <div className="flex gap-8">
              <div>
                <Link to="/extensions" className="no-underline">
                  <Button
                    className="flex items-center gap-2 hover:text-textProminent hover:cursor-pointer"
                  >
                    <ArrowLeft className="h-4 w-4" />
                    Back
                  </Button>
                </Link>
              </div>
              <div className="server-card flex-1">
                <div className="card p-8">
                  <div className="animate-pulse">
                    <div className="h-12 w-48 bg-bgSubtle rounded-lg mb-4"></div>
                    <div className="h-6 w-full bg-bgSubtle rounded-lg mb-2"></div>
                    <div className="h-6 w-2/3 bg-bgSubtle rounded-lg"></div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </Layout>
    );
  }

  if (error || !server) {
    return (
      <Layout>
        <div className="min-h-screen flex items-start justify-center py-16">
          <div className="container max-w-5xl mx-auto px-4">
            <div className="flex gap-8">
              <div>
                <Link to="/extensions" className="no-underline">
                  <Button
                    variant="ghost"
                    className="flex items-center gap-2 hover:text-textProminent hover:cursor-pointer"
                  >
                    <ArrowLeft className="h-4 w-4" />
                    Back
                  </Button>
                </Link>
              </div>
              <div className="server-card flex-1">
                <div className="card p-8">
                  <div className="text-red-500">
                    {error || "Extension not found"}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </Layout>
    );
  }

  return <ExtensionDetail server={server} />;
}
