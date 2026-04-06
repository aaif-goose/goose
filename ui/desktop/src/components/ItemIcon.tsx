import React from 'react';
import {
  Folder,
  File,
  Image,
  Video,
  Music,
  Archive,
  FileText,
  Palette,
  Code,
  Database,
  Settings,
  Terminal,
  Zap,
  BookOpen,
  Wrench,
  Sparkles,
} from 'lucide-react';
import { DisplayItem } from './MentionPopover';

interface FileIconProps {
  item: DisplayItem;
}

interface IconInfo {
  Icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>;
  color: string;
}

export const getItemIcon = (item: DisplayItem): IconInfo => {
  switch (item.itemType) {
    case 'Builtin':
      return { Icon: Zap, color: '#3b82f6' }; // Blue
    case 'Recipe':
      return { Icon: BookOpen, color: '#10b981' }; // Green
    case 'Skill':
      return { Icon: Sparkles, color: '#8b5cf6' }; // Purple
    case 'Directory':
      return { Icon: Folder, color: '#f59e0b' }; // Amber
    default: {
      const ext = item.name.split('.').pop()?.toLowerCase() || '';

      // Image files
      if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico', 'webp', 'bmp', 'tiff', 'tif'].includes(ext)) {
        return { Icon: Image, color: '#8b5cf6' }; // Purple
      }

      // Video files
      if (['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv', 'wmv'].includes(ext)) {
        return { Icon: Video, color: '#ef4444' }; // Red
      }

      // Audio files
      if (['mp3', 'wav', 'flac', 'aac', 'ogg', 'm4a'].includes(ext)) {
        return { Icon: Music, color: '#f97316' }; // Orange
      }

      // Archive/compressed files
      if (['zip', 'tar', 'gz', 'rar', '7z', 'bz2'].includes(ext)) {
        return { Icon: Archive, color: '#6b7280' }; // Gray
      }

      // PDF files
      if (ext === 'pdf') {
        return { Icon: FileText, color: '#dc2626' }; // Red
      }

      // Design files
      if (['ai', 'eps', 'sketch', 'fig', 'xd', 'psd'].includes(ext)) {
        return { Icon: Palette, color: '#ec4899' }; // Pink
      }

      // JavaScript/TypeScript files
      if (['js', 'jsx', 'ts', 'tsx', 'mjs', 'cjs'].includes(ext)) {
        return { Icon: Code, color: '#eab308' }; // Yellow
      }

      // Python files
      if (['py', 'pyw', 'pyc'].includes(ext)) {
        return { Icon: Code, color: '#3b82f6' }; // Blue
      }

      // HTML files
      if (['html', 'htm', 'xhtml'].includes(ext)) {
        return { Icon: Code, color: '#f97316' }; // Orange
      }

      // CSS files
      if (['css', 'scss', 'sass', 'less', 'stylus'].includes(ext)) {
        return { Icon: Code, color: '#06b6d4' }; // Cyan
      }

      // JSON/Data files
      if (['json', 'xml', 'yaml', 'yml', 'toml', 'csv'].includes(ext)) {
        return { Icon: FileText, color: '#10b981' }; // Green
      }

      // Markdown files
      if (['md', 'markdown', 'mdx'].includes(ext)) {
        return { Icon: FileText, color: '#6366f1' }; // Indigo
      }

      // Database files
      if (['sql', 'db', 'sqlite', 'sqlite3'].includes(ext)) {
        return { Icon: Database, color: '#059669' }; // Emerald
      }

      // Configuration files
      if (
        [
          'env',
          'ini',
          'cfg',
          'conf',
          'config',
          'gitignore',
          'dockerignore',
          'editorconfig',
          'prettierrc',
          'eslintrc',
        ].includes(ext || '') ||
        ['dockerfile', 'makefile', 'rakefile', 'gemfile'].includes(item.name.toLowerCase())
      ) {
        return { Icon: Settings, color: '#6b7280' }; // Gray
      }

      // Text files
      if (
        ['txt', 'log', 'readme', 'license', 'changelog', 'contributing'].includes(ext || '') ||
        ['readme', 'license', 'changelog', 'contributing'].includes(item.name.toLowerCase())
      ) {
        return { Icon: FileText, color: '#374151' }; // Dark gray
      }

      // Executable files
      if (['exe', 'app', 'deb', 'rpm', 'dmg', 'pkg', 'msi'].includes(ext || '')) {
        return { Icon: Wrench, color: '#7c3aed' }; // Purple
      }

      // Script files
      if (
        ['sh', 'bash', 'zsh', 'fish', 'bat', 'cmd', 'ps1', 'rb', 'pl', 'php'].includes(ext || '')
      ) {
        return { Icon: Terminal, color: '#059669' }; // Emerald
      }

      // Default file icon
      return { Icon: File, color: '#6b7280' }; // Gray
    }
  }
};

export const ItemIcon: React.FC<FileIconProps> = ({ item }) => {
  const { Icon, color } = getItemIcon(item);

  return <Icon className="w-4 h-4" style={{ color }} />;
};
