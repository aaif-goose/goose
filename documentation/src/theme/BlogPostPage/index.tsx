import React, { type ReactNode } from "react";
import useBaseUrl from "@docusaurus/useBaseUrl";
import OriginalBlogPostPage from "@theme-original/BlogPostPage";
import type { Props } from "@theme/BlogPostPage";
import { GdkBlogContext } from "@site/src/components/GdkNavigation";

export default function BlogPostPage(props: Props): ReactNode {
  const gdkTag = useBaseUrl("/blog/tags/gdk");
  const isGdk = props.content.metadata.tags.some(
    (tag) => tag.permalink === gdkTag,
  );

  return (
    <GdkBlogContext.Provider value={isGdk}>
      <OriginalBlogPostPage
        {...props}
        sidebar={isGdk ? { ...props.sidebar, items: [] } : props.sidebar}
      />
    </GdkBlogContext.Provider>
  );
}
