import React, { type ReactNode } from "react";
import OriginalBlogTagsPostsPage from "@theme-original/BlogTagsPostsPage";
import type { Props } from "@theme/BlogTagsPostsPage";
import { useOiKitNavigation } from "@site/src/components/OiKitNavigation";

export default function BlogTagsPostsPage(props: Props): ReactNode {
  const isOiKit = useOiKitNavigation();

  return (
    <OriginalBlogTagsPostsPage
      {...props}
      sidebar={isOiKit ? { ...props.sidebar, items: [] } : props.sidebar}
    />
  );
}
