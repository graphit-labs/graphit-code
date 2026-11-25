import { Children, isValidElement, type ReactNode, type SelectHTMLAttributes } from 'react';
import * as Select from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp } from 'lucide-react';

type Props = Omit<SelectHTMLAttributes<HTMLSelectElement>, 'onChange' | 'value' | 'defaultValue' | 'multiple'> & {
  value: string;
  menuLabel?: string;
  onChange: (event: { target: { value: string } }) => void;
};
type Option = { value: string; label: ReactNode; disabled?: boolean };
function optionsFrom(children: ReactNode): Option[] {
  return Children.toArray(children).flatMap(child => {
    if (!isValidElement<{ value?: string; children?: ReactNode; disabled?: boolean }>(child)) return [];
    if (child.type !== 'option') return optionsFrom(child.props.children);
    return [{ value: String(child.props.value ?? child.props.children ?? ''), label: child.props.children, disabled: child.props.disabled }];
  });
}

/** Domain selects share the workspace menu, including empty-valued filter options. */
export function StyledSelect({ children, value, onChange, disabled, name, required, className, menuLabel, ...props }: Props) {
  const options = optionsFrom(children);
  return <Select.Root value={'option:' + value} onValueChange={v => onChange({ target: { value: v.slice(7) } })}
    disabled={disabled || !options.length} name={name} required={required}>
    <Select.Trigger {...props as React.ComponentProps<typeof Select.Trigger>} className={'work-select ' + (className || '')}>
      <Select.Value>{options.find(o => o.value === value)?.label || 'Select an option'}</Select.Value>
      <Select.Icon><ChevronDown size={15} /></Select.Icon>
    </Select.Trigger>
    <Select.Portal><Select.Content aria-label={menuLabel ?? props["aria-label"]} className="context-select-menu" position="popper" sideOffset={5} collisionPadding={12}>
      <Select.ScrollUpButton className="context-select-scroll"><ChevronUp size={14} /></Select.ScrollUpButton>
      <Select.Viewport className="context-select-options">
        {options.map(o => <Select.Item key={o.value} value={'option:' + o.value} disabled={o.disabled} className="context-select-option">
          <Select.ItemText>{o.label}</Select.ItemText><Select.ItemIndicator className="context-select-check"><Check size={15} /></Select.ItemIndicator>
        </Select.Item>)}
      </Select.Viewport>
      <Select.ScrollDownButton className="context-select-scroll"><ChevronDown size={14} /></Select.ScrollDownButton>
    </Select.Content></Select.Portal>
  </Select.Root>;
}
