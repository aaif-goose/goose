import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { ItemIcon } from './ItemIcon';
import { CommandType, getSlashCommands } from '../api';
import { getInitialWorkingDir } from '../utils/workingDir';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  scanningFiles: {
    id: 'mentionPopover.scanningFiles',
    defaultMessage: 'Scanning files...',
  },
  loadingCommands: {
    id: 'mentionPopover.loadingCommands',
    defaultMessage: 'Loading commands...',
  },
  itemsFound: {
    id: 'mentionPopover.itemsFound',
    defaultMessage: '{count, plural, one {# item} other {# items}} found',
  },
  noItemsFound: {
    id: 'mentionPopover.noItemsFound',
    defaultMessage: 'No items found matching "{query}"',
  },
  noCommandsFound: {
    id: 'mentionPopover.noCommandsFound',
    defaultMessage: 'No commands found matching "{query}"',
  },
});

type DisplayItemType = CommandType | 'Directory' | 'File';

const typeOrder: Record<DisplayItemType, number> = {
  Agent: 0,
  Directory: 1,
  File: 2,
  Builtin: 3,
  Skill: 4,
  Recipe: 5,
};

export interface DisplayItem {
  name: string;
  extra: string;
  itemType: DisplayItemType;
  relativePath: string;
}

export interface DisplayItemWithMatch extends DisplayItem {
  matchScore: number;
  matches: number[];
  matchedText: string;
}

interface MentionPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (filePath: string) => void;
  position: { x: number; y: number };
  query: string;
  isSlashCommand: boolean;
  selectedIndex: number;
  onSelectedIndexChange: (index: number) => void;
  workingDir?: string;
}

// Enhanced fuzzy matching algorithm
const fuzzyMatch = (pattern: string, text: string): { score: number; matches: number[] } => {
  if (!pattern) return { score: 0, matches: [] };

  const patternLower = pattern.toLowerCase();
  const textLower = text.toLowerCase();
  const matches: number[] = [];

  let patternIndex = 0;
  let score = 0;
  let consecutiveMatches = 0;

  for (let i = 0; i < textLower.length && patternIndex < patternLower.length; i++) {
    if (textLower[i] === patternLower[patternIndex]) {
      matches.push(i);
      patternIndex++;
      consecutiveMatches++;

      // Bonus for consecutive matches
      score += consecutiveMatches * 3;

      // Bonus for matches at word boundaries or path separators
      if (
        i === 0 ||
        textLower[i - 1] === '/' ||
        textLower[i - 1] === '_' ||
        textLower[i - 1] === '-' ||
        textLower[i - 1] === '.'
      ) {
        score += 10;
      }

      // Bonus for matching the start of the filename (after last /)
      const lastSlash = textLower.lastIndexOf('/', i);
      if (lastSlash !== -1 && i === lastSlash + 1) {
        score += 15;
      }
    } else {
      consecutiveMatches = 0;
    }
  }

  // Only return a score if all pattern characters were matched
  if (patternIndex === patternLower.length) {
    // Less penalty for longer strings to allow nested files to rank well
    score -= text.length * 0.05;

    // Bonus for exact substring matches
    if (textLower.includes(patternLower)) {
      score += 20;
    }

    // Bonus for matching the filename specifically (not just the path)
    const fileName = text.split('/').pop()?.toLowerCase() || '';
    if (fileName.includes(patternLower)) {
      score += 25;
    }

    return { score, matches };
  }

  return { score: -1, matches: [] };
};

const MentionPopover = forwardRef<
  { getDisplayFiles: () => DisplayItemWithMatch[]; selectFile: (index: number) => void },
  MentionPopoverProps
>(
  (
    {
      isOpen,
      onClose,
      onSelect,
      position,
      query,
      isSlashCommand,
      selectedIndex,
      onSelectedIndexChange,
      workingDir,
    },
    ref
  ) => {
    const intl = useIntl();
    const [items, setItems] = useState<DisplayItem[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const popoverRef = useRef<HTMLDivElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const currentWorkingDir = workingDir ?? getInitialWorkingDir();

    const scanDirectoryFromRoot = useCallback(
      async (dirPath: string, relativePath = '', depth = 0): Promise<DisplayItem[]> => {
        // Increase depth limit for better file discovery
        if (depth > 5) return [];

        try {
          const items = await window.electron.listFiles(dirPath);
          const results: DisplayItem[] = [];

          // Common directories to prioritize or skip
          const priorityDirs = [
            'Desktop',
            'Documents',
            'Downloads',
            'Projects',
            'Development',
            'Code',
            'src',
            'components',
            'icons',
          ];
          const skipDirs = [
            '.git',
            '.svn',
            '.hg',
            'node_modules',
            '__pycache__',
            'target',
            'dist',
            'build',
            '.cache',
            '.npm',
            '.yarn',
            'Library',
            'System',
            'Applications',
            '.Trash',
          ];

          const allowedHiddenDirs = [
            '.github',
            '.vscode',
            '.idea',
            '.config',
            '.gitlab',
            '.circleci',
            '.azure',
            '.jenkins',
          ];

          // Don't skip as many directories at deeper levels to find more items
          const skipDirsAtDepth =
            depth > 2 ? ['.git', '.svn', '.hg', 'node_modules', '__pycache__'] : skipDirs;

          // Sort items to prioritize certain directories
          const sortedItems = items.sort((a, b) => {
            const aPriority = priorityDirs.includes(a);
            const bPriority = priorityDirs.includes(b);
            if (aPriority && !bPriority) return -1;
            if (!aPriority && bPriority) return 1;
            return a.localeCompare(b);
          });

          // Increase item limit per directory for better coverage
          const itemLimit = depth === 0 ? 50 : depth === 1 ? 40 : 30;

          for (const item of sortedItems.slice(0, itemLimit)) {
            const fullPath = `${dirPath}/${item}`;
            const itemRelativePath = relativePath ? `${relativePath}/${item}` : item;

            // Skip items in the skip list
            if (skipDirsAtDepth.includes(item)) {
              continue;
            }

            // Skip hidden items except for allowed hidden directories
            if (item.startsWith('.') && !allowedHiddenDirs.includes(item)) {
              continue;
            }

            // First, check if this looks like a file based on extension
            const hasExtension = item.includes('.');
            const ext = item.split('.').pop()?.toLowerCase();
            const commonExtensions = [
              // Code items
              'txt',
              'md',
              'js',
              'ts',
              'jsx',
              'tsx',
              'py',
              'java',
              'cpp',
              'c',
              'h',
              'css',
              'html',
              'json',
              'xml',
              'yaml',
              'yml',
              'toml',
              'ini',
              'cfg',
              'sh',
              'bat',
              'ps1',
              'rb',
              'go',
              'rs',
              'php',
              'sql',
              'r',
              'scala',
              'swift',
              'kt',
              'dart',
              'vue',
              'svelte',
              'astro',
              'scss',
              'less',
              // Documentation
              'readme',
              'license',
              'changelog',
              'contributing',
              // Config items
              'gitignore',
              'dockerignore',
              'editorconfig',
              'prettierrc',
              'eslintrc',
              // Images and assets
              'png',
              'jpg',
              'jpeg',
              'gif',
              'svg',
              'ico',
              'webp',
              'bmp',
              'tiff',
              'tif',
              // Vector and design items
              'ai',
              'eps',
              'sketch',
              'fig',
              'xd',
              'psd',
              // Other common items
              'pdf',
              'doc',
              'docx',
              'xls',
              'xlsx',
              'ppt',
              'pptx',
            ];

            // If it has a known file extension, treat it as a file
            if (hasExtension && ext && commonExtensions.includes(ext)) {
              results.push({
                extra: fullPath,
                name: item,
                itemType: 'File',
                relativePath: itemRelativePath,
              });
              continue;
            }

            // If it's a known file without extension (README, LICENSE, etc.)
            const knownFiles = [
              'readme',
              'license',
              'changelog',
              'contributing',
              'dockerfile',
              'makefile',
            ];
            if (!hasExtension && knownFiles.includes(item.toLowerCase())) {
              results.push({
                extra: fullPath,
                name: item,
                itemType: 'File',
                relativePath: itemRelativePath,
              });
              continue;
            }

            // Otherwise, try to determine if it's a directory
            try {
              await window.electron.listFiles(fullPath);

              results.push({
                name: item,
                extra: fullPath,
                itemType: 'Directory',
                relativePath: itemRelativePath,
              });

              // Recursively scan directories more aggressively
              if (depth < 4 || priorityDirs.includes(item)) {
                const subFiles = await scanDirectoryFromRoot(fullPath, itemRelativePath, depth + 1);
                results.push(...subFiles);
              }
            } catch {
              // If we can't list it and it doesn't have a known extension, skip it
              // This could be a file with an unknown extension or a permission issue
            }
          }

          return results;
        } catch (error) {
          console.error(`Error scanning directory ${dirPath}:`, error);
          return [];
        }
      },
      []
    );

    const getDefaultStartPath = (): string => {
      if (window.electron.platform === 'win32') return 'C:\\Users';
      if (window.electron.platform === 'linux') return '/home';
      return '/Users';
    };

    const compareByType = (a: DisplayItemWithMatch, b: DisplayItemWithMatch) => {
      const orderA = typeOrder[a.itemType] ?? Number.MAX_SAFE_INTEGER;
      const orderB = typeOrder[b.itemType] ?? Number.MAX_SAFE_INTEGER;
      return orderA - orderB;
    };

    const displayItems = useMemo((): DisplayItemWithMatch[] => {
      if (!query.trim()) {
        return items
          .map((file) => ({
            ...file,
            matchScore: 0,
            matches: [],
            matchedText: file.name,
            depth: currentWorkingDir
              ? file.extra.replace(currentWorkingDir, '').split('/').length - 1
              : 0,
          }))
          .sort((a, b) => {
            if (a.depth !== b.depth) return a.depth - b.depth;
            const typeComparison = compareByType(a, b);
            return typeComparison || a.name.localeCompare(b.name);
          });
      }

      return items
        .map((file) => {
          const matches = [
            { match: fuzzyMatch(query, file.name), text: file.name },
            { match: fuzzyMatch(query, file.relativePath), text: file.relativePath },
            { match: fuzzyMatch(query, file.extra), text: file.extra },
          ];

          const { match: bestMatch, text: matchedText } = matches.reduce((best, current) =>
            current.match.score > best.match.score ? current : best
          );

          let finalScore = bestMatch.score;
          if (finalScore > 0 && file.itemType === 'Agent') {
            finalScore += 100;
          } else if (finalScore > 0 && currentWorkingDir) {
            const depth = file.extra.replace(currentWorkingDir, '').split('/').length - 1;
            finalScore += depth <= 1 ? 50 : depth <= 2 ? 30 : depth <= 3 ? 15 : 0;
          }

          return {
            ...file,
            matchScore: finalScore,
            matches: bestMatch.matches,
            matchedText,
          };
        })
        .filter((file) => file.matchScore > 0)
        .sort((a, b) => {
          // Sort by score first, then prefer items over directories, then alphabetically
          const scoreDiff = b.matchScore - a.matchScore;
          if (Math.abs(scoreDiff) >= 1) return scoreDiff;
          const typeComparison = compareByType(a, b);
          return typeComparison || a.name.localeCompare(b.name);
        });
    }, [items, query, currentWorkingDir]);

    const getSelectionText = (item: DisplayItem): string => {
      if (item.itemType === 'Agent') {
        return '@' + item.name + ' ';
      }
      if (item.itemType === 'Skill') {
        return `Use the ${item.name} skill to `;
      }
      if (['Builtin', 'Recipe'].includes(item.itemType)) {
        return '/' + item.name;
      }
      return item.extra;
    };

    // Expose methods to parent component
    useImperativeHandle(
      ref,
      () => ({
        getDisplayFiles: () => displayItems,
        selectFile: (index: number) => {
          if (displayItems[index]) {
            onSelect(getSelectionText(displayItems[index]));
            onClose();
          }
        },
      }),
      [displayItems, onSelect, onClose]
    );

    useEffect(() => {
      let cancelled = false;

      const loadData = async () => {
        setItems([]);
        setIsLoading(true);
        try {
          if (isSlashCommand) {
            const response = await getSlashCommands({
              query: { working_dir: currentWorkingDir },
              throwOnError: true,
            });
            if (cancelled) return;
            const commandItems: DisplayItem[] = (response.data?.commands || [])
              .filter((cmd) => cmd.command_type !== 'Agent')
              .map((cmd) => ({
                name: cmd.command,
                extra: cmd.help,
                itemType: cmd.command_type,
                relativePath: cmd.command,
              }));
            setItems(commandItems);
          } else {
            // Fetch agents from server and scan files in parallel
            const [agentResponse, scannedFiles] = await Promise.all([
              getSlashCommands({
                query: { working_dir: currentWorkingDir },
                throwOnError: true,
              }).catch(() => null),
              scanDirectoryFromRoot(currentWorkingDir || getDefaultStartPath()),
            ]);
            if (cancelled) return;
            const agentItems: DisplayItem[] = (agentResponse?.data?.commands || [])
              .filter((cmd) => cmd.command_type === 'Agent')
              .map((cmd) => ({
                name: cmd.command,
                extra: cmd.help,
                itemType: cmd.command_type,
                relativePath: cmd.command,
              }));
            setItems([...agentItems, ...scannedFiles]);
          }
        } catch (error) {
          if (!cancelled) {
            console.error('Error loading popover items:', error);
            setItems([]);
          }
        } finally {
          if (!cancelled) {
            setIsLoading(false);
          }
        }
      };

      if (isOpen) {
        loadData();
      }

      return () => {
        cancelled = true;
      };
    }, [isOpen, isSlashCommand, scanDirectoryFromRoot, currentWorkingDir]);

    useEffect(() => {
      const handleClickOutside = (event: MouseEvent) => {
        if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) {
          onClose();
        }
      };

      if (isOpen) {
        document.addEventListener('mousedown', handleClickOutside);
      }

      return () => {
        document.removeEventListener('mousedown', handleClickOutside);
      };
    }, [isOpen, onClose]);

    // Scroll selected item into view
    useEffect(() => {
      if (listRef.current && selectedIndex >= 0 && selectedIndex < displayItems.length) {
        const selectedElement = listRef.current.children[selectedIndex] as HTMLElement;
        if (selectedElement) {
          selectedElement.scrollIntoView({
            block: 'nearest',
            behavior: 'smooth',
          });
        }
      }
    }, [selectedIndex, displayItems.length]);

    const handleItemClick = (index: number) => {
      if (index >= 0 && index < displayItems.length) {
        onSelectedIndexChange(index);
        onSelect(getSelectionText(displayItems[index]));
        onClose();
      }
    };

    if (!isOpen) return null;

    return (
      <div
        ref={popoverRef}
        className="fixed z-50 bg-background-primary border border-border-primary rounded-lg shadow-lg min-w-96 max-w-lg max-h-80"
        style={{
          left: position.x,
          top: position.y - 10, // Position above the chat input
          transform: 'translateY(-100%)', // Move it fully above
        }}
      >
        <div className="p-3 flex flex-col max-h-80">
          {isLoading ? (
            <div className="flex items-center justify-center py-4">
              <div className="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2"></div>
              <span className="ml-2 text-sm text-text-secondary">{intl.formatMessage(isSlashCommand ? i18n.loadingCommands : i18n.scanningFiles)}</span>
            </div>
          ) : (
            <>
              {displayItems.length > 0 && (
                <div className="text-xs text-text-secondary mb-2 px-1">
                  {intl.formatMessage(i18n.itemsFound, { count: displayItems.length })}
                </div>
              )}
              <div
                ref={listRef}
                className="space-y-1 overflow-y-auto flex-1 scrollbar-thin scrollbar-thumb-borderStandard scrollbar-track-transparent"
                style={{ maxHeight: '280px' }}
              >
                {displayItems.map((item, index) => (
                  <div
                    key={`${item.itemType}-${item.name}`}
                    onClick={() => handleItemClick(index)}
                    data-selected={index === selectedIndex}
                    className={`flex items-center gap-3 p-2 rounded-md cursor-pointer transition-colors ${
                      index === selectedIndex ? 'bg-sidebar-accent' : 'hover:bg-sidebar-accent/50'
                    }`}
                  >
                    <div className="flex-shrink-0 text-text-secondary">
                      <ItemIcon item={item} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="text-sm truncate text-text-primary">{item.name}</div>
                      <div className="text-xs truncate text-text-secondary">{item.extra}</div>
                    </div>
                  </div>
                ))}

                {!isLoading && displayItems.length === 0 && query && (
                  <div className="p-4 text-center text-text-secondary text-sm">
                    {intl.formatMessage(isSlashCommand ? i18n.noCommandsFound : i18n.noItemsFound, { query })}
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    );
  }
);

MentionPopover.displayName = 'MentionPopover';

export default MentionPopover;
