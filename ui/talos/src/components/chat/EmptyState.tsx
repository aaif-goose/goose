import { Composer, type ComposerProps } from "./Composer";

export function EmptyState({
  composerProps,
  name,
}: {
  composerProps: ComposerProps;
  name: string;
}) {
  const hour = new Date().getHours();
  const tod = hour < 5 ? "evening" : hour < 12 ? "morning" : hour < 18 ? "afternoon" : "evening";
  return (
    <div className="empty-state">
      <div className="greeting">
        Good {tod}, <span className="accent">{name}</span>.<br />
        <em>What are we working on?</em>
      </div>
      <Composer {...composerProps} />
    </div>
  );
}
