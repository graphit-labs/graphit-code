import React, { useCallback, useRef, useState } from "react";
import { ModalPortal } from "@/components/shared/ModalPortal";
import { X } from "lucide-react";

export function MarkdownImagePreview({ src, onClose }: { src: string; onClose: () => void }) {
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragging = useRef(false);
  const didDrag = useRef(false);
  const lastPos = useRef({ x: 0, y: 0 });

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setScale((s) => Math.max(0.5, Math.min(5, s - e.deltaY * 0.002)));
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragging.current = true;
    setIsDragging(true);
    didDrag.current = false;
    lastPos.current = { x: e.clientX, y: e.clientY };
    e.preventDefault();
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const dx = e.clientX - lastPos.current.x;
    const dy = e.clientY - lastPos.current.y;
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) didDrag.current = true;
    setPan((p) => ({ x: p.x + dx, y: p.y + dy }));
    lastPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const handleMouseUp = useCallback(() => {
    dragging.current = false;
    setIsDragging(false);
  }, []);

  const handleOverlayClick = useCallback(() => {
    if (!didDrag.current) onClose();
  }, [onClose]);

  return (
    <ModalPortal onClose={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Image preview"
        className="fixed inset-0 z-[9999] bg-black/85 backdrop-blur-md flex items-center justify-center animate-in fade-in duration-200"
        style={{ cursor: isDragging ? "grabbing" : "grab" }}
        onClick={handleOverlayClick}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        <div className="absolute top-4 right-4 flex items-center gap-2 z-10 bg-black/40 backdrop-blur-md border border-white/10 p-1.5 rounded-xl">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale((s) => Math.min(5, s + 0.25));
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
          >
            +
          </button>
          <span className="text-white/80 text-xs font-mono min-w-[5ch] text-center">
            {Math.round(scale * 100)}%
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale((s) => Math.max(0.5, s - 0.25));
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
          >
            −
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setScale(1);
              setPan({ x: 0, y: 0 });
            }}
            className="px-3 py-1.5 rounded-lg bg-white/10 hover:bg-white/20 text-white text-xs font-semibold transition-colors"
          >
            Reset
          </button>
          <div className="w-px h-5 bg-white/10 mx-1" />
          <button
            aria-label="Close image preview"
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white flex items-center justify-center transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <img
          src={src}
          alt="Zoomed view"
          className="max-w-[92vw] max-h-[92vh] bg-white rounded-2xl shadow-2xl select-none pointer-events-none transition-transform duration-75"
          style={{
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
          }}
          draggable={false}
        />
      </div>
    </ModalPortal>
  );
}
