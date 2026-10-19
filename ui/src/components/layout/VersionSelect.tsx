import { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { projectVersionsApi, type ProjectVersionsResponse } from '@/api/projectVersions';
import { describeVersion } from '@/lib/projectVersion';
import { useAppStore } from '@/store/appStore';
import { ContextSelect } from './ContextSelect';

/** Radix Select items cannot have an empty value; the Hub default version uses this one. */
const DEFAULT_VERSION = '__default__';

/**
 * The project version selector of the working context. A version is a Git branch; every
 * store of the active project is read and written in the selected one. "" in the store means
 * the version checked out in the selected directory (for a Hub project, its default version).
 */
export function VersionSelect({ refreshTick }: { refreshTick: number }) {
  const { activeProjectKey, activeProjectDir, activeProjectId, activeProjectOrigin, activeProjectVersion,
    projectsLoaded, setProjectVersion } = useAppStore();
  const navigate = useNavigate();
  const location = useLocation();
  const remote = activeProjectOrigin === 'hub';
  const hasProject = projectsLoaded && Boolean(remote ? activeProjectId : activeProjectDir);
  const requestKey = hasProject ? `${activeProjectKey}|${remote ? activeProjectId : activeProjectDir}|${refreshTick}` : '';
  const [result, setResult] = useState<{ key: string; response: ProjectVersionsResponse | null; error: string }>({ key: '', response: null, error: '' });

  useEffect(() => {
    if (!requestKey) return;
    let current = true;
    projectVersionsApi.list(remote ? { projectId: activeProjectId } : { projectDir: activeProjectDir })
      .then(response => { if (current) setResult({ key: requestKey, response, error: '' }); })
      .catch((e: Error) => { if (current) setResult({ key: requestKey, response: null, error: e.message || 'Versions are unavailable.' }); });
    return () => { current = false; };
  }, [requestKey, remote, activeProjectId, activeProjectDir]);

  const settled = result.key === requestKey;
  const loading = Boolean(requestKey) && !settled;
  const response = settled ? result.response : null;
  const error = settled ? result.error : '';
  const versions = response?.versions ?? [];
  const checkoutVersion = remote ? '' : response?.current ?? '';
  const selected = activeProjectVersion || checkoutVersion;
  const options = versions.map(version => ({
    value: version.name || DEFAULT_VERSION,
    label: version.name || 'Default version',
    detail: describeVersion(version, response ?? { remote, git: false }),
    group: version.checkouts?.length ? 'checkouts' : 'others',
  }));
  if (selected && !versions.some(version => version.name === selected)) {
    options.push({ value: selected, label: selected, detail: 'No Graphit data yet.', group: 'others' });
  }
  const value = selected || (remote ? DEFAULT_VERSION : '');
  const selectedOption = options.find(option => option.value === value);

  let display: string;
  if (!hasProject) display = projectsLoaded ? 'Select a project' : 'Loading…';
  else if (loading && !response) display = 'Loading versions…';
  else if (error) display = selected || (remote ? 'Default version' : 'Unavailable');
  else display = selectedOption?.label || (remote ? 'Default version' : selected || 'No versions');

  const choose = (choice: string) => {
    const version = choice === DEFAULT_VERSION || (!remote && choice === checkoutVersion) ? '' : choice;
    if (version === activeProjectVersion) return;
    setProjectVersion(version);
    // A Hub project's code and docs are exact published artifacts in the URL; let the page
    // pick the artifact published for the new version instead of keeping the old one.
    if (remote) {
      if (location.pathname.startsWith('/ast/explorer/')) navigate('/ast/explorer');
      else if (location.pathname.startsWith('/knowledge/explorer/')) navigate('/knowledge/explorer');
    }
  };

  return <ContextSelect label="Version" value={value} display={display}
    title={error ? `Versions could not be listed: ${error}` : selectedOption?.detail}
    groups={remote ? [['checkouts', ''], ['others', 'Shared versions']] : [['checkouts', 'Checked out'], ['others', 'Other versions with data']]}
    options={options}
    disabled={!hasProject || (loading && !response) || (!options.length && !error)}
    onChange={choose} />;
}
