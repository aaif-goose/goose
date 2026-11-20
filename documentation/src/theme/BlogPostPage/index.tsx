import React, { type ReactNode } from "react";
import useBaseUrl from "@docusaurus/useBaseUrl";
import OriginalBlogPostPage from "@theme-original/BlogPostPage";
import type { Props } from "@theme/BlogPostPage";
import { OiKitBlogContext } from "@site/src/components/OiKitNavigation";

export default function BlogPostPage(props: Props): ReactNode {
  const oiKitTag = useBaseUrl("/blog/tags/oi-kit");
  const isOiKit = props.content.metadata.tags.some(
    (tag) => tag.permalink === oiKitTag,
  );

  return (
    <OiKitBlogContext.Provider value={isOiKit}>
      <OriginalBlogPostPage
        {...props}
        sidebar={isOiKit ? { ...props.sidebar, items: [] } : props.sidebar}
      />
    </OiKitBlogContext.Provider>
  );
}
