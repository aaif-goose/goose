import { describe, expect, it } from 'vitest';
import { resolveMcpAppMetadata } from './ToolCallWithResponse';

describe('MCP app metadata binding', () => {
  it('preserves authoritative ownership when a tool name contains the delimiter', () => {
    const metadata = resolveMcpAppMetadata({
      ui: { resourceUri: 'ui://victim/render' },
      extensionName: 'victim',
      toolName: 'victim__render',
      toolNameIsActual: true,
    });

    expect(metadata).toEqual({
      resourceUri: 'ui://victim/render',
      extensionName: 'victim',
      toolName: 'victim__render',
    });
  });

  it('normalizes the exact owner prefix from trusted legacy replay metadata', () => {
    const metadata = resolveMcpAppMetadata({
      ui: { resourceUri: 'ui://victim/render' },
      extensionName: 'victim',
      toolName: 'victim__render__secret',
    });

    expect(metadata).toEqual({
      resourceUri: 'ui://victim/render',
      extensionName: 'victim',
      toolName: 'render__secret',
    });
  });

  it('does not infer ownership from incomplete metadata', () => {
    const metadata = resolveMcpAppMetadata({
      ui: { resourceUri: 'ui://victim/render' },
    });

    expect(metadata).toBeNull();
  });

  it('rejects untrusted request metadata when authenticated response metadata is absent', () => {
    expect(resolveMcpAppMetadata(undefined)).toBeNull();
  });

  it('uses complete authenticated response metadata', () => {
    const metadata = resolveMcpAppMetadata({
      ui: { resourceUri: 'ui://victim/render' },
      extensionName: 'victim',
      toolName: 'render__secret',
      toolNameIsActual: true,
    });

    expect(metadata).toEqual({
      resourceUri: 'ui://victim/render',
      extensionName: 'victim',
      toolName: 'render__secret',
    });
  });
});
