import React, { createContext, useContext, ReactNode } from 'react';
import { ChatType } from '../types/chat';
import { Recipe } from '../recipe';

// TODO(Douwe): We should not need this anymore
export const DEFAULT_CHAT_TITLE = 'New Chat';

interface ChatContextType {
  chat: ChatType;
  setChat: (chat: ChatType) => void;
  resetChat: () => void;
  hasActiveSession: boolean;
  setRecipe: (recipe: Recipe | null) => void;
  clearRecipe: () => void;
  // Context identification
  contextKey: string; // 'hub' or 'pair-{sessionId}'
}

const ChatContext = createContext<ChatContextType | undefined>(undefined);

interface ChatProviderProps {
  children: ReactNode;
  chat: ChatType;
  setChat: (chat: ChatType) => void;
  contextKey: string;
}

export const ChatProvider: React.FC<ChatProviderProps> = ({
  children,
  chat,
  setChat,
  contextKey,
}) => {
  const resetChat = () => {
    setChat({
      sessionId: '',
      name: DEFAULT_CHAT_TITLE,
      messages: [],
      recipe: null,
      recipeParameterValues: null,
    });
  };

  const setRecipe = (recipe: Recipe | null) => {
    setChat({
      ...chat,
      recipe: recipe,
      recipeParameterValues: null,
    });
  };

  const clearRecipe = () => {
    setChat({
      ...chat,
      recipe: null,
    });
  };

  const hasActiveSession = chat.messages.length > 0;

  const value: ChatContextType = {
    chat,
    setChat,
    resetChat,
    hasActiveSession,
    setRecipe,
    clearRecipe,
    contextKey,
  };

  return <ChatContext.Provider value={value}>{children}</ChatContext.Provider>;
};

export const useChatContext = (): ChatContextType | null => {
  const context = useContext(ChatContext);
  return context || null;
};
