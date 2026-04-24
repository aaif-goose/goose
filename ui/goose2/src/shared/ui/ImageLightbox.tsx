import { Dialog, DialogContent, DialogTitle } from "@/shared/ui/dialog";

interface ImageLightboxProps {
  src: string;
  alt?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function ImageLightbox({
  src,
  alt = "Image preview",
  open,
  onOpenChange,
}: ImageLightboxProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="w-auto flex items-center justify-center border-none bg-transparent p-0 shadow-none sm:max-w-[90vw]"
        showCloseButton={false}
        aria-describedby={undefined}
      >
        {/* Visually hidden title for accessibility */}
        <DialogTitle className="sr-only">{alt}</DialogTitle>
        <img
          src={src}
          alt={alt}
          className="max-h-[85vh] max-w-[90vw] rounded-lg object-contain shadow-lg"
        />
      </DialogContent>
    </Dialog>
  );
}
