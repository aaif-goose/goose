import { ChatState } from '../types/chatState';
import { UserInput } from '../types/message';
import LoadingGoose from './LoadingGoose';
import MarkdownContent from './MarkdownContent';

interface PendingChatViewProps {
  initialMessage?: UserInput;
}

function imageSrc(image: UserInput['images'][number]): string | null {
  if (!image.data || !image.mimeType) return null;
  return `data:${image.mimeType};base64,${image.data}`;
}

/**
 * Shown on /pair while session/new is still running. The submitted message is
 * already on screen; the real session replaces this once the agent is ready.
 */
export default function PendingChatView({ initialMessage }: PendingChatViewProps) {
  const text = initialMessage?.msg.trim() ?? '';
  const images = (initialMessage?.images ?? [])
    .map(imageSrc)
    .filter((src): src is string => src !== null);

  return (
    <div className="flex flex-col w-full h-full min-h-0 bg-background-primary">
      <div className="flex-1 min-h-0 overflow-y-auto px-6 pt-6">
        {(text || images.length > 0) && (
          <div className="message flex justify-end w-full">
            <div className="flex-col max-w-[85%] w-fit">
              {text && (
                <div className="user-message-bubble flex bg-text-primary text-background-primary rounded-xl py-2.5 px-4">
                  <MarkdownContent
                    content={text}
                    className="!text-inherit prose-a:!text-inherit prose-headings:!text-inherit prose-strong:!text-inherit prose-em:!text-inherit prose-li:!text-inherit prose-p:!text-inherit user-message"
                  />
                </div>
              )}
              {images.length > 0 && (
                <div className="flex flex-wrap gap-2 mt-2 justify-end">
                  {images.map((src) => (
                    <img
                      key={src}
                      src={src}
                      alt=""
                      className="max-h-40 max-w-full rounded-lg object-contain"
                    />
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
      <div className="px-4 pb-4">
        <LoadingGoose chatState={ChatState.LoadingConversation} />
      </div>
    </div>
  );
}
