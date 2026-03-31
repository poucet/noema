// Component for displaying images loaded from asset storage via the noema-asset protocol
export function AssetImage({ src, alt }: { src: string; alt: string }) {
  return (
    <div className="relative group">
      <img
        src={src}
        alt={alt}
        className="max-w-full rounded-lg"
      />
    </div>
  );
}
