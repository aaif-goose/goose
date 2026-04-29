import { useHomeWidgetStore } from "../stores/homeWidgetStore";
import type { WidgetNavigationHandlers } from "../widgets/types";
import { WidgetCanvas } from "./WidgetCanvas";

export function HomeView({
  onOpenAgent,
  onStartChatWithPersona,
  onSelectSession,
}: WidgetNavigationHandlers = {}) {
  const instances = useHomeWidgetStore((state) => state.instances);

  return (
    <div className="relative h-full w-full overflow-hidden">
      <WidgetCanvas
        instances={instances}
        onOpenAgent={onOpenAgent}
        onStartChatWithPersona={onStartChatWithPersona}
        onSelectSession={onSelectSession}
      />
    </div>
  );
}
