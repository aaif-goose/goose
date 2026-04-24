import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ImageLightbox } from "@/shared/ui/ImageLightbox";

export function ClickableImage({ src, alt }: { src: string; alt: string }) {
  const { t } = useTranslation("chat");
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="cursor-pointer rounded-lg"
        aria-label={t("image.view", { label: alt })}
      >
        <img
          src={src}
          alt={alt}
          className="max-h-48 max-w-xs rounded-lg object-contain"
        />
      </button>
      <ImageLightbox src={src} alt={alt} open={open} onOpenChange={setOpen} />
    </>
  );
}
