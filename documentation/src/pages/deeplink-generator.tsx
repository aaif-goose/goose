import React, { useState, useCallback, useEffect } from 'react';
import Layout from "@theme/Layout";
import { Copy, Check, Plus, X } from "lucide-react";
import { Button } from "@site/src/components/ui/button";
import { cn } from "@site/src/utils/cn";

interface EnvironmentVariable {
  name: string;
  description: string;
  required: boolean;
}

interface RequestHeader {
  name: string;
  description: string;
  required: boolean;
}

type ExtensionType = 'stdio' | 'streamable_http';

interface ServerConfig {
  is_builtin: boolean;
  id: string;
  name?: string;
  description?: string;
  command?: string;
  url?: string;
  type?: ExtensionType;
  environmentVariables: EnvironmentVariable[];
  headers?: RequestHeader[];
}

export default function DeeplinkGenerator() {
  const [activeTab, setActiveTab] = useState<'form' | 'json'>('form');
  const [isBuiltin, setIsBuiltin] = useState(false);
  const [extensionType, setExtensionType] = useState<ExtensionType>('stdio');
  const [id, setId] = useState('');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [command, setCommand] = useState('');
  const [endpointUrl, setEndpointUrl] = useState('');
  const [envVars, setEnvVars] = useState<EnvironmentVariable[]>([]);
  const [headers, setHeaders] = useState<RequestHeader[]>([]);
  const [generatedLink, setGeneratedLink] = useState('');
  const [jsonInput, setJsonInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    const sampleJson = {
      is_builtin: false,
      type: "stdio",
      id: "example-extension",
      name: "Example Extension",
      description: "An example goose extension",
      command: "npx @gooseai/example-extension",
      environmentVariables: [
        {
          name: "API_KEY",
          description: "Your API key",
          required: true
        }
      ]
    };
    setJsonInput(JSON.stringify(sampleJson, null, 2));
  }, []);

  useEffect(() => {
    const urlParams = new URLSearchParams(window.location.search);
    if (urlParams.toString()) {
      try {
        if (urlParams.get('cmd') === 'goose' && urlParams.getAll('arg').includes('mcp')) {
          const args = urlParams.getAll('arg');
          const extensionId = args[args.indexOf('mcp') + 1];
          if (!extensionId) {
            throw new Error('Missing extension ID in args');
          }

          const server: ServerConfig = {
            is_builtin: true,
            id: extensionId,
            environmentVariables: []
          };
          const link = generateDeeplink(server);
          handleGeneratedLink(link, true);
          return;
        }

        const remoteUrl = urlParams.get('url');
        if (remoteUrl) {
          const id = urlParams.get('id');
          const name = urlParams.get('name');
          const description = urlParams.get('description');
          if (!id || !name || !description) {
            throw new Error('Missing required parameters. Need: id, name, and description');
          }
          const server: ServerConfig = {
            is_builtin: false,
            type: 'streamable_http',
            id,
            name,
            description,
            url: remoteUrl,
            environmentVariables: [],
            headers: [],
          };
          urlParams.getAll('env').forEach(env => {
            const separatorIndex = env.indexOf('=');
            if (separatorIndex > 0) {
              server.environmentVariables.push({
                name: env.slice(0, separatorIndex),
                description: env.slice(separatorIndex + 1),
                required: true,
              });
            }
          });
          urlParams.getAll('header').forEach(header => {
            const separatorIndex = header.indexOf('=');
            if (separatorIndex > 0) {
              server.headers.push({
                name: header.slice(0, separatorIndex),
                description: header.slice(separatorIndex + 1),
                required: true,
              });
            }
          });
          const link = generateDeeplink(server);
          handleGeneratedLink(link, true);
          return;
        }

        const cmd = urlParams.get('cmd');
        if (!cmd) {
          throw new Error('Missing required parameter: cmd');
        }

        const args = urlParams.getAll('arg') || [];
        const command = [cmd, ...args].join(' ');
        const id = urlParams.get('id');
        const name = urlParams.get('name');
        const description = urlParams.get('description');

        if (!id || !name || !description) {
          throw new Error('Missing required parameters. Need: id, name, and description');
        }

        const server: ServerConfig = {
          is_builtin: false,
          type: 'stdio',
          id,
          name,
          description,
          command,
          environmentVariables: []
        };

        const envVars = urlParams.getAll('env');
        if (envVars.length > 0) {
          envVars.forEach(env => {
            const separatorIndex = env.indexOf('=');
            if (separatorIndex > 0) {
              server.environmentVariables.push({
                name: env.slice(0, separatorIndex),
                description: env.slice(separatorIndex + 1),
                required: true
              });
            }
          });
        }

        const link = generateDeeplink(server);
        handleGeneratedLink(link, true);
      } catch (error) {
        setError(error.message);
      }
    }
  }, []);

  const handleGeneratedLink = useCallback((link: string, shouldRedirect = false) => {
    if (shouldRedirect) {
      window.location.href = link;
    } else {
      setGeneratedLink(link);
      setError('');
    }
  }, []);

  const generateDeeplink = (server: ServerConfig): string => {
    if (server.is_builtin) {
      const queryParams = [
        'cmd=goose',
        'arg=mcp',
        `arg=${encodeURIComponent(server.id)}`,
        `description=${encodeURIComponent(server.id)}`
      ].join('&');
      return `goose://extension?${queryParams}`;
    }

    if (server.url || server.type === 'streamable_http') {
      const queryParams = [
        `type=streamable_http`,
        `url=${encodeURIComponent(server.url || '')}`,
        `id=${encodeURIComponent(server.id)}`,
        `name=${encodeURIComponent(server.name)}`,
        `description=${encodeURIComponent(server.description)}`,
        ...(server.environmentVariables || [])
          .filter((env) => env.required)
          .map(
            (env) => `env=${encodeURIComponent(`${env.name}=${env.description}`)}`
          ),
        ...(server.headers || [])
          .filter((header) => header.required)
          .map(
            (header) => `header=${encodeURIComponent(`${header.name}=${header.description}`)}`
          ),
      ].join("&");

      return `goose://extension?${queryParams}`;
    }

    const parts = server.command.split(" ");
    const baseCmd = parts[0];
    const args = parts.slice(1);
    const queryParams = [
      `cmd=${encodeURIComponent(baseCmd)}`,
      ...args.map((arg) => `arg=${encodeURIComponent(arg)}`),
      `id=${encodeURIComponent(server.id)}`,
      `name=${encodeURIComponent(server.name)}`,
      `description=${encodeURIComponent(server.description)}`,
      ...(server.environmentVariables || [])
        .filter((env) => env.required)
        .map(
          (env) => `env=${encodeURIComponent(`${env.name}=${env.description}`)}`
        ),
    ].join("&");

    return `goose://extension?${queryParams}`;
  };

  const handleFormSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    const server: ServerConfig = {
      is_builtin: isBuiltin,
      type: extensionType,
      id,
      name,
      description,
      command: extensionType === 'stdio' ? command : undefined,
      url: extensionType === 'streamable_http' ? endpointUrl : undefined,
      environmentVariables: envVars,
      headers: extensionType === 'streamable_http' ? headers : undefined,
    };

    try {
      const link = generateDeeplink(server);
      handleGeneratedLink(link);
    } catch (error) {
      setError(error.message);
    }
  }, [isBuiltin, extensionType, id, name, description, command, endpointUrl, envVars, headers]);

  const handleJsonSubmit = useCallback(() => {
    try {
      const server = JSON.parse(jsonInput);
      const link = generateDeeplink(server);
      handleGeneratedLink(link);
    } catch (error) {
      setError('Invalid JSON: ' + error.message);
    }
  }, [jsonInput]);

  const handleAddEnvVar = useCallback(() => {
    setEnvVars(prev => [...prev, { name: '', description: '', required: true }]);
  }, []);

  const handleRemoveEnvVar = useCallback((index: number) => {
    setEnvVars(prev => prev.filter((_, i) => i !== index));
  }, []);

  const handleEnvVarChange = useCallback((index: number, field: 'name' | 'description', value: string) => {
    setEnvVars(prev => prev.map((env, i) =>
      i === index ? { ...env, [field]: value } : env
    ));
  }, []);

  const handleAddHeader = useCallback(() => {
    setHeaders(prev => [...prev, { name: '', description: '', required: true }]);
  }, []);

  const handleRemoveHeader = useCallback((index: number) => {
    setHeaders(prev => prev.filter((_, i) => i !== index));
  }, []);

  const handleHeaderChange = useCallback((index: number, field: 'name' | 'description', value: string) => {
    setHeaders(prev => prev.map((h, i) =>
      i === index ? { ...h, [field]: value } : h
    ));
  }, []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(generatedLink)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch(err => setError('Failed to copy: ' + err.message));
  }, [generatedLink]);

  return (
    <Layout>
      <div className="container mx-auto px-4 py-8 md:p-24">
        <div className="pb-8 md:pb-16">
          <h1 className="text-4xl md:text-[64px] font-medium text-textProminent">
            Deeplink Generator
          </h1>
          <p className="text-textProminent">
            Generate installation deeplinks for goose extensions that can be shared with others.
          </p>
        </div>

        <div className="bg-bgApp border border-borderSubtle rounded-lg p-6 mb-8 shadow-sm">
          <div className="tabs mb-6">
            <div className="flex p-1 bg-bgSubtle rounded-lg">
              <button
                onClick={() => setActiveTab('form')}
                className={cn(
                  "flex-1 px-6 py-3",
                  activeTab === "form"
                    ? "bg-black text-white dark:bg-white dark:text-black"
                    : "bg-bgApp text-textStandard"
                )}
              >
                Form
              </button>
              <button
                onClick={() => setActiveTab('json')}
                className={cn(
                  "flex-1 px-6 py-3",
                  activeTab === "json"
                    ? "bg-black text-white dark:bg-white dark:text-black"
                    : "bg-bgApp text-textStandard"
                )}
              >
                JSON
              </button>
            </div>
          </div>

          {error && (
            <div className="mb-6 p-4 bg-red-100 border border-red-400 text-red-700 rounded-lg">
              {error}
            </div>
          )}

          {activeTab === 'form' ? (
            <form onSubmit={handleFormSubmit} className="space-y-6">
              <div>
                <label className="flex items-center space-x-2 text-sm font-medium text-textStandard mb-2">
                  <input
                    type="checkbox"
                    checked={isBuiltin}
                    onChange={(e) => setIsBuiltin(e.target.checked)}
                    className="rounded border-borderSubtle"
                  />
                  <span>Is Built-in Extension</span>
                </label>
              </div>

              <div>
                <label className="block text-sm font-medium text-textStandard mb-2">
                  ID <span className="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={id}
                  onChange={(e) => setId(e.target.value)}
                  required
                  className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                  placeholder="Enter extension ID"
                />
              </div>

              {!isBuiltin && (
                <>
                  <div>
                    <label className="block text-sm font-medium text-textStandard mb-2">
                      Name <span className="text-red-500">*</span>
                    </label>
                    <input
                      type="text"
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      required
                      className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                      placeholder="Extension name"
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-textStandard mb-2">
                      Description <span className="text-red-500">*</span>
                    </label>
                    <input
                      type="text"
                      value={description}
                      onChange={(e) => setDescription(e.target.value)}
                      required
                      className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                      placeholder="Brief description"
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-textStandard mb-2">
                      Extension Type <span className="text-red-500">*</span>
                    </label>
                    <div className="flex gap-4">
                      <label className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="radio"
                          name="extensionType"
                          value="stdio"
                          checked={extensionType === 'stdio'}
                          onChange={() => setExtensionType('stdio')}
                          className="accent-secondary"
                        />
                        <span className="text-sm text-textStandard">STDIO</span>
                      </label>
                      <label className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="radio"
                          name="extensionType"
                          value="streamable_http"
                          checked={extensionType === 'streamable_http'}
                          onChange={() => setExtensionType('streamable_http')}
                          className="accent-secondary"
                        />
                        <span className="text-sm text-textStandard">Streamable HTTP</span>
                      </label>
                    </div>
                  </div>

                  {extensionType === 'stdio' ? (
                    <div>
                      <label className="block text-sm font-medium text-textStandard mb-2">
                        Command <span className="text-red-500">*</span>
                      </label>
                      <input
                        type="text"
                        value={command}
                        onChange={(e) => setCommand(e.target.value)}
                        required
                        className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                        placeholder="npx @gooseai/example-extension"
                      />
                    </div>
                  ) : (
                    <div>
                      <label className="block text-sm font-medium text-textStandard mb-2">
                        Endpoint URL <span className="text-red-500">*</span>
                      </label>
                      <input
                        type="url"
                        value={endpointUrl}
                        onChange={(e) => setEndpointUrl(e.target.value)}
                        required
                        className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                        placeholder="https://example.com/mcp"
                      />
                    </div>
                  )}

                  <div>
                    <label className="block text-sm font-medium text-textStandard mb-2">
                      Environment Variables
                    </label>
                    <div className="space-y-3">
                      {envVars.map((env, index) => (
                        <div key={index} className="flex gap-2">
                          <input
                            type="text"
                            value={env.name}
                            onChange={(e) => handleEnvVarChange(index, 'name', e.target.value)}
                            className="flex-1 p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                            placeholder="Variable Name"
                          />
                          <input
                            type="text"
                            value={env.description}
                            onChange={(e) => handleEnvVarChange(index, 'description', e.target.value)}
                            className="flex-1 p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                            placeholder="Description"
                          />
                          <Button
                            type="button"
                            onClick={() => handleRemoveEnvVar(index)}
                            className="p-3"
                          >
                            <X className="h-4 w-4" />
                          </Button>
                        </div>
                      ))}
                      <Button
                        type="button"
                        onClick={handleAddEnvVar}
                        className="w-full flex items-center justify-center gap-2"
                      >
                        <Plus className="h-4 w-4" />
                        Add Variable
                      </Button>
                    </div>
                  </div>

                  {extensionType === 'streamable_http' && (
                    <div>
                      <label className="block text-sm font-medium text-textStandard mb-2">
                        Request Headers
                      </label>
                      <div className="space-y-3">
                        {headers.map((header, index) => (
                          <div key={index} className="flex gap-2">
                            <input
                              type="text"
                              value={header.name}
                              onChange={(e) => handleHeaderChange(index, 'name', e.target.value)}
                              className="flex-1 p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                              placeholder="Header Name"
                            />
                            <input
                              type="text"
                              value={header.description}
                              onChange={(e) => handleHeaderChange(index, 'description', e.target.value)}
                              className="flex-1 p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard"
                              placeholder="Description"
                            />
                            <Button
                              type="button"
                              onClick={() => handleRemoveHeader(index)}
                              className="p-3"
                            >
                              <X className="h-4 w-4" />
                            </Button>
                          </div>
                        ))}
                        <Button
                          type="button"
                          onClick={handleAddHeader}
                          className="w-full flex items-center justify-center gap-2"
                        >
                          <Plus className="h-4 w-4" />
                          Add Header
                        </Button>
                      </div>
                    </div>
                  )}
                </>
              )}

              <div>
                <Button type="submit">
                  Generate Deeplink
                </Button>
              </div>
            </form>
          ) : (
            <div className="space-y-6">
              <div>
                <label className="block text-sm font-medium text-textStandard mb-2">
                  JSON Configuration
                </label>
                <textarea
                  value={jsonInput}
                  onChange={(e) => setJsonInput(e.target.value)}
                  rows={10}
                  className="w-full p-3 border border-borderSubtle rounded-lg bg-bgSubtle text-textStandard font-mono text-sm"
                />
              </div>
              <div>
                <Button onClick={handleJsonSubmit}>
                  Generate Deeplink
                </Button>
              </div>
            </div>
          )}
        </div>

        {generatedLink && (
          <div className="bg-bgApp border border-borderSubtle rounded-lg p-6 shadow-sm">
            <h2 className="text-2xl font-medium mb-4 text-textProminent">Generated Deeplink</h2>
            <div className="bg-bgSubtle rounded-lg p-4 mb-4 overflow-x-auto">
              <pre className="text-sm text-textStandard font-mono break-all whitespace-pre-wrap">
                {generatedLink}
              </pre>
            </div>
            <div className="flex justify-end">
              <Button
                onClick={handleCopy}
                className="flex items-center gap-2"
              >
                {copied ? (
                  <>
                    <Check className="h-4 w-4" />
                    Copied!
                  </>
                ) : (
                  <>
                    <Copy className="h-4 w-4" />
                    Copy Deeplink
                  </>
                )}
              </Button>
            </div>
          </div>
        )}

        <div className="mt-8 bg-bgApp border border-borderSubtle rounded-lg p-6 shadow-sm">
          <h2 className="text-2xl font-medium mb-4 text-textProminent">How to Use</h2>
          <ol className="list-decimal pl-6 space-y-2 text-textStandard">
            <li>Fill in the form above with your extension details.</li>
            <li>For built-in extensions, just check the "Is Built-in" box and provide the ID.</li>
            <li>For custom extensions:
              <ul className="list-disc pl-6 mt-2">
                <li>Provide a unique ID, name, and description</li>
                <li>Choose <strong>STDIO</strong> and enter the command to run your extension (e.g. <code>npx @gooseai/my-ext</code>), or choose <strong>Streamable HTTP</strong> and enter the endpoint URL</li>
                <li>Add any required environment variables</li>
                <li>For Streamable HTTP, add any required request headers</li>
              </ul>
            </li>
            <li>Click "Generate Deeplink" to create your installation deeplink.</li>
            <li>Copy and share the generated deeplink — when users click it, it will open goose Desktop and prompt them to install your extension.</li>
          </ol>
        </div>
      </div>
    </Layout>
  );
}
