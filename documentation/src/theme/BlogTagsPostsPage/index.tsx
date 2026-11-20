import React, { type ReactNode } from "react";
import OriginalBlogTagsPostsPage from "@theme-original/BlogTagsPostsPage";
import type { Props } from "@theme/BlogTagsPostsPage";
import { useGdkNavigation } from "@site/src/components/GdkNavigation";

export default function BlogTagsPostsPage(props: Props): ReactNode {
  const isGdk = useGdkNavigation();

  return (
    <OriginalBlogTagsPostsPage
      {...props}
      sidebar={isGdk ? { ...props.sidebar, items: [] } : props.sidebar}
    />
  );
}
