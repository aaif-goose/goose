import Layout from "@theme/Layout";
import { ArrowLeft, Download, Copy, ExternalLink, FileText, Check } from "lucide-react";
import { useLocation } from "@docusaurus/router";
import { useEffect, useState } from "react";
import Link from "@docusaurus/Link";
import CodeBlock from "@theme/CodeBlock";
import { Button } from "@site/src/components/ui/button";
import { getSkillById } from "@site/src/utils/skills";
import type { Skill } from "@site/src/pages/skills/types";
import ReactMarkdown from "react-markdown";

type PackageManager = 'npx' | 'pnpm' | 'bun';

const PACKAGE_MANAGERS: { id: PackageManager; label: string; prefix: string }[] = [
  { id: 'npx', label: 'npx', prefix: 'npx' },
  { id: 'pnpm', label: 'pnpm', prefix: 'pnpm dlx' },
  { id: 'bun', label: 'bun', prefix: 'bunx' },
];

function generateInstallCommand(repoUrl: string, skillId: string, packageManager: PackageManager): string {
  const prefix = PACKAGE_MANAGERS.find(pm => pm.id === packageManager)?.prefix || 'npx';
  return `${prefix} skills add ${repoUrl} --skill ${skillId}`;
}

export default function SkillDetailPage(): JSX.Element {
  const location = useLocation();
  const [skill, setSkill] = useState<Skill | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedPM, setSelectedPM] = useState<PackageManager>('npx');
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const loadSkill = async () => {
      try {
        setLoading(true);
        setError(null);

        const params = new URLSearchParams(location.search);
        const id = params.get("id");
        if (!id) {
          setError("No skill ID provided");
          return;
        }

        const skillData = getSkillById(id);
        if (skillData) {
          setSkill(skillData);
        } else {
          setError("Skill not found");
        }
      } catch (err) {
        setError("Failed to load skill details");
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    loadSkill();
  }, [location]);

  const handleCopyInstall = () => {
    if (skill) {
      const command = generateInstallCommand(skill.repoUrl, skill.id, selectedPM);
      navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleDownload = () => {
    if (skill) {
      const zipUrl = `/skills-data-zips/${skill.id}.zip`;
      const link = document.createElement('a');
      link.href = zipUrl;
      link.download = `${skill.id}.zip`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
    }
  };

  if (loading) {
    return (
      <Layout>
        <div className="min-h-screen flex items-start justify-center py-16">
          <div className="container max-w-5xl mx-auto px-4 animate-pulse">
            <div className="h-12 w-48 bg-bgSubtle dark:bg-zinc-800 rounded-lg mb-4"></div>
            <div className="h-6 w-full bg-bgSubtle dark:bg-zinc-800 rounded-lg mb-2"></div>
            <div className="h-6 w-2/3 bg-bgSubtle dark:bg-zinc-800 rounded-lg"></div>
          </div>
        </div>
      </Layout>
    );
  }

  if (error || !skill) {
    return (
      <Layout>
        <div className="min-h-screen flex items-start justify-center py-16">
          <div className="container max-w-5xl mx-auto px-4 text-red-500">
            {error || "Skill not found"}
          </div>
        </div>
      </Layout>
    );
  }

  const currentCommand = generateInstallCommand(skill.repoUrl, skill.id, selectedPM);

  return (
    <Layout
      title={skill.name}
      description={skill.description}
    >
      <div className="min-h-screen py-12">
        <div className="max-w-4xl mx-auto px-4">
          {/* Header */}
          <div className="mb-8 flex justify-between items-start">
            <Link to="/skills">
              <Button className="flex items-center gap-2">
                <ArrowLeft className="h-4 w-4" />
                Back to Skills
              </Button>
            </Link>
            {skill.author && (
              <span className="text-sm text-textSubtle">
                by {skill.author}
              </span>
            )}
          </div>

          <div className="bg-white dark:bg-[#1A1A1A] border border-borderSubtle dark:border-zinc-700 rounded-xl p-8 shadow-md">
            {/* Title and badges */}
            <div className="flex items-start justify-between gap-4 mb-4">
              <h1 className="text-4xl font-semibold text-textProminent dark:text-white">
                {skill.name}
              </h1>
              <div className="flex gap-2 flex-shrink-0">
                {skill.isCommunity && (
                  <span className="inline-flex items-center h-7 px-3 rounded-full bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200 text-sm font-medium border border-yellow-200 dark:border-yellow-800">
                    Community
                  </span>
                )}
                {skill.version && (
                  <span className="inline-flex items-center h-7 px-3 rounded-full bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400 text-sm font-medium">
                    v{skill.version}
                  </span>
                )}
              </div>
            </div>

            {/* Description */}
            <p className="text-textSubtle dark:text-zinc-400 text-lg mb-6">
              {skill.description}
            </p>

            {/* Tags */}
            {skill.tags.length > 0 && (
              <div className="mb-6">
                <div className="flex flex-wrap gap-2">
                  {skill.tags.map((tag, index) => (
                    <Link
                      key={index}
                      to={`/skills?tag=${tag}`}
                      className="inline-flex items-center h-7 px-3 rounded-full border border-zinc-300 bg-zinc-100 text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 text-xs font-medium hover:bg-zinc-200 dark:hover:bg-zinc-800 transition-colors no-underline"
                    >
                      {tag}
                    </Link>
                  ))}
                </div>
              </div>
            )}

            {/* Install section with tabs */}
            <div className="mb-6 p-4 bg-zinc-50 dark:bg-zinc-900 rounded-lg border border-zinc-200 dark:border-zinc-700">
              <h2 className="text-lg font-medium mb-3 text-textProminent dark:text-white flex items-center gap-2">
                <Download className="h-5 w-5" />
                Install
              </h2>
              
              {/* Package manager tabs */}
              <div className="flex gap-1 mb-3 border-b border-zinc-200 dark:border-zinc-700">
                {PACKAGE_MANAGERS.map((pm) => (
                  <button
                    key={pm.id}
                    onClick={() => setSelectedPM(pm.id)}
                    className={`px-4 py-2 text-sm font-medium transition-colors relative ${
                      selectedPM === pm.id
                        ? 'text-purple-600 dark:text-purple-400'
                        : 'text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-300'
                    }`}
                  >
                    {pm.label}
                    {selectedPM === pm.id && (
                      <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-purple-600 dark:bg-purple-400" />
                    )}
                  </button>
                ))}
              </div>

              {/* Install command */}
              <div className="flex items-center gap-2 mb-2">
                <code className="flex-1 bg-zinc-200 dark:bg-zinc-800 px-3 py-2 rounded text-sm font-mono text-zinc-800 dark:text-zinc-200 overflow-x-auto">
                  {currentCommand}
                </code>
                <Button
                  onClick={handleCopyInstall}
                  className={`flex items-center gap-2 flex-shrink-0 transition-colors ${
                    copied
                      ? "bg-green-600 hover:bg-green-700 text-white"
                      : "bg-purple-600 hover:bg-purple-700 text-white"
                  }`}
                >
                  {copied ? (
                    <>
                      <Check className="h-4 w-4" />
                      Copied!
                    </>
                  ) : (
                    <>
                      <Copy className="h-4 w-4" />
                      Copy
                    </>
                  )}
                </Button>
              </div>
              <p className="text-xs text-zinc-500 dark:text-zinc-500">
                Requires <a href="https://goose-docs.ai/docs/mcp/summon-mcp" className="text-purple-600 hover:underline">Summon extension</a> enabled
              </p>
            </div>

            {/* ZIP Download - secondary option */}
            <div className="mb-6 flex items-center gap-3 text-sm">
              <span className="text-zinc-500 dark:text-zinc-400">Prefer manual install?</span>
              <button
                onClick={handleDownload}
                className="inline-flex items-center gap-1.5 text-zinc-600 dark:text-zinc-400 hover:text-purple-600 dark:hover:text-purple-400 transition-colors"
              >
                <Download className="h-4 w-4" />
                Download ZIP
              </button>
            </div>

            {/* View Source - always show, links to Agent-Skills repo */}
            <div className="mb-6">
              <a
                href={skill.viewSourceUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-2 text-purple-600 hover:underline dark:text-purple-400"
              >
                <ExternalLink className="h-4 w-4" />
                View Source on GitHub
              </a>
            </div>

            {/* Supporting files */}
            {skill.hasSupporting && skill.supportingFiles.length > 0 && (
              <div className="mb-6 border-t border-borderSubtle dark:border-zinc-700 pt-6">
                <h2 className="text-xl font-medium mb-3 text-textProminent dark:text-white flex items-center gap-2">
                  <FileText className="h-5 w-5" />
                  Supporting Files
                </h2>
                <p className="text-sm text-zinc-600 dark:text-zinc-400 mb-3">
                  This skill includes additional files that will be installed with it:
                </p>
                <ul className="list-disc list-inside text-sm text-zinc-600 dark:text-zinc-400 space-y-1">
                  {skill.supportingFiles.map((file, index) => (
                    <li key={index}>
                      <code className="bg-zinc-100 dark:bg-zinc-800 px-1 rounded">{file}</code>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Skill content (markdown) */}
            <div className="border-t border-borderSubtle dark:border-zinc-700 pt-6">
              <h2 className="text-2xl font-medium mb-4 text-textProminent dark:text-white">
                Skill Instructions
              </h2>
              <div className="prose prose-zinc dark:prose-invert max-w-none">
                <ReactMarkdown
                  components={{
                    code({ node, inline, className, children, ...props }) {
                      const match = /language-(\w+)/.exec(className || '');
                      if (!inline && match) {
                        return (
                          <CodeBlock language={match[1]}>
                            {String(children).replace(/\n$/, '')}
                          </CodeBlock>
                        );
                      }
                      return (
                        <code className={className} {...props}>
                          {children}
                        </code>
                      );
                    },
                    h1({ children }) {
                      return <h2 className="text-2xl font-semibold mt-6 mb-4">{children}</h2>;
                    },
                  }}
                >
                  {skill.content}
                </ReactMarkdown>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Layout>
  );
}
