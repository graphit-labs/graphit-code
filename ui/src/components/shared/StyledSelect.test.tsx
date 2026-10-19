import '@/test/contextControls';
import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi, afterEach } from 'vitest';
import { ModalPortal } from './ModalPortal';
import { cleanup } from '@testing-library/react';
afterEach(cleanup);
import { StyledSelect } from './StyledSelect';
it('supports empty filters, themed options and keyboard focus return', async () => {
  function Field() {
    const [value, setValue] = useState('');
    return <label>Type<StyledSelect value={value} onChange={e => setValue(e.target.value)}>
      <option value="">All types</option><option value="skill">Skill</option><option disabled value="agent">Agent</option>
    </StyledSelect></label>;
  }
  const user = userEvent.setup();
  render(<Field />);
  const trigger = screen.getByRole('combobox', { name: 'Type' });
  await user.tab(); await user.keyboard('{Enter}');
  expect(screen.getByRole('listbox').className).toContain('context-select-menu');
  expect(screen.getByRole('option', { name: 'Agent' }).getAttribute('data-disabled')).not.toBeNull();
  await user.click(screen.getByRole('option', { name: 'Skill' }));
  expect(trigger.textContent).toContain('Skill');
  await user.keyboard('{Enter}');
  await user.click(screen.getByRole('option', { name: 'All types' }));
  expect(trigger.textContent).toContain('All types');
  await user.keyboard('{Enter}{Escape}');
  expect(document.activeElement).toBe(trigger);
});

it('closes a nested select with Escape before closing its parent modal', async () => {
 const close=vi.fn();const user=userEvent.setup();
 render(<ModalPortal onClose={close}><div role="dialog"><StyledSelect aria-label="Dependency type" value="skill" onChange={()=>{}}><option value="skill">Skill</option><option value="agent">Agent</option></StyledSelect></div></ModalPortal>);
 const trigger=screen.getByRole('combobox',{name:'Dependency type'});
 await user.click(trigger);await user.keyboard('{Escape}');
 expect(screen.queryByRole('listbox')).toBeNull();expect(close).not.toHaveBeenCalled();
 expect(document.activeElement).toBe(trigger);
 await user.keyboard('{Escape}');expect(close).toHaveBeenCalledOnce();
});
