import { cn } from '@/lib/utils'

interface LoadingSpinnerProps {
  size?: 'sm' | 'md' | 'lg'
  className?: string
  label?: string
}

export function LoadingSpinner({ size = 'md', className, label }: LoadingSpinnerProps) {
  const sizes = { sm: 'w-4 h-4 border-2', md: 'w-6 h-6 border-2', lg: 'w-8 h-8 border-[3px]' }
  return (
    <div className={cn('flex flex-col items-center gap-3', className)}>
      <div
        className={cn(
          sizes[size],
          'border-muted-foreground/30 border-t-foreground rounded-full animate-spin',
        )}
      />
      {label && <p className="text-sm text-muted-foreground">{label}</p>}
    </div>
  )
}
