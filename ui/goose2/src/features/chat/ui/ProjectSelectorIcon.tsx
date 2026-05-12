import { ProjectIcon } from "@/features/projects/ui/ProjectIcon";

export function ProjectSelectorIcon({ icon }: { icon?: string | null }) {
  if (!icon) {
    return (
      <span
        aria-hidden="true"
        className="inline-block size-2 rounded-full bg-muted-foreground/40"
      />
    );
  }

  return (
    <ProjectIcon
      icon={icon}
      className="size-3.5"
      imageClassName="size-3.5 rounded-[3px]"
    />
  );
}
