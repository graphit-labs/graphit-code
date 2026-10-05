import * as Select from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp } from 'lucide-react';

type ContextGroup = string

export function ContextSelect({ label, value, display, options, disabled, onChange, groups, title }: {
  label: string; value: string; display: string; disabled: boolean; title?: string;
  options: { value: string; label: string; detail?: string; group?: ContextGroup }[]; onChange: (value: string) => void;
  /** Ordered option groups as [group, heading]; options without a group form one unlabeled list. */
  groups?: readonly (readonly [ContextGroup, string])[];
}) {
  const sections = groups ?? ([['', '']] as const);
  return <Select.Root value={value} onValueChange={onChange} disabled={disabled}>
    <Select.Trigger className="context-select" aria-label={label} title={title || options.find(o => o.value === value)?.detail || display}>
      <span className="context-select-label">{label}</span>
      <span className="context-select-value"><Select.Value placeholder={display}>{display}</Select.Value></span>
      <Select.Icon><ChevronDown size={13} /></Select.Icon>
    </Select.Trigger>
    <Select.Portal>
      <Select.Content className="context-select-menu" position="popper" sideOffset={6} align="end" collisionPadding={12}>
        <Select.ScrollUpButton className="context-select-scroll"><ChevronUp size={14} /></Select.ScrollUpButton>
        <Select.Viewport className="context-select-options">
          {sections.map(([group, groupLabel]) => {
            const groupOptions = options.filter(option => !groups || option.group === group);
            if (!groupOptions.length) return null;
            return <Select.Group key={group || 'all'}>
              {groupLabel && <Select.Label className="context-select-menu-label">{groupLabel}</Select.Label>}
              {groupOptions.map(option => <Select.Item className="context-select-option" key={option.value} value={option.value} textValue={option.label}>
                <span className="context-select-option-copy"><Select.ItemText>{option.label}</Select.ItemText>{option.detail && <small>{option.detail}</small>}</span>
                <Select.ItemIndicator className="context-select-check"><Check size={15} /></Select.ItemIndicator>
              </Select.Item>)}
            </Select.Group>;
          })}
        </Select.Viewport>
        <Select.ScrollDownButton className="context-select-scroll"><ChevronDown size={14} /></Select.ScrollDownButton>
      </Select.Content>
    </Select.Portal>
  </Select.Root>;
}
