import React from "react";
import Admonition from '@theme/Admonition';

export const PlatformExtensionNote = ({ defaultEnabled = true }) => {
  return (
    <Admonition type="info" title="Platform Extension">
       <p>This is a <a href="/docs/getting-started/using-extensions#built-in-platform-extensions">built-in platform extension</a>{defaultEnabled && " that's enabled by default for new users"}. Platform extensions provide core functionality and are used within goose just like external MCP server extensions.</p>
    </Admonition>
  );
};
