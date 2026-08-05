import { describe, it, expect, vi, afterEach } from 'vitest';
import { ApiError } from '../../shared/api/client';
import { provisioningApi, isOrchDisabled } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('provisioningApi.listControlPlanes', () => {
  it('GETs /control-planes', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));

    await provisioningApi.listControlPlanes();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/control-planes');
    expect((init as RequestInit | undefined)?.method ?? 'GET').toBe('GET');
  });
});

describe('provisioningApi.listTemplates', () => {
  it('GETs /templates', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));

    await provisioningApi.listTemplates();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0][0])).toContain('/templates');
  });
});

describe('provisioningApi.provision', () => {
  it('POSTs the full request shape to /templates/{id}/provision', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          project: { id: 'p1', name: 'Blog API', description: null },
          provisioning: {
            status: 'linked',
            control_plane_id: 'cp1',
            remote_project: 'blog-api',
            blueprint: 'software',
            members: [],
            warnings: [],
          },
        }),
        { status: 200 }
      )
    );

    const res = await provisioningApi.provision('tmpl-1', {
      name: 'Blog API',
      description: null,
      provision_pod: {
        control_plane_id: 'cp1',
        remote_project: 'blog-api',
        blueprint: 'software',
        pod_shape: null,
        budget_usd: 10,
        verify_cmd: null,
      },
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/templates/tmpl-1/provision');
    expect((init as RequestInit).method).toBe('POST');
    const sentBody = JSON.parse(String((init as RequestInit).body));
    expect(sentBody.provision_pod.remote_project).toBe('blog-api');
    expect(sentBody.provision_pod.control_plane_id).toBe('cp1');
    expect(res.provisioning.status).toBe('linked');
  });
});

describe('isOrchDisabled', () => {
  it('is true only for a 404 ApiError', () => {
    expect(isOrchDisabled(new ApiError(404, 'not found'))).toBe(true);
    expect(isOrchDisabled(new ApiError(500, 'boom'))).toBe(false);
    expect(isOrchDisabled(new Error('network'))).toBe(false);
    expect(isOrchDisabled(undefined)).toBe(false);
  });
});
