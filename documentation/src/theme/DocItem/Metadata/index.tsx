import React, { type ReactNode } from "react";
import { PageMetadata } from "@docusaurus/theme-common";
import { useDoc } from "@docusaurus/plugin-content-docs/client";
import OriginalDocItemMetadata from "@theme-original/DocItem/Metadata";

export default function DocItemMetadata(): ReactNode {
  const { metadata, frontMatter, assets } = useDoc();
  if (!metadata.id.startsWith("gdk/")) {
    return <OriginalDocItemMetadata />;
  }

  return (
    <PageMetadata
      title={metadata.title}
      description={metadata.description}
      keywords={frontMatter.keywords}
      image={assets.image ?? frontMatter.image ?? "/img/gdk/social-preview.png"}
    />
  );
}
