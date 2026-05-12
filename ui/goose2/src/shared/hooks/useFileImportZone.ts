import { useCallback, useRef, useState } from "react";

interface FileImportZoneOptions {
  onImportFile: (fileBytes: number[], fileName: string) => void;
  validateFile?: (file: Pick<File, "name" | "type">) => string | null;
  onImportError?: (message: string) => void;
}

/**
 * Shared drag-and-drop + file-picker infrastructure for import zones.
 * Returns state, handlers, and a ref for the hidden `<input type="file">`.
 */
export function useFileImportZone({
  onImportFile,
  validateFile,
  onImportError,
}: FileImportZoneOptions) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [isDragOver, setIsDragOver] = useState(false);

  const importFile = useCallback(
    async (file: File) => {
      const validationMessage = validateFile?.(file);
      if (validationMessage) {
        onImportError?.(validationMessage);
        return;
      }
      const buffer = await file.arrayBuffer();
      const bytes = Array.from(new Uint8Array(buffer));
      onImportFile(bytes, file.name);
    },
    [onImportFile, onImportError, validateFile],
  );

  const dropHandlers = {
    onDragLeave: () => setIsDragOver(false),
    onDragOver: (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(true);
    },
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) {
        void importFile(file);
      }
    },
  };

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      void importFile(file);
      e.target.value = "";
    },
    [importFile],
  );

  const openFilePicker = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  return {
    fileInputRef,
    isDragOver,
    dropHandlers,
    handleFileChange,
    openFilePicker,
  };
}
