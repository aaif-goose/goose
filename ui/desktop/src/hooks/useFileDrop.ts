import { useCallback, useState, useRef, useEffect } from 'react';
import { compressImageDataUrl, errorMessage } from '../utils/conversionUtils';
import { defineMessages, useIntl } from '../i18n';
import { toastError } from '../toasts';

const i18n = defineMessages({
  remoteAttachmentUnsupported: {
    id: 'fileDrop.remoteAttachmentUnsupported',
    defaultMessage: 'Only images can be attached when connected to an external backend',
  },
});

export interface DroppedFile {
  id: string;
  path: string;
  name: string;
  type: string;
  isImage: boolean;
  dataUrl?: string;
  isLoading?: boolean;
  error?: string;
}

export const useFileDrop = () => {
  const intl = useIntl();
  const [droppedFiles, setDroppedFiles] = useState<DroppedFile[]>([]);
  const activeReadersRef = useRef<Set<FileReader>>(new Set());

  // Cleanup effect to prevent memory leaks
  useEffect(() => {
    return () => {
      // Abort any active FileReaders on unmount
      // eslint-disable-next-line react-hooks/exhaustive-deps
      const readers = activeReadersRef.current;
      readers.forEach((reader) => {
        try {
          reader.abort();
        } catch {
          // Reader might already be done, ignore errors
        }
      });
      readers.clear();
    };
  }, []);

  // Non-image attachments are sent as a client-local path, which an external
  // backend cannot open. Drop them with a notice and return the files to keep.
  const rejectUnsupportedFiles = useCallback(
    (files: File[]): File[] => {
      if (window.appConfig?.get('GOOSE_EXTERNAL_BACKEND') !== true) {
        return files;
      }
      const rejected = files.filter((file) => !file.type.startsWith('image/'));
      if (rejected.length > 0) {
        toastError({
          title: intl.formatMessage(i18n.remoteAttachmentUnsupported),
          msg: rejected.map((file) => file.name).join(', '),
        });
      }
      return files.filter((file) => file.type.startsWith('image/'));
    },
    [intl]
  );

  const addDroppedFiles = useCallback(async (files: File[]) => {
    if (files.length > 0) {
      const droppedFileObjects: DroppedFile[] = [];

      for (let i = 0; i < files.length; i++) {
        const file = files[i];

        let droppedFile: DroppedFile;

        try {
          const path = window.electron.getPathForFile(file);
          const isImage = file.type.startsWith('image/');

          droppedFile = {
            id: `dropped-${Date.now()}-${i}`,
            path,
            name: file.name,
            type: file.type,
            isImage,
            isLoading: isImage, // Only images need loading state for preview generation
          };
        } catch (error) {
          console.error('Error processing file:', file.name, error);
          // Create an error file object
          droppedFile = {
            id: `dropped-error-${Date.now()}-${i}`,
            path: '',
            name: file.name,
            type: file.type,
            isImage: false,
            isLoading: false,
            error: `Failed to get file path: ${errorMessage(error, 'Unknown error')}`,
          };
        }

        droppedFileObjects.push(droppedFile);

        if (droppedFile.isImage && !droppedFile.error) {
          const reader = new FileReader();
          activeReadersRef.current.add(reader);

          reader.onload = async (event) => {
            const dataUrl = event.target?.result as string;
            try {
              // Compress the image
              const compressedDataUrl = await compressImageDataUrl(dataUrl);
              setDroppedFiles((prev) =>
                prev.map((f) =>
                  f.id === droppedFile.id
                    ? { ...f, dataUrl: compressedDataUrl, isLoading: false }
                    : f
                )
              );
            } catch (compressionError) {
              console.error('Failed to compress image:', file.name, compressionError);
              setDroppedFiles((prev) =>
                prev.map((f) =>
                  f.id === droppedFile.id
                    ? { ...f, error: 'Failed to compress image', isLoading: false }
                    : f
                )
              );
            }
            activeReadersRef.current.delete(reader);
          };

          reader.onerror = () => {
            console.error('Failed to read image:', file.name);
            setDroppedFiles((prev) =>
              prev.map((f) =>
                f.id === droppedFile.id
                  ? { ...f, error: 'Failed to load image', isLoading: false }
                  : f
              )
            );
            activeReadersRef.current.delete(reader);
          };

          reader.onabort = () => {
            activeReadersRef.current.delete(reader);
          };

          reader.readAsDataURL(file);
        }
      }

      setDroppedFiles((prev) => [...prev, ...droppedFileObjects]);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      return addDroppedFiles(rejectUnsupportedFiles(Array.from(e.dataTransfer.files)));
    },
    [addDroppedFiles, rejectUnsupportedFiles]
  );

  const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
  }, []);

  return {
    droppedFiles,
    setDroppedFiles,
    handleDrop,
    handleDragOver,
    rejectUnsupportedFiles,
  };
};
