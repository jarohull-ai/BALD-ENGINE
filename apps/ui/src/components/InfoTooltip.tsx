import { useState, type ReactNode } from 'react';
import { Info } from 'lucide-react';
import { cn } from '../lib';

interface InfoTooltipProps {
  title?: string;
  content: ReactNode;
  side?: 'top' | 'bottom' | 'left' | 'right';
  className?: string;
  iconClassName?: string;
}

export function InfoTooltip({
  title,
  content,
  side = 'top',
  className,
  iconClassName,
}: InfoTooltipProps) {
  const [open, setOpen] = useState(false);

  return (
    <div
      className={cn('relative inline-flex items-center', className)}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onClick={() => setOpen((prev) => !prev)}
    >
      <button
        type="button"
        aria-label="Informacja"
        className={cn(
          'text-muted-foreground/60 hover:text-amber-400 transition-colors p-0.5 rounded focus:outline-none focus:ring-1 focus:ring-amber-500/50',
          iconClassName,
        )}
      >
        <Info className="h-3.5 w-3.5" />
      </button>

      {open && (
        <div
          className={cn(
            'absolute z-50 w-72 rounded-lg border border-amber-500/25 bg-neutral-900/95 p-3 text-xs text-neutral-200 shadow-xl backdrop-blur-md transition-all',
            side === 'top' && 'bottom-full left-1/2 -translate-x-1/2 mb-2',
            side === 'bottom' && 'top-full left-1/2 -translate-x-1/2 mt-2',
            side === 'left' && 'right-full top-1/2 -translate-y-1/2 mr-2',
            side === 'right' && 'left-full top-1/2 -translate-y-1/2 ml-2',
          )}
        >
          {title && (
            <div className="font-semibold text-amber-300 mb-1 flex items-center gap-1.5 border-b border-white/10 pb-1">
              <span>{title}</span>
            </div>
          )}
          <div className="space-y-1 text-neutral-300 leading-relaxed">{content}</div>
        </div>
      )}
    </div>
  );
}
