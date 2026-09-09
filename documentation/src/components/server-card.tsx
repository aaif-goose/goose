import { Star, Download, Terminal, ChevronRight, Info } from "lucide-react";
import type { MCPServer } from "@site/src/types/server";
import Link from "@docusaurus/Link";
import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { getGooseInstallLink } from "@site/src/utils/install-links";
import { fetchGitHubStars, formatStarCount } from "@site/src/utils/github-stars";

const getExtensionCommand = (server: MCPServer): string => {
  switch (server.type) {
    case "remote":
      return `goose session --with-remote-extension "${server.url}"`;
    case "streamable-http":
      return `goose session --with-streamable-http-extension "${server.url}"`;
    case "local":
    default:
      return `goose session --with-extension "${server.command}"`;
  }
};

export function ServerCard({ server }: { server: MCPServer }) {
  const [isCommandVisible, setIsCommandVisible] = useState(false);
  const [githubStars, setGithubStars] = useState<number | null>(null);

  useEffect(() => {
    if (server.link) {
      fetchGitHubStars(server.link).then(stars => {
        setGithubStars(stars);
        });
    }
  }, [server.link]);

  return (
    <div className="extension-title h-full">
      <div className="server-card interactive w-full h-full">
        <div className="card-glow"></div>
        <div className="card">
          <div className="card-header">
            <div className="card-header-content">
              <svg
                className="extension-icon"
                width="13"
                height="12"
                viewBox="0 0 13 12"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <g id="Response Icons">
                  <path
                    id="Vector"
                    d="M6.29521 0.0644531C3.24885 0.0644531 3.43906 1.37819 3.43906 1.37819L3.44245 2.73927H6.34953V3.1479H2.2878C2.2878 3.1479 0.33844 2.92804 0.33844 5.98471C0.33844 9.04147 2.03989 8.93307 2.03989 8.93307H3.05531V7.51461C3.05531 7.51461 3.00057 5.82262 4.7296 5.82262H7.6129C7.6129 5.82262 9.23283 5.84864 9.23283 4.26571V1.64842C9.23283 1.64842 9.47886 0.0644531 6.29521 0.0644531ZM4.69225 0.979748C4.76094 0.979686 4.82898 0.993097 4.89245 1.01921C4.95593 1.04533 5.01361 1.08363 5.06219 1.13194C5.11076 1.18024 5.14928 1.2376 5.17554 1.30073C5.2018 1.36385 5.21529 1.43151 5.21523 1.49982C5.21529 1.56814 5.2018 1.63579 5.17554 1.69892C5.14928 1.76205 5.11076 1.8194 5.06219 1.86771C5.01361 1.91601 4.95593 1.95432 4.89245 1.98044C4.82898 2.00655 4.76094 2.01996 4.69225 2.0199C4.62355 2.01996 4.55551 2.00655 4.49204 1.98044C4.42856 1.95432 4.37088 1.91601 4.3223 1.86771C4.27373 1.8194 4.23521 1.76205 4.20895 1.69892C4.18269 1.63579 4.1692 1.56814 4.16926 1.49982C4.1692 1.43151 4.18269 1.36385 4.20895 1.30073C4.23521 1.2376 4.27373 1.18024 4.3223 1.13194C4.37088 1.08363 4.42856 1.04533 4.49204 1.01921C4.55551 0.993097 4.62355 0.979686 4.69225 0.979748Z"
                    fill="currentColor"
                  />
                  <path
                    id="Vector_2"
                    d="M6.38171 11.9358C9.42806 11.9358 9.23786 10.622 9.23786 10.622L9.23447 9.26098H6.32735V8.85235H10.3891C10.3891 8.85235 12.3384 9.07221 12.3384 6.01544C12.3384 2.95872 10.637 3.06713 10.637 3.06713H9.62156V4.48554C9.62156 4.48554 9.6763 6.17754 7.94727 6.17754H5.06397C5.06397 6.17754 3.44404 6.15151 3.44404 7.73449V10.3518C3.44404 10.3518 3.19806 11.9358 6.38166 11.9358H6.38171ZM7.98467 11.0206C7.91598 11.0206 7.84794 11.0072 7.78447 10.9811C7.72099 10.955 7.66331 10.9167 7.61473 10.8684C7.56616 10.8201 7.52764 10.7627 7.50138 10.6996C7.47512 10.6364 7.46163 10.5688 7.46169 10.5005C7.46163 10.4322 7.47511 10.3645 7.50136 10.3014C7.52762 10.2382 7.56614 10.1809 7.61472 10.1326C7.66329 10.0842 7.72097 10.0459 7.78445 10.0198C7.84794 9.9937 7.91597 9.98029 7.98467 9.98035C8.05337 9.98029 8.1214 9.9937 8.18488 10.0198C8.24836 10.0459 8.30604 10.0842 8.35462 10.1325C8.40319 10.1808 8.44171 10.2382 8.46797 10.3013C8.49423 10.3645 8.50772 10.4321 8.50766 10.5004C8.50772 10.5687 8.49423 10.6364 8.46797 10.6995C8.44171 10.7627 8.40319 10.82 8.35462 10.8683C8.30604 10.9166 8.24836 10.9549 8.18488 10.981C8.1214 11.0072 8.05337 11.0206 7.98467 11.0205V11.0206Z"
                    fill="currentColor"
                  />
                </g>
              </svg>
              <Link to={`/extensions/detail?id=${server.id}`} className="home-page-server-name">
                {server.name}
              </Link>
            </div>
          </div>
          <div className="card-content">
            <div>
              <div>
                <p className="card-description">{server.description}</p>
              </div>

              <div className="py-4">
                {server.is_builtin && (
                  <div className="flex items-center gap-2">
                  <Info
                    style={{ width: "12px", height: "12px" }}
                    className="text-textSubtle shrink-0"
                  />
                  <span
                    style={{ fontSize: "12px" }}
                    className="text-textSubtle leading-normal"
                  >
                    Can be enabled on the Extensions page in goose
                  </span>
                  </div>
                )}

                {!server.is_builtin && (server.show_install_command !== false) && (
                  <>
                  <button
                    onClick={() => setIsCommandVisible(!isCommandVisible)}
                    className="command-toggle"
                  >
                    <Terminal className="h-4 w-4" />
                    <h4 className="mx-2">Command</h4>
                    <ChevronRight
                    className={`ml-auto transition-transform ${
                      isCommandVisible ? "rotate-90" : ""
                    }`}
                    />
                  </button>
                  <AnimatePresence>
                    {isCommandVisible && (
                    <motion.div
                      className="command-content"
                      initial={{ opacity: 0, translateY: -20 }}
                      animate={{ opacity: 1, translateY: 0 }}
                      exit={{
                      opacity: 0,
                      translateY: -20,
                      transition: { duration: 0.1 },
                      }}
                    >
                      <code>{getExtensionCommand(server)}</code>
                    </motion.div>
                    )}
                  </AnimatePresence>
                  </>
                )}
              </div>
            </div>

            <div className="card-footer">
                {githubStars !== null && (
                <Link
                  to={server.link}
                  className="card-stats"
                  onClick={(e) => e.stopPropagation()}
                >
                  <Star className="h-4 w-4" />
                  <span>{formatStarCount(githubStars)} on GitHub</span>
                </Link>
                )}
              <div className="card-action">
                {server.is_builtin ? (
                  <div
                    className="built-in-badge"
                    title="This extension is built into goose and can be enabled in the settings page"
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
                    title={
                      server.installation_notes ||
                      "This extension requires manual setup. See details for instructions."
                    }
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
  );
}
