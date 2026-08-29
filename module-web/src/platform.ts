export interface ModuleApiRequestInit extends RequestInit {
  timeoutMs?: number;
  suppressAuthExpired?: boolean;
  expectedStatus?: number;
}

export interface ModuleApi {
  readonly basePath: string;
  request<T>(path: string, init?: ModuleApiRequestInit): Promise<T>;
}

let activeApi: ModuleApi | null = null;

export function bindModuleApi(api: ModuleApi): void {
  if (activeApi && activeApi.basePath !== api.basePath) {
    throw new Error("模块 API 不能跨越 Manifest 命名空间重新绑定");
  }
  activeApi = api;
}

export function request<T>(path: string, init?: ModuleApiRequestInit): Promise<T> {
  if (!activeApi) return Promise.reject(new Error("模块尚未由 Union Web Shell 激活"));
  return activeApi.request<T>(path, init);
}

export function pathSegment(value: string | number): string {
  return encodeURIComponent(String(value));
}
