import React from 'react';
import {
  Archive,
  Brain,
  Camera,
  Code2,
  Delegate,
  Eye,
  FileEdit,
  FilePlus,
  FileText,
  Globe,
  Monitor,
  Numbers,
  Save,
  Search,
  Settings,
  Terminal,
  Tool,
} from '../components/icons/toolcalls';

export type ToolIconProps = {
  className?: string;
};

/**
 * Maps tool names to their corresponding icon components
 * @param toolName - The name of the tool (extracted from toolCall.name)
 * @returns React component for the tool icon
 */
export const getToolIcon = (toolName: string): React.ComponentType<ToolIconProps> => {
  switch (toolName) {
    // Developer Extension Tools
    case 'text_editor':
      return FileEdit;
    case 'shell':
      return Terminal;

    // Memory Extension Tools
    case 'remember_memory':
      return Save;
    case 'retrieve_memories':
      return Brain;

    // Computer Controller Extension Tools
    case 'automation_script':
      return Settings;
    case 'computer_control':
      return Monitor;
    case 'web_scrape':
      return Globe;
    case 'screen_capture':
      return Camera;
    case 'pdf_tool':
      return FileText;
    case 'docx_tool':
      return FileText;
    case 'xlsx_tool':
      return Numbers;
    case 'cache':
      return Archive;

    // File Operations
    case 'search':
      return Search;
    case 'read':
      return Eye;
    case 'create_file':
      return FilePlus;
    case 'update_file':
      return FileEdit;

    // Google Workspace Tools (if still supported)
    case 'sheets_tool':
      return Numbers;
    case 'docs_tool':
      return FileText;

    // Delegation Tools
    case 'delegate':
      return Delegate;
    case 'load':
      return Eye;

    // Special Tools
    case 'final_output':
      return Tool; // Could be a checkmark icon if we had one

    // Default fallback for unknown tools
    default:
      return Tool;
  }
};

/**
 * Maps extension names to their corresponding icon components
 * @param extensionName - The name of the extension
 * @returns React component for the extension icon
 */
export const getExtensionIcon = (extensionName: string): React.ComponentType<ToolIconProps> => {
  switch (extensionName) {
    case 'developer':
      return Code2;
    case 'memory':
      return Brain;
    case 'computercontroller':
      return Monitor;
    default:
      return Tool;
  }
};

/**
 * Helper function to extract tool name from full tool call name
 * @param toolCallName - Full tool call name (e.g., "developer__text_editor")
 * @returns Extracted tool name (e.g., "text_editor")
 */
export const extractToolName = (toolCallName: string): string => {
  const lastIndex = toolCallName.lastIndexOf('__');
  return lastIndex === -1 ? toolCallName : toolCallName.substring(lastIndex + 2);
};

/**
 * Helper function to extract extension name from full tool call name
 * @param toolCallName - Full tool call name (e.g., "developer__text_editor")
 * @returns Extracted extension name (e.g., "developer")
 */
export const extractExtensionName = (toolCallName: string): string => {
  const lastIndex = toolCallName.lastIndexOf('__');
  return lastIndex === -1 ? '' : toolCallName.substring(0, lastIndex);
};

/**
 * Main function to get the appropriate icon for a tool call
 * @param toolCallName - Full tool call name (e.g., "developer__text_editor")
 * @param useExtensionIcon - Whether to use extension icon instead of tool icon
 * @returns React component for the icon
 */
export const getToolCallIcon = (
  toolCallName: string,
  useExtensionIcon: boolean = false
): React.ComponentType<ToolIconProps> => {
  if (useExtensionIcon) {
    const extensionName = extractExtensionName(toolCallName);
    return getExtensionIcon(extensionName);
  }

  const toolName = extractToolName(toolCallName);
  return getToolIcon(toolName);
};
